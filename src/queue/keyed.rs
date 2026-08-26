use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use flume::{Receiver, bounded};
use tokio::sync::watch;

use super::pipeline::RequestError;

#[derive(Debug, Clone)]
pub struct KeyedBufferConfig {
    pub buffer_size: usize,
    pub idle_ttl: Duration,
    pub cleanup_interval: Duration,
}

impl Default for KeyedBufferConfig {
    fn default() -> Self {
        Self {
            buffer_size: 2,
            idle_ttl: Duration::from_secs(60),
            cleanup_interval: Duration::from_secs(15),
        }
    }
}

impl KeyedBufferConfig {
    pub fn new(buffer_size: usize, idle_ttl: Duration) -> Self {
        Self {
            buffer_size,
            idle_ttl,
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    pub fn with_cleanup_interval(mut self, interval: Duration) -> Self {
        self.cleanup_interval = interval;
        self
    }
}

struct KeyedSession<In, Out> {
    out_rx: Receiver<Out>,
    input_tx: watch::Sender<In>,
    last_accessed: Instant,
}

pub struct KeyedBufferedQueue<Key, In, Out> {
    sessions: Arc<Mutex<HashMap<Key, KeyedSession<In, Out>>>>,
    spawn_worker: Arc<dyn Fn(Key, In) -> KeyedSession<In, Out> + Send + Sync>,
}

impl<Key, In, Out> Clone for KeyedBufferedQueue<Key, In, Out> {
    fn clone(&self) -> Self {
        Self {
            sessions: self.sessions.clone(),
            spawn_worker: self.spawn_worker.clone(),
        }
    }
}

impl<Key, In, Out> KeyedBufferedQueue<Key, In, Out>
where
    Key: std::hash::Hash + Eq + Clone + Send + Sync + 'static,
    In: Clone + Send + Sync + 'static,
    Out: Send + 'static,
{
    pub async fn request(&self, key: Key, input: In) -> Result<Out, RequestError> {
        let out_rx = {
            let mut sessions = self.sessions.lock().unwrap();
            let session = sessions
                .entry(key.clone())
                .or_insert_with(|| (self.spawn_worker)(key, input.clone()));

            session.last_accessed = Instant::now();
            let _ = session.input_tx.send(input);
            session.out_rx.clone()
        };

        out_rx
            .recv_async()
            .await
            .map_err(|_| RequestError::WorkerDropped)
    }

    #[allow(dead_code)]
    pub async fn request_timeout(
        &self,
        key: Key,
        input: In,
        timeout: Duration,
    ) -> Result<Out, RequestError> {
        tokio::time::timeout(timeout, self.request(key, input))
            .await
            .map_err(|_| RequestError::Timeout)?
    }
}

pub fn spawn_keyed_buffered_queue<S, Key, In, Dep, Out, F, Fut>(
    state: Arc<S>,
    dep_rx: Receiver<Dep>,
    config: KeyedBufferConfig,
    handler: F,
) -> KeyedBufferedQueue<Key, In, Out>
where
    S: Send + Sync + 'static,
    Key: std::hash::Hash + Eq + Clone + Send + Sync + 'static,
    In: Clone + Send + Sync + 'static,
    Dep: Send + 'static,
    Out: Send + 'static,
    F: Fn(In, Dep, Arc<S>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Out> + Send + 'static,
{
    let sessions = Arc::new(Mutex::new(HashMap::<Key, KeyedSession<In, Out>>::new()));

    let spawn_worker = {
        let state = state.clone();
        let dep_rx = dep_rx.clone();
        let handler = handler.clone();
        let buffer_size = config.buffer_size;

        Arc::new(move |_key: Key, initial_input: In| {
            let (out_tx, out_rx) = bounded::<Out>(buffer_size);
            let (input_tx, input_rx) = watch::channel(initial_input);

            let state = state.clone();
            let dep_rx = dep_rx.clone();
            let handler = handler.clone();

            tokio::spawn(async move {
                loop {
                    let dep_item = match dep_rx.recv_async().await {
                        Ok(item) => item,
                        Err(_) => break,
                    };

                    let current_input = input_rx.borrow().clone();
                    let output = handler(current_input, dep_item, state.clone()).await;

                    // Automatically pauses (backpressure) when per-user buffer is full.
                    // Terminates if session was evicted from HashMap (drops out_rx).
                    if out_tx.send_async(output).await.is_err() {
                        break;
                    }
                }
            });

            KeyedSession {
                out_rx,
                input_tx,
                last_accessed: Instant::now(),
            }
        })
    };

    // Supervisor: periodically evicts idle sessions
    {
        let sessions = sessions.clone();
        let idle_ttl = config.idle_ttl;
        let check_interval = config.cleanup_interval;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(check_interval).await;
                let now = Instant::now();
                let mut sessions_lock = sessions.lock().unwrap();
                sessions_lock
                    .retain(|_, session| now.duration_since(session.last_accessed) <= idle_ttl);
            }
        });
    }

    KeyedBufferedQueue {
        sessions,
        spawn_worker,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_keyed_buffered_queue_flow() {
        let (dep_tx, dep_rx) = bounded::<i32>(10);
        for i in 1..=5 {
            dep_tx.send_async(i).await.unwrap();
        }

        let state = Arc::new(());
        let config = KeyedBufferConfig::new(2, Duration::from_secs(10));

        let queue: KeyedBufferedQueue<u64, String, String> = spawn_keyed_buffered_queue(
            state,
            dep_rx,
            config,
            |input: String, dep: i32, _| async move { format!("{}-{}", input, dep) },
        );

        let res1 = queue.request(1, "hello".to_string()).await.unwrap();
        assert_eq!(res1, "hello-1");

        let res2 = queue.request(1, "world".to_string()).await.unwrap();
        // Since input channel was updated, next generated item uses latest input
        assert!(res2.starts_with("hello-") || res2.starts_with("world-"));
    }
}
