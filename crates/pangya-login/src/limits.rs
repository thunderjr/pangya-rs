//! Fixed-capacity admission, rate-limit, and duplicate-login registries.

use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

use thiserror::Error;

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
    /// The key is already present (duplicate authenticated LoginService session).
    #[error("duplicate active login")]
    Duplicate,
    /// The bounded registry is full or unavailable.
    #[error("active registry is full")]
    Capacity,
}

/// Bounded set whose guards remove keys through RAII.
#[derive(Debug)]
pub struct CapacityRegistry<K> {
    entries: Arc<Mutex<HashMap<K, u64>>>,
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
        let mut entries = self.entries.lock().map_err(|_| RegistryError::Capacity)?;
        if entries.contains_key(&key) {
            return Err(RegistryError::Duplicate);
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
        entries.insert(key.clone(), lease);
        Ok(RegistryGuard {
            entries: Arc::downgrade(&self.entries),
            key: Some(key),
            lease,
        })
    }

    /// Invalidates one active registration. A stale guard cannot remove a later lease for the same
    /// key, which makes ghost recovery safe when the old connection eventually unwinds.
    #[must_use]
    pub fn invalidate(&self, key: &K) -> bool {
        self.entries
            .lock()
            .ok()
            .and_then(|mut entries| entries.remove(key))
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

/// RAII registration removed on every normal/error/cancellation exit.
#[derive(Debug)]
pub struct RegistryGuard<K>
where
    K: Eq + Hash,
{
    entries: Weak<Mutex<HashMap<K, u64>>>,
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
            && entries.get(&key).copied() == Some(self.lease)
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
        assert!(matches!(registry.acquire(7), Err(RegistryError::Duplicate)));
        drop(guard);
        assert!(registry.acquire(7).is_ok());
    }

    #[test]
    fn invalidation_does_not_let_stale_guard_remove_new_lease() {
        let registry = CapacityRegistry::new(1);
        let stale = registry.acquire(7).expect("stale lease");
        assert!(registry.invalidate(&7));
        let current = registry.acquire(7).expect("replacement lease");
        drop(stale);
        assert!(matches!(registry.acquire(7), Err(RegistryError::Duplicate)));
        drop(current);
        assert!(registry.acquire(7).is_ok());
    }
}
