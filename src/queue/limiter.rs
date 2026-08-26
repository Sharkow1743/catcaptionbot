use std::sync::{Arc, OnceLock};
use tokio::sync::{Semaphore, SemaphorePermit, oneshot};

static CPU_POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();

/// Global dedicated CPU thread pool so heavy rendering never blocks Tokio reactor threads.
pub fn cpu_pool() -> &'static rayon::ThreadPool {
    CPU_POOL.get_or_init(|| {
        let threads = num_cpus::get().max(2);
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("cpu-worker-{}", i))
            .build()
            .expect("Failed to initialize Rayon CPU thread pool")
    })
}

/// Offloads a synchronous CPU-bound task (e.g., image rendering/decoding) to the Rayon pool.
pub async fn run_cpu<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    cpu_pool().spawn(move || {
        let res = f();
        let _ = tx.send(res);
    });
    rx.await
        .expect("CPU worker thread panicked or dropped channel")
}

/// Dynamic compute semaphore that prioritizes foreground/interactive work
/// while throttling background speculative generation.
#[derive(Clone, Debug)]
pub struct ComputeLimiter {
    total_semaphore: Arc<Semaphore>,
    bg_semaphore: Arc<Semaphore>,
}

impl Default for ComputeLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl ComputeLimiter {
    /// Creates a limiter using available logical cores.
    /// Background tasks are capped at 50% of CPU permits, reserving the rest for immediate user requests.
    pub fn new() -> Self {
        let cpus = num_cpus::get().max(2);
        let bg_permits = (cpus / 2).max(1);
        Self::with_limits(cpus, bg_permits)
    }

    pub fn with_limits(total_cores: usize, max_background_cores: usize) -> Self {
        Self {
            total_semaphore: Arc::new(Semaphore::new(total_cores.max(1))),
            bg_semaphore: Arc::new(Semaphore::new(max_background_cores.max(1))),
        }
    }

    /// Foreground / High-Priority: Acquires an execution permit immediately from the total pool.
    pub async fn acquire_foreground(&self) -> SemaphorePermit<'_> {
        self.total_semaphore
            .acquire()
            .await
            .expect("Total compute semaphore closed")
    }

    /// Background / Speculative: Must acquire a background permit *and* a total permit.
    /// If foreground tasks saturate CPU, background tasks will pause.
    pub async fn acquire_background(&self) -> (SemaphorePermit<'_>, SemaphorePermit<'_>) {
        let bg = self
            .bg_semaphore
            .acquire()
            .await
            .expect("Background compute semaphore closed");
        let total = self
            .total_semaphore
            .acquire()
            .await
            .expect("Total compute semaphore closed");
        (bg, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_compute_limiter_permits() {
        let limiter = ComputeLimiter::with_limits(2, 1);

        // Foreground permit
        let permit1 = limiter.acquire_foreground().await;
        assert_eq!(limiter.total_semaphore.available_permits(), 1);

        // Background permit
        let permit2 = limiter.acquire_background().await;
        assert_eq!(limiter.total_semaphore.available_permits(), 0);
        assert_eq!(limiter.bg_semaphore.available_permits(), 0);

        drop(permit1);
        drop(permit2);

        assert_eq!(limiter.total_semaphore.available_permits(), 2);
        assert_eq!(limiter.bg_semaphore.available_permits(), 1);
    }
}
