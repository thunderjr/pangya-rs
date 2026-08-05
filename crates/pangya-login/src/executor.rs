//! Bounded executor for CPU-heavy credential operations.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use pangya_domain::CredentialHash;
use thiserror::Error;
use tokio::{
    sync::{Notify, Semaphore},
    time::timeout,
};

use crate::{CanonicalTransportSecret, CredentialError, CredentialPolicy};

/// Synchronous credential engine run only on bounded blocking workers.
pub trait CredentialEngine: Send + Sync + 'static {
    /// Hashes one canonical transport secret.
    fn hash(&self, secret: &CanonicalTransportSecret) -> Result<CredentialHash, CredentialError>;

    /// Verifies one canonical transport secret.
    fn verify(
        &self,
        secret: &CanonicalTransportSecret,
        stored: &CredentialHash,
    ) -> Result<(), CredentialError>;
}

impl CredentialEngine for CredentialPolicy {
    fn hash(&self, secret: &CanonicalTransportSecret) -> Result<CredentialHash, CredentialError> {
        CredentialPolicy::hash(self, secret)
    }

    fn verify(
        &self,
        secret: &CanonicalTransportSecret,
        stored: &CredentialHash,
    ) -> Result<(), CredentialError> {
        CredentialPolicy::verify(self, secret, stored)
    }
}

/// Public credential-operation outcome with no secret material.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CredentialExecutorError {
    /// No bounded worker slot became available within the queue wait.
    #[error("credential workers are saturated")]
    Overloaded,
    /// A started operation exceeded its bounded runtime.
    #[error("credential operation timed out")]
    Timeout,
    /// The blocking task could not be joined.
    #[error("credential worker failed")]
    Worker,
    /// The credential policy returned a specific redacted failure.
    #[error(transparent)]
    Credential(#[from] CredentialError),
}

#[derive(Default)]
struct WorkerTracker {
    active: AtomicUsize,
    idle: Notify,
}

struct WorkerGuard(Arc<WorkerTracker>);

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        if self.0.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.idle.notify_waiters();
        }
    }
}

/// Cloneable bounded admission wrapper around Tokio blocking workers.
#[derive(Clone)]
pub struct BoundedCredentialExecutor {
    engine: Arc<dyn CredentialEngine>,
    permits: Arc<Semaphore>,
    queue_timeout: Duration,
    operation_timeout: Duration,
    workers: Arc<WorkerTracker>,
}

impl std::fmt::Debug for BoundedCredentialExecutor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BoundedCredentialExecutor")
            .field("available_permits", &self.permits.available_permits())
            .field("queue_timeout", &self.queue_timeout)
            .field("operation_timeout", &self.operation_timeout)
            .field(
                "active_workers",
                &self.workers.active.load(Ordering::Acquire),
            )
            .finish_non_exhaustive()
    }
}

impl BoundedCredentialExecutor {
    /// Creates an executor that acquires a permit before creating a blocking task.
    ///
    /// # Errors
    /// Returns [`CredentialExecutorError::Overloaded`] when concurrency is zero.
    pub fn new(
        engine: Arc<dyn CredentialEngine>,
        concurrency: usize,
        queue_timeout: Duration,
        operation_timeout: Duration,
    ) -> Result<Self, CredentialExecutorError> {
        if concurrency == 0
            || concurrency > 64
            || queue_timeout.is_zero()
            || queue_timeout > Duration::from_secs(10)
            || operation_timeout.is_zero()
            || operation_timeout > Duration::from_secs(60)
        {
            return Err(CredentialExecutorError::Overloaded);
        }
        Ok(Self {
            engine,
            permits: Arc::new(Semaphore::new(concurrency)),
            queue_timeout,
            operation_timeout,
            workers: Arc::new(WorkerTracker::default()),
        })
    }

    /// Returns the number of admitted blocking workers still running.
    #[must_use]
    pub fn active_workers(&self) -> usize {
        self.workers.active.load(Ordering::Acquire)
    }

    /// Waits until all admitted blocking workers finish within the supplied bound.
    pub async fn wait_idle(&self, maximum: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + maximum;
        loop {
            if self.active_workers() == 0 {
                return true;
            }
            let notified = self.workers.idle.notified();
            if self.active_workers() == 0 {
                return true;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() || timeout(remaining, notified).await.is_err() {
                return self.active_workers() == 0;
            }
        }
    }

    /// Hashes a secret after bounded admission and on Tokio's blocking pool.
    ///
    /// # Errors
    /// Returns a redacted overload, timeout, worker, or credential failure.
    pub async fn hash(
        &self,
        secret: CanonicalTransportSecret,
    ) -> Result<CredentialHash, CredentialExecutorError> {
        let engine = Arc::clone(&self.engine);
        self.execute(move || engine.hash(&secret)).await
    }

    /// Verifies a secret after bounded admission and on Tokio's blocking pool.
    ///
    /// # Errors
    /// Returns a redacted overload, timeout, worker, or credential failure.
    pub async fn verify(
        &self,
        secret: CanonicalTransportSecret,
        stored: CredentialHash,
    ) -> Result<(), CredentialExecutorError> {
        let engine = Arc::clone(&self.engine);
        self.execute(move || engine.verify(&secret, &stored)).await
    }

    async fn execute<T, F>(&self, work: F) -> Result<T, CredentialExecutorError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, CredentialError> + Send + 'static,
    {
        let permit = timeout(
            self.queue_timeout,
            Arc::clone(&self.permits).acquire_owned(),
        )
        .await
        .map_err(|_| CredentialExecutorError::Overloaded)?
        .map_err(|_| CredentialExecutorError::Worker)?;
        self.workers.active.fetch_add(1, Ordering::AcqRel);
        let tracker = Arc::clone(&self.workers);
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _worker = WorkerGuard(tracker);
            work()
        });
        timeout(self.operation_timeout, task)
            .await
            .map_err(|_| CredentialExecutorError::Timeout)?
            .map_err(|_| CredentialExecutorError::Worker)?
            .map_err(CredentialExecutorError::Credential)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct InstrumentedEngine {
        active: AtomicUsize,
        maximum: AtomicUsize,
    }

    impl CredentialEngine for InstrumentedEngine {
        fn hash(
            &self,
            _secret: &CanonicalTransportSecret,
        ) -> Result<CredentialHash, CredentialError> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum.fetch_max(active, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(25));
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(CredentialHash::new("synthetic".to_owned()))
        }

        fn verify(
            &self,
            secret: &CanonicalTransportSecret,
            _stored: &CredentialHash,
        ) -> Result<(), CredentialError> {
            self.hash(secret).map(drop)
        }
    }

    async fn wait_until_active(engine: &InstrumentedEngine) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while engine.active.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("worker entered");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn blocking_tasks_never_exceed_admitted_concurrency() {
        let engine = Arc::new(InstrumentedEngine {
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
        });
        let executor = BoundedCredentialExecutor::new(
            engine.clone(),
            2,
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .expect("executor");
        let secret =
            CanonicalTransportSecret::parse("0123456789abcdef0123456789abcdef").expect("secret");
        let tasks = (0..8)
            .map(|_| {
                let executor = executor.clone();
                let secret = secret.clone();
                tokio::spawn(async move { executor.hash(secret).await })
            })
            .collect::<Vec<_>>();
        for task in tasks {
            task.await.expect("join").expect("hash");
        }
        assert_eq!(engine.maximum.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn overload_occurs_before_any_extra_blocking_task_is_created() {
        let engine = Arc::new(InstrumentedEngine {
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
        });
        let executor = BoundedCredentialExecutor::new(
            engine.clone(),
            1,
            Duration::from_millis(2),
            Duration::from_secs(1),
        )
        .expect("executor");
        let secret =
            CanonicalTransportSecret::parse("0123456789abcdef0123456789abcdef").expect("secret");
        let first = tokio::spawn({
            let executor = executor.clone();
            let secret = secret.clone();
            async move { executor.hash(secret).await }
        });
        wait_until_active(&engine).await;
        assert_eq!(
            executor.hash(secret).await,
            Err(CredentialExecutorError::Overloaded)
        );
        first.await.expect("join").expect("first");
        assert_eq!(engine.maximum.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn timeout_and_cancellation_retain_permit_until_worker_finishes() {
        let engine = Arc::new(InstrumentedEngine {
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
        });
        let executor = BoundedCredentialExecutor::new(
            engine.clone(),
            1,
            Duration::from_millis(2),
            Duration::from_millis(3),
        )
        .expect("executor");
        let secret =
            CanonicalTransportSecret::parse("0123456789abcdef0123456789abcdef").expect("secret");
        assert_eq!(
            executor.hash(secret.clone()).await,
            Err(CredentialExecutorError::Timeout)
        );
        assert_eq!(
            executor.hash(secret.clone()).await,
            Err(CredentialExecutorError::Overloaded)
        );
        tokio::time::sleep(Duration::from_millis(30)).await;

        let cancelled = tokio::spawn({
            let executor = executor.clone();
            let secret = secret.clone();
            async move { executor.hash(secret).await }
        });
        wait_until_active(&engine).await;
        cancelled.abort();
        let _ = cancelled.await;
        assert_eq!(
            executor.hash(secret.clone()).await,
            Err(CredentialExecutorError::Overloaded)
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
        // A long operation timeout executor over the same engine proves recovery.
        let recovered = BoundedCredentialExecutor::new(
            engine,
            1,
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .expect("recovered executor");
        recovered.hash(secret).await.expect("recovered");
    }
}
