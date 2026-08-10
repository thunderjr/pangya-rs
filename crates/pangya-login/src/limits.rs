//! Fixed-capacity admission, rate-limit, and duplicate-login registries.

use std::{
    collections::HashMap,
    hash::Hash,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use thiserror::Error;
use tokio::{
    sync::{Notify, mpsc, oneshot},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

/// Fixed-window limiter outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateDecision {
    /// Request is within its configured window.
    Allowed,
    /// Request exceeded the configured count.
    Limited,
    /// Bounded key storage had no safe admission slot.
    Capacity,
}

#[derive(Debug, Clone, Copy)]
struct Window {
    started: Instant,
    weight: u64,
}

/// A fixed-capacity, fixed-window limiter with weighted-budget support.
#[derive(Debug)]
pub struct FixedWindowLimiter<K> {
    entries: Mutex<HashMap<K, Window>>,
    capacity: usize,
    limit: u64,
    interval: Duration,
}

impl<K> FixedWindowLimiter<K>
where
    K: Clone + Eq + Hash,
{
    /// Creates a count limiter. Zero configuration rejects every admission.
    #[must_use]
    pub fn new(capacity: usize, limit: u32, interval: Duration) -> Self {
        Self::new_weighted(capacity, u64::from(limit), interval)
    }

    /// Creates a weighted limiter for byte budgets.
    #[must_use]
    pub fn new_weighted(capacity: usize, limit: u64, interval: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::with_capacity(capacity)),
            capacity,
            limit,
            interval,
        }
    }

    /// Checks and records one count event without holding a lock across an await.
    #[must_use]
    pub fn check(&self, key: K, now: Instant) -> RateDecision {
        self.check_weighted(key, now, 1)
    }

    /// Checks and records one weighted event, such as plaintext packet bytes.
    #[must_use]
    pub fn check_weighted(&self, key: K, now: Instant, weight: u64) -> RateDecision {
        if self.capacity == 0 || self.limit == 0 || self.interval.is_zero() || weight == 0 {
            return RateDecision::Capacity;
        }
        let Ok(mut entries) = self.entries.lock() else {
            return RateDecision::Capacity;
        };
        entries.retain(|_, window| now.saturating_duration_since(window.started) < self.interval);
        if let Some(window) = entries.get_mut(&key) {
            if now.saturating_duration_since(window.started) >= self.interval {
                *window = Window {
                    started: now,
                    weight,
                };
                return if weight <= self.limit {
                    RateDecision::Allowed
                } else {
                    RateDecision::Limited
                };
            }
            let Some(next) = window.weight.checked_add(weight) else {
                return RateDecision::Limited;
            };
            if next > self.limit {
                return RateDecision::Limited;
            }
            window.weight = next;
            return RateDecision::Allowed;
        }
        if entries.len() >= self.capacity {
            return RateDecision::Capacity;
        }
        if weight > self.limit {
            return RateDecision::Limited;
        }
        entries.insert(
            key,
            Window {
                started: now,
                weight,
            },
        );
        RateDecision::Allowed
    }
}

/// A live LoginService session's revocation and liveness control.
#[derive(Clone, Debug)]
pub struct SessionControl {
    revoked: Arc<AtomicBool>,
    task_alive: Arc<AtomicBool>,
    task_started: Arc<AtomicBool>,
    task_ended: Arc<Notify>,
    cancellation: CancellationToken,
    probes: mpsc::UnboundedSender<oneshot::Sender<()>>,
}

/// RAII liveness lease held by an owning LoginService task.
#[derive(Debug)]
pub struct SessionTaskLease {
    task_alive: Arc<AtomicBool>,
    task_ended: Arc<Notify>,
}

impl Drop for SessionTaskLease {
    fn drop(&mut self) {
        self.task_alive.store(false, Ordering::Release);
        self.task_ended.notify_waiters();
    }
}

/// Receiver held by the connection task so a duplicate can prove it is live.
pub struct SessionProbeReceiver {
    probes: mpsc::UnboundedReceiver<oneshot::Sender<()>>,
}

impl SessionControl {
    /// Creates a revocable session control and its task-side liveness receiver.
    #[must_use]
    pub fn new() -> (Self, SessionProbeReceiver) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Self {
                revoked: Arc::new(AtomicBool::new(false)),
                // A control without an attached task is conservatively live. Runtime owners
                // attach a SessionTaskLease immediately; only that RAII lease can authoritatively
                // transition the entry to stale.
                task_alive: Arc::new(AtomicBool::new(true)),
                task_started: Arc::new(AtomicBool::new(false)),
                task_ended: Arc::new(Notify::new()),
                cancellation: CancellationToken::new(),
                probes: sender,
            },
            SessionProbeReceiver { probes: receiver },
        )
    }

    /// Revokes the old connection and wakes every task waiting on it.
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
        self.cancellation.cancel();
    }

    /// Starts the owning connection task's liveness lease.
    #[must_use]
    pub fn start_task(&self) -> SessionTaskLease {
        self.task_started.store(true, Ordering::Release);
        self.task_alive.store(true, Ordering::Release);
        SessionTaskLease {
            task_alive: Arc::clone(&self.task_alive),
            task_ended: Arc::clone(&self.task_ended),
        }
    }

    /// Returns whether the lease has been revoked and must not proceed.
    #[must_use]
    pub fn is_revoked(&self) -> bool {
        self.revoked.load(Ordering::Acquire)
    }

    /// Returns whether the owning task is authoritatively live.
    #[must_use]
    pub fn is_live(&self) -> bool {
        !self.is_revoked() && self.task_alive.load(Ordering::Acquire)
    }

    /// Waits until an attached owning task has dropped its liveness lease.
    pub async fn wait_terminated(&self) {
        if !self.task_started.load(Ordering::Acquire) {
            return;
        }
        while self.task_alive.load(Ordering::Acquire) {
            let notified = self.task_ended.notified();
            if !self.task_alive.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    /// Returns a cancellation future for the owning connection task.
    pub async fn cancelled(&self) {
        self.cancellation.cancelled().await;
    }

    /// Proves that the owning task is still servicing its connection loop.
    pub async fn probe(&self, maximum: Duration) -> bool {
        if self.is_revoked() {
            return false;
        }
        let (reply, response) = oneshot::channel();
        if self.probes.send(reply).is_err() {
            return false;
        }
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => false,
            result = timeout(maximum, response) => matches!(result, Ok(Ok(()))),
        }
    }
}

impl SessionProbeReceiver {
    pub(crate) async fn recv(&mut self) -> Option<oneshot::Sender<()>> {
        self.probes.recv().await
    }
}

/// Bounded keyed concurrent-count registry.
#[derive(Debug)]
pub struct KeyedCapacityRegistry<K> {
    entries: Arc<Mutex<HashMap<K, usize>>>,
    key_capacity: usize,
    per_key: usize,
}

impl<K> KeyedCapacityRegistry<K>
where
    K: Clone + Eq + Hash,
{
    /// Creates hard limits for tracked keys and concurrent values per key.
    #[must_use]
    pub fn new(key_capacity: usize, per_key: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::with_capacity(key_capacity))),
            key_capacity,
            per_key,
        }
    }

    /// Acquires one concurrent slot until the guard is dropped.
    ///
    /// # Errors
    /// Returns [`RegistryError::Capacity`] at either hard limit.
    pub fn acquire(&self, key: K) -> Result<KeyedCapacityGuard<K>, RegistryError> {
        let mut entries = self.entries.lock().map_err(|_| RegistryError::Capacity)?;
        if let Some(count) = entries.get_mut(&key) {
            if *count >= self.per_key {
                return Err(RegistryError::Capacity);
            }
            *count += 1;
        } else {
            if entries.len() >= self.key_capacity || self.per_key == 0 {
                return Err(RegistryError::Capacity);
            }
            entries.insert(key.clone(), 1);
        }
        Ok(KeyedCapacityGuard {
            entries: Arc::downgrade(&self.entries),
            key: Some(key),
        })
    }
}

/// RAII keyed concurrent-count registration.
#[derive(Debug)]
pub struct KeyedCapacityGuard<K>
where
    K: Eq + Hash,
{
    entries: Weak<Mutex<HashMap<K, usize>>>,
    key: Option<K>,
}

impl<K> Drop for KeyedCapacityGuard<K>
where
    K: Eq + Hash,
{
    fn drop(&mut self) {
        let Some(entries) = self.entries.upgrade() else {
            return;
        };
        let Some(key) = self.key.take() else {
            return;
        };
        if let Ok(mut entries) = entries.lock()
            && let Some(count) = entries.get_mut(&key)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                entries.remove(&key);
            }
        }
    }
}

/// Registry admission failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RegistryError {
    /// The key belongs to a demonstrably live authenticated LoginService session.
    #[error("duplicate live login (lease {0})")]
    Duplicate(u64),
    /// The key belongs to a revoked/stale session awaiting controlled ghost recovery.
    #[error("stale login (lease {0})")]
    Stale(u64),
    /// The bounded registry is full or unavailable.
    #[error("active registry is full")]
    Capacity,
}

/// Bounded set whose guards remove keys through RAII.
#[derive(Debug)]
struct RegistryEntry {
    lease: u64,
    control: Option<SessionControl>,
    retiring: bool,
}

/// Bounded set whose guards remove keys through generation-checked RAII.
pub struct CapacityRegistry<K> {
    entries: Arc<Mutex<HashMap<K, RegistryEntry>>>,
    capacity: usize,
    next_generation: Arc<Mutex<u64>>,
}

impl<K> CapacityRegistry<K>
where
    K: Clone + Eq + Hash,
{
    /// Creates a registry with a hard capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::with_capacity(capacity))),
            capacity,
            next_generation: Arc::new(Mutex::new(1)),
        }
    }

    /// Acquires one unique key until the returned guard is dropped.
    ///
    /// # Errors
    /// Returns a duplicate or capacity failure without exposing the key.
    pub fn acquire(&self, key: K) -> Result<RegistryGuard<K>, RegistryError> {
        self.acquire_inner(key, None)
    }

    /// Acquires a session lease with explicit task liveness and cancellation control.
    pub fn acquire_with_control(
        &self,
        key: K,
        control: SessionControl,
    ) -> Result<RegistryGuard<K>, RegistryError> {
        self.acquire_inner(key, Some(control))
    }

    fn acquire_inner(
        &self,
        key: K,
        control: Option<SessionControl>,
    ) -> Result<RegistryGuard<K>, RegistryError> {
        let mut entries = self.entries.lock().map_err(|_| RegistryError::Capacity)?;
        if let Some(entry) = entries.get(&key) {
            if entry.retiring
                || entry
                    .control
                    .as_ref()
                    .is_some_and(|control| !control.is_live())
            {
                return Err(RegistryError::Stale(entry.lease));
            }
            return Err(RegistryError::Duplicate(entry.lease));
        }
        if entries.len() >= self.capacity {
            return Err(RegistryError::Capacity);
        }
        let mut generation = self
            .next_generation
            .lock()
            .map_err(|_| RegistryError::Capacity)?;
        let lease = *generation;
        *generation = generation.wrapping_add(1).max(1);
        entries.insert(
            key.clone(),
            RegistryEntry {
                lease,
                control,
                retiring: false,
            },
        );
        Ok(RegistryGuard {
            entries: Arc::downgrade(&self.entries),
            key: Some(key),
            lease,
        })
    }

    /// Returns the owning control for a matching lease without exposing account data.
    #[must_use]
    pub fn control(&self, key: &K, lease: u64) -> Option<SessionControl> {
        self.entries.lock().ok().and_then(|entries| {
            entries.get(key).and_then(|entry| {
                (entry.lease == lease)
                    .then(|| entry.control.clone())
                    .flatten()
            })
        })
    }

    fn remove_lease(&self, key: &K, lease: u64) -> Option<RegistryEntry> {
        self.entries.lock().ok().and_then(|mut entries| {
            if entries.get(key).is_some_and(|entry| entry.lease == lease) {
                entries.remove(key)
            } else {
                None
            }
        })
    }

    /// Invalidates one active registration only when it still owns `lease`.
    ///
    /// The generation comparison prevents a delayed ghost request from removing a replacement
    /// session that acquired the same account after the original connection ended.
    #[must_use]
    pub fn invalidate(&self, key: &K, lease: u64) -> bool {
        let entry = self.remove_lease(key, lease);
        if let Some(control) = entry.as_ref().and_then(|entry| entry.control.as_ref()) {
            control.revoke();
        }
        entry.is_some()
    }

    /// Revokes a stale lease and waits for its owning task to terminate before returning.
    ///
    /// Ghost recovery uses this stronger form so a replacement cannot race a still-running old
    /// task. The lease is marked retiring before the await, so concurrent acquires continue to
    /// observe the same stale generation until termination and removal complete. Delayed old guard
    /// cleanup is harmless because it checks the replacement lease before removing anything.
    pub async fn invalidate_and_wait(&self, key: &K, lease: u64) -> bool {
        let control = self.entries.lock().ok().and_then(|mut entries| {
            let entry = entries.get_mut(key)?;
            if entry.lease != lease {
                return None;
            }
            entry.retiring = true;
            entry.control.clone()
        });
        let Some(control) = control else {
            // A matching un-controlled registry entry can be removed synchronously. A missing
            // entry is distinguishable from an entry whose control is absent by checking once.
            let removed = self
                .entries
                .lock()
                .ok()
                .and_then(|mut entries| {
                    entries
                        .get(key)
                        .is_some_and(|entry| entry.lease == lease)
                        .then(|| entries.remove(key))
                })
                .is_some();
            return removed;
        };
        control.revoke();
        control.wait_terminated().await;
        self.entries
            .lock()
            .ok()
            .and_then(|mut entries| {
                entries
                    .get(key)
                    .is_some_and(|entry| entry.lease == lease && entry.retiring)
                    .then(|| entries.remove(key))
            })
            .is_some()
    }

    /// Returns current occupancy for low-cardinality metrics/tests.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .map_or(self.capacity, |entries| entries.len())
    }

    /// Returns whether no keys are currently registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<K> RegistryGuard<K>
where
    K: Eq + Hash,
{
    /// Returns the generation held by this lease.
    #[must_use]
    pub const fn lease(&self) -> u64 {
        self.lease
    }

    /// Returns whether this guard still owns the current account generation.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.entries
            .upgrade()
            .and_then(|entries| {
                entries.lock().ok().map(|entries| {
                    self.key
                        .as_ref()
                        .and_then(|key| entries.get(key))
                        .is_some_and(|entry| entry.lease == self.lease)
                })
            })
            .unwrap_or(false)
    }
}

/// RAII registration removed on normal exits; an ended controlled task remains as a stale lease
/// until authenticated ghost recovery explicitly revokes that generation.
#[derive(Debug)]
pub struct RegistryGuard<K>
where
    K: Eq + Hash,
{
    entries: Weak<Mutex<HashMap<K, RegistryEntry>>>,
    key: Option<K>,
    lease: u64,
}

impl<K> Drop for RegistryGuard<K>
where
    K: Eq + Hash,
{
    fn drop(&mut self) {
        let Some(entries) = self.entries.upgrade() else {
            return;
        };
        let Some(key) = self.key.take() else {
            return;
        };
        if let Ok(mut entries) = entries.lock()
            && entries.get(&key).is_some_and(|entry| {
                entry.lease == self.lease
                    && !entry.retiring
                    && entry.control.as_ref().is_none_or(SessionControl::is_live)
            })
        {
            entries.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_is_bounded_and_resets() {
        let limiter = FixedWindowLimiter::new(1, 2, Duration::from_secs(1));
        let now = Instant::now();
        assert_eq!(limiter.check("a", now), RateDecision::Allowed);
        assert_eq!(limiter.check("a", now), RateDecision::Allowed);
        assert_eq!(limiter.check("a", now), RateDecision::Limited);
        assert_eq!(limiter.check("b", now), RateDecision::Capacity);
        assert_eq!(
            limiter.check("b", now + Duration::from_secs(2)),
            RateDecision::Allowed
        );
    }

    #[test]
    fn weighted_limiter_has_hard_budget_capacity_and_reset() {
        let limiter = FixedWindowLimiter::new_weighted(1, 10, Duration::from_secs(1));
        let now = Instant::now();
        assert_eq!(limiter.check_weighted("a", now, 6), RateDecision::Allowed);
        assert_eq!(limiter.check_weighted("a", now, 4), RateDecision::Allowed);
        assert_eq!(limiter.check_weighted("a", now, 1), RateDecision::Limited);
        assert_eq!(limiter.check_weighted("b", now, 1), RateDecision::Capacity);
        assert_eq!(
            limiter.check_weighted("b", now + Duration::from_secs(2), 10),
            RateDecision::Allowed
        );
    }

    #[test]
    fn guard_cleans_up_duplicate_registry() {
        let registry = CapacityRegistry::new(1);
        let guard = registry.acquire(7).expect("first");
        assert!(matches!(
            registry.acquire(7),
            Err(RegistryError::Duplicate(_))
        ));
        drop(guard);
        assert!(registry.acquire(7).is_ok());
    }

    #[test]
    fn invalidation_does_not_let_stale_guard_remove_new_lease() {
        let registry = CapacityRegistry::new(1);
        let stale = registry.acquire(7).expect("stale lease");
        let stale_lease = stale.lease();
        assert!(registry.invalidate(&7, stale_lease));
        let current = registry.acquire(7).expect("replacement lease");
        assert!(!registry.invalidate(&7, stale_lease));
        drop(stale);
        assert!(matches!(
            registry.acquire(7),
            Err(RegistryError::Duplicate(_))
        ));
        drop(current);
        assert!(registry.acquire(7).is_ok());
    }

    #[tokio::test]
    async fn blocked_live_owner_remains_duplicate_past_probe_window() {
        let registry = CapacityRegistry::new(1);
        let (control, _probes) = SessionControl::new();
        let _task = control.start_task();
        let _guard = registry
            .acquire_with_control(7, control)
            .expect("live session");
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(matches!(
            registry.acquire(7),
            Err(RegistryError::Duplicate(_))
        ));
    }

    #[test]
    fn ended_task_is_stale_until_explicit_ghost_invalidation() {
        let registry = CapacityRegistry::new(1);
        let (control, _probes) = SessionControl::new();
        let (_guard, lease) = {
            let _task = control.start_task();
            let guard = registry
                .acquire_with_control(7, control.clone())
                .expect("session");
            let lease = guard.lease();
            assert!(control.is_live());
            (guard, lease)
        };
        assert!(!control.is_live());
        assert!(matches!(registry.acquire(7), Err(RegistryError::Stale(found)) if found == lease));
        assert!(registry.invalidate(&7, lease));
        assert!(registry.acquire(7).is_ok());
    }

    #[test]
    fn revoked_session_is_stale_and_can_only_be_ghosted_by_its_lease() {
        let registry = CapacityRegistry::new(1);
        let (session, _probes) = SessionControl::new();
        let guard = registry
            .acquire_with_control(7, session.clone())
            .expect("session");
        let lease = guard.lease();
        session.revoke();
        assert!(matches!(
            registry.acquire(7),
            Err(RegistryError::Stale(found)) if found == lease
        ));
        assert!(registry.invalidate(&7, lease));
        let replacement = registry.acquire(7).expect("replacement");
        drop(guard);
        assert!(matches!(
            registry.acquire(7),
            Err(RegistryError::Duplicate(_))
        ));
        drop(replacement);
    }

    #[tokio::test]
    async fn invalidation_revokes_the_old_task_before_replacement() {
        let registry = CapacityRegistry::new(1);
        let (session, mut probes) = SessionControl::new();
        let guard = registry
            .acquire_with_control(7, session.clone())
            .expect("session");
        let task_session = session.clone();
        let task_lease = session.start_task();
        let task = tokio::spawn(async move {
            let _task_lease = task_lease;
            loop {
                tokio::select! {
                    () = task_session.cancelled() => break true,
                    Some(reply) = probes.recv() => {
                        let _ = reply.send(());
                    }
                    else => break false,
                }
            }
        });
        assert!(session.probe(Duration::from_secs(1)).await);
        assert!(registry.invalidate_and_wait(&7, guard.lease()).await);
        assert!(task.is_finished(), "replacement raced old task termination");
        assert!(task.await.expect("task join"));
        let replacement = registry.acquire(7).expect("replacement");
        drop(guard);
        assert!(matches!(
            registry.acquire(7),
            Err(RegistryError::Duplicate(_))
        ));
        drop(replacement);
    }

    #[test]
    fn concurrent_duplicate_acquires_never_create_a_second_live_lease() {
        use std::sync::Arc;
        use std::thread;

        let registry = Arc::new(CapacityRegistry::new(1));
        let first = registry.acquire(7).expect("first");
        let attempts = (0..8)
            .map(|_| {
                let registry = Arc::clone(&registry);
                thread::spawn(move || registry.acquire(7).map(|guard| guard.lease()))
            })
            .collect::<Vec<_>>();
        for attempt in attempts {
            assert!(matches!(
                attempt.join().expect("join"),
                Err(RegistryError::Duplicate(_))
            ));
        }
        drop(first);
    }
}
