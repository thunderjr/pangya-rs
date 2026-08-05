//! Compatibility and connection context.

use thiserror::Error;

/// Supported client region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    /// United States distribution.
    Us,
}
/// Supported client build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientVersion {
    /// U.S. build 852.00 / GB.852.
    Us852,
}
/// Protocol service endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceKind {
    /// Login service.
    Login,
    /// Game service.
    Game,
    /// Message service.
    Message,
    /// Authentication service.
    Auth,
    /// Ranking service.
    Ranking,
}
/// Packet flow direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Client to server.
    ClientToServer,
    /// Server to client.
    ServerToClient,
}
/// Transport/application connection state used in registry keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionState {
    /// Socket accepted; hello not sent.
    Accepted,
    /// Plain hello sent.
    HelloSent,
    /// Awaiting the first encrypted packet.
    AwaitingFirstPacket,
    /// Service identity established.
    ServiceAuthenticated,
    /// Normal packet processing before channel entry.
    Active,
    /// Authenticated player is in a channel but not a room.
    InChannel,
    /// Authenticated player is a room member.
    InRoom,
    /// Graceful shutdown in progress.
    Draining,
    /// Terminal state.
    Closed,
}
/// Immutable compatibility choice owned by a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompatibilityProfile {
    region: Region,
    version: ClientVersion,
}
/// A packet family was asked to use a compatibility profile it does not implement.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("unsupported compatibility profile {region:?}/{version:?}; expected U.S. 852")]
pub struct ProfileError {
    region: Region,
    version: ClientVersion,
}
impl CompatibilityProfile {
    /// The normative first compatibility profile.
    pub const US_852: Self = Self {
        region: Region::Us,
        version: ClientVersion::Us852,
    };
    /// Verifies that a U.S. 852-only packet family supports this profile.
    ///
    /// Keeping this gate here makes future profile additions fail closed until
    /// each packet family deliberately opts in.
    ///
    /// # Errors
    /// Returns an unsupported-profile error for every profile except `US_852`.
    pub fn require_us852(self) -> Result<(), ProfileError> {
        if self == Self::US_852 {
            Ok(())
        } else {
            Err(ProfileError {
                region: self.region,
                version: self.version,
            })
        }
    }
    /// Profile region.
    #[must_use]
    pub const fn region(self) -> Region {
        self.region
    }
    /// Client version.
    #[must_use]
    pub const fn version(self) -> ClientVersion {
        self.version
    }
}
