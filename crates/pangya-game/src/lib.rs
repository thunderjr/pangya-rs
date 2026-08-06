#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Bounded synthetic GameService handover, bootstrap, lobby, and room runtime.

pub mod lobby;
pub mod match_state;
pub mod room;
pub mod stroke_state;

pub use lobby::{
    LobbyHandle, LobbyLimits, LobbyRoomCommand, LobbyRouteResult, LobbyShutdownError,
    LobbyShutdownOutcome, LobbySoloCommand, LobbySoloRouteResult, spawn_lobby,
};
pub use match_state::{
    LOADING_TIMEOUT_HARD_CAP, MAX_SOLO_STROKES, RelayDisposition, SoloMatchError, SoloMatchPhase,
    SoloMatchState, SoloStartPlan, deterministic_conditions,
};
pub use room::{RoomActorLimits, RoomDisconnect, RoomEvent, RoomHandle, RoomIdentity, spawn_room};

use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use futures_util::{SinkExt as _, StreamExt as _};
use pangya_data::Catalog;
use pangya_domain::{
    AbortMatch, AbortMatchOutcome, AccountId, BeginSoloMatch, BeginSoloMatchOutcome,
    CatalogFingerprint, ConsumeHandover, HandoverRepository, MarkSoloInGame, MarkSoloInGameOutcome,
    MatchAbortReason, MatchId, MatchRepository, MatchResultKey, MatchSeed, Nickname, OneHoleConfig,
    PlayerConnectionId, PlayerRepository, PlayerSnapshot, RepositoryError, RoomError, RoomId,
    ServiceKind as DomainServiceKind, SoloMatchResult, SourceAddressPrefix,
};
use pangya_login::{
    CapacityRegistry, FixedWindowLimiter, KeyedCapacityGuard, KeyedCapacityRegistry, RateDecision,
    RegistryError, RegistryGuard, parse_handover,
};
use pangya_protocol::{
    BalanceUpdate, ChannelJoined, CharacterBootstrap, CharacterInfo, CodecLimits,
    CompatibilityProfile, DecodePacket, EncodePacket, EquipmentInfo, FinishHole, FrameCodec,
    GAME_INVENTORY_SEGMENT_ITEMS, GameAuth, HoleResult, InventoryBootstrap, InventorySegment,
    LoadingComplete, MatchAbortReason as ProtocolMatchAbortReason, MatchAborted, MatchPhase,
    MatchStarted, OutboundFrame, PacketEncodeError, PlayerInfo, RoomChatEvent, RoomChatRequest,
    RoomCommand, RoomCommandResult, RoomCommandResultResponse, RoomCreateRequest, RoomJoinRequest,
    RoomKickRequest, RoomLeaveRequest, RoomListRequest, RoomListResponse, RoomMembershipEvent,
    RoomMembershipKind, RoomReadyRequest, RoomSettingsRequest, RoomStateRequest, RoomStateResponse,
    SYNTHETIC_M4_C2S_CHAT, SYNTHETIC_M4_C2S_CREATE, SYNTHETIC_M4_C2S_JOIN, SYNTHETIC_M4_C2S_KICK,
    SYNTHETIC_M4_C2S_LEAVE, SYNTHETIC_M4_C2S_LIST, SYNTHETIC_M4_C2S_READY,
    SYNTHETIC_M4_C2S_SETTINGS, SYNTHETIC_M4_C2S_STATE, SYNTHETIC_M5_C2S_FINISH_HOLE,
    SYNTHETIC_M5_C2S_LOADING_COMPLETE, SYNTHETIC_M5_C2S_SHOT_ACTION, SYNTHETIC_M5_C2S_SHOT_RESULT,
    SYNTHETIC_M5_C2S_START_SOLO, SelectChannel, ServiceKind, ShotAction, ShotActionRelay,
    ShotResult, ShotResultRelay, SoloCommand, SoloCommandOutcome, SoloCommandResult, SoloPhase,
    StartSolo, Weather as ProtocolWeather, Wind, decode_packet_payload, encode_packet_payload,
    synthetic_game_hello,
};
use rand::{RngCore as _, rngs::OsRng};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::{
    io::AsyncWriteExt as _,
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc},
    task::JoinSet,
    time::{sleep_until, timeout},
};
use tokio_util::{codec::Framed, sync::CancellationToken};
use tracing::Instrument as _;

/// Process-local GameService connection ID safe for observability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GameConnectionId(u64);

impl GameConnectionId {
    /// Returns the process-local value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Explicit GameService application state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameState {
    /// Waiting for bearer authentication.
    AwaitHandover,
    /// Authenticated snapshot emitted; waiting for channel selection.
    AwaitChannel,
    /// In the selected channel, but not in a room.
    InChannel,
    /// Authoritatively registered in a room.
    InRoom,
    /// Solo begin committed; waiting for loading completion.
    InMatchLoading,
    /// Solo gameplay is active or waiting for durable commit.
    InMatch,
    /// Terminal state.
    Closed,
}

/// Policy for a truly unknown opcode after channel selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnknownOpcodePolicy {
    /// Disconnect immediately.
    Disconnect,
    /// Ignore up to the fixed strike limit.
    Ignore,
    /// Store only bounded metadata and a SHA-256 body digest, then ignore.
    Capture,
}

/// Redacted metadata retained for an unknown opcode. The raw body is never retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownOpcodeCapture {
    /// State in which the frame arrived.
    pub state: GameState,
    /// Unknown opcode.
    pub opcode: u16,
    /// Plaintext payload length.
    pub payload_len: usize,
    /// SHA-256 of the plaintext payload.
    pub sha256: [u8; 32],
}

/// Fixed GameService termination outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameTermination {
    /// Peer closed the socket.
    PeerClosed,
    /// Service cancellation closed the socket.
    Cancelled,
    /// Authentication was rejected.
    Rejected,
    /// A deadline elapsed.
    Timeout,
    /// A fixed resource limit was reached.
    Limited,
    /// Malformed or state-invalid protocol was received.
    Protocol,
    /// Other redacted runtime failure.
    Error,
}

impl GameTermination {
    /// Fixed metrics/tracing label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PeerClosed => "peer_closed",
            Self::Cancelled => "cancelled",
            Self::Rejected => "rejected",
            Self::Timeout => "timeout",
            Self::Limited => "limited",
            Self::Protocol => "protocol",
            Self::Error => "error",
        }
    }
}

/// Fixed GameService rate/resource classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameRateClass {
    /// Global accepts.
    AcceptGlobal,
    /// Source accepts.
    AcceptSource,
    /// Global concurrent connections.
    ConnectionGlobal,
    /// Source concurrent connections.
    ConnectionSource,
    /// Global authentication attempts.
    AuthGlobal,
    /// Source authentication attempts.
    AuthSource,
    /// Global packet count.
    PacketGlobal,
    /// Source packet count.
    PacketSource,
    /// Per-connection packets or bytes.
    PacketOrBytesConnection,
    /// Global plaintext bytes.
    BytesGlobal,
    /// Source plaintext bytes.
    BytesSource,
    /// Per-connection room commands.
    RoomCommandsConnection,
    /// Per-connection chat messages.
    ChatConnection,
    /// Per-connection solo shot action/result packets.
    ShotPacketsConnection,
}

/// Fixed solo match lifecycle observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameMatchObservation {
    /// Begin was durably confirmed.
    Started,
    /// Loading completed.
    LoadingComplete,
    /// Result committed and emitted.
    Finished,
    /// Match aborted without reward.
    Aborted,
    /// Actor loading deadline elapsed.
    LoadingTimeout,
}

/// Fixed repository transaction observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameCommitObservation {
    /// New begin row inserted.
    Begun,
    /// Exact begin already existed.
    Existing,
    /// Hole commit completed.
    Committed,
    /// A committed result won an abort race.
    Idempotent,
    /// Repository or actor application failed.
    Failed,
    /// Deadline or shutdown cancelled repository work.
    Cancelled,
}

/// Fixed shot handling observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameShotObservation {
    /// New shot relay accepted.
    Accepted,
    /// Exact relay duplicate accepted without mutation.
    Duplicate,
    /// Shot was rejected by validation/state.
    Rejected,
    /// Local shot budget was exceeded.
    RateLimited,
}

/// Fixed room observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameRoomObservation {
    /// Room list sent.
    Listed,
    /// Room created.
    Created,
    /// Room joined.
    Joined,
    /// Room left.
    Left,
    /// Settings changed.
    SettingsChanged,
    /// Ready state changed.
    ReadyChanged,
    /// Member kicked or local connection received a kick.
    Kicked,
    /// Authoritative room state sent.
    StateSent,
    /// Room closed.
    Closed,
}

/// Fixed queue observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameQueueObservation {
    /// Lobby command or cleanup was rejected.
    LobbyRejected,
    /// A room event did not fit the connection's outbound queue.
    OutboundDropped,
}

/// Fixed chat observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameChatObservation {
    /// Chat command accepted.
    Accepted,
    /// Chat event delivered to a socket.
    Delivered,
    /// Chat command rate-limited.
    RateLimited,
}

/// Fixed unknown-opcode observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameUnknownObservation {
    /// Policy disconnected immediately.
    Disconnected,
    /// Policy ignored the frame.
    Ignored,
    /// Policy retained bounded metadata and digest.
    Captured,
    /// Fixed strike limit was reached.
    StrikeLimit,
}

/// Low-cardinality GameService observation boundary.
pub trait GameObserver: Send + Sync + 'static {
    /// Accepted connection with masked source only.
    fn accepted(&self, _id: GameConnectionId, _source: &SourceAddressPrefix) {}
    /// Fixed terminal outcome.
    fn closed(&self, _outcome: GameTermination) {}
    /// Packet metadata without bodies or bearer values.
    fn frame(&self, _direction: &'static str, _opcode: u16, _bytes: usize) {}
    /// Fixed authentication outcome.
    fn authentication(&self, _outcome: &'static str) {}
    /// Fixed rate/resource rejection.
    fn rate_limited(&self, _class: GameRateClass) {}
    /// Authenticated numeric identity only.
    fn authenticated(&self, _account_id: AccountId) {}
    /// Fixed room lifecycle observation.
    fn room(&self, _event: GameRoomObservation) {}
    /// Stores the sole registry's exact active-room count.
    fn rooms_active(&self, _active_count: usize) {}
    /// Fixed queue observation.
    fn queue(&self, _event: GameQueueObservation) {}
    /// Fixed chat observation.
    fn chat(&self, _event: GameChatObservation) {}
    /// Fixed unknown-opcode observation.
    fn unknown(&self, _event: GameUnknownObservation) {}
    /// Exact process-local active solo match gauge.
    fn matches_active(&self, _active: usize) {}
    /// Fixed solo lifecycle observation.
    fn match_event(&self, _event: GameMatchObservation) {}
    /// Fixed persistence outcome.
    fn commit(&self, _outcome: GameCommitObservation) {}
    /// Fixed shot outcome.
    fn shot(&self, _outcome: GameShotObservation) {}
}

/// No-op GameService observer.
#[derive(Debug, Default)]
pub struct NoopGameObserver;
impl GameObserver for NoopGameObserver {}

/// Hard-bounded GameService runtime policy.
#[derive(Clone, Debug)]
pub struct GameRuntimeLimits {
    /// Concurrent connections.
    pub global_connections: usize,
    /// Concurrent connections per masked source.
    pub connections_per_source: usize,
    /// Maximum masked-source registry keys.
    pub source_capacity: usize,
    /// Global accepts per window.
    pub global_accepts_per_window: u32,
    /// Source accepts per window.
    pub accepts_per_window: u32,
    /// Global authentication attempts per window.
    pub global_auth_per_window: u32,
    /// Source authentication attempts per window.
    pub auth_per_window: u32,
    /// Global packets per window.
    pub global_packets_per_window: u32,
    /// Global plaintext bytes per window.
    pub global_bytes_per_window: u64,
    /// Source packets per window.
    pub source_packets_per_window: u32,
    /// Source plaintext bytes per window.
    pub source_bytes_per_window: u64,
    /// Connection packets per window.
    pub packets_per_window: u32,
    /// Connection plaintext bytes per window.
    pub bytes_per_window: u64,
    /// Per-connection room commands per fixed window.
    pub room_commands_per_window: u32,
    /// Per-connection room chat messages per fixed window.
    pub chat_messages_per_window: u32,
    /// Maximum unknown-opcode strikes before disconnect.
    pub unknown_opcode_strikes: u32,
    /// Maximum metadata-digest captures retained process-locally.
    pub unknown_capture_capacity: usize,
    /// Capacity of each connection's room-event queue.
    pub outbound_room_event_capacity: usize,
    /// Lobby and nested room actor bounds.
    pub lobby: LobbyLimits,
    /// Shared fixed-window duration.
    pub rate_window: Duration,
    /// Total handover-to-channel deadline.
    pub authentication_timeout: Duration,
    /// In-channel and packet idle deadline.
    pub idle_timeout: Duration,
    /// Deadline for an individual lobby command and disconnect cleanup.
    pub command_timeout: Duration,
    /// Connection and lobby drain deadline.
    pub shutdown_grace: Duration,
    /// Framed transport limits.
    pub codec: CodecLimits,
}

impl Default for GameRuntimeLimits {
    fn default() -> Self {
        Self {
            global_connections: 256,
            connections_per_source: 8,
            source_capacity: 1_024,
            global_accepts_per_window: 1_000,
            accepts_per_window: 30,
            global_auth_per_window: 1_000,
            auth_per_window: 10,
            global_packets_per_window: 10_000,
            global_bytes_per_window: 16 * 1024 * 1024,
            source_packets_per_window: 600,
            source_bytes_per_window: 2 * 1024 * 1024,
            packets_per_window: 60,
            bytes_per_window: 256 * 1024,
            room_commands_per_window: 30,
            chat_messages_per_window: 10,
            unknown_opcode_strikes: 3,
            unknown_capture_capacity: 256,
            outbound_room_event_capacity: 64,
            lobby: LobbyLimits::default(),
            rate_window: Duration::from_secs(60),
            authentication_timeout: Duration::from_secs(15),
            idle_timeout: Duration::from_secs(120),
            command_timeout: Duration::from_secs(3),
            shutdown_grace: Duration::from_secs(10),
            codec: CodecLimits::default(),
        }
    }
}

/// Checked optional synthetic solo-practice composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoloRuntimeConfig {
    /// Catalog-validated one-hole course.
    pub course: OneHoleConfig,
    /// Exact fingerprint of the loaded catalog.
    pub catalog_fingerprint: CatalogFingerprint,
    /// Actor-owned loading deadline, represented exactly in protocol milliseconds.
    pub loading_timeout: Duration,
    /// Repository operation deadline.
    pub commit_timeout: Duration,
    /// Authoritative stroke cap.
    pub max_strokes: u8,
    /// Startup recovery work cap.
    pub startup_recovery_limit: pangya_domain::IncompleteMatchAbortLimit,
    /// Per-connection action/result packet budget per shared rate window.
    pub shot_packets_per_window: u32,
}

/// Immutable GameService composition.
#[derive(Clone, Debug)]
pub struct GameRuntimeConfig {
    /// Sole locally configured channel ID.
    pub channel_id: u32,
    /// Post-channel handling policy for truly unknown opcodes.
    pub unknown_opcode_policy: UnknownOpcodePolicy,
    /// Resource, rate, actor, and deadline limits.
    pub limits: GameRuntimeLimits,
    /// Optional local-only synthetic solo practice.
    pub solo_practice: Option<SoloRuntimeConfig>,
}

impl Default for GameRuntimeConfig {
    fn default() -> Self {
        Self {
            channel_id: 1,
            unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
            limits: GameRuntimeLimits::default(),
            solo_practice: None,
        }
    }
}

/// Redacted GameService failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GameRuntimeError {
    /// Listener failed.
    #[error("GameService listener failed")]
    Accept,
    /// Framed socket I/O failed.
    #[error("GameService connection I/O failed")]
    Io,
    /// Packet was malformed or invalid for current state.
    #[error("GameService protocol rejected the connection")]
    Protocol,
    /// Handover was rejected.
    #[error("GameService handover rejected")]
    Authentication,
    /// Player bootstrap was unavailable or invalid.
    #[error("GameService player bootstrap failed")]
    Snapshot,
    /// Catalog cross-check rejected persisted identifiers.
    #[error("GameService catalog validation failed")]
    Catalog,
    /// Fixed resource or rate limit was reached.
    #[error("GameService resource limit reached")]
    Limited,
    /// Authentication or idle deadline elapsed.
    #[error("GameService connection timed out")]
    Timeout,
    /// Runtime composition was invalid.
    #[error("GameService runtime configuration is invalid")]
    InvalidConfig,
    /// Match persistence failed with details redacted.
    #[error("GameService match persistence failed")]
    MatchPersistence,
    /// Connection or lobby drain exceeded grace.
    #[error("GameService graceful shutdown timed out")]
    ShutdownTimeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AbortResolution {
    Aborted,
    Committed,
}

struct Admission {
    source: SourceAddressPrefix,
    _global: OwnedSemaphorePermit,
    _source: KeyedCapacityGuard<SourceAddressPrefix>,
}

#[derive(Debug)]
struct CaptureSink {
    capacity: usize,
    entries: Mutex<VecDeque<UnknownOpcodeCapture>>,
}

impl CaptureSink {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    fn push(&self, state: GameState, opcode: u16, payload: &[u8]) {
        let capture = UnknownOpcodeCapture {
            state,
            opcode,
            payload_len: payload.len(),
            sha256: Sha256::digest(payload).into(),
        };
        if let Ok(mut entries) = self.entries.lock() {
            if entries.len() == self.capacity {
                entries.pop_front();
            }
            entries.push_back(capture);
        }
    }

    fn snapshot(&self) -> Vec<UnknownOpcodeCapture> {
        self.entries
            .lock()
            .map_or_else(|_| Vec::new(), |entries| entries.iter().cloned().collect())
    }
}

/// Generic bounded GameService over domain repositories and an immutable catalog.
pub struct GameService<R>
where
    R: HandoverRepository + PlayerRepository + MatchRepository + 'static,
{
    repository: Arc<R>,
    catalog: Catalog,
    config: GameRuntimeConfig,
    observer: Arc<dyn GameObserver>,
    lobby: LobbyHandle,
    captures: CaptureSink,
    connection_ids: AtomicU64,
    global_connections: Arc<Semaphore>,
    source_connections: KeyedCapacityRegistry<SourceAddressPrefix>,
    global_accepts: FixedWindowLimiter<()>,
    source_accepts: FixedWindowLimiter<SourceAddressPrefix>,
    global_auth: FixedWindowLimiter<()>,
    source_auth: FixedWindowLimiter<SourceAddressPrefix>,
    global_packets: FixedWindowLimiter<()>,
    source_packets: FixedWindowLimiter<SourceAddressPrefix>,
    global_bytes: FixedWindowLimiter<()>,
    source_bytes: FixedWindowLimiter<SourceAddressPrefix>,
    active_accounts: CapacityRegistry<AccountId>,
}

impl<R> std::fmt::Debug for GameService<R>
where
    R: HandoverRepository + PlayerRepository + MatchRepository + 'static,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GameService")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<R> GameService<R>
where
    R: HandoverRepository + PlayerRepository + MatchRepository + 'static,
{
    /// Creates a GameService after validating every direct and actor bound.
    pub fn new(
        repository: Arc<R>,
        catalog: Catalog,
        config: GameRuntimeConfig,
        observer: Arc<dyn GameObserver>,
    ) -> Result<Self, GameRuntimeError> {
        let limits = &config.limits;
        let invalid = config.channel_id == 0
            || limits.global_connections == 0
            || limits.global_connections > 10_000
            || limits.connections_per_source == 0
            || limits.connections_per_source > limits.global_connections
            || limits.source_capacity == 0
            || limits.source_capacity > 65_536
            || [
                limits.global_accepts_per_window,
                limits.accepts_per_window,
                limits.global_auth_per_window,
                limits.auth_per_window,
                limits.global_packets_per_window,
                limits.source_packets_per_window,
                limits.packets_per_window,
                limits.room_commands_per_window,
                limits.chat_messages_per_window,
                limits.unknown_opcode_strikes,
            ]
            .into_iter()
            .any(|value| value == 0 || value > 1_000_000)
            || [
                limits.global_bytes_per_window,
                limits.source_bytes_per_window,
                limits.bytes_per_window,
            ]
            .into_iter()
            .any(|value| value == 0 || value > 1024 * 1024 * 1024)
            || limits.unknown_capture_capacity == 0
            || limits.unknown_capture_capacity > 65_536
            || limits.outbound_room_event_capacity == 0
            || (config.solo_practice.is_some() && limits.outbound_room_event_capacity < 2)
            || limits.outbound_room_event_capacity > 65_536
            || !limits.lobby.is_valid()
            || limits.lobby.cleanup_capacity() < limits.global_connections.saturating_add(1)
            || limits.rate_window.is_zero()
            || limits.rate_window > Duration::from_secs(3_600)
            || limits.authentication_timeout.is_zero()
            || limits.authentication_timeout > Duration::from_secs(3_600)
            || limits.idle_timeout.is_zero()
            || limits.idle_timeout > Duration::from_secs(3_600)
            || limits.command_timeout.is_zero()
            || limits.command_timeout > Duration::from_secs(300)
            || limits.shutdown_grace.is_zero()
            || limits.shutdown_grace < limits.command_timeout
            || limits.shutdown_grace > Duration::from_secs(300)
            || limits.codec.max_client_frame_bytes < 5
            || limits.codec.max_client_frame_bytes > 65_535
            || limits.codec.max_server_plaintext_bytes < 2
            || limits.codec.max_server_plaintext_bytes > 64 * 1024 * 1024
            || limits.codec.max_expansion_ratio == 0
            || limits.codec.max_expansion_ratio > 1_024
            || config.solo_practice.is_some_and(|solo| {
                let loading_ms = solo.loading_timeout.as_millis();
                solo.loading_timeout.is_zero()
                    || solo.loading_timeout > LOADING_TIMEOUT_HARD_CAP
                    || loading_ms == 0
                    || loading_ms > u128::from(u32::MAX)
                    || solo.commit_timeout.is_zero()
                    || solo.commit_timeout > limits.shutdown_grace
                    || solo.commit_timeout > Duration::from_secs(60)
                    || !(1..=MAX_SOLO_STROKES).contains(&solo.max_strokes)
                    || solo.shot_packets_per_window == 0
                    || solo.shot_packets_per_window > 1_000_000
            });
        if invalid {
            return Err(GameRuntimeError::InvalidConfig);
        }
        if let Some(solo) = config.solo_practice {
            let catalog_course = catalog
                .one_hole_course(solo.course.course_id())
                .map_err(|_| GameRuntimeError::Catalog)?;
            if catalog_course != solo.course || catalog.fingerprint() != solo.catalog_fingerprint {
                return Err(GameRuntimeError::Catalog);
            }
        }
        let lobby = spawn_lobby(limits.lobby);
        Ok(Self {
            repository,
            catalog,
            connection_ids: AtomicU64::new(1),
            global_connections: Arc::new(Semaphore::new(limits.global_connections)),
            source_connections: KeyedCapacityRegistry::new(
                limits.source_capacity,
                limits.connections_per_source,
            ),
            global_accepts: FixedWindowLimiter::new(
                1,
                limits.global_accepts_per_window,
                limits.rate_window,
            ),
            source_accepts: FixedWindowLimiter::new(
                limits.source_capacity,
                limits.accepts_per_window,
                limits.rate_window,
            ),
            global_auth: FixedWindowLimiter::new(
                1,
                limits.global_auth_per_window,
                limits.rate_window,
            ),
            source_auth: FixedWindowLimiter::new(
                limits.source_capacity,
                limits.auth_per_window,
                limits.rate_window,
            ),
            global_packets: FixedWindowLimiter::new(
                1,
                limits.global_packets_per_window,
                limits.rate_window,
            ),
            source_packets: FixedWindowLimiter::new(
                limits.source_capacity,
                limits.source_packets_per_window,
                limits.rate_window,
            ),
            global_bytes: FixedWindowLimiter::new_weighted(
                1,
                limits.global_bytes_per_window,
                limits.rate_window,
            ),
            source_bytes: FixedWindowLimiter::new_weighted(
                limits.source_capacity,
                limits.source_bytes_per_window,
                limits.rate_window,
            ),
            active_accounts: CapacityRegistry::new(limits.global_connections),
            captures: CaptureSink::new(limits.unknown_capture_capacity),
            config,
            observer,
            lobby,
        })
    }

    /// Returns a bounded copy of captured metadata-digest records.
    #[must_use]
    pub fn unknown_opcode_captures(&self) -> Vec<UnknownOpcodeCapture> {
        self.captures.snapshot()
    }

    /// Serves admitted connections, drains them, then boundedly shuts down the lobby.
    pub async fn serve(
        self: Arc<Self>,
        listener: TcpListener,
        shutdown: CancellationToken,
    ) -> Result<(), GameRuntimeError> {
        let mut tasks = JoinSet::new();
        let mut room_lifecycle = self.lobby.subscribe_room_lifecycle();
        let mut match_lifecycle = self.lobby.subscribe_match_lifecycle();
        let mut service_failure = None;
        'accept: loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                lifecycle = room_lifecycle.recv() => match lifecycle {
                    Ok(lifecycle) => observe_room_lifecycle(self.observer.as_ref(), lifecycle),
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        if !drain_room_lifecycle(self.observer.as_ref(), &mut room_lifecycle) {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                lifecycle = match_lifecycle.recv() => match lifecycle {
                    Ok(lifecycle) => observe_match_lifecycle(self.observer.as_ref(), lifecycle),
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        if !drain_match_lifecycle(self.observer.as_ref(), &mut match_lifecycle) {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                accepted = listener.accept() => {
                    let (stream, peer) = accepted.map_err(|_| GameRuntimeError::Accept)?;
                    match self.admit(peer) {
                        Ok(admission) => {
                            while tasks.len() >= self.config.limits.global_connections {
                                if joined_match_persistence(tasks.join_next().await) {
                                    service_failure = Some(GameRuntimeError::MatchPersistence);
                                    shutdown.cancel();
                                    break 'accept;
                                }
                            }
                            let service = Arc::clone(&self);
                            let child = shutdown.child_token();
                            tasks.spawn(async move {
                                service.run_admitted(stream, admission, child).await
                            });
                        }
                        Err(_) => drop(stream),
                    }
                }
                joined = tasks.join_next(), if !tasks.is_empty() => {
                    if joined_match_persistence(joined) {
                        service_failure = Some(GameRuntimeError::MatchPersistence);
                        shutdown.cancel();
                        break;
                    }
                }
            }
        }
        drop(listener);
        let drained = timeout(self.config.limits.shutdown_grace, async {
            while let Some(joined) = tasks.join_next().await {
                if joined_match_persistence(Some(joined)) {
                    service_failure = Some(GameRuntimeError::MatchPersistence);
                }
            }
            let outcome = self.lobby.shutdown().await.map_err(|error| match error {
                LobbyShutdownError::Room(RoomError::Timeout) => GameRuntimeError::ShutdownTimeout,
                _ => GameRuntimeError::MatchPersistence,
            })?;
            for abort in outcome.into_aborts() {
                self.persist_shutdown_abort(abort).await?;
            }
            service_failure.map_or(Ok(()), Err)
        })
        .await;
        let _room_lifecycle_open =
            drain_room_lifecycle(self.observer.as_ref(), &mut room_lifecycle);
        let _match_lifecycle_open =
            drain_match_lifecycle(self.observer.as_ref(), &mut match_lifecycle);
        match drained {
            Ok(result) => result,
            Err(_) => {
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
                Err(GameRuntimeError::ShutdownTimeout)
            }
        }
    }

    fn admit(&self, peer: SocketAddr) -> Result<Admission, GameRuntimeError> {
        let source = SourceAddressPrefix::from_ip(peer.ip());
        let now = Instant::now();
        if self.global_accepts.check((), now) != RateDecision::Allowed {
            self.observer.rate_limited(GameRateClass::AcceptGlobal);
            return Err(GameRuntimeError::Limited);
        }
        if self.source_accepts.check(source.clone(), now) != RateDecision::Allowed {
            self.observer.rate_limited(GameRateClass::AcceptSource);
            return Err(GameRuntimeError::Limited);
        }
        let global = Arc::clone(&self.global_connections)
            .try_acquire_owned()
            .map_err(|_| {
                self.observer.rate_limited(GameRateClass::ConnectionGlobal);
                GameRuntimeError::Limited
            })?;
        let source_guard = self
            .source_connections
            .acquire(source.clone())
            .map_err(|_| {
                self.observer.rate_limited(GameRateClass::ConnectionSource);
                GameRuntimeError::Limited
            })?;
        Ok(Admission {
            source,
            _global: global,
            _source: source_guard,
        })
    }

    async fn run_admitted(
        self: Arc<Self>,
        mut stream: TcpStream,
        admission: Admission,
        shutdown: CancellationToken,
    ) -> Result<(), GameRuntimeError> {
        let source = admission.source.clone();
        let raw_id = self.connection_ids.fetch_add(1, Ordering::Relaxed);
        let id = GameConnectionId(raw_id);
        let player_connection_id =
            PlayerConnectionId::new(raw_id).map_err(|_| GameRuntimeError::Limited)?;
        self.observer.accepted(id, &source);
        let span = tracing::info_span!(
            "connection",
            connection_id = id.get(),
            service = "game",
            client_profile = "us_852_synthetic_m4",
            source_prefix = %source,
            account_id = tracing::field::Empty,
        );
        let result = async {
            let key = (OsRng.next_u32() & 0x0f) as u8;
            let hello = synthetic_game_hello(key).map_err(|_| GameRuntimeError::Protocol)?;
            stream
                .write_all(&hello)
                .await
                .map_err(|_| GameRuntimeError::Io)?;
            let framed = Framed::new(
                stream,
                FrameCodec::new(key, ServiceKind::Game, self.config.limits.codec),
            );
            self.run_connection(framed, source, player_connection_id, shutdown)
                .await
        }
        .instrument(span)
        .await;
        drop(admission);
        let outcome = match &result {
            Ok(outcome) => *outcome,
            Err(GameRuntimeError::Authentication) => GameTermination::Rejected,
            Err(GameRuntimeError::Timeout) => GameTermination::Timeout,
            Err(GameRuntimeError::Limited) => GameTermination::Limited,
            Err(GameRuntimeError::Protocol) => GameTermination::Protocol,
            Err(_) => GameTermination::Error,
        };
        self.observer.closed(outcome);
        result.map(drop)
    }

    async fn run_connection(
        &self,
        mut framed: Framed<TcpStream, FrameCodec>,
        source: SourceAddressPrefix,
        connection_id: PlayerConnectionId,
        shutdown: CancellationToken,
    ) -> Result<GameTermination, GameRuntimeError> {
        let started = Instant::now();
        let mut idle_deadline = Instant::now() + self.config.limits.idle_timeout;
        let mut state = GameState::AwaitHandover;
        let mut presence: Option<RegistryGuard<AccountId>> = None;
        let mut identity: Option<RoomIdentity> = None;
        let mut local = LocalRateWindow::new(self.config.limits.rate_window);
        let mut commands = LocalRateWindow::new(self.config.limits.rate_window);
        let mut chats = LocalRateWindow::new(self.config.limits.rate_window);
        let mut shots = LocalRateWindow::new(self.config.limits.rate_window);
        let mut unknown_strikes = 0_u32;
        let (outbound, mut room_events) =
            mpsc::channel(self.config.limits.outbound_room_event_capacity);
        let room_cancellation = CancellationToken::new();
        let mut room_id: Option<RoomId> = None;

        let result = loop {
            let deadline = if matches!(state, GameState::AwaitHandover | GameState::AwaitChannel) {
                (started + self.config.limits.authentication_timeout).min(idle_deadline)
            } else {
                idle_deadline
            };
            let sleeper = sleep_until(deadline.into());
            tokio::pin!(sleeper);
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                () = room_cancellation.cancelled() => {
                    self.observer.queue(GameQueueObservation::OutboundDropped);
                    break Err(GameRuntimeError::Limited);
                }
                event = room_events.recv(), if matches!(state, GameState::InRoom | GameState::InMatchLoading | GameState::InMatch) => {
                    let Some(event) = event else { break Err(GameRuntimeError::Limited); };
                    let handled = tokio::select! {
                        biased;
                        () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                        handled = self.handle_room_event(
                            &mut framed,
                            event,
                            room_id,
                            connection_id,
                        ) => handled,
                    };
                    match handled {
                        Ok(RoomEventEffect::Remain) => {}
                        Ok(RoomEventEffect::EnterChannel) => {
                            state = GameState::InChannel;
                            room_id = None;
                        }
                        Ok(RoomEventEffect::EnterRoom) => state = GameState::InRoom,
                        Ok(RoomEventEffect::EnterLoading) => state = GameState::InMatchLoading,
                        Ok(RoomEventEffect::EnterMatch) => state = GameState::InMatch,
                        Err(error) => break Err(error),
                    }
                }
                frame = framed.next() => {
                    let Some(frame) = frame else { break Ok(GameTermination::PeerClosed); };
                    let frame = match frame {
                        Ok(frame) => frame,
                        Err(_) => break Err(GameRuntimeError::Protocol),
                    };
                    idle_deadline = Instant::now() + self.config.limits.idle_timeout;
                    let bytes = frame.payload.len().saturating_add(2);
                    if let Err(error) = self.admit_packet(&source, &mut local, bytes) {
                        break Err(error);
                    }
                    self.observer.frame("in", frame.opcode, bytes);
                    match state {
                        GameState::AwaitHandover if frame.opcode == GameAuth::OPCODE => {
                            let authenticated = tokio::select! {
                                biased;
                                () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                                authenticated = self.authenticate(
                                    &mut framed,
                                    &source,
                                    started,
                                    connection_id,
                                    &frame.payload,
                                ) => authenticated,
                            };
                            match authenticated {
                                Ok((guard, established)) => {
                                    presence = Some(guard);
                                    identity = Some(established);
                                    state = GameState::AwaitChannel;
                                }
                                Err(error) => break Err(error),
                            }
                        }
                        GameState::AwaitChannel if frame.opcode == SelectChannel::OPCODE => {
                            let selected = match decode_packet_payload::<SelectChannel>(
                                &frame.payload,
                                &CompatibilityProfile::US_852,
                                ServiceKind::Game,
                            ) {
                                Ok(selected) => selected,
                                Err(_) => break Err(GameRuntimeError::Protocol),
                            };
                            if selected.channel_id != self.config.channel_id
                                || identity.is_none()
                                || presence.is_none()
                            {
                                break Err(GameRuntimeError::Protocol);
                            }
                            let joined = ChannelJoined {
                                channel_id: selected.channel_id,
                            };
                            let sent = tokio::select! {
                                biased;
                                () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                                sent = self.send(&mut framed, &joined) => sent,
                            };
                            if let Err(error) = sent {
                                break Err(error);
                            }
                            state = GameState::InChannel;
                        }
                        GameState::InChannel | GameState::InRoom | GameState::InMatchLoading | GameState::InMatch => {
                            if matches!(frame.opcode, GameAuth::OPCODE | SelectChannel::OPCODE) {
                                break Err(GameRuntimeError::Protocol);
                            } else if is_known_room_opcode(frame.opcode) {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                if !commands.admit_count(self.config.limits.room_commands_per_window) {
                                    self.observer.rate_limited(GameRateClass::RoomCommandsConnection);
                                    break Err(GameRuntimeError::Limited);
                                }
                                let handled = tokio::select! {
                                    biased;
                                    () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                                    handled = self.handle_room_command(
                                        &mut framed,
                                        state,
                                        established,
                                        outbound.clone(),
                                        room_cancellation.clone(),
                                        &mut chats,
                                        frame.opcode,
                                        &frame.payload,
                                        &mut room_id,
                                    ) => handled,
                                };
                                match handled {
                                    Ok(next) => state = next,
                                    Err(error) => break Err(error),
                                }
                            } else if is_known_solo_opcode(frame.opcode) {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                let handled = self
                                    .handle_solo_command(
                                        &mut framed,
                                        state,
                                        established,
                                        &shutdown,
                                        idle_deadline,
                                        &mut shots,
                                        frame.opcode,
                                        &frame.payload,
                                    )
                                    .await;
                                match handled {
                                    Ok(next) => state = next,
                                    Err(error) => break Err(error),
                                }
                            } else {
                                unknown_strikes = unknown_strikes.saturating_add(1);
                                let decision = unknown_decision(
                                    self.config.unknown_opcode_policy,
                                    unknown_strikes,
                                    self.config.limits.unknown_opcode_strikes,
                                );
                                if decision.capture {
                                    self.captures.push(state, frame.opcode, &frame.payload);
                                }
                                self.observer.unknown(decision.observation);
                                if decision.strike_limit {
                                    self.observer.unknown(GameUnknownObservation::StrikeLimit);
                                }
                                if decision.disconnect {
                                    break Err(GameRuntimeError::Protocol);
                                }
                            }
                        }
                        _ => break Err(GameRuntimeError::Protocol),
                    }
                }
                () = &mut sleeper => break Err(GameRuntimeError::Timeout),
            }
        };

        state = GameState::Closed;
        let cleanup_reason = if shutdown.is_cancelled() {
            MatchAbortReason::Shutdown
        } else {
            MatchAbortReason::Disconnect
        };
        let cleanup = self
            .lobby
            .disconnect_with_reason(connection_id, cleanup_reason)
            .await;
        let cleanup_result = match cleanup {
            Ok(Some(abort)) => self.persist_cleanup_abort(abort).await.map(drop),
            Ok(None) | Err(RoomError::NotMember | RoomError::RoomNotFound) => Ok(()),
            Err(_) => {
                self.observer.queue(GameQueueObservation::LobbyRejected);
                Ok(())
            }
        };
        drop(presence);
        let _terminal_state = state;
        match cleanup_result {
            Err(error) => Err(error),
            Ok(()) => result,
        }
    }

    async fn authenticate(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        source: &SourceAddressPrefix,
        started: Instant,
        connection_id: PlayerConnectionId,
        payload: &[u8],
    ) -> Result<(RegistryGuard<AccountId>, RoomIdentity), GameRuntimeError> {
        let now = Instant::now();
        if self.global_auth.check((), now) != RateDecision::Allowed {
            self.observer.rate_limited(GameRateClass::AuthGlobal);
            return Err(GameRuntimeError::Limited);
        }
        if self.source_auth.check(source.clone(), now) != RateDecision::Allowed {
            self.observer.rate_limited(GameRateClass::AuthSource);
            return Err(GameRuntimeError::Limited);
        }
        let auth = decode_packet_payload::<GameAuth>(
            payload,
            &CompatibilityProfile::US_852,
            ServiceKind::Game,
        )
        .map_err(|_| GameRuntimeError::Protocol)?;
        let claimed = i64::try_from(auth.claimed_account_id)
            .ok()
            .and_then(|value| AccountId::new(value).ok())
            .ok_or_else(|| {
                self.observer.authentication("rejected");
                GameRuntimeError::Authentication
            })?;
        let bearer = std::str::from_utf8(&auth.handover).map_err(|_| {
            self.observer.authentication("rejected");
            GameRuntimeError::Authentication
        })?;
        let parsed = parse_handover(bearer).map_err(|_| {
            self.observer.authentication("rejected");
            GameRuntimeError::Authentication
        })?;
        let session = timeout(
            self.authentication_remaining(started)?,
            self.repository.consume(ConsumeHandover {
                id: parsed.id,
                digest: parsed.digest,
                target: DomainServiceKind::Game,
                now: SystemTime::now(),
            }),
        )
        .await
        .map_err(|_| GameRuntimeError::Timeout)?
        .map_err(|_| {
            self.observer.authentication("rejected");
            GameRuntimeError::Authentication
        })?;
        if session.account_id != claimed {
            self.observer.authentication("identity_mismatch");
            return Err(GameRuntimeError::Authentication);
        }
        let loaded = timeout(
            self.authentication_remaining(started)?,
            self.repository.load_player_snapshot(session.account_id),
        )
        .await
        .map_err(|_| GameRuntimeError::Timeout)?
        .map_err(|error| match error {
            RepositoryError::AccountInactive => {
                self.observer.authentication("rejected");
                GameRuntimeError::Authentication
            }
            _ => GameRuntimeError::Snapshot,
        })?;
        self.catalog
            .validate_snapshot(&loaded)
            .map_err(|_| GameRuntimeError::Catalog)?;
        let nickname = loaded
            .profile
            .nickname
            .as_deref()
            .ok_or(GameRuntimeError::Snapshot)
            .and_then(|value| Nickname::parse(value).map_err(|_| GameRuntimeError::Snapshot))?;
        match self.authentication_remaining(started) {
            Ok(remaining) => timeout(remaining, self.send_bootstrap(framed, &loaded))
                .await
                .map_err(|_| GameRuntimeError::Timeout)
                .and_then(|result| result),
            Err(error) => Err(error),
        }?;
        let guard = self
            .active_accounts
            .acquire(session.account_id)
            .map_err(|error| {
                match error {
                    RegistryError::Duplicate => self.observer.authentication("duplicate"),
                    RegistryError::Capacity => {
                        self.observer.rate_limited(GameRateClass::ConnectionGlobal)
                    }
                }
                GameRuntimeError::Limited
            })?;
        self.observer.authenticated(session.account_id);
        self.observer.authentication("success");
        Ok((
            guard,
            RoomIdentity {
                connection_id,
                account_id: session.account_id,
                nickname,
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_room_command(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        state: GameState,
        identity: &RoomIdentity,
        outbound: mpsc::Sender<RoomEvent>,
        room_cancellation: CancellationToken,
        chats: &mut LocalRateWindow,
        opcode: u16,
        payload: &[u8],
        room_id: &mut Option<RoomId>,
    ) -> Result<GameState, GameRuntimeError> {
        let profile = &CompatibilityProfile::US_852;
        match (state, opcode) {
            (GameState::InChannel, SYNTHETIC_M4_C2S_LIST) => {
                decode_packet_payload::<RoomListRequest>(payload, profile, ServiceKind::Game)
                    .map_err(|_| GameRuntimeError::Protocol)?;
                match self.lobby.list().await {
                    Ok(rooms) => {
                        self.send(framed, &RoomListResponse { rooms }).await?;
                        self.observer.room(GameRoomObservation::Listed);
                    }
                    Err(error) => {
                        self.send_result(framed, RoomCommand::List, Err(error))
                            .await?;
                    }
                }
                Ok(state)
            }
            (GameState::InChannel, SYNTHETIC_M4_C2S_CREATE) => {
                let request =
                    decode_packet_payload::<RoomCreateRequest>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let result = self
                    .lobby
                    .create(
                        request.name,
                        request.password,
                        request.settings,
                        identity.clone(),
                        outbound,
                        room_cancellation.clone(),
                    )
                    .await;
                match result {
                    Ok(summary) => {
                        let initial = match self
                            .lobby
                            .route(identity.connection_id, LobbyRoomCommand::GetState)
                            .await
                        {
                            Ok(LobbyRouteResult::Snapshot(snapshot)) => Ok(snapshot),
                            Ok(LobbyRouteResult::ChatAccepted) => Err(RoomError::Closed),
                            Err(error) => Err(error),
                        };
                        match initial {
                            Ok(snapshot) => {
                                *room_id = Some(summary.id());
                                self.send_result(framed, RoomCommand::Create, Ok(()))
                                    .await?;
                                self.send(framed, &RoomStateResponse { room: snapshot })
                                    .await?;
                                Ok(GameState::InRoom)
                            }
                            Err(error) => {
                                if !matches!(
                                    self.lobby.disconnect(identity.connection_id).await,
                                    Ok(_) | Err(RoomError::NotMember | RoomError::RoomNotFound)
                                ) {
                                    self.observer.queue(GameQueueObservation::LobbyRejected);
                                }
                                self.send_result(framed, RoomCommand::Create, Err(error))
                                    .await?;
                                Ok(GameState::InChannel)
                            }
                        }
                    }
                    Err(error) => {
                        self.send_result(framed, RoomCommand::Create, Err(error))
                            .await?;
                        Ok(state)
                    }
                }
            }
            (GameState::InChannel, SYNTHETIC_M4_C2S_JOIN) => {
                let request =
                    decode_packet_payload::<RoomJoinRequest>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let requested_room = request.room_id;
                let result = self
                    .lobby
                    .join(
                        requested_room,
                        identity.clone(),
                        request.password,
                        outbound,
                        room_cancellation,
                    )
                    .await;
                match result {
                    Ok(snapshot) => {
                        *room_id = Some(requested_room);
                        self.send_result(framed, RoomCommand::Join, Ok(())).await?;
                        self.send(framed, &RoomStateResponse { room: snapshot })
                            .await?;
                        self.observer.room(GameRoomObservation::Joined);
                        Ok(GameState::InRoom)
                    }
                    Err(error) => {
                        self.send_result(framed, RoomCommand::Join, Err(error))
                            .await?;
                        Ok(state)
                    }
                }
            }
            (GameState::InRoom, SYNTHETIC_M4_C2S_LEAVE) => {
                decode_packet_payload::<RoomLeaveRequest>(payload, profile, ServiceKind::Game)
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let result = self.lobby.leave(identity.connection_id).await;
                match result {
                    Ok(_snapshot) => {
                        self.send_result(framed, RoomCommand::Leave, Ok(())).await?;
                        self.observer.room(GameRoomObservation::Left);
                        *room_id = None;
                        Ok(GameState::InChannel)
                    }
                    Err(error) => {
                        self.send_result(framed, RoomCommand::Leave, Err(error))
                            .await?;
                        Ok(state)
                    }
                }
            }
            (GameState::InRoom, SYNTHETIC_M4_C2S_SETTINGS) => {
                let request = decode_packet_payload::<RoomSettingsRequest>(
                    payload,
                    profile,
                    ServiceKind::Game,
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                self.route_snapshot(
                    framed,
                    identity.connection_id,
                    RoomCommand::Settings,
                    LobbyRoomCommand::UpdateSettings(request.settings),
                    GameRoomObservation::SettingsChanged,
                )
                .await?;
                Ok(state)
            }
            (GameState::InRoom, SYNTHETIC_M4_C2S_READY) => {
                let request =
                    decode_packet_payload::<RoomReadyRequest>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                self.route_snapshot(
                    framed,
                    identity.connection_id,
                    RoomCommand::Ready,
                    LobbyRoomCommand::SetReady(request.ready),
                    GameRoomObservation::ReadyChanged,
                )
                .await?;
                Ok(state)
            }
            (GameState::InRoom, SYNTHETIC_M4_C2S_CHAT) => {
                let request =
                    decode_packet_payload::<RoomChatRequest>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                if !chats.admit_count(self.config.limits.chat_messages_per_window) {
                    self.observer.rate_limited(GameRateClass::ChatConnection);
                    self.observer.chat(GameChatObservation::RateLimited);
                    return Err(GameRuntimeError::Limited);
                }
                let result = self
                    .lobby
                    .route(identity.connection_id, LobbyRoomCommand::Chat(request.text))
                    .await;
                match result {
                    Ok(LobbyRouteResult::ChatAccepted) => {
                        self.send_result(framed, RoomCommand::Chat, Ok(())).await?;
                        self.observer.chat(GameChatObservation::Accepted);
                    }
                    Ok(LobbyRouteResult::Snapshot(_)) => return Err(GameRuntimeError::Protocol),
                    Err(error) => {
                        self.send_result(framed, RoomCommand::Chat, Err(error))
                            .await?
                    }
                }
                Ok(state)
            }
            (GameState::InRoom, SYNTHETIC_M4_C2S_KICK) => {
                let request =
                    decode_packet_payload::<RoomKickRequest>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                self.route_snapshot(
                    framed,
                    identity.connection_id,
                    RoomCommand::Kick,
                    LobbyRoomCommand::Kick(request.target),
                    GameRoomObservation::Kicked,
                )
                .await?;
                Ok(state)
            }
            (GameState::InRoom, SYNTHETIC_M4_C2S_STATE) => {
                decode_packet_payload::<RoomStateRequest>(payload, profile, ServiceKind::Game)
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let result = self
                    .lobby
                    .route(identity.connection_id, LobbyRoomCommand::GetState)
                    .await;
                match result {
                    Ok(LobbyRouteResult::Snapshot(snapshot)) => {
                        self.send(framed, &RoomStateResponse { room: snapshot })
                            .await?;
                        self.observer.room(GameRoomObservation::StateSent);
                    }
                    Ok(LobbyRouteResult::ChatAccepted) => return Err(GameRuntimeError::Protocol),
                    Err(error) => {
                        self.send_result(framed, RoomCommand::State, Err(error))
                            .await?
                    }
                }
                Ok(state)
            }
            _ => Err(GameRuntimeError::Protocol),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_solo_command(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        state: GameState,
        identity: &RoomIdentity,
        shutdown: &CancellationToken,
        idle_deadline: Instant,
        shots: &mut LocalRateWindow,
        opcode: u16,
        payload: &[u8],
    ) -> Result<GameState, GameRuntimeError> {
        let solo = self
            .config
            .solo_practice
            .ok_or(GameRuntimeError::Protocol)?;
        let profile = &CompatibilityProfile::US_852;
        match (state, opcode) {
            (GameState::InRoom, SYNTHETIC_M5_C2S_START_SOLO) => {
                decode_packet_payload::<StartSolo>(payload, profile, ServiceKind::Game)
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let match_id = MatchId::new(uuid::Uuid::new_v4());
                let result_key = MatchResultKey::new(uuid::Uuid::new_v4());
                let mut seed_bytes = [0_u8; 32];
                OsRng.fill_bytes(&mut seed_bytes);
                let seed = MatchSeed::new(seed_bytes);
                let (weather, wind) =
                    deterministic_conditions(seed).map_err(|_| GameRuntimeError::InvalidConfig)?;
                let begin = BeginSoloMatch::new(
                    match_id,
                    result_key,
                    identity.account_id,
                    solo.course,
                    solo.catalog_fingerprint,
                    seed,
                    weather,
                    wind,
                );
                let plan = SoloStartPlan::new(begin, solo.loading_timeout, solo.max_strokes)
                    .map_err(|_| GameRuntimeError::InvalidConfig)?;
                let prepared = self
                    .lobby
                    .route_solo(identity.connection_id, LobbySoloCommand::PrepareStart(plan))
                    .await;
                let begin = match prepared {
                    Ok(LobbySoloRouteResult::Begin(begin)) => begin,
                    Ok(_) => return Err(GameRuntimeError::Protocol),
                    Err(error) => {
                        self.send_solo_result(
                            framed,
                            SoloCommand::StartSolo,
                            solo_error_outcome(error),
                        )
                        .await?;
                        return Ok(GameState::InRoom);
                    }
                };
                self.persist_and_confirm_begin(
                    identity.connection_id,
                    begin,
                    shutdown,
                    idle_deadline,
                )
                .await?;
                self.send_solo_result(framed, SoloCommand::StartSolo, SoloCommandOutcome::Success)
                    .await?;
                Ok(GameState::InRoom)
            }
            (GameState::InMatchLoading, SYNTHETIC_M5_C2S_LOADING_COMPLETE) => {
                let loading =
                    decode_packet_payload::<LoadingComplete>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let mark = match self
                    .lobby
                    .route_solo(
                        identity.connection_id,
                        LobbySoloCommand::LoadingComplete(loading),
                    )
                    .await
                {
                    Ok(LobbySoloRouteResult::InGame(mark)) => mark,
                    Ok(_) => return Err(GameRuntimeError::Protocol),
                    Err(error) => {
                        self.send_solo_result(
                            framed,
                            SoloCommand::LoadingComplete,
                            solo_error_outcome(error),
                        )
                        .await?;
                        return Ok(state);
                    }
                };
                self.persist_in_game(identity.connection_id, mark, shutdown, idle_deadline)
                    .await?;
                self.observer
                    .match_event(GameMatchObservation::LoadingComplete);
                self.send_solo_result(
                    framed,
                    SoloCommand::LoadingComplete,
                    SoloCommandOutcome::Success,
                )
                .await?;
                Ok(state)
            }
            (GameState::InMatch, SYNTHETIC_M5_C2S_SHOT_ACTION) => {
                if !shots.admit_count(solo.shot_packets_per_window) {
                    self.observer
                        .rate_limited(GameRateClass::ShotPacketsConnection);
                    self.observer.shot(GameShotObservation::RateLimited);
                    self.send_solo_result(
                        framed,
                        SoloCommand::ShotAction,
                        SoloCommandOutcome::Timeout,
                    )
                    .await?;
                    return Ok(state);
                }
                let action =
                    decode_packet_payload::<ShotAction>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let result = self
                    .lobby
                    .route_solo(identity.connection_id, LobbySoloCommand::ShotAction(action))
                    .await;
                let outcome = observe_relay_result(self.observer.as_ref(), &result);
                self.send_solo_result(framed, SoloCommand::ShotAction, outcome)
                    .await?;
                Ok(state)
            }
            (GameState::InMatch, SYNTHETIC_M5_C2S_SHOT_RESULT) => {
                if !shots.admit_count(solo.shot_packets_per_window) {
                    self.observer
                        .rate_limited(GameRateClass::ShotPacketsConnection);
                    self.observer.shot(GameShotObservation::RateLimited);
                    self.send_solo_result(
                        framed,
                        SoloCommand::ShotResult,
                        SoloCommandOutcome::Timeout,
                    )
                    .await?;
                    return Ok(state);
                }
                let shot_result =
                    decode_packet_payload::<ShotResult>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let result = self
                    .lobby
                    .route_solo(
                        identity.connection_id,
                        LobbySoloCommand::ShotResult(shot_result),
                    )
                    .await;
                let outcome = observe_relay_result(self.observer.as_ref(), &result);
                self.send_solo_result(framed, SoloCommand::ShotResult, outcome)
                    .await?;
                Ok(state)
            }
            (GameState::InMatch, SYNTHETIC_M5_C2S_FINISH_HOLE) => {
                decode_packet_payload::<FinishHole>(payload, profile, ServiceKind::Game)
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let commit = match self
                    .lobby
                    .route_solo(identity.connection_id, LobbySoloCommand::PrepareFinish)
                    .await
                {
                    Ok(LobbySoloRouteResult::Commit(commit)) => commit,
                    Ok(_) => return Err(GameRuntimeError::Protocol),
                    Err(error) => {
                        self.send_solo_result(
                            framed,
                            SoloCommand::FinishHole,
                            solo_error_outcome(error),
                        )
                        .await?;
                        return Ok(state);
                    }
                };
                self.persist_and_apply_commit(
                    identity.connection_id,
                    commit,
                    shutdown,
                    idle_deadline,
                )
                .await?;
                Ok(state)
            }
            _ => Err(GameRuntimeError::Protocol),
        }
    }

    async fn persist_and_confirm_begin(
        &self,
        connection_id: PlayerConnectionId,
        begin: BeginSoloMatch,
        shutdown: &CancellationToken,
        idle_deadline: Instant,
    ) -> Result<(), GameRuntimeError> {
        let solo = self
            .config
            .solo_practice
            .ok_or(GameRuntimeError::InvalidConfig)?;
        let cancelled_before = shutdown.is_cancelled() || Instant::now() >= idle_deadline;
        let persisted = if cancelled_before {
            self.observer.commit(GameCommitObservation::Cancelled);
            None
        } else {
            match timeout(
                solo.commit_timeout,
                self.repository.begin_solo(begin.clone()),
            )
            .await
            {
                Ok(Ok(outcome)) => Some(outcome),
                Ok(Err(_)) => {
                    self.observer.commit(GameCommitObservation::Failed);
                    None
                }
                Err(_) => {
                    self.observer.commit(GameCommitObservation::Cancelled);
                    None
                }
            }
        };
        let caller_cancelled = shutdown.is_cancelled() || Instant::now() >= idle_deadline;
        let Some(persisted) = persisted else {
            self.abort_actor_match(connection_id, MatchAbortReason::PersistenceFailure, false)
                .await?;
            return Err(if cancelled_before {
                GameRuntimeError::Timeout
            } else {
                GameRuntimeError::MatchPersistence
            });
        };
        self.observer.commit(match persisted {
            BeginSoloMatchOutcome::Begun => GameCommitObservation::Begun,
            BeginSoloMatchOutcome::Existing => GameCommitObservation::Existing,
        });
        if caller_cancelled {
            self.abort_actor_match(connection_id, MatchAbortReason::PersistenceFailure, false)
                .await?;
            return Err(GameRuntimeError::Timeout);
        }
        let confirmation = self
            .lobby
            .route_solo(
                connection_id,
                LobbySoloCommand::ConfirmBegin {
                    match_id: begin.match_id(),
                    result_key: begin.result_key(),
                },
            )
            .await;
        self.resolve_persisted_begin(connection_id, confirmation)
            .await
    }

    async fn resolve_persisted_begin(
        &self,
        connection_id: PlayerConnectionId,
        confirmation: Result<LobbySoloRouteResult, SoloMatchError>,
    ) -> Result<(), GameRuntimeError> {
        if matches!(confirmation, Ok(LobbySoloRouteResult::Applied)) {
            return Ok(());
        }
        self.abort_actor_match(connection_id, MatchAbortReason::PersistenceFailure, false)
            .await?;
        Err(GameRuntimeError::MatchPersistence)
    }

    async fn persist_in_game(
        &self,
        connection_id: PlayerConnectionId,
        mark: MarkSoloInGame,
        shutdown: &CancellationToken,
        idle_deadline: Instant,
    ) -> Result<(), GameRuntimeError> {
        let solo = self
            .config
            .solo_practice
            .ok_or(GameRuntimeError::InvalidConfig)?;
        let cancelled_before = shutdown.is_cancelled() || Instant::now() >= idle_deadline;
        let marked = if cancelled_before {
            None
        } else {
            match timeout(solo.commit_timeout, self.repository.mark_solo_in_game(mark)).await {
                Ok(Ok(MarkSoloInGameOutcome::Marked | MarkSoloInGameOutcome::Existing)) => Some(()),
                Ok(Err(_)) | Err(_) => None,
            }
        };
        let cancelled_after = shutdown.is_cancelled() || Instant::now() >= idle_deadline;
        if marked.is_some() && !cancelled_after {
            return Ok(());
        }
        let reason = if shutdown.is_cancelled() {
            MatchAbortReason::Shutdown
        } else {
            MatchAbortReason::PersistenceFailure
        };
        self.abort_actor_match(connection_id, reason, true).await?;
        Err(if cancelled_before || cancelled_after {
            GameRuntimeError::Timeout
        } else {
            GameRuntimeError::MatchPersistence
        })
    }

    async fn persist_and_apply_commit(
        &self,
        connection_id: PlayerConnectionId,
        commit: pangya_domain::CommitSoloHole,
        shutdown: &CancellationToken,
        idle_deadline: Instant,
    ) -> Result<(), GameRuntimeError> {
        let solo = self
            .config
            .solo_practice
            .ok_or(GameRuntimeError::InvalidConfig)?;
        let committed = if shutdown.is_cancelled() || Instant::now() >= idle_deadline {
            self.observer.commit(GameCommitObservation::Cancelled);
            None
        } else {
            match timeout(
                solo.commit_timeout,
                self.repository.commit_solo_hole(commit),
            )
            .await
            {
                Ok(Ok(committed)) => Some(committed),
                Ok(Err(_)) => {
                    self.observer.commit(GameCommitObservation::Failed);
                    None
                }
                Err(_) => {
                    self.observer.commit(GameCommitObservation::Cancelled);
                    None
                }
            }
        };
        let Some(committed) = committed else {
            self.abort_actor_match(connection_id, MatchAbortReason::PersistenceFailure, true)
                .await?;
            return Err(GameRuntimeError::MatchPersistence);
        };
        self.observer.commit(GameCommitObservation::Committed);
        match self
            .lobby
            .route_solo(connection_id, LobbySoloCommand::ApplyCommit(committed))
            .await
        {
            Ok(LobbySoloRouteResult::Committed(_)) => {
                self.observer.match_event(GameMatchObservation::Finished);
                Ok(())
            }
            _ => {
                self.observer.commit(GameCommitObservation::Failed);
                self.abort_actor_match(connection_id, MatchAbortReason::PersistenceFailure, true)
                    .await?;
                Err(GameRuntimeError::MatchPersistence)
            }
        }
    }

    async fn send_solo_result(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        command: SoloCommand,
        outcome: SoloCommandOutcome,
    ) -> Result<(), GameRuntimeError> {
        self.send(framed, &SoloCommandResult::new(command, outcome))
            .await
    }

    async fn route_snapshot(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        connection_id: PlayerConnectionId,
        command: RoomCommand,
        route: LobbyRoomCommand,
        observation: GameRoomObservation,
    ) -> Result<(), GameRuntimeError> {
        let result = self.lobby.route(connection_id, route).await;
        match result {
            Ok(LobbyRouteResult::Snapshot(snapshot)) => {
                self.send_result(framed, command, Ok(())).await?;
                self.send(framed, &RoomStateResponse { room: snapshot })
                    .await?;
                self.observer.room(observation);
            }
            Ok(LobbyRouteResult::ChatAccepted) => return Err(GameRuntimeError::Protocol),
            Err(error) => self.send_result(framed, command, Err(error)).await?,
        }
        Ok(())
    }

    async fn handle_room_event(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        event: RoomEvent,
        room_id: Option<RoomId>,
        connection_id: PlayerConnectionId,
    ) -> Result<RoomEventEffect, GameRuntimeError> {
        match event {
            RoomEvent::Snapshot(room) => {
                self.send(framed, &RoomStateResponse { room }).await?;
                self.observer.room(GameRoomObservation::StateSent);
                Ok(RoomEventEffect::Remain)
            }
            RoomEvent::Chat { from, text } => {
                let room_id = room_id.ok_or(GameRuntimeError::Protocol)?;
                self.send(
                    framed,
                    &RoomChatEvent {
                        room_id,
                        sender: from,
                        text,
                    },
                )
                .await?;
                self.observer.chat(GameChatObservation::Delivered);
                Ok(RoomEventEffect::Remain)
            }
            RoomEvent::Kicked { by } => {
                let room_id = room_id.ok_or(GameRuntimeError::Protocol)?;
                self.send(
                    framed,
                    &RoomMembershipEvent {
                        room_id,
                        kind: RoomMembershipKind::Kicked,
                        member: by,
                    },
                )
                .await?;
                self.observer.room(GameRoomObservation::Kicked);
                Ok(RoomEventEffect::EnterChannel)
            }
            RoomEvent::Closed => {
                self.send_result(framed, RoomCommand::State, Err(RoomError::Closed))
                    .await?;
                Ok(RoomEventEffect::EnterChannel)
            }
            RoomEvent::SoloStarted(plan) => {
                let begin = plan.begin();
                let weather = match begin.weather() {
                    pangya_domain::Weather::Clear => ProtocolWeather::Clear,
                    pangya_domain::Weather::Cloudy => ProtocolWeather::Cloudy,
                    pangya_domain::Weather::Rain => ProtocolWeather::Rain,
                };
                let wind = Wind::new(
                    f32::from(begin.wind().speed_tenths()) / 10.0,
                    f32::from(begin.wind().angle_degrees()),
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                let timeout_ms = u32::try_from(plan.loading_timeout().as_millis())
                    .map_err(|_| GameRuntimeError::InvalidConfig)?;
                self.send(
                    framed,
                    &MatchStarted::new(
                        begin.match_id().get(),
                        begin.config().course_id().get(),
                        begin.config().par(),
                        *begin.seed().as_bytes(),
                        weather,
                        wind,
                        timeout_ms,
                    )
                    .map_err(|_| GameRuntimeError::Protocol)?,
                )
                .await?;
                Ok(RoomEventEffect::EnterLoading)
            }
            RoomEvent::SoloPhase { match_id, phase } => {
                let protocol_phase = match phase {
                    SoloMatchPhase::Loading => Some(SoloPhase::Loading),
                    SoloMatchPhase::AwaitAction { .. } | SoloMatchPhase::AwaitResult { .. } => {
                        Some(SoloPhase::Playing)
                    }
                    SoloMatchPhase::HoleComplete | SoloMatchPhase::ResultsPendingCommit => {
                        Some(SoloPhase::HoleComplete)
                    }
                    SoloMatchPhase::Open | SoloMatchPhase::Starting | SoloMatchPhase::Aborted => {
                        None
                    }
                };
                if let Some(protocol_phase) = protocol_phase {
                    self.send(framed, &MatchPhase::new(match_id.get(), protocol_phase))
                        .await?;
                }
                if matches!(
                    phase,
                    SoloMatchPhase::AwaitAction { .. } | SoloMatchPhase::AwaitResult { .. }
                ) {
                    Ok(RoomEventEffect::EnterMatch)
                } else {
                    Ok(RoomEventEffect::Remain)
                }
            }
            RoomEvent::SoloActionRelay { from, action } => {
                let relay = ShotActionRelay::new(from.get(), action)
                    .map_err(|_| GameRuntimeError::Protocol)?;
                self.send(framed, &relay).await?;
                Ok(RoomEventEffect::Remain)
            }
            RoomEvent::SoloResultRelay { from, result } => {
                let relay = ShotResultRelay::new(from.get(), result)
                    .map_err(|_| GameRuntimeError::Protocol)?;
                self.send(framed, &relay).await?;
                Ok(RoomEventEffect::Remain)
            }
            RoomEvent::AbortRequested(abort) => {
                match self.persist_actor_abort(connection_id, abort, true).await? {
                    AbortResolution::Aborted => {
                        self.send(
                            framed,
                            &MatchAborted::new(
                                abort.match_id().get(),
                                protocol_abort_reason(abort.reason()),
                            ),
                        )
                        .await?;
                        Ok(RoomEventEffect::EnterRoom)
                    }
                    AbortResolution::Committed => Ok(RoomEventEffect::Remain),
                }
            }
            RoomEvent::SoloCommitted(result) => {
                self.send_committed_result(framed, result).await?;
                Ok(RoomEventEffect::EnterRoom)
            }
            // M6 network/repository composition is deliberately deferred to the next checkpoint.
            RoomEvent::StrokeStarted(_)
            | RoomEvent::StrokePhase { .. }
            | RoomEvent::StrokeTurn(_)
            | RoomEvent::StrokeActionRelay { .. }
            | RoomEvent::StrokeResultRelay { .. }
            | RoomEvent::StrokeSettlementRequested(_)
            | RoomEvent::StrokeAbortRequested(_)
            | RoomEvent::StrokeCommitted(_)
            | RoomEvent::StrokeAborted(_) => Ok(RoomEventEffect::Remain),
        }
    }

    async fn send_committed_result(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        result: SoloMatchResult,
    ) -> Result<(), GameRuntimeError> {
        self.send(
            framed,
            &HoleResult::new(
                result.match_id().get(),
                result.strokes().get(),
                result.score(),
                result.pang_reward(),
                result.experience_reward(),
                result.result_key().get(),
            )
            .map_err(|_| GameRuntimeError::Protocol)?,
        )
        .await?;
        self.send(
            framed,
            &BalanceUpdate::new(result.pang_balance(), result.experience_balance()),
        )
        .await?;
        self.send_solo_result(framed, SoloCommand::FinishHole, SoloCommandOutcome::Success)
            .await?;
        self.send(
            framed,
            &MatchPhase::new(result.match_id().get(), SoloPhase::Finished),
        )
        .await
    }

    async fn abort_actor_match(
        &self,
        connection_id: PlayerConnectionId,
        reason: MatchAbortReason,
        classify_terminal: bool,
    ) -> Result<AbortResolution, GameRuntimeError> {
        let routed = self
            .lobby
            .route_solo(connection_id, LobbySoloCommand::Abort(reason))
            .await
            .map_err(|_| GameRuntimeError::MatchPersistence)?;
        let LobbySoloRouteResult::Abort(Some(abort)) = routed else {
            return Err(GameRuntimeError::MatchPersistence);
        };
        self.persist_actor_abort(connection_id, abort, classify_terminal)
            .await
    }

    async fn persist_actor_abort(
        &self,
        connection_id: PlayerConnectionId,
        abort: AbortMatch,
        classify_terminal: bool,
    ) -> Result<AbortResolution, GameRuntimeError> {
        let Some(solo) = self.config.solo_practice else {
            return Err(GameRuntimeError::InvalidConfig);
        };
        let outcome = timeout(solo.commit_timeout, self.repository.abort(abort))
            .await
            .map_err(|_| GameRuntimeError::MatchPersistence)?;
        match outcome {
            Ok(AbortMatchOutcome::Aborted | AbortMatchOutcome::AlreadyAborted)
            | Err(pangya_domain::MatchRepositoryError::NotFound) => {
                self.lobby
                    .route_solo(connection_id, LobbySoloCommand::AcknowledgeAbort(abort))
                    .await
                    .map_err(|_| GameRuntimeError::MatchPersistence)?;
                if classify_terminal {
                    observe_abort_terminal(self.observer.as_ref(), abort.reason());
                }
                Ok(AbortResolution::Aborted)
            }
            Ok(AbortMatchOutcome::AlreadyCommitted(committed)) => {
                self.observer.commit(GameCommitObservation::Idempotent);
                self.lobby
                    .route_solo(connection_id, LobbySoloCommand::ApplyCommit(committed))
                    .await
                    .map_err(|_| GameRuntimeError::MatchPersistence)?;
                if classify_terminal {
                    self.observer.match_event(GameMatchObservation::Finished);
                }
                Ok(AbortResolution::Committed)
            }
            Err(_) => Err(GameRuntimeError::MatchPersistence),
        }
    }

    async fn persist_cleanup_abort(
        &self,
        abort: AbortMatch,
    ) -> Result<AbortResolution, GameRuntimeError> {
        let Some(solo) = self.config.solo_practice else {
            return Err(GameRuntimeError::InvalidConfig);
        };
        let outcome = timeout(solo.commit_timeout, self.repository.abort(abort))
            .await
            .map_err(|_| GameRuntimeError::MatchPersistence)?;
        match outcome {
            Ok(AbortMatchOutcome::Aborted | AbortMatchOutcome::AlreadyAborted)
            | Err(pangya_domain::MatchRepositoryError::NotFound) => {
                observe_abort_terminal(self.observer.as_ref(), abort.reason());
                Ok(AbortResolution::Aborted)
            }
            Ok(AbortMatchOutcome::AlreadyCommitted(_)) => {
                self.observer.commit(GameCommitObservation::Idempotent);
                self.observer.match_event(GameMatchObservation::Finished);
                Ok(AbortResolution::Committed)
            }
            Err(_) => Err(GameRuntimeError::MatchPersistence),
        }
    }

    async fn persist_shutdown_abort(
        &self,
        abort: AbortMatch,
    ) -> Result<AbortResolution, GameRuntimeError> {
        self.persist_cleanup_abort(abort).await
    }

    async fn send_result(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        command: RoomCommand,
        result: Result<(), RoomError>,
    ) -> Result<(), GameRuntimeError> {
        self.send(
            framed,
            &RoomCommandResultResponse {
                command,
                result: result.map_or_else(room_error_result, |()| RoomCommandResult::Success),
            },
        )
        .await
    }

    fn authentication_remaining(&self, started: Instant) -> Result<Duration, GameRuntimeError> {
        self.config
            .limits
            .authentication_timeout
            .checked_sub(started.elapsed())
            .ok_or(GameRuntimeError::Timeout)
    }

    fn admit_packet(
        &self,
        source: &SourceAddressPrefix,
        local: &mut LocalRateWindow,
        bytes: usize,
    ) -> Result<(), GameRuntimeError> {
        let now = Instant::now();
        let weight = u64::try_from(bytes).map_or(u64::MAX, |value| value);
        for (decision, class) in [
            (
                self.global_packets.check((), now),
                GameRateClass::PacketGlobal,
            ),
            (
                self.source_packets.check(source.clone(), now),
                GameRateClass::PacketSource,
            ),
            (
                self.global_bytes.check_weighted((), now, weight),
                GameRateClass::BytesGlobal,
            ),
            (
                self.source_bytes
                    .check_weighted(source.clone(), now, weight),
                GameRateClass::BytesSource,
            ),
        ] {
            if decision != RateDecision::Allowed {
                self.observer.rate_limited(class);
                return Err(GameRuntimeError::Limited);
            }
        }
        if !local.admit(
            bytes,
            self.config.limits.packets_per_window,
            self.config.limits.bytes_per_window,
        ) {
            self.observer
                .rate_limited(GameRateClass::PacketOrBytesConnection);
            return Err(GameRuntimeError::Limited);
        }
        Ok(())
    }

    async fn send_bootstrap(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        snapshot: &PlayerSnapshot,
    ) -> Result<(), GameRuntimeError> {
        let account_id =
            u64::try_from(snapshot.account.id.get()).map_err(|_| GameRuntimeError::Snapshot)?;
        self.send(
            framed,
            &PlayerInfo {
                account_id,
                nickname: snapshot
                    .profile
                    .nickname
                    .as_deref()
                    .ok_or(GameRuntimeError::Snapshot)?
                    .as_bytes()
                    .to_vec(),
                pang: snapshot.profile.pang,
                points: snapshot.profile.points,
                experience: snapshot.profile.experience,
            },
        )
        .await?;
        let characters = snapshot
            .characters
            .iter()
            .map(|character| {
                Ok(CharacterBootstrap {
                    id: u64::try_from(character.id.get())
                        .map_err(|_| GameRuntimeError::Snapshot)?,
                    type_id: character.item_type_id.get(),
                })
            })
            .collect::<Result<Vec<_>, GameRuntimeError>>()?;
        self.send(framed, &CharacterInfo { characters }).await?;
        let segment_count = u16::try_from(
            snapshot
                .inventory
                .len()
                .div_ceil(GAME_INVENTORY_SEGMENT_ITEMS),
        )
        .map_err(|_| GameRuntimeError::Snapshot)?;
        for (index, items) in snapshot
            .inventory
            .chunks(GAME_INVENTORY_SEGMENT_ITEMS)
            .enumerate()
        {
            let items = items
                .iter()
                .map(|item| {
                    Ok(InventoryBootstrap {
                        id: u64::try_from(item.id.get()).map_err(|_| GameRuntimeError::Snapshot)?,
                        type_id: item.item_type_id.get(),
                        quantity: item.quantity,
                    })
                })
                .collect::<Result<Vec<_>, GameRuntimeError>>()?;
            self.send(
                framed,
                &InventorySegment {
                    segment_index: u16::try_from(index).map_err(|_| GameRuntimeError::Snapshot)?,
                    segment_count,
                    items,
                },
            )
            .await?;
        }
        self.send(
            framed,
            &EquipmentInfo {
                character_id: u64::try_from(snapshot.equipment.character_id.get())
                    .map_err(|_| GameRuntimeError::Snapshot)?,
                club_item_id: snapshot
                    .equipment
                    .club_item_id
                    .map(|id| u64::try_from(id.get()))
                    .transpose()
                    .map_err(|_| GameRuntimeError::Snapshot)?
                    .unwrap_or(0),
                ball_item_id: snapshot
                    .equipment
                    .ball_item_id
                    .map(|id| u64::try_from(id.get()))
                    .transpose()
                    .map_err(|_| GameRuntimeError::Snapshot)?
                    .unwrap_or(0),
                version: snapshot.equipment.version,
            },
        )
        .await
    }

    async fn send<T: EncodePacket>(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        packet: &T,
    ) -> Result<(), GameRuntimeError> {
        let payload = encode_packet_payload(packet, &CompatibilityProfile::US_852)
            .map_err(|_| GameRuntimeError::Protocol)?;
        let bytes = payload.len().saturating_add(2);
        timeout(
            self.config.limits.command_timeout,
            framed.send(OutboundFrame {
                opcode: T::OPCODE,
                payload,
                salt: OsRng.next_u32() as u8,
            }),
        )
        .await
        .map_err(|_| GameRuntimeError::Timeout)?
        .map_err(|error| match error {
            PacketEncodeError::Io(_) => GameRuntimeError::Io,
            _ => GameRuntimeError::Protocol,
        })?;
        self.observer.frame("out", T::OPCODE, bytes);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RoomEventEffect {
    Remain,
    EnterChannel,
    EnterRoom,
    EnterLoading,
    EnterMatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UnknownDecision {
    observation: GameUnknownObservation,
    capture: bool,
    disconnect: bool,
    strike_limit: bool,
}

fn unknown_decision(
    policy: UnknownOpcodePolicy,
    strikes: u32,
    strike_limit: u32,
) -> UnknownDecision {
    let limit_reached = strikes >= strike_limit;
    match policy {
        UnknownOpcodePolicy::Disconnect => UnknownDecision {
            observation: GameUnknownObservation::Disconnected,
            capture: false,
            disconnect: true,
            strike_limit: false,
        },
        UnknownOpcodePolicy::Ignore => UnknownDecision {
            observation: GameUnknownObservation::Ignored,
            capture: false,
            disconnect: limit_reached,
            strike_limit: limit_reached,
        },
        UnknownOpcodePolicy::Capture => UnknownDecision {
            observation: GameUnknownObservation::Captured,
            capture: true,
            disconnect: limit_reached,
            strike_limit: limit_reached,
        },
    }
}

fn is_known_solo_opcode(opcode: u16) -> bool {
    matches!(
        opcode,
        SYNTHETIC_M5_C2S_START_SOLO
            | SYNTHETIC_M5_C2S_LOADING_COMPLETE
            | SYNTHETIC_M5_C2S_SHOT_ACTION
            | SYNTHETIC_M5_C2S_SHOT_RESULT
            | SYNTHETIC_M5_C2S_FINISH_HOLE
    )
}

fn solo_error_outcome(error: SoloMatchError) -> SoloCommandOutcome {
    match error {
        SoloMatchError::InvalidSequence | SoloMatchError::ConflictingReplay => {
            SoloCommandOutcome::InvalidSequence
        }
        SoloMatchError::Timeout | SoloMatchError::QueueFull | SoloMatchError::Closed => {
            SoloCommandOutcome::Timeout
        }
        SoloMatchError::DeterministicConditionsInvariant
        | SoloMatchError::InvalidPlan
        | SoloMatchError::InvalidPhase
        | SoloMatchError::IdentityMismatch
        | SoloMatchError::AccountMismatch
        | SoloMatchError::InvalidProgress
        | SoloMatchError::InvalidStrokes
        | SoloMatchError::NotMember
        | SoloMatchError::NotOwner
        | SoloMatchError::NotSolo => SoloCommandOutcome::InvalidPhase,
    }
}

fn observe_relay_result(
    observer: &dyn GameObserver,
    result: &Result<LobbySoloRouteResult, SoloMatchError>,
) -> SoloCommandOutcome {
    match result {
        Ok(LobbySoloRouteResult::Relay(RelayDisposition::Accepted)) => {
            observer.shot(GameShotObservation::Accepted);
            SoloCommandOutcome::Success
        }
        Ok(LobbySoloRouteResult::Relay(RelayDisposition::Duplicate)) => {
            observer.shot(GameShotObservation::Duplicate);
            SoloCommandOutcome::Success
        }
        Ok(_) => {
            observer.shot(GameShotObservation::Rejected);
            SoloCommandOutcome::InvalidPhase
        }
        Err(error) => {
            observer.shot(GameShotObservation::Rejected);
            solo_error_outcome(*error)
        }
    }
}

const fn protocol_abort_reason(reason: MatchAbortReason) -> ProtocolMatchAbortReason {
    match reason {
        MatchAbortReason::Disconnect => ProtocolMatchAbortReason::PlayerDisconnected,
        MatchAbortReason::LoadingTimeout => ProtocolMatchAbortReason::LoadingTimeout,
        MatchAbortReason::Shutdown
        | MatchAbortReason::StartupRecovery
        | MatchAbortReason::PersistenceFailure => ProtocolMatchAbortReason::ServerShutdown,
    }
}

fn is_known_room_opcode(opcode: u16) -> bool {
    matches!(
        opcode,
        SYNTHETIC_M4_C2S_LIST
            | SYNTHETIC_M4_C2S_CREATE
            | SYNTHETIC_M4_C2S_JOIN
            | SYNTHETIC_M4_C2S_LEAVE
            | SYNTHETIC_M4_C2S_SETTINGS
            | SYNTHETIC_M4_C2S_READY
            | SYNTHETIC_M4_C2S_CHAT
            | SYNTHETIC_M4_C2S_KICK
            | SYNTHETIC_M4_C2S_STATE
    )
}

fn joined_match_persistence(
    joined: Option<Result<Result<(), GameRuntimeError>, tokio::task::JoinError>>,
) -> bool {
    matches!(joined, Some(Ok(Err(GameRuntimeError::MatchPersistence))))
}

fn observe_room_lifecycle(observer: &dyn GameObserver, lifecycle: lobby::RoomLifecycle) {
    let event = match lifecycle.event {
        lobby::RoomLifecycleEvent::Created => GameRoomObservation::Created,
        lobby::RoomLifecycleEvent::Closed => GameRoomObservation::Closed,
    };
    observer.room(event);
    observer.rooms_active(lifecycle.active_count);
}

fn drain_room_lifecycle(
    observer: &dyn GameObserver,
    lifecycle: &mut broadcast::Receiver<lobby::RoomLifecycle>,
) -> bool {
    loop {
        match lifecycle.try_recv() {
            Ok(lifecycle) => observe_room_lifecycle(observer, lifecycle),
            Err(broadcast::error::TryRecvError::Lagged(_)) => {}
            Err(broadcast::error::TryRecvError::Empty) => return true,
            Err(broadcast::error::TryRecvError::Closed) => return false,
        }
    }
}

fn observe_match_lifecycle(observer: &dyn GameObserver, lifecycle: lobby::MatchLifecycle) {
    if lifecycle.event == lobby::MatchLifecycleEvent::Activated {
        observer.match_event(GameMatchObservation::Started);
    }
    observer.matches_active(lifecycle.active_count);
}

fn observe_abort_terminal(observer: &dyn GameObserver, reason: MatchAbortReason) {
    let event = if reason == MatchAbortReason::LoadingTimeout {
        GameMatchObservation::LoadingTimeout
    } else {
        GameMatchObservation::Aborted
    };
    observer.match_event(event);
}

fn drain_match_lifecycle(
    observer: &dyn GameObserver,
    lifecycle: &mut broadcast::Receiver<lobby::MatchLifecycle>,
) -> bool {
    loop {
        match lifecycle.try_recv() {
            Ok(lifecycle) => observe_match_lifecycle(observer, lifecycle),
            Err(broadcast::error::TryRecvError::Lagged(_)) => {}
            Err(broadcast::error::TryRecvError::Empty) => return true,
            Err(broadcast::error::TryRecvError::Closed) => return false,
        }
    }
}

fn room_error_result(error: RoomError) -> RoomCommandResult {
    match error {
        RoomError::QueueFull => RoomCommandResult::QueueFull,
        RoomError::Closed => RoomCommandResult::Closed,
        RoomError::AlreadyMember => RoomCommandResult::AlreadyMember,
        RoomError::Full => RoomCommandResult::Full,
        RoomError::InvalidPassword => RoomCommandResult::InvalidPassword,
        RoomError::NotMember => RoomCommandResult::NotMember,
        RoomError::NotOwner => RoomCommandResult::NotOwner,
        RoomError::CannotKickSelf => RoomCommandResult::CannotKickSelf,
        RoomError::MemberNotFound => RoomCommandResult::MemberNotFound,
        RoomError::CapacityBelowOccupancy => RoomCommandResult::CapacityBelowOccupancy,
        RoomError::MaxRooms => RoomCommandResult::MaxRooms,
        RoomError::RoomNotFound => RoomCommandResult::RoomNotFound,
        RoomError::IdExhausted => RoomCommandResult::IdExhausted,
        RoomError::Timeout => RoomCommandResult::Timeout,
        // M4 has no match-active discriminator; M5 network mapping is intentionally deferred.
        RoomError::MatchActive => RoomCommandResult::Closed,
    }
}

#[derive(Debug)]
struct LocalRateWindow {
    started: Instant,
    packets: u32,
    bytes: u64,
    interval: Duration,
}

impl LocalRateWindow {
    fn new(interval: Duration) -> Self {
        Self {
            started: Instant::now(),
            packets: 0,
            bytes: 0,
            interval,
        }
    }

    fn reset_if_elapsed(&mut self) {
        let now = Instant::now();
        if now.saturating_duration_since(self.started) >= self.interval {
            self.started = now;
            self.packets = 0;
            self.bytes = 0;
        }
    }

    fn admit_count(&mut self, limit: u32) -> bool {
        self.reset_if_elapsed();
        self.packets = self.packets.saturating_add(1);
        self.packets <= limit
    }

    fn admit(&mut self, bytes: usize, packet_limit: u32, byte_limit: u64) -> bool {
        self.reset_if_elapsed();
        self.packets = self.packets.saturating_add(1);
        self.bytes = self
            .bytes
            .saturating_add(u64::try_from(bytes).map_or(u64::MAX, |value| value));
        self.packets <= packet_limit && self.bytes <= byte_limit
    }
}

/// Marker retained for the crate-boundary test.
#[must_use]
pub const fn crate_boundary() -> &'static str {
    "game"
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use pangya_domain::{
        AuthenticatedSession, HandoverError, IncompleteMatchAbortLimit, MatchRepositoryError,
        NewHandover, RepositoryFuture,
    };

    use super::*;

    struct FakeRepository {
        begin_calls: AtomicUsize,
        mark_calls: AtomicUsize,
        commit_calls: AtomicUsize,
        abort_calls: AtomicUsize,
        begin_delay: Mutex<Duration>,
        mark_delay: Mutex<Duration>,
        commit_delay: Mutex<Duration>,
        abort_delay: Mutex<Duration>,
        begin_outcome: Mutex<Result<BeginSoloMatchOutcome, MatchRepositoryError>>,
        mark_outcome: Mutex<Result<MarkSoloInGameOutcome, MatchRepositoryError>>,
        commit_outcome: Mutex<Result<SoloMatchResult, MatchRepositoryError>>,
        abort_outcome: Mutex<Result<AbortMatchOutcome, MatchRepositoryError>>,
    }

    #[derive(Default)]
    struct RecordingObserver {
        active: Mutex<Vec<usize>>,
        events: Mutex<Vec<GameMatchObservation>>,
        commits: Mutex<Vec<GameCommitObservation>>,
    }

    impl GameObserver for RecordingObserver {
        fn matches_active(&self, active: usize) {
            if let Ok(mut values) = self.active.lock() {
                values.push(active);
            }
        }

        fn match_event(&self, event: GameMatchObservation) {
            if let Ok(mut values) = self.events.lock() {
                values.push(event);
            }
        }

        fn commit(&self, outcome: GameCommitObservation) {
            if let Ok(mut values) = self.commits.lock() {
                values.push(outcome);
            }
        }
    }

    impl Default for FakeRepository {
        fn default() -> Self {
            Self {
                begin_calls: AtomicUsize::new(0),
                mark_calls: AtomicUsize::new(0),
                commit_calls: AtomicUsize::new(0),
                abort_calls: AtomicUsize::new(0),
                begin_delay: Mutex::new(Duration::ZERO),
                mark_delay: Mutex::new(Duration::ZERO),
                commit_delay: Mutex::new(Duration::ZERO),
                abort_delay: Mutex::new(Duration::ZERO),
                begin_outcome: Mutex::new(Ok(BeginSoloMatchOutcome::Begun)),
                mark_outcome: Mutex::new(Ok(MarkSoloInGameOutcome::Marked)),
                commit_outcome: Mutex::new(Err(MatchRepositoryError::Storage)),
                abort_outcome: Mutex::new(Ok(AbortMatchOutcome::Aborted)),
            }
        }
    }

    impl HandoverRepository for FakeRepository {
        fn issue(&self, _handover: NewHandover) -> RepositoryFuture<'_, Result<(), HandoverError>> {
            Box::pin(async { Err(HandoverError::Storage) })
        }

        fn consume(
            &self,
            _request: ConsumeHandover,
        ) -> RepositoryFuture<'_, Result<AuthenticatedSession, HandoverError>> {
            Box::pin(async { Err(HandoverError::Storage) })
        }
    }

    impl PlayerRepository for FakeRepository {
        fn load_player_snapshot(
            &self,
            _account_id: AccountId,
        ) -> RepositoryFuture<'_, Result<PlayerSnapshot, RepositoryError>> {
            Box::pin(async { Err(RepositoryError::Storage) })
        }
    }

    impl MatchRepository for FakeRepository {
        fn begin_solo(
            &self,
            _request: BeginSoloMatch,
        ) -> RepositoryFuture<'_, Result<BeginSoloMatchOutcome, MatchRepositoryError>> {
            self.begin_calls.fetch_add(1, Ordering::Relaxed);
            let delay = self
                .begin_delay
                .lock()
                .map_or(Duration::ZERO, |value| *value);
            let outcome = self
                .begin_outcome
                .lock()
                .map_or(Err(MatchRepositoryError::Storage), |value| *value);
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                outcome
            })
        }

        fn mark_solo_in_game(
            &self,
            _request: MarkSoloInGame,
        ) -> RepositoryFuture<'_, Result<MarkSoloInGameOutcome, MatchRepositoryError>> {
            self.mark_calls.fetch_add(1, Ordering::Relaxed);
            let delay = self
                .mark_delay
                .lock()
                .map_or(Duration::ZERO, |value| *value);
            let outcome = self
                .mark_outcome
                .lock()
                .map_or(Err(MatchRepositoryError::Storage), |value| *value);
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                outcome
            })
        }

        fn abort(
            &self,
            _request: AbortMatch,
        ) -> RepositoryFuture<'_, Result<AbortMatchOutcome, MatchRepositoryError>> {
            self.abort_calls.fetch_add(1, Ordering::Relaxed);
            let delay = self
                .abort_delay
                .lock()
                .map_or(Duration::ZERO, |value| *value);
            let outcome = self
                .abort_outcome
                .lock()
                .map_or(Err(MatchRepositoryError::Storage), |value| *value);
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                outcome
            })
        }

        fn commit_solo_hole(
            &self,
            _request: pangya_domain::CommitSoloHole,
        ) -> RepositoryFuture<'_, Result<SoloMatchResult, MatchRepositoryError>> {
            self.commit_calls.fetch_add(1, Ordering::Relaxed);
            let delay = self
                .commit_delay
                .lock()
                .map_or(Duration::ZERO, |value| *value);
            let outcome = self
                .commit_outcome
                .lock()
                .map_or(Err(MatchRepositoryError::Storage), |value| *value);
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                outcome
            })
        }

        fn abort_incomplete_matches(
            &self,
            _limit: IncompleteMatchAbortLimit,
        ) -> RepositoryFuture<'_, Result<u32, MatchRepositoryError>> {
            Box::pin(async { Ok(0) })
        }
    }

    fn test_catalog() -> Catalog {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../pangya-data/tests/fixtures/synthetic-catalog");
        Catalog::load(&root, std::path::Path::new("manifest.toml"))
            .unwrap_or_else(|_| unreachable!())
    }

    fn solo_config(catalog: &Catalog, commit_timeout: Duration) -> SoloRuntimeConfig {
        SoloRuntimeConfig {
            course: catalog
                .one_hole_course(pangya_domain::CourseId::new(7).unwrap_or_else(|_| unreachable!()))
                .unwrap_or_else(|_| unreachable!()),
            catalog_fingerprint: catalog.fingerprint(),
            loading_timeout: Duration::from_secs(5),
            commit_timeout,
            max_strokes: 9,
            startup_recovery_limit: IncompleteMatchAbortLimit::new(10)
                .unwrap_or_else(|_| unreachable!()),
            shot_packets_per_window: 80,
        }
    }

    fn test_service(
        repository: Arc<FakeRepository>,
        commit_timeout: Duration,
    ) -> GameService<FakeRepository> {
        let catalog = test_catalog();
        GameService::new(
            repository,
            catalog.clone(),
            GameRuntimeConfig {
                solo_practice: Some(solo_config(&catalog, commit_timeout)),
                ..GameRuntimeConfig::default()
            },
            Arc::new(NoopGameObserver),
        )
        .unwrap_or_else(|_| unreachable!())
    }

    fn test_identity() -> RoomIdentity {
        RoomIdentity {
            connection_id: PlayerConnectionId::new(1).unwrap_or_else(|_| unreachable!()),
            account_id: AccountId::new(7).unwrap_or_else(|_| unreachable!()),
            nickname: Nickname::parse("Tester").unwrap_or_else(|_| unreachable!()),
        }
    }

    fn test_plan(service: &GameService<FakeRepository>, nonce: u128) -> SoloStartPlan {
        let solo = service
            .config
            .solo_practice
            .unwrap_or_else(|| unreachable!());
        let seed = MatchSeed::new([u8::try_from(nonce).unwrap_or(1); 32]);
        let (weather, wind) = deterministic_conditions(seed).unwrap_or_else(|_| unreachable!());
        SoloStartPlan::new(
            BeginSoloMatch::new(
                MatchId::new(uuid::Uuid::from_u128(nonce)),
                MatchResultKey::new(uuid::Uuid::from_u128(nonce.saturating_add(100))),
                test_identity().account_id,
                solo.course,
                solo.catalog_fingerprint,
                seed,
                weather,
                wind,
            ),
            solo.loading_timeout,
            solo.max_strokes,
        )
        .unwrap_or_else(|_| unreachable!())
    }

    async fn prepare_test_room(
        service: &GameService<FakeRepository>,
        plan: SoloStartPlan,
    ) -> mpsc::Receiver<RoomEvent> {
        let (outbound, receiver) = mpsc::channel(32);
        service
            .lobby
            .create(
                pangya_domain::RoomName::parse("solo").unwrap_or_else(|_| unreachable!()),
                None,
                pangya_domain::RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                test_identity(),
                outbound,
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(matches!(
            service
                .lobby
                .route_solo(
                    test_identity().connection_id,
                    LobbySoloCommand::PrepareStart(plan)
                )
                .await,
            Ok(LobbySoloRouteResult::Begin(_))
        ));
        receiver
    }

    #[test]
    fn state_policy_and_termination_labels_are_fixed() {
        assert_eq!(GameState::InRoom, GameState::InRoom);
        assert_eq!(UnknownOpcodePolicy::Capture, UnknownOpcodePolicy::Capture);
        assert_eq!(GameTermination::Cancelled.label(), "cancelled");
    }

    #[test]
    fn unknown_policies_disconnect_ignore_capture_and_bound_strikes() {
        assert!(unknown_decision(UnknownOpcodePolicy::Disconnect, 1, 3).disconnect);
        let ignored = unknown_decision(UnknownOpcodePolicy::Ignore, 1, 2);
        assert!(!ignored.disconnect);
        assert!(!ignored.capture);
        let ignored_limit = unknown_decision(UnknownOpcodePolicy::Ignore, 2, 2);
        assert!(ignored_limit.disconnect);
        assert!(ignored_limit.strike_limit);
        let captured = unknown_decision(UnknownOpcodePolicy::Capture, 1, 2);
        assert!(captured.capture);
        assert!(!captured.disconnect);
    }

    #[test]
    fn capture_sink_is_bounded_and_retains_no_body() {
        let sink = CaptureSink::new(2);
        sink.push(GameState::InChannel, 1, b"secret-one");
        sink.push(GameState::InRoom, 2, b"secret-two");
        sink.push(GameState::InRoom, 3, b"secret-three");
        let captures = sink.snapshot();
        assert_eq!(captures.len(), 2);
        assert_eq!(captures[0].opcode, 2);
        assert_eq!(captures[1].payload_len, b"secret-three".len());
        let expected: [u8; 32] = Sha256::digest(b"secret-three").into();
        assert_eq!(captures[1].sha256, expected);
    }

    #[test]
    fn new_runtime_bounds_have_valid_defaults_and_reject_extremes() {
        let defaults = GameRuntimeLimits::default();
        assert!(defaults.lobby.is_valid());
        assert!(defaults.outbound_room_event_capacity <= 65_536);
        let limits = GameRuntimeLimits {
            unknown_capture_capacity: usize::MAX,
            ..defaults
        };
        assert!(limits.unknown_capture_capacity > 65_536);
    }

    #[tokio::test]
    async fn solo_requires_two_event_slots_and_capacity_two_drains_start_pair() {
        let repository = Arc::new(FakeRepository::default());
        let catalog = test_catalog();
        let limits = GameRuntimeLimits {
            outbound_room_event_capacity: 1,
            ..GameRuntimeLimits::default()
        };
        assert_eq!(
            GameService::new(
                Arc::clone(&repository),
                catalog.clone(),
                GameRuntimeConfig {
                    limits,
                    solo_practice: Some(solo_config(&catalog, Duration::from_millis(50))),
                    ..GameRuntimeConfig::default()
                },
                Arc::new(NoopGameObserver),
            )
            .err(),
            Some(GameRuntimeError::InvalidConfig)
        );

        let limits = GameRuntimeLimits {
            outbound_room_event_capacity: 2,
            ..GameRuntimeLimits::default()
        };
        let service = GameService::new(
            repository,
            catalog.clone(),
            GameRuntimeConfig {
                limits,
                solo_practice: Some(solo_config(&catalog, Duration::from_millis(50))),
                ..GameRuntimeConfig::default()
            },
            Arc::new(NoopGameObserver),
        )
        .unwrap_or_else(|_| unreachable!());
        let plan = test_plan(&service, 31);
        let (outbound, mut events) = mpsc::channel(2);
        let cancellation = CancellationToken::new();
        service
            .lobby
            .create(
                pangya_domain::RoomName::parse("two-events").unwrap_or_else(|_| unreachable!()),
                None,
                pangya_domain::RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                test_identity(),
                outbound,
                cancellation.clone(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        service
            .lobby
            .route_solo(
                test_identity().connection_id,
                LobbySoloCommand::PrepareStart(plan.clone()),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        service
            .persist_and_confirm_begin(
                test_identity().connection_id,
                plan.begin().clone(),
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(matches!(
            events.recv().await,
            Some(RoomEvent::SoloStarted(_))
        ));
        assert!(matches!(
            events.recv().await,
            Some(RoomEvent::SoloPhase {
                phase: SoloMatchPhase::Loading,
                ..
            })
        ));
        assert!(!cancellation.is_cancelled());
    }

    #[test]
    fn fixed_windows_bound_room_and_chat_commands() {
        let mut commands = LocalRateWindow::new(Duration::from_secs(60));
        assert!(commands.admit_count(2));
        assert!(commands.admit_count(2));
        assert!(!commands.admit_count(2));
    }

    #[tokio::test]
    async fn begin_failure_and_timeout_abort_exact_reservation_without_orphan() {
        for (delay, expected) in [
            (Duration::ZERO, GameRuntimeError::MatchPersistence),
            (
                Duration::from_millis(50),
                GameRuntimeError::MatchPersistence,
            ),
        ] {
            let repository = Arc::new(FakeRepository::default());
            if let Ok(mut configured) = repository.begin_delay.lock() {
                *configured = delay;
            }
            if delay.is_zero()
                && let Ok(mut configured) = repository.begin_outcome.lock()
            {
                *configured = Err(MatchRepositoryError::Storage);
            }
            let service = test_service(Arc::clone(&repository), Duration::from_millis(5));
            let plan = test_plan(&service, if delay.is_zero() { 1 } else { 2 });
            let _events = prepare_test_room(&service, plan.clone()).await;
            let result = service
                .persist_and_confirm_begin(
                    test_identity().connection_id,
                    plan.begin().clone(),
                    &CancellationToken::new(),
                    Instant::now() + Duration::from_secs(1),
                )
                .await;
            assert_eq!(result, Err(expected));
            assert_eq!(repository.begin_calls.load(Ordering::Relaxed), 1);
            assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 1);
            let replacement = test_plan(&service, if delay.is_zero() { 3 } else { 4 });
            assert!(matches!(
                service
                    .lobby
                    .route_solo(
                        test_identity().connection_id,
                        LobbySoloCommand::PrepareStart(replacement)
                    )
                    .await,
                Ok(LobbySoloRouteResult::Begin(_))
            ));
        }
    }

    #[tokio::test]
    async fn forced_confirm_failure_persists_one_abort_without_cleanup_duplicate() {
        let repository = Arc::new(FakeRepository::default());
        let observer = Arc::new(RecordingObserver::default());
        let catalog = test_catalog();
        let service = GameService::new(
            Arc::clone(&repository),
            catalog.clone(),
            GameRuntimeConfig {
                solo_practice: Some(solo_config(&catalog, Duration::from_millis(50))),
                ..GameRuntimeConfig::default()
            },
            observer.clone(),
        )
        .unwrap_or_else(|_| unreachable!());
        let plan = test_plan(&service, 5);
        let _events = prepare_test_room(&service, plan.clone()).await;
        assert!(repository.begin_solo(plan.begin().clone()).await.is_ok());
        assert_eq!(
            service
                .resolve_persisted_begin(
                    test_identity().connection_id,
                    Err(SoloMatchError::Closed),
                )
                .await,
            Err(GameRuntimeError::MatchPersistence)
        );
        assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            service
                .lobby
                .disconnect(test_identity().connection_id)
                .await,
            Ok(None)
        );
        assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 1);
        assert!(observer.events.lock().is_ok_and(|events| events.is_empty()));
    }

    #[tokio::test]
    async fn commit_timeout_routes_persists_and_acknowledges_abort() {
        let repository = Arc::new(FakeRepository::default());
        if let Ok(mut delay) = repository.commit_delay.lock() {
            *delay = Duration::from_millis(50);
        }
        let service = test_service(Arc::clone(&repository), Duration::from_millis(5));
        let plan = test_plan(&service, 9);
        let _events = prepare_test_room(&service, plan.clone()).await;
        service
            .persist_and_confirm_begin(
                test_identity().connection_id,
                plan.begin().clone(),
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let mark = match service
            .lobby
            .route_solo(
                test_identity().connection_id,
                LobbySoloCommand::LoadingComplete(
                    LoadingComplete::new(100).unwrap_or_else(|_| unreachable!()),
                ),
            )
            .await
        {
            Ok(LobbySoloRouteResult::InGame(mark)) => mark,
            _ => unreachable!(),
        };
        service
            .persist_in_game(
                test_identity().connection_id,
                mark,
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let action = ShotAction::new(1, 1, 1.0, 0.0, 0.0, 0.0).unwrap_or_else(|_| unreachable!());
        service
            .lobby
            .route_solo(
                test_identity().connection_id,
                LobbySoloCommand::ShotAction(action),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let result = ShotResult::new(1, 1.0, 0.0, 0.0, pangya_protocol::Lie::Green, true)
            .unwrap_or_else(|_| unreachable!());
        service
            .lobby
            .route_solo(
                test_identity().connection_id,
                LobbySoloCommand::ShotResult(result),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let commit = match service
            .lobby
            .route_solo(
                test_identity().connection_id,
                LobbySoloCommand::PrepareFinish,
            )
            .await
        {
            Ok(LobbySoloRouteResult::Commit(commit)) => commit,
            _ => unreachable!(),
        };
        assert_eq!(
            service
                .persist_and_apply_commit(
                    test_identity().connection_id,
                    commit,
                    &CancellationToken::new(),
                    Instant::now() + Duration::from_secs(1),
                )
                .await,
            Err(GameRuntimeError::MatchPersistence)
        );
        assert_eq!(repository.commit_calls.load(Ordering::Relaxed), 1);
        assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 1);
        let replacement = test_plan(&service, 12);
        assert!(matches!(
            service
                .lobby
                .route_solo(
                    test_identity().connection_id,
                    LobbySoloCommand::PrepareStart(replacement)
                )
                .await,
            Ok(LobbySoloRouteResult::Begin(_))
        ));
    }

    #[tokio::test]
    async fn in_game_mark_failure_aborts_actor_and_durable_match() {
        let repository = Arc::new(FakeRepository::default());
        if let Ok(mut outcome) = repository.mark_outcome.lock() {
            *outcome = Err(MatchRepositoryError::Storage);
        }
        let service = test_service(Arc::clone(&repository), Duration::from_millis(50));
        let plan = test_plan(&service, 13);
        let _events = prepare_test_room(&service, plan.clone()).await;
        service
            .persist_and_confirm_begin(
                test_identity().connection_id,
                plan.begin().clone(),
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let mark = match service
            .lobby
            .route_solo(
                test_identity().connection_id,
                LobbySoloCommand::LoadingComplete(
                    LoadingComplete::new(100).unwrap_or_else(|_| unreachable!()),
                ),
            )
            .await
        {
            Ok(LobbySoloRouteResult::InGame(mark)) => mark,
            _ => unreachable!(),
        };
        assert_eq!(
            service
                .persist_in_game(
                    test_identity().connection_id,
                    mark,
                    &CancellationToken::new(),
                    Instant::now() + Duration::from_secs(1),
                )
                .await,
            Err(GameRuntimeError::MatchPersistence)
        );
        assert_eq!(repository.mark_calls.load(Ordering::Relaxed), 1);
        assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 1);
        assert!(matches!(
            service
                .lobby
                .route_solo(
                    test_identity().connection_id,
                    LobbySoloCommand::PrepareStart(test_plan(&service, 14)),
                )
                .await,
            Ok(LobbySoloRouteResult::Begin(_))
        ));
    }

    #[tokio::test]
    async fn abort_persistence_failure_is_retained_and_already_committed_race_applies() {
        let repository = Arc::new(FakeRepository::default());
        if let Ok(mut begin) = repository.begin_outcome.lock() {
            *begin = Err(MatchRepositoryError::Storage);
        }
        if let Ok(mut abort) = repository.abort_outcome.lock() {
            *abort = Err(MatchRepositoryError::Storage);
        }
        let service = test_service(Arc::clone(&repository), Duration::from_millis(20));
        let plan = test_plan(&service, 10);
        let _events = prepare_test_room(&service, plan.clone()).await;
        assert_eq!(
            service
                .persist_and_confirm_begin(
                    test_identity().connection_id,
                    plan.begin().clone(),
                    &CancellationToken::new(),
                    Instant::now() + Duration::from_secs(1),
                )
                .await,
            Err(GameRuntimeError::MatchPersistence)
        );
        assert!(matches!(
            service
                .lobby
                .route_solo(
                    test_identity().connection_id,
                    LobbySoloCommand::Abort(MatchAbortReason::PersistenceFailure)
                )
                .await,
            Ok(LobbySoloRouteResult::Abort(Some(abort))) if abort.match_id() == plan.begin().match_id()
        ));

        let committed = SoloMatchResult::new(
            plan.begin().match_id(),
            plan.begin().result_key(),
            plan.begin().account_id(),
            pangya_domain::StrokeCount::new(1).unwrap_or_else(|_| unreachable!()),
            pangya_domain::SoloReward::from_persisted(0, 10, 5),
            pangya_domain::ServerBalances::from_persisted(10, 5),
        );
        if let Ok(mut abort) = repository.abort_outcome.lock() {
            *abort = Ok(AbortMatchOutcome::AlreadyCommitted(committed));
        }
        assert_eq!(
            service
                .abort_actor_match(
                    test_identity().connection_id,
                    MatchAbortReason::PersistenceFailure,
                    false,
                )
                .await,
            Ok(AbortResolution::Committed)
        );
        let replacement = test_plan(&service, 11);
        assert!(matches!(
            service
                .lobby
                .route_solo(
                    test_identity().connection_id,
                    LobbySoloCommand::PrepareStart(replacement)
                )
                .await,
            Ok(LobbySoloRouteResult::Begin(_))
        ));
    }

    #[tokio::test]
    async fn runtime_path_mutates_storage_only_at_lifecycle_marks_and_deduplicates_relays() {
        let repository = Arc::new(FakeRepository::default());
        let service = test_service(Arc::clone(&repository), Duration::from_millis(50));
        let plan = test_plan(&service, 20);
        let _events = prepare_test_room(&service, plan.clone()).await;
        service
            .persist_and_confirm_begin(
                test_identity().connection_id,
                plan.begin().clone(),
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(repository.begin_calls.load(Ordering::Relaxed), 1);
        assert_eq!(repository.mark_calls.load(Ordering::Relaxed), 0);
        assert_eq!(repository.commit_calls.load(Ordering::Relaxed), 0);
        assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 0);
        let mark = match service
            .lobby
            .route_solo(
                test_identity().connection_id,
                LobbySoloCommand::LoadingComplete(
                    LoadingComplete::new(100).unwrap_or_else(|_| unreachable!()),
                ),
            )
            .await
        {
            Ok(LobbySoloRouteResult::InGame(mark)) => mark,
            _ => unreachable!(),
        };
        service
            .persist_in_game(
                test_identity().connection_id,
                mark,
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(repository.mark_calls.load(Ordering::Relaxed), 1);
        let action = ShotAction::new(1, 1, 1.0, 0.0, 0.0, 0.0).unwrap_or_else(|_| unreachable!());
        assert_eq!(
            service
                .lobby
                .route_solo(
                    test_identity().connection_id,
                    LobbySoloCommand::ShotAction(action)
                )
                .await,
            Ok(LobbySoloRouteResult::Relay(RelayDisposition::Accepted))
        );
        assert_eq!(
            service
                .lobby
                .route_solo(
                    test_identity().connection_id,
                    LobbySoloCommand::ShotAction(action)
                )
                .await,
            Ok(LobbySoloRouteResult::Relay(RelayDisposition::Duplicate))
        );
        let result = ShotResult::new(1, 1.0, 0.0, 0.0, pangya_protocol::Lie::Green, true)
            .unwrap_or_else(|_| unreachable!());
        assert!(matches!(
            service
                .lobby
                .route_solo(
                    test_identity().connection_id,
                    LobbySoloCommand::ShotResult(result)
                )
                .await,
            Ok(LobbySoloRouteResult::Relay(RelayDisposition::Accepted))
        ));
        let commit = match service
            .lobby
            .route_solo(
                test_identity().connection_id,
                LobbySoloCommand::PrepareFinish,
            )
            .await
        {
            Ok(LobbySoloRouteResult::Commit(commit)) => commit,
            _ => unreachable!(),
        };
        let committed = SoloMatchResult::new(
            commit.match_id(),
            commit.result_key(),
            commit.account_id(),
            commit.strokes(),
            pangya_domain::SoloReward::from_persisted(0, 10, 5),
            pangya_domain::ServerBalances::from_persisted(10, 5),
        );
        if let Ok(mut outcome) = repository.commit_outcome.lock() {
            *outcome = Ok(committed);
        }
        service
            .persist_and_apply_commit(
                test_identity().connection_id,
                commit,
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(repository.begin_calls.load(Ordering::Relaxed), 1);
        assert_eq!(repository.mark_calls.load(Ordering::Relaxed), 1);
        assert_eq!(repository.commit_calls.load(Ordering::Relaxed), 1);
        assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn outbound_failure_cleanup_aborts_active_match_once() {
        let repository = Arc::new(FakeRepository::default());
        let service = test_service(Arc::clone(&repository), Duration::from_millis(20));
        let plan = test_plan(&service, 28);
        let (outbound, receiver) = mpsc::channel(1);
        drop(receiver);
        service
            .lobby
            .create(
                pangya_domain::RoomName::parse("failed-outbound")
                    .unwrap_or_else(|_| unreachable!()),
                None,
                pangya_domain::RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                test_identity(),
                outbound,
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        service
            .lobby
            .route_solo(
                test_identity().connection_id,
                LobbySoloCommand::PrepareStart(plan.clone()),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        service
            .persist_and_confirm_begin(
                test_identity().connection_id,
                plan.begin().clone(),
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let abort = service
            .lobby
            .disconnect(test_identity().connection_id)
            .await
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|| unreachable!());
        assert_eq!(abort.match_id(), plan.begin().match_id());
        assert_eq!(abort.reason(), MatchAbortReason::Disconnect);
        service
            .persist_cleanup_abort(abort)
            .await
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn connection_cleanup_abort_failure_from_serve_is_match_persistence() {
        let repository = Arc::new(FakeRepository::default());
        let service = Arc::new(test_service(
            Arc::clone(&repository),
            Duration::from_millis(50),
        ));
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|_| unreachable!());
        let address = listener.local_addr().unwrap_or_else(|_| unreachable!());
        let shutdown = CancellationToken::new();
        let task_service = Arc::clone(&service);
        let task_shutdown = shutdown.clone();
        let task = tokio::spawn(async move { task_service.serve(listener, task_shutdown).await });
        tokio::task::yield_now().await;

        let plan = test_plan(service.as_ref(), 32);
        let _events = prepare_test_room(service.as_ref(), plan.clone()).await;
        service
            .persist_and_confirm_begin(
                test_identity().connection_id,
                plan.begin().clone(),
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        if let Ok(mut abort) = repository.abort_outcome.lock() {
            *abort = Err(MatchRepositoryError::Storage);
        }
        let client = TcpStream::connect(address)
            .await
            .unwrap_or_else(|_| unreachable!());
        drop(client);
        assert_eq!(
            timeout(Duration::from_secs(1), task)
                .await
                .unwrap_or_else(|_| unreachable!())
                .unwrap_or_else(|_| unreachable!()),
            Err(GameRuntimeError::MatchPersistence)
        );
        assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn lobby_shutdown_abort_persistence_failure_fails_service() {
        let repository = Arc::new(FakeRepository::default());
        let service = Arc::new(test_service(
            Arc::clone(&repository),
            Duration::from_millis(20),
        ));
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|_| unreachable!());
        let shutdown = CancellationToken::new();
        let task_service = Arc::clone(&service);
        let task_shutdown = shutdown.clone();
        let task = tokio::spawn(async move { task_service.serve(listener, task_shutdown).await });
        tokio::task::yield_now().await;
        let plan = test_plan(service.as_ref(), 29);
        let _events = prepare_test_room(service.as_ref(), plan.clone()).await;
        service
            .persist_and_confirm_begin(
                test_identity().connection_id,
                plan.begin().clone(),
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        if let Ok(mut abort) = repository.abort_outcome.lock() {
            *abort = Err(MatchRepositoryError::Storage);
        }
        shutdown.cancel();
        assert_eq!(
            task.await.unwrap_or_else(|_| unreachable!()),
            Err(GameRuntimeError::MatchPersistence)
        );
        assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn terminal_abort_is_recorded_before_socket_write_failure() {
        let repository = Arc::new(FakeRepository::default());
        let observer = Arc::new(RecordingObserver::default());
        let catalog = test_catalog();
        let service = GameService::new(
            repository,
            catalog.clone(),
            GameRuntimeConfig {
                solo_practice: Some(solo_config(&catalog, Duration::from_millis(50))),
                ..GameRuntimeConfig::default()
            },
            observer.clone(),
        )
        .unwrap_or_else(|_| unreachable!());
        let plan = test_plan(&service, 33);
        let mut events = prepare_test_room(&service, plan.clone()).await;
        service
            .persist_and_confirm_begin(
                test_identity().connection_id,
                plan.begin().clone(),
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let _ = events.recv().await;
        let _ = events.recv().await;
        let abort = match service
            .lobby
            .route_solo(
                test_identity().connection_id,
                LobbySoloCommand::Abort(MatchAbortReason::Disconnect),
            )
            .await
        {
            Ok(LobbySoloRouteResult::Abort(Some(abort))) => abort,
            _ => unreachable!(),
        };
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|_| unreachable!());
        let address = listener.local_addr().unwrap_or_else(|_| unreachable!());
        let client = TcpStream::connect(address);
        let accepted = listener.accept();
        let (client, accepted) = tokio::join!(client, accepted);
        let client = client.unwrap_or_else(|_| unreachable!());
        let (mut server, _) = accepted.unwrap_or_else(|_| unreachable!());
        server.shutdown().await.unwrap_or_else(|_| unreachable!());
        drop(client);
        let mut framed = Framed::new(
            server,
            FrameCodec::new(0, ServiceKind::Game, CodecLimits::default()),
        );
        assert_eq!(
            service
                .handle_room_event(
                    &mut framed,
                    RoomEvent::AbortRequested(abort),
                    None,
                    test_identity().connection_id,
                )
                .await,
            Err(GameRuntimeError::Io)
        );
        assert_eq!(
            observer
                .events
                .lock()
                .map_or_else(|_| Vec::new(), |events| events.clone()),
            vec![GameMatchObservation::Aborted]
        );
    }

    #[tokio::test]
    async fn service_observes_registry_exact_gauge_despite_outbound_failure() {
        let repository = Arc::new(FakeRepository::default());
        let observer = Arc::new(RecordingObserver::default());
        let catalog = test_catalog();
        let service = Arc::new(
            GameService::new(
                Arc::clone(&repository),
                catalog.clone(),
                GameRuntimeConfig {
                    solo_practice: Some(solo_config(&catalog, Duration::from_millis(50))),
                    ..GameRuntimeConfig::default()
                },
                observer.clone(),
            )
            .unwrap_or_else(|_| unreachable!()),
        );
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|_| unreachable!());
        let shutdown = CancellationToken::new();
        let task_service = Arc::clone(&service);
        let task_shutdown = shutdown.clone();
        let task = tokio::spawn(async move { task_service.serve(listener, task_shutdown).await });
        tokio::task::yield_now().await;

        let plan = test_plan(service.as_ref(), 30);
        let (outbound, receiver) = mpsc::channel(1);
        drop(receiver);
        service
            .lobby
            .create(
                pangya_domain::RoomName::parse("failed-outbound")
                    .unwrap_or_else(|_| unreachable!()),
                None,
                pangya_domain::RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                test_identity(),
                outbound,
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(matches!(
            service
                .lobby
                .route_solo(
                    test_identity().connection_id,
                    LobbySoloCommand::PrepareStart(plan.clone())
                )
                .await,
            Ok(LobbySoloRouteResult::Begin(_))
        ));
        service
            .persist_and_confirm_begin(
                test_identity().connection_id,
                plan.begin().clone(),
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if observer
                    .active
                    .lock()
                    .is_ok_and(|values| values.last() == Some(&1))
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap_or_else(|_| unreachable!());

        shutdown.cancel();
        assert!(task.await.unwrap_or_else(|_| unreachable!()).is_ok());
        assert_eq!(
            observer
                .active
                .lock()
                .map_or_else(|_| Vec::new(), |values| values.clone()),
            vec![1, 0]
        );
        assert_eq!(
            observer
                .events
                .lock()
                .map_or_else(|_| Vec::new(), |values| values.clone()),
            vec![GameMatchObservation::Started, GameMatchObservation::Aborted]
        );
    }

    #[tokio::test]
    async fn disconnect_already_committed_is_finished_once_with_zero_gauge() {
        let repository = Arc::new(FakeRepository::default());
        let observer = Arc::new(RecordingObserver::default());
        let catalog = test_catalog();
        let service = Arc::new(
            GameService::new(
                Arc::clone(&repository),
                catalog.clone(),
                GameRuntimeConfig {
                    solo_practice: Some(solo_config(&catalog, Duration::from_millis(50))),
                    ..GameRuntimeConfig::default()
                },
                observer.clone(),
            )
            .unwrap_or_else(|_| unreachable!()),
        );
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|_| unreachable!());
        let shutdown = CancellationToken::new();
        let task_service = Arc::clone(&service);
        let task_shutdown = shutdown.clone();
        let task = tokio::spawn(async move { task_service.serve(listener, task_shutdown).await });
        tokio::task::yield_now().await;
        let plan = test_plan(service.as_ref(), 34);
        let _events = prepare_test_room(service.as_ref(), plan.clone()).await;
        service
            .persist_and_confirm_begin(
                test_identity().connection_id,
                plan.begin().clone(),
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let committed = SoloMatchResult::new(
            plan.begin().match_id(),
            plan.begin().result_key(),
            plan.begin().account_id(),
            pangya_domain::StrokeCount::new(1).unwrap_or_else(|_| unreachable!()),
            pangya_domain::SoloReward::from_persisted(0, 10, 5),
            pangya_domain::ServerBalances::from_persisted(10, 5),
        );
        if let Ok(mut abort) = repository.abort_outcome.lock() {
            *abort = Ok(AbortMatchOutcome::AlreadyCommitted(committed));
        }
        let abort = service
            .lobby
            .disconnect(test_identity().connection_id)
            .await
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|| unreachable!());
        assert_eq!(
            service.persist_cleanup_abort(abort).await,
            Ok(AbortResolution::Committed)
        );
        shutdown.cancel();
        assert!(task.await.unwrap_or_else(|_| unreachable!()).is_ok());
        assert_eq!(
            observer
                .active
                .lock()
                .map_or_else(|_| Vec::new(), |values| values.clone()),
            vec![1, 0]
        );
        assert_eq!(
            observer
                .events
                .lock()
                .map_or_else(|_| Vec::new(), |values| values.clone()),
            vec![
                GameMatchObservation::Started,
                GameMatchObservation::Finished
            ]
        );
        assert!(observer.commits.lock().is_ok_and(|commits| {
            commits
                .iter()
                .filter(|outcome| **outcome == GameCommitObservation::Idempotent)
                .count()
                == 1
        }));
    }

    #[tokio::test]
    async fn shutdown_already_committed_is_finished_once_with_zero_gauge() {
        let repository = Arc::new(FakeRepository::default());
        let observer = Arc::new(RecordingObserver::default());
        let catalog = test_catalog();
        let service = Arc::new(
            GameService::new(
                Arc::clone(&repository),
                catalog.clone(),
                GameRuntimeConfig {
                    solo_practice: Some(solo_config(&catalog, Duration::from_millis(50))),
                    ..GameRuntimeConfig::default()
                },
                observer.clone(),
            )
            .unwrap_or_else(|_| unreachable!()),
        );
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|_| unreachable!());
        let shutdown = CancellationToken::new();
        let task_service = Arc::clone(&service);
        let task_shutdown = shutdown.clone();
        let task = tokio::spawn(async move { task_service.serve(listener, task_shutdown).await });
        tokio::task::yield_now().await;
        let plan = test_plan(service.as_ref(), 35);
        let _events = prepare_test_room(service.as_ref(), plan.clone()).await;
        service
            .persist_and_confirm_begin(
                test_identity().connection_id,
                plan.begin().clone(),
                &CancellationToken::new(),
                Instant::now() + Duration::from_secs(1),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let committed = SoloMatchResult::new(
            plan.begin().match_id(),
            plan.begin().result_key(),
            plan.begin().account_id(),
            pangya_domain::StrokeCount::new(1).unwrap_or_else(|_| unreachable!()),
            pangya_domain::SoloReward::from_persisted(0, 10, 5),
            pangya_domain::ServerBalances::from_persisted(10, 5),
        );
        if let Ok(mut abort) = repository.abort_outcome.lock() {
            *abort = Ok(AbortMatchOutcome::AlreadyCommitted(committed));
        }
        shutdown.cancel();
        assert!(task.await.unwrap_or_else(|_| unreachable!()).is_ok());
        assert_eq!(
            observer
                .active
                .lock()
                .map_or_else(|_| Vec::new(), |values| values.clone()),
            vec![1, 0]
        );
        assert_eq!(
            observer
                .events
                .lock()
                .map_or_else(|_| Vec::new(), |values| values.clone()),
            vec![
                GameMatchObservation::Started,
                GameMatchObservation::Finished
            ]
        );
        assert!(observer.commits.lock().is_ok_and(|commits| {
            commits
                .iter()
                .filter(|outcome| **outcome == GameCommitObservation::Idempotent)
                .count()
                == 1
        }));
    }

    #[tokio::test]
    async fn solo_runtime_cross_checks_catalog_and_persists_cleanup_abort_once() {
        let catalog = test_catalog();
        let course = catalog
            .one_hole_course(pangya_domain::CourseId::new(7).unwrap_or_else(|_| unreachable!()))
            .unwrap_or_else(|_| unreachable!());
        let solo = SoloRuntimeConfig {
            course,
            catalog_fingerprint: catalog.fingerprint(),
            loading_timeout: Duration::from_secs(5),
            commit_timeout: Duration::from_secs(1),
            max_strokes: 9,
            startup_recovery_limit: IncompleteMatchAbortLimit::new(10)
                .unwrap_or_else(|_| unreachable!()),
            shot_packets_per_window: 80,
        };
        let repository = Arc::new(FakeRepository::default());
        let service = GameService::new(
            Arc::clone(&repository),
            catalog.clone(),
            GameRuntimeConfig {
                solo_practice: Some(solo),
                ..GameRuntimeConfig::default()
            },
            Arc::new(NoopGameObserver),
        )
        .unwrap_or_else(|_| unreachable!());
        let abort = AbortMatch::new(
            MatchId::new(uuid::Uuid::from_u128(1)),
            MatchResultKey::new(uuid::Uuid::from_u128(2)),
            AccountId::new(7).unwrap_or_else(|_| unreachable!()),
            MatchAbortReason::Disconnect,
        );
        assert!(service.persist_cleanup_abort(abort).await.is_ok());
        assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 1);

        let drifted = SoloRuntimeConfig {
            catalog_fingerprint: CatalogFingerprint::new([0; 32]),
            ..solo
        };
        assert_eq!(
            GameService::new(
                Arc::new(FakeRepository::default()),
                catalog,
                GameRuntimeConfig {
                    solo_practice: Some(drifted),
                    ..GameRuntimeConfig::default()
                },
                Arc::new(NoopGameObserver),
            )
            .err(),
            Some(GameRuntimeError::Catalog)
        );
    }
}
