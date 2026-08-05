use crate::{ClientVersion, ConnectionState, Direction, ServiceKind};
use std::collections::HashSet;

/// Complete state-aware packet registry key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegistryKey {
    /// Service endpoint.
    pub service: ServiceKind,
    /// Packet direction.
    pub direction: Direction,
    /// Client layout version.
    pub version: ClientVersion,
    /// Required connection state.
    pub state: ConnectionState,
    /// Wire opcode.
    pub opcode: u16,
}
/// Result of a contextual registry lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryLookup {
    /// The complete contextual key is registered.
    Accepted,
    /// The opcode is known for this service, direction, and version, but not this state.
    InvalidState,
    /// No packet is registered for the service, direction, version, and opcode.
    Unknown,
}

/// Set of packet keys accepted by implemented handlers.
#[derive(Debug, Default)]
pub struct PacketRegistry {
    keys: HashSet<RegistryKey>,
}
impl PacketRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Registers a key, returning whether it was newly inserted.
    pub fn register(&mut self, key: RegistryKey) -> bool {
        self.keys.insert(key)
    }
    /// Tests all contextual dimensions, including connection state.
    #[must_use]
    pub fn accepts(&self, key: RegistryKey) -> bool {
        self.classify(key) == RegistryLookup::Accepted
    }
    /// Classifies an exact match, a known opcode in the wrong state, or a true unknown.
    #[must_use]
    pub fn classify(&self, key: RegistryKey) -> RegistryLookup {
        if self.keys.contains(&key) {
            return RegistryLookup::Accepted;
        }
        if self.keys.iter().any(|candidate| {
            candidate.service == key.service
                && candidate.direction == key.direction
                && candidate.version == key.version
                && candidate.opcode == key.opcode
        }) {
            RegistryLookup::InvalidState
        } else {
            RegistryLookup::Unknown
        }
    }
    /// Number of registered contextual entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }
    /// Whether there are no registered entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}
