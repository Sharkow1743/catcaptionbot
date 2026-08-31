use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use flume::{Receiver, bounded};
use tokio::sync::watch;

use super::pipeline::{RequestError, WorkerGuard};

#[derive(Debug, Clone)]
pub struct KeyedBufferConfig {
    pub buffer_size: usize,
    pub idle_ttl: Duration,
    pub cleanup_interval: Duration,
    pub min_workers: usize,
    pub max_workers: usize,
    pub check_interval: Duration,
    pub scale_up_threshold: f32,
    pub scale_down_threshold: f32,
}

impl Default for KeyedBufferConfig {
    fn default() -> Self {
        Self {
            buffer_size: 2,
            idle_ttl: Duration::from_secs(60),
            cleanup_interval: Duration::from_secs(15),
            min_workers: 1,
            max_workers: 1,
            check_interval: Duration::from_millis(250),
            scale_up_threshold: 0.75,
            scale_down_threshold: 0.25,
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

    #[allow(dead_code)]
    pub fn with_workers(mut self, min_workers: usize, max_workers: usize) -> Self {
        self.min_workers = min_workers.max(1);
        self.max_workers = max_workers.max(self.min_workers);
        self
    }

    #[allow(dead_code)]
    pub fn with_check_interval(mut self, interval: Duration) -> Self {
        self.check_interval = interval;
        self
    }

    #[allow(dead_code)]
    pub fn with_thresholds(mut self, scale_up: f32, scale_down: f32) -> Self {
        self.scale_up_threshold = scale_up;
        self.scale_down_threshold = scale_down;
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
        let config = config.clone();

        Arc::new(move |_key: Key, initial_input: In| {
            let (out_tx, out_rx) = bounded::<Out>(config.buffer_size);
            let (input_tx, input_rx) = watch::channel(initial_input);

            let min_workers = config.min_workers.max(1);
            let target = Arc::new(AtomicUsize::new(min_workers));
            let active = Arc::new(AtomicUsize::new(0));

            let spawn_one_worker = {
                let state = state.clone();
                let dep_rx = dep_rx.clone();
                let handler = handler.clone();
                let input_rx = input_rx.clone();
                let out_tx = out_tx.clone();
                let active = active.clone();
                let target = target.clone();

                Arc::new(move || {
                    let _guard = WorkerGuard::new(&active);
                    let state = state.clone();
                    let dep_rx = dep_rx.clone();
                    let handler = handler.clone();
                    let input_rx = input_rx.clone();
                    let out_tx = out_tx.clone();
                    let active = active.clone();
                    let target = target.clone();

                    tokio::spawn(async move {
                        let _guard = _guard;
                        loop {
                            let dep_item = match dep_rx.recv_async().await {
                                Ok(item) => item,
                                Err(_) => break,
                            };

                            let current_input = input_rx.borrow().clone();
                            let output = handler(current_input, dep_item, state.clone()).await;

                            // Automatically pauses (backpressure) when per-session buffer is full.
                            // Terminates if session was evicted from HashMap (drops out_rx).
                            if out_tx.send_async(output).await.is_err() {
                                break;
                            }

                            if active.load(Ordering::Relaxed) > target.load(Ordering::Relaxed) {
                                break;
                            }
                        }
                    });
                })
            };

            for _ in 0..min_workers {
                spawn_one_worker();
            }

            // Autoscaler: scales workers up when consumption empties the buffer, and down when idle
            if config.max_workers > min_workers {
                let out_tx = out_tx.clone();
                let target = target.clone();
                let spawn_one_worker = spawn_one_worker.clone();
                let config = config.clone();

                tokio::spawn(async move {
                    let capacity = out_tx.capacity().unwrap_or(config.buffer_size) as f32;
                    loop {
                        tokio::time::sleep(config.check_interval).await;
                        if out_tx.is_disconnected() {
                            break;
                        }
                        let free = (capacity as usize - out_tx.len()) as f32;
                        let cur_target = target.load(Ordering::Relaxed);

                        if free > capacity * config.scale_up_threshold
                            && cur_target < config.max_workers
                        {
                            target.store(cur_target + 1, Ordering::Relaxed);
                            spawn_one_worker();
                        } else if free < capacity * config.scale_down_threshold
                            && cur_target > config.min_workers
                        {
                            target.store(cur_target - 1, Ordering::Relaxed);
                        }
                    }
                });
            }

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

    #[tokio::test]
    async fn test_keyed_buffered_queue_scaling() {
        let (dep_tx, dep_rx) = bounded::<i32>(100);
        for i in 1..=20 {
            dep_tx.send_async(i).await.unwrap();
        }

        let state = Arc::new(());
        let config = KeyedBufferConfig::new(4, Duration::from_secs(10))
            .with_workers(1, 4)
            .with_check_interval(Duration::from_millis(10));

        let queue: KeyedBufferedQueue<u64, String, String> = spawn_keyed_buffered_queue(
            state,
            dep_rx,
            config,
            |input: String, dep: i32, _| async move {
                tokio::time::sleep(Duration::from_millis(5)).await;
                format!("{}-{}", input, dep)
            },
        );

        for _ in 0..10 {
            let res = queue.request(1, "load".to_string()).await.unwrap();
            assert!(res.starts_with("load-"));
        }
    }
}
