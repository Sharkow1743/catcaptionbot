use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use flume::{Receiver, Sender, bounded};
use tokio::sync::oneshot;

// ============================================================================
// 1. CONVERSION TRAIT (Option, Result, and Plain T support)
// ============================================================================

/// Helper trait enabling worker functions to return `T`, `Option<T>`, or `Result<T, E>`.
/// - `T` -> Pushed to queue
/// - `Some(T)` -> Pushed to queue
/// - `None` -> Skipped (nothing pushed)
/// - `Ok(T)` -> Pushed to queue
/// - `Err(E)` -> Skipped (nothing pushed)
pub trait IntoOptional<T> {
    fn into_option(self) -> Option<T>;
}

impl<T> IntoOptional<T> for T {
    #[inline]
    fn into_option(self) -> Option<T> {
        Some(self)
    }
}

impl<T> IntoOptional<T> for Option<T> {
    #[inline]
    fn into_option(self) -> Option<T> {
        self
    }
}

impl<T, E> IntoOptional<T> for Result<T, E> {
    #[inline]
    fn into_option(self) -> Option<T> {
        self.ok()
    }
}

// ============================================================================
// 2. CONFIGURATION & RAII WORKER GUARD
// ============================================================================

/// Configuration for dynamic worker pools and auto-scaling.
#[derive(Debug, Clone)]
pub struct WorkerPoolConfig {
    pub min_workers: usize,
    pub max_workers: usize,
    pub queue_size: usize,
    pub check_interval: Duration,
    /// Threshold to scale up (fraction of capacity, e.g. 0.75).
    pub scale_up_threshold: f32,
    /// Threshold to scale down (fraction of capacity, e.g. 0.25).
    pub scale_down_threshold: f32,
}

impl Default for WorkerPoolConfig {
    fn default() -> Self {
        Self {
            min_workers: 1,
            max_workers: 8,
            queue_size: 64,
            check_interval: Duration::from_millis(250),
            scale_up_threshold: 0.75,
            scale_down_threshold: 0.25,
        }
    }
}

impl WorkerPoolConfig {
    pub fn new(min_workers: usize, max_workers: usize, queue_size: usize) -> Self {
        Self {
            min_workers,
            max_workers,
            queue_size,
            ..Default::default()
        }
    }

    pub fn with_check_interval(mut self, interval: Duration) -> Self {
        self.check_interval = interval;
        self
    }

    pub fn with_thresholds(mut self, scale_up: f32, scale_down: f32) -> Self {
        self.scale_up_threshold = scale_up;
        self.scale_down_threshold = scale_down;
        self
    }
}

/// RAII Guard ensuring active worker counts are decremented even on panic or task cancellation.
struct WorkerGuard(Arc<AtomicUsize>);

impl WorkerGuard {
    fn new(active: &Arc<AtomicUsize>) -> Self {
        active.fetch_add(1, Ordering::Relaxed);
        Self(active.clone())
    }
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

// ============================================================================
// 3. REQUEST-RESPONSE (RPC) TYPES & CLIENT HANDLE
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("the request queue is closed or disconnected")]
    QueueClosed,
    #[error("the worker dropped before returning a response")]
    WorkerDropped,
    #[error("the request timed out")]
    Timeout,
    #[error("upstream dependency queue closed")]
    UpstreamClosed,
}

/// Internal wrapper pairing an input with its single-use response channel.
struct RequestEnvelope<In, Out> {
    input: In,
    reply_tx: oneshot::Sender<Out>,
}

/// A cloneable client handle used to send inputs and await the corresponding output.
pub struct RequestQueue<In, Out> {
    tx: Sender<RequestEnvelope<In, Out>>,
}

impl<In, Out> Clone for RequestQueue<In, Out> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<In: Send + 'static, Out: Send + 'static> RequestQueue<In, Out> {
    /// Sends an input and awaits the processed result.
    pub async fn request(&self, input: In) -> Result<Out, RequestError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let envelope = RequestEnvelope { input, reply_tx };

        self.tx
            .send_async(envelope)
            .await
            .map_err(|_| RequestError::QueueClosed)?;

        reply_rx.await.map_err(|_| RequestError::WorkerDropped)
    }

    /// Sends an input with a maximum timeout for receiving the result.
    pub async fn request_timeout(&self, input: In, timeout: Duration) -> Result<Out, RequestError> {
        tokio::time::timeout(timeout, self.request(input))
            .await
            .map_err(|_| RequestError::Timeout)?
    }
}

// ============================================================================
// 4. PRODUCER QUEUE (Source: Generates items autonomously into a Receiver)
// ============================================================================

/// Spawns an auto-scaling generator pool that autonomously produces items into a queue.
/// Supports functions returning `T`, `Option<T>`, or `Result<T, E>`.
/// Scales up when the output channel is starving (mostly empty), scales down when full.
pub fn spawn_producer<S, Out, R, F, Fut>(
    state: Arc<S>,
    config: WorkerPoolConfig,
    produce_fn: F,
) -> Receiver<Out>
where
    S: Send + Sync + 'static,
    Out: Send + 'static,
    R: IntoOptional<Out> + Send + 'static,
    F: Fn(Arc<S>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
{
    let (tx, rx) = bounded::<Out>(config.queue_size);
    let target = Arc::new(AtomicUsize::new(config.min_workers));
    let active = Arc::new(AtomicUsize::new(0));

    let spawn_worker = {
        let produce_fn = produce_fn.clone();
        let target = target.clone();
        let active = active.clone();
        let state = state.clone();

        move |tx: Sender<Out>| {
            let _guard = WorkerGuard::new(&active);
            let produce_fn = produce_fn.clone();
            let target = target.clone();
            let active = active.clone();
            let state = state.clone();

            tokio::spawn(async move {
                let _guard = _guard;
                loop {
                    if let Some(item) = produce_fn(state.clone()).await.into_option() {
                        if tx.send_async(item).await.is_err() {
                            break; // All receivers dropped
                        }
                    }

                    if active.load(Ordering::Relaxed) > target.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });
        }
    };

    for _ in 0..config.min_workers {
        spawn_worker(tx.clone());
    }

    // Auto-scaler supervisor
    {
        let tx = tx.clone();
        let target = target.clone();
        let config = config.clone();

        tokio::spawn(async move {
            let capacity = tx.capacity().unwrap_or(config.queue_size) as f32;

            loop {
                tokio::time::sleep(config.check_interval).await;
                if tx.is_disconnected() {
                    break;
                }

                let free = (capacity as usize - tx.len()) as f32;
                let cur_target = target.load(Ordering::Relaxed);

                if free > capacity * config.scale_up_threshold && cur_target < config.max_workers {
                    target.store(cur_target + 1, Ordering::Relaxed);
                    spawn_worker(tx.clone());
                } else if free < capacity * config.scale_down_threshold
                    && cur_target > config.min_workers
                {
                    target.store(cur_target - 1, Ordering::Relaxed);
                }
            }
        });
    }

    rx
}

// ============================================================================
// 5. CONSUMER POOL (Sink: Consumes from user input or upstream queue)
// ============================================================================

/// Spawns an auto-scaling consumer worker pool that processes items from an input queue.
/// Scales up when the input queue backlog grows, scales down when idle.
pub fn spawn_consumer<S, In, F, Fut>(
    state: Arc<S>,
    rx: Receiver<In>,
    config: WorkerPoolConfig,
    consume_fn: F,
) where
    S: Send + Sync + 'static,
    In: Send + 'static,
    F: Fn(In, Arc<S>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let target = Arc::new(AtomicUsize::new(config.min_workers));
    let active = Arc::new(AtomicUsize::new(0));

    let spawn_worker = {
        let consume_fn = consume_fn.clone();
        let target = target.clone();
        let active = active.clone();
        let state = state.clone();
        move |rx: Receiver<In>| {
            let _guard = WorkerGuard::new(&active);
            let consume_fn = consume_fn.clone();
            let target = target.clone();
            let active = active.clone();
            let state = state.clone();

            tokio::spawn(async move {
                let _guard = _guard;
                while let Ok(item) = rx.recv_async().await {
                    consume_fn(item, state.clone()).await;

                    if active.load(Ordering::Relaxed) > target.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });
        }
    };

    for _ in 0..config.min_workers {
        spawn_worker(rx.clone());
    }

    // Auto-scaler supervisor
    {
        let rx = rx.clone();
        let target = target.clone();
        let config = config.clone();

        tokio::spawn(async move {
            let capacity = rx.capacity().unwrap_or(config.queue_size) as f32;

            loop {
                tokio::time::sleep(config.check_interval).await;
                if rx.is_disconnected() {
                    break;
                }

                let backlog = rx.len() as f32;
                let cur_target = target.load(Ordering::Relaxed);

                if backlog > capacity * config.scale_up_threshold && cur_target < config.max_workers
                {
                    target.store(cur_target + 1, Ordering::Relaxed);
                    spawn_worker(rx.clone());
                } else if backlog < capacity * config.scale_down_threshold
                    && cur_target > config.min_workers
                {
                    target.store(cur_target - 1, Ordering::Relaxed);
                }
            }
        });
    }
}

// ============================================================================
// 6. TRANSFORMER QUEUE (Pipeline: In -> Out)
// ============================================================================

/// Spawns an auto-scaling transformer pool reading from `rx_in` and writing to returned `Receiver<Out>`.
/// Supports functions returning `Out`, `Option<Out>`, or `Result<Out, E>`.
pub fn spawn_transformer<S, In, Out, R, F, Fut>(
    state: Arc<S>,
    rx_in: Receiver<In>,
    config: WorkerPoolConfig,
    transform_fn: F,
) -> Receiver<Out>
where
    S: Send + Sync + 'static,
    In: Send + 'static,
    Out: Send + 'static,
    R: IntoOptional<Out> + Send + 'static,
    F: Fn(In, Arc<S>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
{
    let (tx_out, rx_out) = bounded::<Out>(config.queue_size);
    let target = Arc::new(AtomicUsize::new(config.min_workers));
    let active = Arc::new(AtomicUsize::new(0));

    let spawn_worker = {
        let transform_fn = transform_fn.clone();
        let target = target.clone();
        let active = active.clone();
        let state = state.clone();

        move |rx_in: Receiver<In>, tx_out: Sender<Out>| {
            let _guard = WorkerGuard::new(&active);
            let transform_fn = transform_fn.clone();
            let target = target.clone();
            let active = active.clone();
            let state = state.clone();

            tokio::spawn(async move {
                let _guard = _guard;
                while let Ok(in_item) = rx_in.recv_async().await {
                    if let Some(out_item) = transform_fn(in_item, state.clone()).await.into_option()
                    {
                        if tx_out.send_async(out_item).await.is_err() {
                            break;
                        }
                    }

                    if active.load(Ordering::Relaxed) > target.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });
        }
    };

    for _ in 0..config.min_workers {
        spawn_worker(rx_in.clone(), tx_out.clone());
    }

    // Auto-scaler supervisor
    {
        let rx_in = rx_in.clone();
        let tx_out = tx_out.clone();
        let target = target.clone();
        let config = config.clone();

        tokio::spawn(async move {
            let capacity = rx_in.capacity().unwrap_or(config.queue_size) as f32;

            loop {
                tokio::time::sleep(config.check_interval).await;
                if rx_in.is_disconnected() || tx_out.is_disconnected() {
                    break;
                }

                let backlog = rx_in.len() as f32;
                let cur_target = target.load(Ordering::Relaxed);

                if backlog > capacity * config.scale_up_threshold && cur_target < config.max_workers
                {
                    target.store(cur_target + 1, Ordering::Relaxed);
                    spawn_worker(rx_in.clone(), tx_out.clone());
                } else if backlog < capacity * config.scale_down_threshold
                    && cur_target > config.min_workers
                {
                    target.store(cur_target - 1, Ordering::Relaxed);
                }
            }
        });
    }

    rx_out
}

// ============================================================================
// 7. STANDALONE REQUEST QUEUE (User Input -> Output with await)
// ============================================================================

/// Spawns an auto-scaling request queue where callers invoke `.request(input).await`.
pub fn spawn_request_queue<S, In, Out, F, Fut>(
    state: Arc<S>,
    config: WorkerPoolConfig,
    handler: F,
) -> RequestQueue<In, Out>
where
    S: Send + Sync + 'static,
    In: Send + 'static,
    Out: Send + 'static,
    F: Fn(In, Arc<S>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Out> + Send + 'static,
{
    let (req_tx, req_rx) = bounded::<RequestEnvelope<In, Out>>(config.queue_size);
    let target = Arc::new(AtomicUsize::new(config.min_workers));
    let active = Arc::new(AtomicUsize::new(0));

    let spawn_worker = {
        let handler = handler.clone();
        let target = target.clone();
        let active = active.clone();
        let state = state.clone();

        move |req_rx: Receiver<RequestEnvelope<In, Out>>| {
            let _guard = WorkerGuard::new(&active);
            let handler = handler.clone();
            let target = target.clone();
            let active = active.clone();
            let state = state.clone();

            tokio::spawn(async move {
                let _guard = _guard;
                while let Ok(envelope) = req_rx.recv_async().await {
                    let output = handler(envelope.input, state.clone()).await;
                    let _ = envelope.reply_tx.send(output);

                    if active.load(Ordering::Relaxed) > target.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });
        }
    };

    for _ in 0..config.min_workers {
        spawn_worker(req_rx.clone());
    }

    // Auto-scaler supervisor
    {
        let req_rx = req_rx.clone();
        let target = target.clone();
        let config = config.clone();

        tokio::spawn(async move {
            let capacity = req_rx.capacity().unwrap_or(config.queue_size) as f32;

            loop {
                tokio::time::sleep(config.check_interval).await;
                if req_rx.is_disconnected() {
                    break;
                }

                let backlog = req_rx.len() as f32;
                let cur_target = target.load(Ordering::Relaxed);

                if backlog > capacity * config.scale_up_threshold && cur_target < config.max_workers
                {
                    target.store(cur_target + 1, Ordering::Relaxed);
                    spawn_worker(req_rx.clone());
                } else if backlog < capacity * config.scale_down_threshold
                    && cur_target > config.min_workers
                {
                    target.store(cur_target - 1, Ordering::Relaxed);
                }
            }
        });
    }

    RequestQueue { tx: req_tx }
}

// ============================================================================
// 8. DEPENDENT REQUEST QUEUE (User Input + Upstream Queue -> Output with await)
// ============================================================================

/// Spawns an auto-scaling request queue that combines user input with an item
/// from an upstream queue `dep_rx` and returns the output directly to the caller.
pub fn spawn_dependent_request_queue<S, In, Dep, Out, F, Fut>(
    state: Arc<S>,
    dep_rx: Receiver<Dep>,
    config: WorkerPoolConfig,
    handler: F,
) -> RequestQueue<In, Out>
where
    S: Send + Sync + 'static,
    In: Send + 'static,
    Dep: Send + 'static,
    Out: Send + 'static,
    F: Fn(In, Dep, Arc<S>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Out> + Send + 'static,
{
    let (req_tx, req_rx) = bounded::<RequestEnvelope<In, Out>>(config.queue_size);
    let target = Arc::new(AtomicUsize::new(config.min_workers));
    let active = Arc::new(AtomicUsize::new(0));

    let spawn_worker = {
        let handler = handler.clone();
        let target = target.clone();
        let active = active.clone();
        let state = state.clone();

        move |req_rx: Receiver<RequestEnvelope<In, Out>>, dep_rx: Receiver<Dep>| {
            let _guard = WorkerGuard::new(&active);
            let handler = handler.clone();
            let target = target.clone();
            let active = active.clone();
            let state = state.clone();

            tokio::spawn(async move {
                let _guard = _guard;
                while let Ok(envelope) = req_rx.recv_async().await {
                    let dep_item = match dep_rx.recv_async().await {
                        Ok(item) => item,
                        Err(_) => break, // Upstream dependency closed
                    };

                    let output = handler(envelope.input, dep_item, state.clone()).await;
                    let _ = envelope.reply_tx.send(output);

                    if active.load(Ordering::Relaxed) > target.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });
        }
    };

    for _ in 0..config.min_workers {
        spawn_worker(req_rx.clone(), dep_rx.clone());
    }

    // Auto-scaler supervisor
    {
        let req_rx = req_rx.clone();
        let dep_rx = dep_rx.clone();
        let target = target.clone();
        let config = config.clone();

        tokio::spawn(async move {
            let capacity = req_rx.capacity().unwrap_or(config.queue_size) as f32;

            loop {
                tokio::time::sleep(config.check_interval).await;
                if req_rx.is_disconnected() || dep_rx.is_disconnected() {
                    break;
                }

                let backlog = req_rx.len() as f32;
                let cur_target = target.load(Ordering::Relaxed);

                if backlog > capacity * config.scale_up_threshold && cur_target < config.max_workers
                {
                    target.store(cur_target + 1, Ordering::Relaxed);
                    spawn_worker(req_rx.clone(), dep_rx.clone());
                } else if backlog < capacity * config.scale_down_threshold
                    && cur_target > config.min_workers
                {
                    target.store(cur_target - 1, Ordering::Relaxed);
                }
            }
        });
    }

    RequestQueue { tx: req_tx }
}
