use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use flume::{Receiver, Sender, bounded};
use tokio::sync::{Notify, watch};

use super::pipeline::RequestError;

#[derive(Debug, Clone)]
pub struct KeyedBufferConfig {
    /// Number of pre-generated items stored ahead per key
    pub buffer_size: usize,
    /// Maximum concurrent workers that can serve a single heavy key
    pub max_workers_per_key: usize,
    /// Inactivity time before an idle worker terminates itself
    pub idle_worker_ttl: Duration,
    /// Inactivity time before an idle user session is evicted
    pub idle_ttl: Duration,
    /// Supervisor interval for session eviction
    pub cleanup_interval: Duration,
}

impl Default for KeyedBufferConfig {
    fn default() -> Self {
        Self {
            buffer_size: 2,
            max_workers_per_key: 4,
            idle_worker_ttl: Duration::from_secs(10),
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
    pub fn with_max_workers_per_key(mut self, max: usize) -> Self {
        self.max_workers_per_key = max.max(1);
        self
    }

    #[allow(dead_code)]
    pub fn with_idle_worker_ttl(mut self, ttl: Duration) -> Self {
        self.idle_worker_ttl = ttl;
        self
    }

    #[allow(dead_code)]
    pub fn with_cleanup_interval(mut self, interval: Duration) -> Self {
        self.cleanup_interval = interval;
        self
    }
}

struct Session<In, Out> {
    out_tx: Sender<Out>,
    out_rx: Receiver<Out>,
    input_rx: watch::Receiver<In>,
    input_tx: watch::Sender<In>,
    last_accessed: Instant,
    /// Currently assigned worker capacity target for this key (starts at 1 for pre-generation)
    target_workers: usize,
    /// Active worker tasks currently generating for this key
    active_workers: Arc<AtomicUsize>,
}

pub struct KeyedBufferedQueue<Key, In, Out> {
    sessions: Arc<Mutex<HashMap<Key, Session<In, Out>>>>,
    work_notify: Arc<Notify>,
    spawn_worker: Arc<dyn Fn() + Send + Sync>,
    total_live_workers: Arc<AtomicUsize>,
    config: KeyedBufferConfig,
}

impl<Key, In, Out> Clone for KeyedBufferedQueue<Key, In, Out> {
    fn clone(&self) -> Self {
        Self {
            sessions: self.sessions.clone(),
            work_notify: self.work_notify.clone(),
            spawn_worker: self.spawn_worker.clone(),
            total_live_workers: self.total_live_workers.clone(),
            config: self.config.clone(),
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
        let (out_rx, needs_more_workers) = {
            let mut sessions = self.sessions.lock().unwrap();
            let now = Instant::now();

            let session = sessions.entry(key.clone()).or_insert_with(|| {
                let (out_tx, out_rx) = bounded::<Out>(self.config.buffer_size);
                let (input_tx, input_rx) = watch::channel(input.clone());
                Session {
                    out_tx,
                    out_rx,
                    input_rx,
                    input_tx,
                    last_accessed: now,
                    // New user starts with target 1 (pre-generation only, not max)
                    target_workers: 1,
                    active_workers: Arc::new(AtomicUsize::new(0)),
                }
            });

            session.last_accessed = now;
            let _ = session.input_tx.send(input);

            // True wait detection: If buffer is empty, consumer cannot get an instant return
            let had_to_wait = session.out_rx.is_empty();

            if had_to_wait {
                // Gradual scale up: increase key target by 1 up to max_workers_per_key
                if session.target_workers < self.config.max_workers_per_key {
                    session.target_workers += 1;
                }
            } else {
                // When consumer is relaxed and buffer is well-stocked, gradually step down
                if session.out_rx.len() >= self.config.buffer_size && session.target_workers > 1 {
                    session.target_workers -= 1;
                }
            }

            let active = session.active_workers.load(Ordering::Relaxed);
            let needs_worker = active < session.target_workers;

            (session.out_rx.clone(), needs_worker)
        };

        // Spawn a new worker on demand if available pool isn't keeping up
        if needs_more_workers {
            (self.spawn_worker)();
        } else {
            self.work_notify.notify_one();
        }

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
    let sessions = Arc::new(Mutex::new(HashMap::<Key, Session<In, Out>>::new()));
    let work_notify = Arc::new(Notify::new());
    let total_live_workers = Arc::new(AtomicUsize::new(0));

    let spawn_worker = {
        let sessions = sessions.clone();
        let dep_rx = dep_rx.clone();
        let state = state.clone();
        let handler = handler.clone();
        let work_notify = work_notify.clone();
        let total_live_workers = total_live_workers.clone();
        let config = config.clone();

        Arc::new(move || {
            total_live_workers.fetch_add(1, Ordering::Relaxed);

            let sessions = sessions.clone();
            let dep_rx = dep_rx.clone();
            let state = state.clone();
            let handler = handler.clone();
            let work_notify = work_notify.clone();
            let total_live_workers = total_live_workers.clone();
            let config = config.clone();

            tokio::spawn(async move {
                let mut round_robin_cursor: usize = 0;
                let mut last_work_time = Instant::now();

                loop {
                    // Pick a session that has work pending and hasn't exceeded its current worker target
                    let chosen = {
                        let sessions_guard = sessions.lock().unwrap();
                        if sessions_guard.is_empty() {
                            None
                        } else {
                            let keys: Vec<Key> = sessions_guard.keys().cloned().collect();
                            let total_keys = keys.len();
                            let mut selected = None;

                            for i in 0..total_keys {
                                let idx = (round_robin_cursor + i) % total_keys;
                                let key = &keys[idx];

                                if let Some(session) = sessions_guard.get(key) {
                                    let cur_active = session.active_workers.load(Ordering::Relaxed);
                                    let in_flight = session.out_tx.len() + cur_active;

                                    // Must be within current scaled target and have buffer room
                                    if cur_active < session.target_workers
                                        && in_flight < config.buffer_size
                                    {
                                        session.active_workers.fetch_add(1, Ordering::Relaxed);
                                        round_robin_cursor = (idx + 1) % total_keys;
                                        selected = Some((
                                            session.out_tx.clone(),
                                            session.input_rx.clone(),
                                            session.active_workers.clone(),
                                        ));
                                        break;
                                    }
                                }
                            }
                            selected
                        }
                    };

                    if let Some((out_tx, input_rx, active_workers)) = chosen {
                        last_work_time = Instant::now();

                        let dep_item = match dep_rx.recv_async().await {
                            Ok(item) => item,
                            Err(_) => {
                                active_workers.fetch_sub(1, Ordering::Relaxed);
                                break;
                            }
                        };

                        let current_input = input_rx.borrow().clone();
                        let output = handler(current_input, dep_item, state.clone()).await;

                        let _ = out_tx.send_async(output).await;
                        active_workers.fetch_sub(1, Ordering::Relaxed);

                        // Wake up other waiting workers to pick up the next task
                        work_notify.notify_one();
                    } else {
                        // Worker is idle: wait for work, or kill itself if idle TTL exceeded
                        let notified = work_notify.notified();
                        tokio::pin!(notified);

                        tokio::select! {
                            _ = &mut notified => {},
                            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                                if last_work_time.elapsed() > config.idle_worker_ttl {
                                    // Kill idle worker
                                    break;
                                }
                            }
                        }
                    }
                }

                total_live_workers.fetch_sub(1, Ordering::Relaxed);
            });
        })
    };

    // Supervisor: Session TTL cleanup
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
        work_notify,
        spawn_worker,
        total_live_workers,
        config,
    }
}
