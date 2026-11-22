//! Work that runs outside the core's loop — git, gh, a CLI's one-off
//! command — a few at a time. A burst (startup, a tick reading every
//! folder's pull request and every link) queues instead of starting dozens
//! of processes at once, each holding its pipes open: an app opened from the
//! Dock may hold only 256 files.

use tokio::sync::Semaphore;

/// How many run at once.
const AT_ONCE: usize = 6;

static SLOTS: Semaphore = Semaphore::const_new(AT_ONCE);

/// Runs blocking `job` on a worker thread once a slot is free.
pub(crate) fn spawn(job: impl FnOnce() + Send + 'static) {
    tokio::spawn(async move {
        let Ok(_slot) = SLOTS.acquire().await else {
            return;
        };
        if let Err(error) = tokio::task::spawn_blocking(job).await {
            tracing::warn!(%error, "a background job failed");
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::{AT_ONCE, spawn};

    /// However many are asked for at once, no more than a few run.
    #[tokio::test(flavor = "multi_thread")]
    async fn only_a_few_jobs_run_at_once() {
        let running = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        let done = Arc::new(AtomicUsize::new(0));
        for _ in 0..40 {
            let (running, most, done) = (running.clone(), most.clone(), done.clone());
            spawn(move || {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                running.fetch_sub(1, Ordering::SeqCst);
                done.fetch_add(1, Ordering::SeqCst);
            });
        }
        for _ in 0..400 {
            if done.load(Ordering::SeqCst) == 40 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(done.load(Ordering::SeqCst), 40);
        assert!(most.load(Ordering::SeqCst) <= AT_ONCE);
    }
}
