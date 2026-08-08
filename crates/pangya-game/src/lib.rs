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
    LobbyShutdownOutcome, LobbySoloCommand, LobbySoloRouteResult, LobbyStrokeCommand,
    LobbyStrokePersistence, LobbyStrokeRouteResult, spawn_lobby,
};
pub use match_state::{
    LOADING_TIMEOUT_HARD_CAP, MAX_SOLO_STROKES, RelayDisposition, SoloMatchError, SoloMatchPhase,
    SoloMatchState, SoloStartPlan, deterministic_conditions,
};
pub use room::{
    MAX_RETAIL_RELAY_BYTES, RetailMatchRelay, RoomActorLimits, RoomDisconnect, RoomEvent,
    RoomHandle, RoomIdentity, spawn_room,
};
pub use stroke_state::{
    MAX_STROKE_STROKES, STROKE_GAME_TIMEOUT_HARD_CAP, StrokeHoleOutOutcome, StrokeLoadingOutcome,
    StrokeMatchError, StrokeMatchPhase, StrokeStartPlan,
};

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
    AbortMatch, AbortMatchOutcome, AbortStrokeMatch, AbortStrokeMatchOutcome, AccountId,
    BeginSoloMatch, BeginSoloMatchOutcome, BeginStrokeMatch, BeginStrokeMatchOutcome,
    CatalogFingerprint, CharacterId, ConsumeHandover, ConsumeItem, EconomyCommit, EconomyError,
    EconomyItemSelector, EconomyOperationId, EconomyRepository, EquipmentChange,
    HandoverRepository, InventoryItemId, ItemDurability, ItemKind, ItemStacking, ItemTypeId,
    MarkSoloInGame, MarkSoloInGameOutcome, MarkStrokeInGame, MarkStrokeInGameOutcome,
    MatchAbortReason, MatchId, MatchRepository, MatchResultKey, MatchSeed, Nickname, OneHoleConfig,
    PlayerConnectionId, PlayerRepository, PlayerSnapshot, PurchaseRequest, RepairItem,
    RepositoryError, RoomError, RoomId, RoomName, RoomPassword, RoomSettings, RoomSnapshot,
    RoomSummary, ServiceKind as DomainServiceKind, SoloMatchResult, SourceAddressPrefix,
    StrokeCompletion as DomainStrokeCompletion, StrokeMatchResult, StrokeParticipant,
    StrokeRosterOrder,
};
use pangya_login::{
    CapacityRegistry, FixedWindowLimiter, KeyedCapacityGuard, KeyedCapacityRegistry, RateDecision,
    RegistryError, RegistryGuard, parse_handover,
};
use pangya_protocol::{
    BalanceUpdate, CHARACTER_PARTS, CHARACTER_STATS, ChannelJoined, CharacterBootstrap,
    CharacterInfo, CodecLimits, CompatibilityProfile, ConsumeOneRequest, DecodePacket,
    EQUIPPED_ITEM_SLOTS, EconomyCommand, EconomyCommandResult, EconomyItemKind, EconomyOutcome,
    EncodePacket, EquipRequest, EquipmentChanged, EquipmentInfo, FinishHole, FrameCodec,
    GAME_INVENTORY_SEGMENT_ITEMS, GameAuth, HandoverControl, HandoverReply, HoleResult,
    IffContainerChunk, IffContainerKind, InventoryBootstrap, InventoryChanged, InventorySegment,
    Lie, LoadingComplete, MatchAbortReason as ProtocolMatchAbortReason, MatchAborted, MatchPhase,
    MatchStarted, OutboundFrame, PacketEncodeError, PacketWriter, PlayerInfo, PurchaseCommitted,
    PurchaseRequestPacket, RepairCommitted, RepairRequest, RetailCaddie, RetailChannel,
    RetailChannelJoinNotice, RetailChannelJoined, RetailCharacter, RetailEquipment,
    RetailEquipmentSlot, RetailEquipmentUpdate, RetailEquipmentUpdated, RetailFinishHole,
    RetailGameAuth, RetailHole, RetailHoleProgression, RetailHoleWeather, RetailHoleWind,
    RetailLockerCombinationAttempt, RetailLockerCombinationResponse, RetailLockerInventoryRequest,
    RetailLockerInventoryResponse, RetailLoginBonusRequest, RetailLoginBonusStatus,
    RetailMatchFinish, RetailMatchInfo, RetailMatchStart, RetailMultiplayerJoined,
    RetailMultiplayerLeft, RetailMyRoomEnter, RetailMyRoomEntered, RetailMyRoomInventoryRequest,
    RetailMyRoomLayout, RetailPangBalance, RetailPangSpent, RetailPlayerHistory,
    RetailPlayerHistoryRequest, RetailPlayerIdentity, RetailPlayerInfo, RetailPlayerStartHole,
    RetailPlayerStatistics, RetailPointBalance, RetailPurchaseItem, RetailPurchaseRequest,
    RetailPurchaseResponse, RetailRoom, RetailRoomCensus, RetailRoomCreate, RetailRoomJoin,
    RetailRoomJoinResult, RetailRoomLeave, RetailRoomList, RetailRoomPlayer, RetailRoomState,
    RetailRoomType, RetailSelectChannel, RetailShopJoin, RetailShopJoined, RetailShotCommitRelay,
    RetailShotSync, RetailStanding, RetailTurnEnd, RetailTurnStart, RetailWeather, RoomChatEvent,
    RoomChatRequest, RoomCommand, RoomCommandResult, RoomCommandResultResponse, RoomCreateRequest,
    RoomJoinRejection, RoomJoinRequest, RoomKickRequest, RoomLeaveRequest, RoomListKind,
    RoomListRequest, RoomListResponse, RoomMembershipEvent, RoomMembershipKind, RoomPlayerFlags,
    RoomReadyRequest, RoomSettingsRequest, RoomStateRequest, RoomStateResponse,
    SYNTHETIC_M4_C2S_CHAT, SYNTHETIC_M4_C2S_CREATE, SYNTHETIC_M4_C2S_JOIN, SYNTHETIC_M4_C2S_KICK,
    SYNTHETIC_M4_C2S_LEAVE, SYNTHETIC_M4_C2S_LIST, SYNTHETIC_M4_C2S_READY,
    SYNTHETIC_M4_C2S_SETTINGS, SYNTHETIC_M4_C2S_STATE, SYNTHETIC_M5_C2S_FINISH_HOLE,
    SYNTHETIC_M5_C2S_LOADING_COMPLETE, SYNTHETIC_M5_C2S_SHOT_ACTION, SYNTHETIC_M5_C2S_SHOT_RESULT,
    SYNTHETIC_M5_C2S_START_SOLO, SYNTHETIC_M6_C2S_GIVE_UP, SYNTHETIC_M6_C2S_LOADING_COMPLETE,
    SYNTHETIC_M6_C2S_SHOT_ACTION, SYNTHETIC_M6_C2S_SHOT_RESULT, SYNTHETIC_M6_C2S_START_STROKE_TWO,
    SYNTHETIC_M7_C2S_CONSUME, SYNTHETIC_M7_C2S_EQUIP, SYNTHETIC_M7_C2S_PURCHASE,
    SYNTHETIC_M7_C2S_REPAIR, SYNTHETIC_M7_C2S_SHOP_PAGE, SelectChannel, ServerChannelList,
    ServiceKind, ShopOffer, ShopPage, ShopPageRequest, ShotAction, ShotActionRelay, ShotResult,
    ShotResultRelay, SoloCommand, SoloCommandOutcome, SoloCommandResult, SoloPhase, StartSolo,
    StartStrokeTwo, StrokeAbortReason, StrokeActionRelay, StrokeBalanceUpdate, StrokeCommand,
    StrokeCommandOutcome, StrokeCommandResult, StrokeCompletion as ProtocolStrokeCompletion,
    StrokeGiveUp, StrokeLoadingComplete, StrokeMatchAborted, StrokeMatchStarted, StrokePhase,
    StrokePhaseKind, StrokeResultRelay, StrokeShotAction, StrokeShotResult, StrokeStandingEntry,
    StrokeStandings, StrokeTurnStarted, Weather as ProtocolWeather, Wind, decode_packet_payload,
    encode_packet_payload, is_retail_accepted_session_opcode, synthetic_game_hello,
    us852_game_hello,
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
    /// Stroke-two begin committed; waiting for both loading completions.
    InStrokeLoading,
    /// Stroke-two gameplay is active or waiting for aggregate settlement.
    InStrokeMatch,
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
    /// Per-connection stroke-two action/result packets.
    StrokePacketsConnection,
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
    /// Active turn deadline elapsed.
    TurnTimeout,
    /// Whole-game deadline elapsed.
    GameTimeout,
    /// An in-game participant forfeited.
    Forfeit,
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
    /// Stroke-two packet came from the nonactive participant.
    OutOfTurn,
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

/// Fixed synthetic economy command kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameEconomyCommand {
    /// Catalog shop page request.
    ShopPage,
    /// Purchase request.
    Purchase,
    /// Equipment change request.
    Equip,
    /// Single-unit consume request.
    Consume,
    /// Durability repair request.
    Repair,
}

/// Fixed synthetic economy command outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameEconomyOutcome {
    /// Command committed or replayed an identical commit.
    Success,
    /// Economy is not composed; the request decoded but was refused.
    Disabled,
    /// Request failed a bound, catalog, or identifier check.
    Invalid,
    /// Referenced inventory or character is not owned.
    NotOwned,
    /// Referenced item cannot satisfy the command.
    Incompatible,
    /// Balance could not cover the catalog price.
    InsufficientPang,
    /// Stack limit would be exceeded.
    StackFull,
    /// Equipment version did not match.
    VersionConflict,
    /// Replay carried different parameters than the original commit.
    IdempotencyDrift,
    /// Repository command exceeded its deadline.
    Timeout,
}

/// Low-cardinality GameService observation boundary.
pub trait GameObserver: Send + Sync + 'static {
    /// Fixed synthetic economy command and outcome.
    fn economy(&self, _command: GameEconomyCommand, _outcome: GameEconomyOutcome) {}
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
    /// Exact process-local active stroke-two match gauge.
    fn stroke_matches_active(&self, _active: usize) {}
    /// Fixed solo lifecycle observation.
    fn match_event(&self, _event: GameMatchObservation) {}
    /// Fixed stroke-two lifecycle observation.
    fn stroke_match_event(&self, _event: GameMatchObservation) {}
    /// Fixed persistence outcome for solo practice.
    fn commit(&self, _outcome: GameCommitObservation) {}
    /// Fixed persistence outcome for stroke two.
    fn stroke_commit(&self, _outcome: GameCommitObservation) {}
    /// Fixed solo shot outcome.
    fn shot(&self, _outcome: GameShotObservation) {}
    /// Fixed stroke-two shot outcome.
    fn stroke_shot(&self, _outcome: GameShotObservation) {}
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

/// Checked optional synthetic exactly-two stroke composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeRuntimeConfig {
    /// Catalog-validated one-hole course.
    pub course: OneHoleConfig,
    /// Exact fingerprint of the loaded catalog.
    pub catalog_fingerprint: CatalogFingerprint,
    /// Actor-owned loading barrier deadline.
    pub loading_timeout: Duration,
    /// Actor-owned active-turn deadline.
    pub turn_timeout: Duration,
    /// Actor-owned whole-game deadline.
    pub game_timeout: Duration,
    /// Repository operation deadline.
    pub commit_timeout: Duration,
    /// Authoritative per-player stroke cap.
    pub max_strokes: u8,
    /// Startup recovery work cap.
    pub startup_recovery_limit: pangya_domain::IncompleteMatchAbortLimit,
    /// Per-connection action/result packet budget per shared rate window.
    pub shot_packets_per_window: u32,
}

/// Checked optional synthetic economy composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EconomyRuntimeConfig {
    /// Repository command deadline.
    pub command_timeout: Duration,
    /// Per-connection commands per shared rate window.
    pub commands_per_window: u32,
    /// Offers emitted per page.
    pub page_size: usize,
    /// Maximum purchase quantity accepted from the wire.
    pub max_purchase_quantity: u32,
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
    /// Optional local-only synthetic exactly-two stroke mode.
    pub stroke_two: Option<StrokeRuntimeConfig>,
    /// Optional local-only synthetic economy.
    pub economy: Option<EconomyRuntimeConfig>,
    /// Emits the reference-derived retail bootstrap instead of the synthetic one.
    ///
    /// Required by a real U.S. client, which never sends or understands the synthetic
    /// family. Reference-derived and unverified against a client, so it stays opt-in.
    pub retail_bootstrap: bool,
}

impl Default for GameRuntimeConfig {
    fn default() -> Self {
        Self {
            channel_id: 1,
            unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
            limits: GameRuntimeLimits::default(),
            solo_practice: None,
            stroke_two: None,
            economy: None,
            retail_bootstrap: false,
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
    /// Economy persistence failed with details redacted.
    #[error("GameService economy persistence failed")]
    EconomyPersistence,
    /// Connection or lobby drain exceeded grace.
    #[error("GameService graceful shutdown timed out")]
    ShutdownTimeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AbortResolution {
    Aborted,
    Committed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ConnectionStrokeContext {
    match_id: MatchId,
    roster: [PlayerConnectionId; 2],
    /// The participant this connection was last told owns the turn, so a handover can name
    /// the player whose turn ended. Retail has no phase frame that carries it.
    active: Option<PlayerConnectionId>,
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
    R: HandoverRepository + PlayerRepository + MatchRepository + EconomyRepository + 'static,
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
    R: HandoverRepository + PlayerRepository + MatchRepository + EconomyRepository + 'static,
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
    R: HandoverRepository + PlayerRepository + MatchRepository + EconomyRepository + 'static,
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
            || (config.stroke_two.is_some() && limits.outbound_room_event_capacity < 3)
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
            })
            || config.economy.is_some_and(|economy| {
                economy.command_timeout.is_zero()
                    || economy.command_timeout > limits.shutdown_grace
                    || economy.command_timeout > Duration::from_secs(60)
                    || economy.commands_per_window == 0
                    || economy.commands_per_window > 1_000_000
                    || economy.page_size == 0
                    || economy.page_size > pangya_protocol::MAX_SHOP_PAGE_ENTRIES
                    || economy.max_purchase_quantity == 0
                    || economy.max_purchase_quantity > pangya_protocol::MAX_PURCHASE_QUANTITY
            })
            || config.stroke_two.is_some_and(|stroke| {
                let wire_duration = |duration: Duration| {
                    duration.is_zero()
                        || duration.as_millis() == 0
                        || duration.as_millis() > u128::from(u32::MAX)
                };
                wire_duration(stroke.loading_timeout)
                    || stroke.loading_timeout > LOADING_TIMEOUT_HARD_CAP
                    || wire_duration(stroke.turn_timeout)
                    || stroke.turn_timeout > STROKE_GAME_TIMEOUT_HARD_CAP
                    || wire_duration(stroke.game_timeout)
                    || stroke.game_timeout > STROKE_GAME_TIMEOUT_HARD_CAP
                    || stroke.turn_timeout > stroke.game_timeout
                    || stroke.commit_timeout.is_zero()
                    || stroke.commit_timeout > limits.shutdown_grace
                    || stroke.commit_timeout > Duration::from_secs(60)
                    || !(1..=MAX_STROKE_STROKES).contains(&stroke.max_strokes)
                    || stroke.shot_packets_per_window == 0
                    || stroke.shot_packets_per_window > 1_000_000
            });
        if invalid {
            return Err(GameRuntimeError::InvalidConfig);
        }
        if config.economy.is_some()
            && (catalog.shop_offers().is_empty()
                || !catalog
                    .shop_offers()
                    .iter()
                    .any(|offer| offer.kind == ItemKind::Consumable))
        {
            return Err(GameRuntimeError::Catalog);
        }
        for (course, fingerprint) in config
            .solo_practice
            .map(|value| (value.course, value.catalog_fingerprint))
            .into_iter()
            .chain(
                config
                    .stroke_two
                    .map(|value| (value.course, value.catalog_fingerprint)),
            )
        {
            // The course must be one this catalog actually has, and the fingerprint must be
            // the one the configuration was resolved against, so a stale mode configuration
            // cannot start a match against a course the client cannot load.
            //
            // Par is only re-checked when the catalog has a par of its own. A real client
            // catalog does not: its Course table is a presentation row with no par field, so
            // par is operator-declared and there is nothing here to compare it against.
            let catalog_course = catalog
                .declared_one_hole_course(course.course_id(), course.par())
                .map_err(|_| GameRuntimeError::Catalog)?;
            if catalog_course != course || catalog.fingerprint() != fingerprint {
                return Err(GameRuntimeError::Catalog);
            }
            if let Ok(derived) = catalog.one_hole_course(course.course_id())
                && derived != course
            {
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
            for abort in outcome.aborts() {
                self.persist_shutdown_abort(*abort).await?;
            }
            for work in outcome.stroke() {
                self.persist_shutdown_stroke(*work).await?;
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
            // The two hellos are different lengths, so this is not cosmetic: a real client that
            // receives the four-byte synthetic hello reads the next frame at the wrong offset and
            // drops the connection, which it surfaces as "Server is full" on its server list.
            let hello: &[u8] = &if self.config.retail_bootstrap {
                us852_game_hello(key)
                    .map_err(|_| GameRuntimeError::Protocol)?
                    .to_vec()
            } else {
                synthetic_game_hello(key)
                    .map_err(|_| GameRuntimeError::Protocol)?
                    .to_vec()
            };
            stream
                .write_all(hello)
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
            Err(error) => {
                // The metric class is deliberately coarse, but discarding the error entirely left
                // a real-client failure here indistinguishable from any other, with nothing in the
                // log to work from. The variant name carries no player data.
                tracing::debug!(error = %error, "game connection failed");
                GameTermination::Error
            }
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
        let mut economy_commands = LocalRateWindow::new(self.config.limits.rate_window);
        // Retail shots are opaque client payloads, so the server counts strokes itself.
        let mut retail_strokes = 0_u32;
        let mut unknown_strikes = 0_u32;
        let (outbound, mut room_events) =
            mpsc::channel(self.config.limits.outbound_room_event_capacity);
        let room_cancellation = CancellationToken::new();
        let mut room_id: Option<RoomId> = None;
        let mut stroke_context: Option<ConnectionStrokeContext> = None;

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
                event = room_events.recv(), if matches!(state, GameState::InRoom | GameState::InMatchLoading | GameState::InMatch | GameState::InStrokeLoading | GameState::InStrokeMatch) => {
                    let Some(event) = event else { break Err(GameRuntimeError::Limited); };
                    let handled = tokio::select! {
                        biased;
                        () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                        handled = self.handle_room_event(
                            &mut framed,
                            state,
                            event,
                            room_id,
                            connection_id,
                            &mut stroke_context,
                        ) => handled,
                    };
                    match handled {
                        Ok(RoomEventEffect::Remain) => {}
                        Ok(RoomEventEffect::EnterChannel) => {
                            state = GameState::InChannel;
                            room_id = None;
                        }
                        Ok(RoomEventEffect::EnterRoom) => {
                            state = GameState::InRoom;
                            stroke_context = None;
                        }
                        Ok(RoomEventEffect::EnterLoading) => state = GameState::InMatchLoading,
                        Ok(RoomEventEffect::EnterMatch) => state = GameState::InMatch,
                        Ok(RoomEventEffect::EnterStrokeLoading) => state = GameState::InStrokeLoading,
                        Ok(RoomEventEffect::EnterStrokeMatch) => state = GameState::InStrokeMatch,
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
                            // A real client sends the one-byte sub-server ID documented for this
                            // opcode; the synthetic packet carries a `u32` channel ID.
                            let channel_id = if self.config.retail_bootstrap {
                                match decode_packet_payload::<RetailSelectChannel>(
                                    &frame.payload,
                                    &CompatibilityProfile::US_852,
                                    ServiceKind::Game,
                                ) {
                                    Ok(selected) => u32::from(selected.sub_server_id),
                                    Err(_) => break Err(GameRuntimeError::Protocol),
                                }
                            } else {
                                match decode_packet_payload::<SelectChannel>(
                                    &frame.payload,
                                    &CompatibilityProfile::US_852,
                                    ServiceKind::Game,
                                ) {
                                    Ok(selected) => selected.channel_id,
                                    Err(_) => break Err(GameRuntimeError::Protocol),
                                }
                            };
                            if channel_id != self.config.channel_id
                                || identity.is_none()
                                || presence.is_none()
                            {
                                break Err(GameRuntimeError::Protocol);
                            }
                            let sent = tokio::select! {
                                biased;
                                () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                                sent = async {
                                    if self.config.retail_bootstrap {
                                        // Upstream follows the connect response with this notice
                                        // before the client asks for anything.
                                        self.send(&mut framed, &RetailChannelJoined).await?;
                                        self.send(&mut framed, &RetailChannelJoinNotice).await
                                    } else {
                                        self.send(&mut framed, &ChannelJoined { channel_id }).await
                                    }
                                } => sent,
                            };
                            if let Err(error) = sent {
                                break Err(error);
                            }
                            state = GameState::InChannel;
                        }
                        GameState::InChannel | GameState::InRoom | GameState::InMatchLoading | GameState::InMatch | GameState::InStrokeLoading | GameState::InStrokeMatch => {
                            if matches!(frame.opcode, GameAuth::OPCODE | SelectChannel::OPCODE) {
                                break Err(GameRuntimeError::Protocol);
                            } else if self.config.retail_bootstrap
                                && matches!(
                                    frame.opcode,
                                    RetailLoginBonusRequest::OPCODE
                                        | RetailPlayerHistoryRequest::OPCODE
                                )
                            {
                                // A real client sends both of these the moment it finishes entering
                                // a channel. Leaving them unanswered means the lobby only survives
                                // under a permissive unknown-opcode policy.
                                if !frame.payload.is_empty() {
                                    break Err(GameRuntimeError::Protocol);
                                }
                                let sent = tokio::select! {
                                    biased;
                                    () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                                    sent = async {
                                        if frame.opcode == RetailLoginBonusRequest::OPCODE {
                                            self.send(&mut framed, &RetailLoginBonusStatus).await
                                        } else {
                                            self.send(&mut framed, &RetailPlayerHistory).await
                                        }
                                    } => sent,
                                };
                                if let Err(error) = sent {
                                    break Err(error);
                                }
                            } else if self.config.retail_bootstrap
                                && frame.opcode == RetailEquipmentUpdate::OPCODE
                            {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                if let Err(error) = self
                                    .handle_retail_equipment_update(
                                        &mut framed,
                                        established.account_id,
                                        &frame.payload,
                                    )
                                    .await
                                {
                                    break Err(error);
                                }
                            } else if self.config.retail_bootstrap
                                && frame.opcode == RetailPurchaseRequest::OPCODE
                            {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                if let Err(error) = self
                                    .handle_retail_purchase(
                                        &mut framed,
                                        established.account_id,
                                        &frame.payload,
                                    )
                                    .await
                                {
                                    break Err(error);
                                }
                            } else if self.config.retail_bootstrap
                                && matches!(
                                    frame.opcode,
                                    RetailShopJoin::OPCODE
                                        | RetailMyRoomEnter::OPCODE
                                        | RetailMyRoomInventoryRequest::OPCODE
                                        | RetailLockerInventoryRequest::OPCODE
                                        | RetailLockerCombinationAttempt::OPCODE
                                )
                            {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                let handled = self
                                    .handle_retail_lobby_service(
                                        &mut framed,
                                        established,
                                        frame.opcode,
                                        &frame.payload,
                                    )
                                    .await;
                                if let Err(error) = handled {
                                    break Err(error);
                                }
                            } else if self.config.retail_bootstrap
                                && is_retail_accepted_session_opcode(frame.opcode)
                            {
                                // Documented session-level chatter the client does not wait on.
                                // Accepting it explicitly keeps the unknown-opcode policy for
                                // opcodes that really are unrecognized.
                                self.observer.unknown(GameUnknownObservation::Ignored);
                            } else if is_known_economy_opcode(frame.opcode) {
                                if state != GameState::InChannel {
                                    break Err(GameRuntimeError::Protocol);
                                }
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                if let Some(economy) = self.config.economy
                                    && !economy_commands.admit_count(economy.commands_per_window)
                                {
                                    break Err(GameRuntimeError::Limited);
                                }
                                let handled = self
                                    .handle_economy_command(
                                        &mut framed,
                                        established.account_id,
                                        frame.opcode,
                                        &frame.payload,
                                    )
                                    .await;
                                if let Err(error) = handled {
                                    break Err(error);
                                }
                            } else if self.config.retail_bootstrap
                                && is_retail_match_opcode(frame.opcode)
                            {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                let handled = self
                                    .handle_retail_match_command(
                                        &mut framed,
                                        state,
                                        established,
                                        room_id,
                                        &shutdown,
                                        idle_deadline,
                                        &mut shots,
                                        &mut retail_strokes,
                                        frame.opcode,
                                        &frame.payload,
                                    )
                                    .await;
                                match handled {
                                    Ok(next) => state = next,
                                    Err(error) => break Err(error),
                                }
                            } else if self.config.retail_bootstrap
                                && is_retail_room_opcode(frame.opcode)
                            {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                if !commands.admit_count(self.config.limits.room_commands_per_window)
                                {
                                    self.observer
                                        .rate_limited(GameRateClass::RoomCommandsConnection);
                                    break Err(GameRuntimeError::Limited);
                                }
                                let handled = tokio::select! {
                                    biased;
                                    () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                                    handled = self.handle_retail_room_command(
                                        &mut framed,
                                        state,
                                        established,
                                        outbound.clone(),
                                        room_cancellation.clone(),
                                        frame.opcode,
                                        &frame.payload,
                                        &mut room_id,
                                    ) => handled,
                                };
                                match handled {
                                    Ok(next) => state = next,
                                    Err(error) => break Err(error),
                                }
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
                            } else if is_known_stroke_opcode(frame.opcode) {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                let handled = self
                                    .handle_stroke_command(
                                        &mut framed,
                                        state,
                                        established,
                                        room_id,
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
                    // Only client inactivity may consume the idle budget. Server-side work
                    // for this frame — snapshot loads, bootstrap writes, actor round trips —
                    // is charged by its own deadline, so a slow database must never shorten
                    // the window the client has to send its next packet.
                    idle_deadline = Instant::now() + self.config.limits.idle_timeout;
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
            .disconnect_with_work(connection_id, cleanup_reason)
            .await;
        let cleanup_result = match cleanup {
            Ok(room::RoomCloseOutcome::M5Abort { request, .. }) => {
                self.persist_cleanup_abort(request).await.map(drop)
            }
            Ok(
                outcome @ (room::RoomCloseOutcome::M6Abort { .. }
                | room::RoomCloseOutcome::M6Settlement { .. }),
            ) => {
                self.persist_connection_stroke_cleanup(outcome, &shutdown)
                    .await
            }
            Ok(room::RoomCloseOutcome::None)
            | Err(RoomError::NotMember | RoomError::RoomNotFound) => Ok(()),
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

    async fn handle_economy_command(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        account_id: AccountId,
        opcode: u16,
        payload: &[u8],
    ) -> Result<(), GameRuntimeError> {
        let profile = &CompatibilityProfile::US_852;
        let Some(economy) = self.config.economy else {
            let command = economy_command_for_opcode(opcode).ok_or(GameRuntimeError::Protocol)?;
            decode_economy_request_shape(opcode, payload)?;
            return self
                .send_economy_result(framed, command, EconomyOutcome::Disabled)
                .await;
        };
        match opcode {
            SYNTHETIC_M7_C2S_SHOP_PAGE => {
                let request =
                    decode_packet_payload::<ShopPageRequest>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let offers = self.catalog.shop_offers();
                let page_size = economy.page_size;
                let total_pages_usize = offers.len().div_ceil(page_size).max(1);
                let total_pages = u16::try_from(total_pages_usize)
                    .map_err(|_| GameRuntimeError::InvalidConfig)?;
                let page_index = usize::from(request.page());
                if page_index >= total_pages_usize {
                    return self
                        .send_economy_result(
                            framed,
                            EconomyCommand::ShopPage,
                            EconomyOutcome::Invalid,
                        )
                        .await;
                }
                let start = page_index
                    .checked_mul(page_size)
                    .ok_or(GameRuntimeError::InvalidConfig)?;
                let end = start.saturating_add(page_size).min(offers.len());
                let entries = offers[start..end]
                    .iter()
                    .copied()
                    .map(protocol_shop_offer)
                    .collect::<Result<Vec<_>, _>>()?;
                // The page itself is the success reply, so observe it here rather than in
                // `send_economy_result`, which this arm never reaches.
                self.observe_economy(EconomyCommand::ShopPage, EconomyOutcome::Success);
                self.send(
                    framed,
                    &ShopPage::new(request.page(), total_pages, entries)
                        .map_err(|_| GameRuntimeError::Catalog)?,
                )
                .await
            }
            SYNTHETIC_M7_C2S_PURCHASE => {
                let request = decode_packet_payload::<PurchaseRequestPacket>(
                    payload,
                    profile,
                    ServiceKind::Game,
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                if request.quantity() > economy.max_purchase_quantity {
                    return self
                        .send_economy_result(
                            framed,
                            EconomyCommand::Purchase,
                            EconomyOutcome::Invalid,
                        )
                        .await;
                }
                let type_id = ItemTypeId::new(request.type_id());
                let Some(definition) = self.catalog.shop_offer(type_id).copied() else {
                    return self
                        .send_economy_result(
                            framed,
                            EconomyCommand::Purchase,
                            EconomyOutcome::Invalid,
                        )
                        .await;
                };
                let input = PurchaseRequest {
                    account_id,
                    operation_id: EconomyOperationId::new(request.operation_id()),
                    catalog: self.catalog.fingerprint(),
                    definition,
                    quantity: request.quantity(),
                };
                let result =
                    timeout(economy.command_timeout, self.repository.purchase(input)).await;
                let committed = match result {
                    Err(_) => {
                        return self
                            .send_economy_result(
                                framed,
                                EconomyCommand::Purchase,
                                EconomyOutcome::Timeout,
                            )
                            .await;
                    }
                    Ok(Err(error)) => {
                        return self
                            .send_economy_error(framed, EconomyCommand::Purchase, error)
                            .await;
                    }
                    Ok(Ok(EconomyCommit::Committed(value) | EconomyCommit::Replayed(value))) => {
                        value
                    }
                };
                self.send_economy_result(framed, EconomyCommand::Purchase, EconomyOutcome::Success)
                    .await?;
                self.send(
                    framed,
                    &PurchaseCommitted::new(
                        committed.operation_id.get(),
                        committed
                            .inventory_id
                            .get()
                            .try_into()
                            .map_err(|_| GameRuntimeError::EconomyPersistence)?,
                        committed.item_type_id.get(),
                        committed.quantity_after,
                        committed.durability,
                        committed.pang_balance,
                    )
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?,
                )
                .await
            }
            SYNTHETIC_M7_C2S_EQUIP => {
                let request =
                    decode_packet_payload::<EquipRequest>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let snapshot = timeout(
                    economy.command_timeout,
                    self.repository.load_player_snapshot(account_id),
                )
                .await
                .map_err(|_| GameRuntimeError::Timeout)?
                .map_err(|_| GameRuntimeError::Snapshot)?;
                let character_id = i64::try_from(request.character_id())
                    .ok()
                    .and_then(|v| CharacterId::new(v).ok());
                let Some(character_id) = character_id else {
                    return self
                        .send_economy_result(framed, EconomyCommand::Equip, EconomyOutcome::Invalid)
                        .await;
                };
                let Some(character) = snapshot
                    .characters
                    .iter()
                    .find(|value| value.id == character_id)
                else {
                    return self
                        .send_economy_result(
                            framed,
                            EconomyCommand::Equip,
                            EconomyOutcome::NotOwned,
                        )
                        .await;
                };
                let selector =
                    |raw: Option<u64>,
                     expected: ItemKind|
                     -> Result<Option<EconomyItemSelector>, EconomyOutcome> {
                        let Some(raw) = raw else {
                            return Ok(None);
                        };
                        let id = i64::try_from(raw)
                            .ok()
                            .and_then(|value| InventoryItemId::new(value).ok())
                            .ok_or(EconomyOutcome::Invalid)?;
                        let item = snapshot
                            .inventory
                            .iter()
                            .find(|value| value.id == id)
                            .ok_or(EconomyOutcome::NotOwned)?;
                        let definition = self
                            .catalog
                            .item_definition(item.item_type_id)
                            .copied()
                            .ok_or(EconomyOutcome::Incompatible)?;
                        if definition.kind != expected {
                            return Err(EconomyOutcome::Incompatible);
                        }
                        Ok(Some(EconomyItemSelector {
                            inventory_id: id,
                            definition,
                        }))
                    };
                let club = match selector(request.club_id(), ItemKind::ClubSet) {
                    Ok(v) => v,
                    Err(o) => {
                        return self
                            .send_economy_result(framed, EconomyCommand::Equip, o)
                            .await;
                    }
                };
                let ball = match selector(request.ball_id(), ItemKind::Ball) {
                    Ok(v) => v,
                    Err(o) => {
                        return self
                            .send_economy_result(framed, EconomyCommand::Equip, o)
                            .await;
                    }
                };
                let input = EquipmentChange {
                    account_id,
                    operation_id: EconomyOperationId::new(request.operation_id()),
                    catalog: self.catalog.fingerprint(),
                    expected_version: request.expected_version(),
                    character_id,
                    character_type_id: character.item_type_id,
                    club,
                    ball,
                };
                let result = timeout(economy.command_timeout, self.repository.equip(input)).await;
                let committed = match result {
                    Err(_) => {
                        return self
                            .send_economy_result(
                                framed,
                                EconomyCommand::Equip,
                                EconomyOutcome::Timeout,
                            )
                            .await;
                    }
                    Ok(Err(e)) => {
                        return self
                            .send_economy_error(framed, EconomyCommand::Equip, e)
                            .await;
                    }
                    Ok(Ok(EconomyCommit::Committed(v) | EconomyCommit::Replayed(v))) => v,
                };
                self.send_economy_result(framed, EconomyCommand::Equip, EconomyOutcome::Success)
                    .await?;
                self.send(
                    framed,
                    &EquipmentChanged::new(
                        committed.operation_id.get(),
                        u64::try_from(committed.character_id.get())
                            .map_err(|_| GameRuntimeError::EconomyPersistence)?,
                        committed
                            .club_item_id
                            .map(|v| {
                                u64::try_from(v.get())
                                    .map_err(|_| GameRuntimeError::EconomyPersistence)
                            })
                            .transpose()?,
                        committed
                            .ball_item_id
                            .map(|v| {
                                u64::try_from(v.get())
                                    .map_err(|_| GameRuntimeError::EconomyPersistence)
                            })
                            .transpose()?,
                        committed.version,
                    )
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?,
                )
                .await
            }
            SYNTHETIC_M7_C2S_CONSUME | SYNTHETIC_M7_C2S_REPAIR => {
                let (command, operation_id, raw_id) = if opcode == SYNTHETIC_M7_C2S_CONSUME {
                    let request = decode_packet_payload::<ConsumeOneRequest>(
                        payload,
                        profile,
                        ServiceKind::Game,
                    )
                    .map_err(|_| GameRuntimeError::Protocol)?;
                    (
                        EconomyCommand::Consume,
                        request.operation_id(),
                        request.inventory_id(),
                    )
                } else {
                    let request =
                        decode_packet_payload::<RepairRequest>(payload, profile, ServiceKind::Game)
                            .map_err(|_| GameRuntimeError::Protocol)?;
                    (
                        EconomyCommand::Repair,
                        request.operation_id(),
                        request.inventory_id(),
                    )
                };
                let id = i64::try_from(raw_id)
                    .ok()
                    .and_then(|v| InventoryItemId::new(v).ok());
                let Some(id) = id else {
                    return self
                        .send_economy_result(framed, command, EconomyOutcome::Invalid)
                        .await;
                };
                let snapshot = timeout(
                    economy.command_timeout,
                    self.repository.load_player_snapshot(account_id),
                )
                .await
                .map_err(|_| GameRuntimeError::Timeout)?
                .map_err(|_| GameRuntimeError::Snapshot)?;
                let Some(item) = snapshot.inventory.iter().find(|value| value.id == id) else {
                    return self
                        .send_economy_result(framed, command, EconomyOutcome::NotOwned)
                        .await;
                };
                let Some(definition) = self.catalog.item_definition(item.item_type_id).copied()
                else {
                    return self
                        .send_economy_result(framed, command, EconomyOutcome::Incompatible)
                        .await;
                };
                let selector = EconomyItemSelector {
                    inventory_id: id,
                    definition,
                };
                if command == EconomyCommand::Consume {
                    let input = ConsumeItem {
                        account_id,
                        operation_id: EconomyOperationId::new(operation_id),
                        catalog: self.catalog.fingerprint(),
                        item: selector,
                    };
                    let result =
                        timeout(economy.command_timeout, self.repository.consume_one(input)).await;
                    let committed = match result {
                        Err(_) => {
                            return self
                                .send_economy_result(framed, command, EconomyOutcome::Timeout)
                                .await;
                        }
                        Ok(Err(e)) => return self.send_economy_error(framed, command, e).await,
                        Ok(Ok(EconomyCommit::Committed(v) | EconomyCommit::Replayed(v))) => v,
                    };
                    self.send_economy_result(framed, command, EconomyOutcome::Success)
                        .await?;
                    self.send(
                        framed,
                        &InventoryChanged::new(
                            committed.operation_id.get(),
                            u64::try_from(committed.inventory_id.get())
                                .map_err(|_| GameRuntimeError::EconomyPersistence)?,
                            committed.item_type_id.get(),
                            committed.quantity_after,
                            None,
                        )
                        .map_err(|_| GameRuntimeError::EconomyPersistence)?,
                    )
                    .await
                } else {
                    let input = RepairItem {
                        account_id,
                        operation_id: EconomyOperationId::new(operation_id),
                        catalog: self.catalog.fingerprint(),
                        item: selector,
                    };
                    let result =
                        timeout(economy.command_timeout, self.repository.repair(input)).await;
                    let committed = match result {
                        Err(_) => {
                            return self
                                .send_economy_result(framed, command, EconomyOutcome::Timeout)
                                .await;
                        }
                        Ok(Err(e)) => return self.send_economy_error(framed, command, e).await,
                        Ok(Ok(EconomyCommit::Committed(v) | EconomyCommit::Replayed(v))) => v,
                    };
                    self.send_economy_result(framed, command, EconomyOutcome::Success)
                        .await?;
                    self.send(
                        framed,
                        &RepairCommitted::new(
                            committed.operation_id.get(),
                            u64::try_from(committed.inventory_id.get())
                                .map_err(|_| GameRuntimeError::EconomyPersistence)?,
                            committed.durability,
                            committed.pang_balance,
                        )
                        .map_err(|_| GameRuntimeError::EconomyPersistence)?,
                    )
                    .await
                }
            }
            _ => Err(GameRuntimeError::Protocol),
        }
    }

    async fn send_economy_result(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        command: EconomyCommand,
        outcome: EconomyOutcome,
    ) -> Result<(), GameRuntimeError> {
        self.observe_economy(command, outcome);
        self.send(framed, &EconomyCommandResult::new(command, outcome))
            .await
    }

    /// Records one fixed-label economy command outcome; never carries identifiers.
    fn observe_economy(&self, command: EconomyCommand, outcome: EconomyOutcome) {
        let command = match command {
            EconomyCommand::ShopPage => GameEconomyCommand::ShopPage,
            EconomyCommand::Purchase => GameEconomyCommand::Purchase,
            EconomyCommand::Equip => GameEconomyCommand::Equip,
            EconomyCommand::Consume => GameEconomyCommand::Consume,
            EconomyCommand::Repair => GameEconomyCommand::Repair,
        };
        let outcome = match outcome {
            EconomyOutcome::Success => GameEconomyOutcome::Success,
            EconomyOutcome::Disabled => GameEconomyOutcome::Disabled,
            EconomyOutcome::Invalid => GameEconomyOutcome::Invalid,
            EconomyOutcome::NotOwned => GameEconomyOutcome::NotOwned,
            EconomyOutcome::Incompatible => GameEconomyOutcome::Incompatible,
            EconomyOutcome::InsufficientPang => GameEconomyOutcome::InsufficientPang,
            EconomyOutcome::StackFull => GameEconomyOutcome::StackFull,
            EconomyOutcome::VersionConflict => GameEconomyOutcome::VersionConflict,
            EconomyOutcome::IdempotencyDrift => GameEconomyOutcome::IdempotencyDrift,
            EconomyOutcome::Timeout => GameEconomyOutcome::Timeout,
        };
        self.observer.economy(command, outcome);
    }

    async fn send_economy_error(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        command: EconomyCommand,
        error: EconomyError,
    ) -> Result<(), GameRuntimeError> {
        let outcome = match error {
            EconomyError::Invalid => EconomyOutcome::Invalid,
            EconomyError::InsufficientPang => EconomyOutcome::InsufficientPang,
            EconomyError::NotOwned => EconomyOutcome::NotOwned,
            EconomyError::Incompatible | EconomyError::Expired | EconomyError::Depleted => {
                EconomyOutcome::Incompatible
            }
            EconomyError::StackFull => EconomyOutcome::StackFull,
            EconomyError::VersionConflict => EconomyOutcome::VersionConflict,
            EconomyError::IdempotencyDrift => EconomyOutcome::IdempotencyDrift,
            EconomyError::AccountInactive => EconomyOutcome::NotOwned,
            EconomyError::ArithmeticOverflow
            | EconomyError::CorruptData
            | EconomyError::Storage(_) => return Err(GameRuntimeError::EconomyPersistence),
        };
        self.send_economy_result(framed, command, outcome).await
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
        // A real client sends the retail `0x0002`, whose user id is a u32 and whose
        // handover bearer is the login key. The synthetic packet carries the same two
        // facts in a different shape, so both normalize to (claimed id, bearer).
        let (claimed_account_id, handover) = if self.config.retail_bootstrap {
            let auth = decode_packet_payload::<RetailGameAuth>(
                payload,
                &CompatibilityProfile::US_852,
                ServiceKind::Game,
            )
            .map_err(|error| {
                tracing::debug!(stage = "retail_auth_decode", %error, "game auth rejected");
                GameRuntimeError::Protocol
            })?;
            (u64::from(auth.user_id), auth.login_key)
        } else {
            let auth = decode_packet_payload::<GameAuth>(
                payload,
                &CompatibilityProfile::US_852,
                ServiceKind::Game,
            )
            .map_err(|_| GameRuntimeError::Protocol)?;
            (auth.claimed_account_id, auth.handover)
        };
        // Each rejection below is logged with the stage that produced it. The bearer itself is
        // never logged, only its length: an operator debugging a real client otherwise cannot
        // tell an unusable claimed id from a mangled login key from a consumed token.
        let claimed = i64::try_from(claimed_account_id)
            .ok()
            .and_then(|value| AccountId::new(value).ok())
            .ok_or_else(|| {
                tracing::debug!(stage = "claimed_account_id", "game auth rejected");
                self.observer.authentication("rejected");
                GameRuntimeError::Authentication
            })?;
        let bearer = std::str::from_utf8(&handover).map_err(|_| {
            tracing::debug!(
                stage = "bearer_utf8",
                bearer_bytes = handover.len(),
                "game auth rejected"
            );
            self.observer.authentication("rejected");
            GameRuntimeError::Authentication
        })?;
        let parsed = parse_handover(bearer).map_err(|_| {
            tracing::debug!(
                stage = "bearer_parse",
                bearer_bytes = handover.len(),
                "game auth rejected"
            );
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
            tracing::debug!(stage = "handover_consume", "game auth rejected");
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
                // Carried from the authenticated snapshot so a room roster can render the
                // player's character instead of an empty slot.
                character_id: Some(loaded.equipment.character_id),
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

    #[allow(clippy::too_many_arguments)]
    async fn handle_stroke_command(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        state: GameState,
        identity: &RoomIdentity,
        room_id: Option<RoomId>,
        shutdown: &CancellationToken,
        idle_deadline: Instant,
        shots: &mut LocalRateWindow,
        opcode: u16,
        payload: &[u8],
    ) -> Result<GameState, GameRuntimeError> {
        let stroke = self.config.stroke_two.ok_or(GameRuntimeError::Protocol)?;
        let profile = &CompatibilityProfile::US_852;
        match (state, opcode) {
            (GameState::InRoom, SYNTHETIC_M6_C2S_START_STROKE_TWO) => {
                decode_packet_payload::<StartStrokeTwo>(payload, profile, ServiceKind::Game)
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let snapshot = match self
                    .lobby
                    .route(identity.connection_id, LobbyRoomCommand::GetState)
                    .await
                {
                    Ok(LobbyRouteResult::Snapshot(snapshot)) => snapshot,
                    _ => return Err(GameRuntimeError::Protocol),
                };
                if snapshot.members().len() != 2 {
                    self.send_stroke_result(
                        framed,
                        StrokeCommand::Start,
                        StrokeCommandOutcome::InvalidPhase,
                    )
                    .await?;
                    return Ok(state);
                }
                let members = snapshot.members();
                let participants = [
                    StrokeParticipant::new(
                        members[0].account_id(),
                        StrokeRosterOrder::First,
                        MatchResultKey::new(uuid::Uuid::new_v4()),
                    ),
                    StrokeParticipant::new(
                        members[1].account_id(),
                        StrokeRosterOrder::Second,
                        MatchResultKey::new(uuid::Uuid::new_v4()),
                    ),
                ];
                let mut seed_bytes = [0_u8; 32];
                OsRng.fill_bytes(&mut seed_bytes);
                let seed = MatchSeed::new(seed_bytes);
                let (weather, wind) =
                    deterministic_conditions(seed).map_err(|_| GameRuntimeError::InvalidConfig)?;
                let begin = BeginStrokeMatch::new(
                    MatchId::new(uuid::Uuid::new_v4()),
                    MatchResultKey::new(uuid::Uuid::new_v4()),
                    participants,
                    stroke.course,
                    stroke.catalog_fingerprint,
                    seed,
                    weather,
                    wind,
                )
                .map_err(|_| GameRuntimeError::InvalidConfig)?;
                let plan = StrokeStartPlan::new(
                    begin,
                    [members[0].connection_id(), members[1].connection_id()],
                    stroke.loading_timeout,
                    stroke.turn_timeout,
                    stroke.game_timeout,
                    stroke.max_strokes,
                )
                .map_err(|_| GameRuntimeError::InvalidConfig)?;
                let begin = match self
                    .lobby
                    .route_stroke(
                        identity.connection_id,
                        LobbyStrokeCommand::PrepareStart(plan),
                    )
                    .await
                {
                    Ok(LobbyStrokeRouteResult::Begin(begin)) => begin,
                    Ok(_) => return Err(GameRuntimeError::Protocol),
                    Err(error) => {
                        self.send_stroke_result(
                            framed,
                            StrokeCommand::Start,
                            stroke_error_outcome(error),
                        )
                        .await?;
                        return Ok(state);
                    }
                };
                let room_id = room_id.ok_or(GameRuntimeError::Protocol)?;
                self.persist_and_confirm_stroke_begin(
                    identity.connection_id,
                    room_id,
                    begin,
                    shutdown,
                    idle_deadline,
                )
                .await?;
                self.send_stroke_result(
                    framed,
                    StrokeCommand::Start,
                    StrokeCommandOutcome::Success,
                )
                .await?;
                Ok(state)
            }
            (GameState::InStrokeLoading, SYNTHETIC_M6_C2S_LOADING_COMPLETE) => {
                let loading = decode_packet_payload::<StrokeLoadingComplete>(
                    payload,
                    profile,
                    ServiceKind::Game,
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                let routed = self
                    .lobby
                    .route_stroke(
                        identity.connection_id,
                        LobbyStrokeCommand::LoadingComplete(loading),
                    )
                    .await;
                match routed {
                    Ok(LobbyStrokeRouteResult::Loading(
                        StrokeLoadingOutcome::Waiting | StrokeLoadingOutcome::Duplicate,
                    )) => {}
                    Ok(LobbyStrokeRouteResult::Loading(
                        StrokeLoadingOutcome::PersistenceRequired(mark),
                    )) => {
                        self.persist_stroke_in_game(
                            identity.connection_id,
                            room_id.ok_or(GameRuntimeError::Protocol)?,
                            mark,
                            shutdown,
                            idle_deadline,
                        )
                        .await?;
                        self.observer
                            .stroke_match_event(GameMatchObservation::LoadingComplete);
                    }
                    Ok(_) => return Err(GameRuntimeError::Protocol),
                    Err(error) => {
                        self.send_stroke_result(
                            framed,
                            StrokeCommand::Load,
                            stroke_error_outcome(error),
                        )
                        .await?;
                        return Ok(state);
                    }
                }
                self.send_stroke_result(framed, StrokeCommand::Load, StrokeCommandOutcome::Success)
                    .await?;
                Ok(state)
            }
            (GameState::InStrokeMatch, SYNTHETIC_M6_C2S_SHOT_ACTION) => {
                if !shots.admit_count(stroke.shot_packets_per_window) {
                    self.observer
                        .rate_limited(GameRateClass::StrokePacketsConnection);
                    self.observer.stroke_shot(GameShotObservation::RateLimited);
                    self.send_stroke_result(
                        framed,
                        StrokeCommand::Action,
                        StrokeCommandOutcome::Timeout,
                    )
                    .await?;
                    return Ok(state);
                }
                let action =
                    decode_packet_payload::<StrokeShotAction>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let result = self
                    .lobby
                    .route_stroke(
                        identity.connection_id,
                        LobbyStrokeCommand::ShotAction(action),
                    )
                    .await;
                let outcome = observe_stroke_relay(self.observer.as_ref(), &result);
                self.send_stroke_result(framed, StrokeCommand::Action, outcome)
                    .await?;
                Ok(state)
            }
            (GameState::InStrokeMatch, SYNTHETIC_M6_C2S_SHOT_RESULT) => {
                if !shots.admit_count(stroke.shot_packets_per_window) {
                    self.observer
                        .rate_limited(GameRateClass::StrokePacketsConnection);
                    self.observer.stroke_shot(GameShotObservation::RateLimited);
                    self.send_stroke_result(
                        framed,
                        StrokeCommand::Result,
                        StrokeCommandOutcome::Timeout,
                    )
                    .await?;
                    return Ok(state);
                }
                let result =
                    decode_packet_payload::<StrokeShotResult>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let routed = self
                    .lobby
                    .route_stroke(
                        identity.connection_id,
                        LobbyStrokeCommand::ShotResult(result),
                    )
                    .await;
                let outcome = observe_stroke_result(self.observer.as_ref(), &routed);
                self.send_stroke_result(framed, StrokeCommand::Result, outcome)
                    .await?;
                Ok(state)
            }
            (GameState::InStrokeMatch, SYNTHETIC_M6_C2S_GIVE_UP) => {
                decode_packet_payload::<StrokeGiveUp>(payload, profile, ServiceKind::Game)
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let routed = self
                    .lobby
                    .route_stroke(identity.connection_id, LobbyStrokeCommand::GiveUp)
                    .await;
                let outcome = match routed {
                    Ok(LobbyStrokeRouteResult::Settlement(_)) => StrokeCommandOutcome::Success,
                    Ok(_) => return Err(GameRuntimeError::Protocol),
                    Err(error) => stroke_error_outcome(error),
                };
                self.send_stroke_result(framed, StrokeCommand::GiveUp, outcome)
                    .await?;
                Ok(state)
            }
            _ => Err(GameRuntimeError::Protocol),
        }
    }

    async fn persist_and_confirm_stroke_begin(
        &self,
        connection_id: PlayerConnectionId,
        room_id: RoomId,
        begin: BeginStrokeMatch,
        shutdown: &CancellationToken,
        idle_deadline: Instant,
    ) -> Result<(), GameRuntimeError> {
        let stroke = self
            .config
            .stroke_two
            .ok_or(GameRuntimeError::InvalidConfig)?;
        let persisted = if shutdown.is_cancelled() || Instant::now() >= idle_deadline {
            self.observer
                .stroke_commit(GameCommitObservation::Cancelled);
            None
        } else {
            match timeout(
                stroke.commit_timeout,
                self.repository.begin_stroke(begin.clone()),
            )
            .await
            {
                Ok(Ok(outcome)) => Some(outcome),
                Ok(Err(_)) => {
                    self.observer.stroke_commit(GameCommitObservation::Failed);
                    None
                }
                Err(_) => {
                    self.observer
                        .stroke_commit(GameCommitObservation::Cancelled);
                    None
                }
            }
        };
        let Some(persisted) = persisted else {
            self.abort_stroke_actor(connection_id, room_id, MatchAbortReason::PersistenceFailure)
                .await?;
            return Err(GameRuntimeError::MatchPersistence);
        };
        self.observer.stroke_commit(match persisted {
            BeginStrokeMatchOutcome::Begun => GameCommitObservation::Begun,
            BeginStrokeMatchOutcome::Existing => GameCommitObservation::Existing,
        });
        if shutdown.is_cancelled() || Instant::now() >= idle_deadline {
            self.abort_stroke_actor(connection_id, room_id, MatchAbortReason::PersistenceFailure)
                .await?;
            return Err(GameRuntimeError::Timeout);
        }
        match self
            .lobby
            .route_stroke(
                connection_id,
                LobbyStrokeCommand::ConfirmBegin {
                    match_id: begin.match_id(),
                    result_key: begin.result_key(),
                },
            )
            .await
        {
            Ok(LobbyStrokeRouteResult::Applied) => Ok(()),
            _ => {
                self.abort_stroke_actor(
                    connection_id,
                    room_id,
                    MatchAbortReason::PersistenceFailure,
                )
                .await?;
                Err(GameRuntimeError::MatchPersistence)
            }
        }
    }

    async fn persist_stroke_in_game(
        &self,
        connection_id: PlayerConnectionId,
        room_id: RoomId,
        mark: MarkStrokeInGame,
        shutdown: &CancellationToken,
        idle_deadline: Instant,
    ) -> Result<(), GameRuntimeError> {
        let stroke = self
            .config
            .stroke_two
            .ok_or(GameRuntimeError::InvalidConfig)?;
        let marked = if shutdown.is_cancelled() || Instant::now() >= idle_deadline {
            None
        } else {
            match timeout(
                stroke.commit_timeout,
                self.repository.mark_stroke_in_game(mark),
            )
            .await
            {
                Ok(Ok(MarkStrokeInGameOutcome::Marked | MarkStrokeInGameOutcome::Existing)) => {
                    Some(())
                }
                Ok(Err(_)) | Err(_) => None,
            }
        };
        if marked.is_some() && !shutdown.is_cancelled() && Instant::now() < idle_deadline {
            self.lobby
                .apply_stroke_in_game_by_room(room_id, mark)
                .await
                .map_err(|_| GameRuntimeError::MatchPersistence)?;
            return Ok(());
        }
        self.abort_stroke_actor(connection_id, room_id, MatchAbortReason::PersistenceFailure)
            .await?;
        Err(GameRuntimeError::MatchPersistence)
    }

    async fn abort_stroke_actor(
        &self,
        connection_id: PlayerConnectionId,
        room_id: RoomId,
        reason: MatchAbortReason,
    ) -> Result<AbortResolution, GameRuntimeError> {
        let routed = self
            .lobby
            .route_stroke(connection_id, LobbyStrokeCommand::Abort(reason))
            .await
            .map_err(|_| GameRuntimeError::MatchPersistence)?;
        let LobbyStrokeRouteResult::Abort(Some(abort)) = routed else {
            return Err(GameRuntimeError::MatchPersistence);
        };
        self.persist_stroke_abort_by_room(room_id, abort, true)
            .await
    }

    async fn persist_connection_stroke_cleanup(
        &self,
        outcome: room::RoomCloseOutcome,
        shutdown: &CancellationToken,
    ) -> Result<(), GameRuntimeError> {
        tokio::task::yield_now().await;
        let (room_id, original_abort, settlement) = match outcome {
            room::RoomCloseOutcome::M6Abort { room_id, request } => (room_id, Some(request), None),
            room::RoomCloseOutcome::M6Settlement { room_id, request } => {
                (room_id, None, Some(request))
            }
            _ => return Err(GameRuntimeError::MatchPersistence),
        };
        if shutdown.is_cancelled() {
            return self
                .persist_cancelled_stroke_cleanup(room_id, original_abort)
                .await;
        }
        if let Some(abort) = original_abort {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => self.persist_cancelled_stroke_cleanup(room_id, Some(abort)).await,
                result = self.persist_stroke_abort_by_room(room_id, abort, true) => result.map(drop),
            }
        } else if let Some(commit) = settlement {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => self.persist_cancelled_stroke_cleanup(room_id, None).await,
                result = self.persist_stroke_commit_by_room(room_id, commit) => result,
            }
        } else {
            Err(GameRuntimeError::MatchPersistence)
        }
    }

    async fn persist_cancelled_stroke_cleanup(
        &self,
        room_id: RoomId,
        original_abort: Option<AbortStrokeMatch>,
    ) -> Result<(), GameRuntimeError> {
        if let Some(abort) = original_abort
            && abort.reason() == MatchAbortReason::Shutdown
        {
            return self
                .persist_stroke_abort_by_room(room_id, abort, true)
                .await
                .map(drop);
        }
        self.persist_priority_shutdown_stroke(room_id).await
    }

    async fn persist_priority_shutdown_stroke(
        &self,
        room_id: RoomId,
    ) -> Result<(), GameRuntimeError> {
        let abort = self
            .lobby
            .prioritize_stroke_abort_by_room(room_id, MatchAbortReason::Shutdown)
            .await
            .map_err(|_| GameRuntimeError::MatchPersistence)?;
        self.persist_stroke_abort_by_room(room_id, abort, true)
            .await
            .map(drop)
    }

    async fn persist_stroke_commit_by_room(
        &self,
        room_id: RoomId,
        commit: pangya_domain::CommitStrokeMatch,
    ) -> Result<(), GameRuntimeError> {
        let stroke = self
            .config
            .stroke_two
            .ok_or(GameRuntimeError::InvalidConfig)?;
        let completions = commit.players().map(|player| player.completion());
        if completions.contains(&DomainStrokeCompletion::TurnTimeout) {
            self.observer
                .stroke_match_event(GameMatchObservation::TurnTimeout);
        }
        if completions.contains(&DomainStrokeCompletion::GameTimeout) {
            self.observer
                .stroke_match_event(GameMatchObservation::GameTimeout);
        }
        if completions.iter().any(|completion| completion.is_forfeit()) {
            self.observer
                .stroke_match_event(GameMatchObservation::Forfeit);
        }
        let committed = match timeout(
            stroke.commit_timeout,
            self.repository.commit_stroke_match(commit),
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                self.observer.stroke_commit(GameCommitObservation::Failed);
                let abort = AbortStrokeMatch::new(
                    commit.match_id(),
                    commit.result_key(),
                    MatchAbortReason::PersistenceFailure,
                );
                return match self
                    .persist_stroke_abort_by_room(room_id, abort, true)
                    .await?
                {
                    AbortResolution::Committed => Ok(()),
                    AbortResolution::Aborted => Err(GameRuntimeError::MatchPersistence),
                };
            }
            Err(_) => {
                self.observer
                    .stroke_commit(GameCommitObservation::Cancelled);
                let abort = AbortStrokeMatch::new(
                    commit.match_id(),
                    commit.result_key(),
                    MatchAbortReason::PersistenceFailure,
                );
                return match self
                    .persist_stroke_abort_by_room(room_id, abort, true)
                    .await?
                {
                    AbortResolution::Committed => Ok(()),
                    AbortResolution::Aborted => Err(GameRuntimeError::MatchPersistence),
                };
            }
        };
        self.observer
            .stroke_commit(GameCommitObservation::Committed);
        self.lobby
            .apply_stroke_commit_by_room(room_id, committed)
            .await
            .map_err(|_| GameRuntimeError::MatchPersistence)?;
        self.observer
            .stroke_match_event(GameMatchObservation::Finished);
        Ok(())
    }

    async fn persist_stroke_abort_by_room(
        &self,
        room_id: RoomId,
        abort: AbortStrokeMatch,
        classify_terminal: bool,
    ) -> Result<AbortResolution, GameRuntimeError> {
        let stroke = self
            .config
            .stroke_two
            .ok_or(GameRuntimeError::InvalidConfig)?;
        let outcome = timeout(stroke.commit_timeout, self.repository.abort_stroke(abort))
            .await
            .map_err(|_| GameRuntimeError::MatchPersistence)?;
        match outcome {
            Ok(AbortStrokeMatchOutcome::Aborted | AbortStrokeMatchOutcome::AlreadyAborted)
            | Err(pangya_domain::MatchRepositoryError::NotFound) => {
                self.lobby
                    .acknowledge_stroke_abort_by_room(room_id, abort)
                    .await
                    .map_err(|_| GameRuntimeError::MatchPersistence)?;
                if classify_terminal {
                    observe_stroke_abort_terminal(self.observer.as_ref(), abort.reason());
                }
                Ok(AbortResolution::Aborted)
            }
            Ok(AbortStrokeMatchOutcome::AlreadyCommitted(result)) => {
                self.observer
                    .stroke_commit(GameCommitObservation::Idempotent);
                self.lobby
                    .apply_stroke_commit_by_room(room_id, result)
                    .await
                    .map_err(|_| GameRuntimeError::MatchPersistence)?;
                if classify_terminal {
                    self.observer
                        .stroke_match_event(GameMatchObservation::Finished);
                }
                Ok(AbortResolution::Committed)
            }
            Err(_) => Err(GameRuntimeError::MatchPersistence),
        }
    }

    async fn send_stroke_result(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        command: StrokeCommand,
        outcome: StrokeCommandOutcome,
    ) -> Result<(), GameRuntimeError> {
        self.send(framed, &StrokeCommandResult::new(command, outcome))
            .await
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
        state: GameState,
        event: RoomEvent,
        room_id: Option<RoomId>,
        connection_id: PlayerConnectionId,
        stroke_context: &mut Option<ConnectionStrokeContext>,
    ) -> Result<RoomEventEffect, GameRuntimeError> {
        let solo_event = matches!(
            &event,
            RoomEvent::SoloStarted(_)
                | RoomEvent::SoloPhase { .. }
                | RoomEvent::SoloActionRelay { .. }
                | RoomEvent::SoloResultRelay { .. }
                | RoomEvent::AbortRequested(_)
                | RoomEvent::SoloCommitted(_)
        );
        let stroke_event = matches!(
            &event,
            RoomEvent::RetailRelay { .. }
                | RoomEvent::StrokeStarted(_)
                | RoomEvent::StrokePhase { .. }
                | RoomEvent::StrokeTurn(_)
                | RoomEvent::StrokeActionRelay { .. }
                | RoomEvent::StrokeResultRelay { .. }
                | RoomEvent::StrokeSettlementRequested(_)
                | RoomEvent::StrokeAbortRequested(_)
                | RoomEvent::StrokeCommitted(_)
                | RoomEvent::StrokeAborted(_)
        );
        if (solo_event && matches!(state, GameState::InStrokeLoading | GameState::InStrokeMatch))
            || (stroke_event && matches!(state, GameState::InMatchLoading | GameState::InMatch))
        {
            return Err(GameRuntimeError::Protocol);
        }
        // Lobby/room broadcasts are still encoded in the synthetic family. Emitting one
        // into a retail stream would desynchronize the client far more badly than the
        // missing information does, so in retail mode the ones with no retail equivalent are
        // dropped while their state effect is preserved. See
        // `docs/protocol/US852_RETAIL_BOOTSTRAP.md`.
        if self.config.retail_bootstrap {
            match &event {
                // The census is the retail form of a room snapshot, and a client sitting in a
                // room learns of everyone else only from it: without this the roster never
                // changes on screen, and a host waiting for a full room is never offered
                // Start. Only in the room — a census mid-hole would contradict the match.
                RoomEvent::Snapshot(room) => {
                    if state == GameState::InRoom {
                        self.send(framed, &retail_census_from_snapshot(room))
                            .await?;
                        self.observer.room(GameRoomObservation::StateSent);
                    }
                    return Ok(RoomEventEffect::Remain);
                }
                RoomEvent::Chat { .. } => {
                    return Ok(RoomEventEffect::Remain);
                }
                RoomEvent::SoloStarted(plan) => {
                    let begin = plan.begin();
                    // A solo hole has no turn arbitration, so the client's own timers are the
                    // only ones, and it is told the same defaults it shows for practice.
                    self.send_retail_hole_intro(
                        framed,
                        begin.config().course_id().get(),
                        begin.weather(),
                        begin.wind(),
                        RETAIL_SOLO_SHOT_TIMER,
                        RETAIL_SOLO_GAME_TIMER,
                    )
                    .await?;
                    return Ok(RoomEventEffect::EnterLoading);
                }
                RoomEvent::SoloPhase { phase, .. } => {
                    // The actor republishes this phase after every stroke, so the intro is
                    // emitted only on the first transition out of loading. Later turns are
                    // handed back explicitly by the shot handler.
                    if matches!(phase, SoloMatchPhase::AwaitAction { .. })
                        && state == GameState::InMatchLoading
                    {
                        let connection = u32::try_from(connection_id.get()).unwrap_or(0);
                        self.send(
                            framed,
                            &RetailPlayerStartHole {
                                connection_id: connection,
                            },
                        )
                        .await?;
                        self.send(
                            framed,
                            &RetailTurnStart {
                                connection_id: connection,
                            },
                        )
                        .await?;
                        return Ok(RoomEventEffect::EnterMatch);
                    }
                    return Ok(RoomEventEffect::Remain);
                }
                RoomEvent::Kicked { .. } => {
                    self.observer.room(GameRoomObservation::Kicked);
                    return Ok(RoomEventEffect::EnterChannel);
                }
                RoomEvent::Closed => return Ok(RoomEventEffect::EnterChannel),
                // Shot relays are self-echo in a solo hole and are already covered by the
                // explicit retail turn frames, so they carry no information here.
                RoomEvent::SoloActionRelay { .. } | RoomEvent::SoloResultRelay { .. } => {
                    return Ok(RoomEventEffect::Remain);
                }
                // Settlement is acknowledged by the retail hole-finished frame the command
                // handler sends, so the synthetic result/balance pair is redundant.
                RoomEvent::SoloCommitted(_) => return Ok(RoomEventEffect::EnterRoom),
                RoomEvent::StrokeStarted(plan) => {
                    let begin = plan.begin();
                    // The timers the client counts down with are the ones the room actor will
                    // actually enforce, so a shot that runs out on screen is the shot the
                    // server forfeits.
                    self.send_retail_hole_intro(
                        framed,
                        begin.config().course_id().get(),
                        begin.weather(),
                        begin.wind(),
                        plan.turn_timeout(),
                        plan.game_timeout(),
                    )
                    .await?;
                    *stroke_context = Some(ConnectionStrokeContext {
                        match_id: begin.match_id(),
                        roster: *plan.roster(),
                        active: None,
                    });
                    return Ok(RoomEventEffect::EnterStrokeLoading);
                }
                // Phase is carried by the turn frames a retail client actually reads.
                RoomEvent::StrokePhase { .. } => return Ok(RoomEventEffect::Remain),
                RoomEvent::StrokeTurn(phase) => {
                    let StrokeMatchPhase::AwaitAction { active, .. } = *phase else {
                        return Err(GameRuntimeError::Protocol);
                    };
                    let context = stroke_context.as_mut().ok_or(GameRuntimeError::Protocol)?;
                    let connection = |id: PlayerConnectionId| u32::try_from(id.get()).unwrap_or(0);
                    match context.active {
                        // The first turn of the hole is introduced rather than handed over.
                        None => {
                            self.send(
                                framed,
                                &RetailPlayerStartHole {
                                    connection_id: connection(active),
                                },
                            )
                            .await?;
                        }
                        Some(previous) if previous != active => {
                            self.send(
                                framed,
                                &RetailTurnEnd {
                                    connection_id: connection(previous),
                                },
                            )
                            .await?;
                        }
                        Some(_) => {}
                    }
                    self.send(
                        framed,
                        &RetailTurnStart {
                            connection_id: connection(active),
                        },
                    )
                    .await?;
                    context.active = Some(active);
                    return Ok(if state == GameState::InStrokeLoading {
                        RoomEventEffect::EnterStrokeMatch
                    } else {
                        RoomEventEffect::Remain
                    });
                }
                // The relays a retail client reads are the client's own frames, forwarded by
                // the retail relay event. The synthetic pair carries nothing it understands.
                RoomEvent::StrokeActionRelay { .. } | RoomEvent::StrokeResultRelay { .. } => {
                    return Ok(RoomEventEffect::Remain);
                }
                RoomEvent::RetailRelay { from, relay } => {
                    let from = u32::try_from(from.get()).unwrap_or(0);
                    match relay {
                        RetailMatchRelay::Shot(body) => {
                            self.send(
                                framed,
                                &RetailShotCommitRelay {
                                    connection_id: from,
                                    shot: body.clone(),
                                },
                            )
                            .await?;
                        }
                        RetailMatchRelay::Sync(body) => {
                            self.send(framed, &RetailShotSync { data: body.clone() })
                                .await?;
                        }
                    }
                    return Ok(RoomEventEffect::Remain);
                }
                RoomEvent::StrokeCommitted(result) => {
                    self.send_retail_stroke_committed(framed, *result, *stroke_context)
                        .await?;
                    *stroke_context = None;
                    return Ok(RoomEventEffect::EnterRoom);
                }
                // An abort pays nothing, and there is no retail frame that says so. The room
                // is what the client returns to either way.
                RoomEvent::StrokeAborted(_) => {
                    *stroke_context = None;
                    return Ok(RoomEventEffect::EnterRoom);
                }
                _ => {}
            }
        }
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
            RoomEvent::StrokeStarted(plan) => {
                if state != GameState::InRoom || self.config.stroke_two.is_none() {
                    return Err(GameRuntimeError::Protocol);
                }
                let begin = plan.begin();
                let weather = protocol_weather(begin.weather());
                let wind = protocol_wind(begin.wind())?;
                let millis = |duration: Duration| {
                    u32::try_from(duration.as_millis()).map_err(|_| GameRuntimeError::InvalidConfig)
                };
                let roster = *plan.roster();
                self.send(
                    framed,
                    &StrokeMatchStarted::new(
                        begin.match_id().get(),
                        begin.config().course_id().get(),
                        begin.config().par(),
                        *begin.seed().as_bytes(),
                        weather,
                        wind,
                        millis(plan.loading_timeout())?,
                        millis(plan.turn_timeout())?,
                        millis(plan.game_timeout())?,
                        roster[0].get(),
                        roster[1].get(),
                    )
                    .map_err(|_| GameRuntimeError::Protocol)?,
                )
                .await?;
                *stroke_context = Some(ConnectionStrokeContext {
                    match_id: begin.match_id(),
                    roster,
                    active: None,
                });
                Ok(RoomEventEffect::EnterStrokeLoading)
            }
            RoomEvent::StrokePhase { match_id, phase } => {
                let protocol_phase = match phase {
                    StrokeMatchPhase::Loading { .. } => Some(StrokePhaseKind::Loading),
                    StrokeMatchPhase::AwaitAction { .. } | StrokeMatchPhase::AwaitResult { .. } => {
                        Some(StrokePhaseKind::Playing)
                    }
                    StrokeMatchPhase::ResultsPending => Some(StrokePhaseKind::ResultsPending),
                    StrokeMatchPhase::Open
                    | StrokeMatchPhase::Starting
                    | StrokeMatchPhase::LoadingPersistencePending
                    | StrokeMatchPhase::Aborted => None,
                };
                if let Some(protocol_phase) = protocol_phase {
                    self.send(framed, &StrokePhase::new(match_id.get(), protocol_phase))
                        .await?;
                }
                if matches!(
                    phase,
                    StrokeMatchPhase::AwaitAction { .. } | StrokeMatchPhase::AwaitResult { .. }
                ) {
                    Ok(RoomEventEffect::EnterStrokeMatch)
                } else {
                    Ok(RoomEventEffect::Remain)
                }
            }
            RoomEvent::StrokeTurn(phase) => {
                let StrokeMatchPhase::AwaitAction {
                    active,
                    turn,
                    sequence,
                } = phase
                else {
                    return Err(GameRuntimeError::Protocol);
                };
                let context = stroke_context.ok_or(GameRuntimeError::Protocol)?;
                let timeout_ms = self
                    .config
                    .stroke_two
                    .and_then(|config| u32::try_from(config.turn_timeout.as_millis()).ok())
                    .ok_or(GameRuntimeError::InvalidConfig)?;
                self.send(
                    framed,
                    &StrokeTurnStarted::new(
                        context.match_id.get(),
                        turn,
                        active.get(),
                        sequence,
                        timeout_ms,
                    )
                    .map_err(|_| GameRuntimeError::Protocol)?,
                )
                .await?;
                Ok(RoomEventEffect::Remain)
            }
            RoomEvent::StrokeActionRelay { from, action } => {
                self.send(
                    framed,
                    &StrokeActionRelay::new(from.get(), action)
                        .map_err(|_| GameRuntimeError::Protocol)?,
                )
                .await?;
                Ok(RoomEventEffect::Remain)
            }
            RoomEvent::StrokeResultRelay { from, result } => {
                self.send(
                    framed,
                    &StrokeResultRelay::new(from.get(), result)
                        .map_err(|_| GameRuntimeError::Protocol)?,
                )
                .await?;
                Ok(RoomEventEffect::Remain)
            }
            // Only a retail connection produces one of these, and only a retail connection can
            // read one. Reaching a synthetic stream means the room mixed the two families.
            RoomEvent::RetailRelay { .. } => Err(GameRuntimeError::Protocol),
            RoomEvent::StrokeSettlementRequested(commit) => {
                self.persist_stroke_commit_by_room(
                    room_id.ok_or(GameRuntimeError::Protocol)?,
                    commit,
                )
                .await?;
                Ok(RoomEventEffect::Remain)
            }
            RoomEvent::StrokeAbortRequested(abort) => {
                self.persist_stroke_abort_by_room(
                    room_id.ok_or(GameRuntimeError::Protocol)?,
                    abort,
                    true,
                )
                .await?;
                Ok(RoomEventEffect::Remain)
            }
            RoomEvent::StrokeCommitted(result) => {
                self.send_stroke_committed(framed, connection_id, result, *stroke_context)
                    .await?;
                Ok(RoomEventEffect::EnterRoom)
            }
            RoomEvent::StrokeAborted(abort) => {
                self.send(
                    framed,
                    &StrokeMatchAborted::new(
                        abort.match_id().get(),
                        protocol_stroke_abort_reason(abort.reason()),
                    ),
                )
                .await?;
                Ok(RoomEventEffect::EnterRoom)
            }
        }
    }

    async fn send_stroke_committed(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        connection_id: PlayerConnectionId,
        result: StrokeMatchResult,
        context: Option<ConnectionStrokeContext>,
    ) -> Result<(), GameRuntimeError> {
        let context = context.ok_or(GameRuntimeError::Protocol)?;
        if context.match_id != result.match_id() {
            return Err(GameRuntimeError::Protocol);
        }
        let entries = [0_usize, 1_usize].map(|index| {
            let player = result.players()[index];
            StrokeStandingEntry::new(
                context.roster[index].get(),
                player.place().get(),
                protocol_stroke_completion(player.completion()),
                player.strokes(),
                player.score(),
                player.pang_reward(),
                player.experience_reward(),
                player.participant().player_result_key().get(),
            )
            .map_err(|_| GameRuntimeError::Protocol)
        });
        let first = entries[0]?;
        let second = entries[1]?;
        let ordered = if first.place() == 1 {
            [first, second]
        } else {
            [second, first]
        };
        self.send(
            framed,
            &StrokeStandings::new(result.match_id().get(), ordered)
                .map_err(|_| GameRuntimeError::Protocol)?,
        )
        .await?;
        let own_index = context
            .roster
            .iter()
            .position(|candidate| *candidate == connection_id)
            .ok_or(GameRuntimeError::Protocol)?;
        let own = result.players()[own_index];
        self.send(
            framed,
            &StrokeBalanceUpdate::new(own.pang_balance(), own.experience_balance()),
        )
        .await?;
        self.send(
            framed,
            &StrokePhase::new(result.match_id().get(), StrokePhaseKind::Finished),
        )
        .await
    }

    /// Sends the four frames a retail client needs before it will load a hole.
    ///
    /// The plan is one hole because that is what this server settles; the client reads the
    /// whole plan up front, so an incomplete one strands it on the loading screen.
    async fn send_retail_hole_intro(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        course_id: u32,
        weather: pangya_domain::Weather,
        wind: pangya_domain::WindConditions,
        shot_timer: Duration,
        game_timer: Duration,
    ) -> Result<(), GameRuntimeError> {
        let millis = |duration: Duration| {
            u32::try_from(duration.as_millis()).map_err(|_| GameRuntimeError::InvalidConfig)
        };
        let weather = match weather {
            pangya_domain::Weather::Clear => RetailWeather::Clear,
            pangya_domain::Weather::Cloudy => RetailWeather::Cloudy,
            pangya_domain::Weather::Rain => RetailWeather::Raining,
        };
        let course = u8::try_from(course_id).unwrap_or(0);
        self.send(
            framed,
            &RetailMatchStart {
                room_ui_type: 0,
                start_time: [0; 16],
            },
        )
        .await?;
        self.send(
            framed,
            &RetailMatchInfo {
                course,
                room_ui_type: 0,
                hole_mode: 0,
                hole_count: 1,
                shot_timer_ms: millis(shot_timer)?,
                game_timer_ms: millis(game_timer)?,
                holes: vec![RetailHole {
                    random_id: 1,
                    pin: 0,
                    course,
                    number: 1,
                }],
                random_seed: 1,
            },
        )
        .await?;
        self.send(framed, &RetailHoleWeather { weather }).await?;
        self.send(
            framed,
            &RetailHoleWind {
                strength: u8::try_from(wind.speed_tenths() / 10).unwrap_or(u8::MAX),
                direction: wind.angle_degrees(),
            },
        )
        .await
    }

    /// Puts the durable settlement of a two-player hole on a retail client's results screen.
    ///
    /// Every figure here is the committed server-side result. A forfeit has no golf score, so
    /// its line reports zero rather than inventing one.
    async fn send_retail_stroke_committed(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        result: StrokeMatchResult,
        context: Option<ConnectionStrokeContext>,
    ) -> Result<(), GameRuntimeError> {
        let context = context.ok_or(GameRuntimeError::Protocol)?;
        if context.match_id != result.match_id() {
            return Err(GameRuntimeError::Protocol);
        }
        let mut standings = [0_usize, 1_usize]
            .map(|index| {
                let player = result.players()[index];
                RetailStanding {
                    connection_id: u32::try_from(context.roster[index].get()).unwrap_or(0),
                    place: player.place().get(),
                    score: player
                        .score()
                        .and_then(|score| i8::try_from(score).ok())
                        .unwrap_or(0),
                    experience: u16::try_from(player.experience_reward()).unwrap_or(u16::MAX),
                    pang: player.pang_reward(),
                    bonus_pang: 0,
                }
            })
            .to_vec();
        standings.sort_by_key(|standing| standing.place);
        self.send(framed, &RetailFinishHole).await?;
        self.send(framed, &RetailMatchFinish { standings }).await
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

    async fn persist_shutdown_stroke(
        &self,
        work: LobbyStrokePersistence,
    ) -> Result<(), GameRuntimeError> {
        let stroke = self
            .config
            .stroke_two
            .ok_or(GameRuntimeError::InvalidConfig)?;
        match work {
            LobbyStrokePersistence::Abort { request, .. } => {
                match timeout(stroke.commit_timeout, self.repository.abort_stroke(request)).await {
                    Ok(Ok(
                        AbortStrokeMatchOutcome::Aborted
                        | AbortStrokeMatchOutcome::AlreadyAborted
                        | AbortStrokeMatchOutcome::AlreadyCommitted(_),
                    ))
                    | Ok(Err(pangya_domain::MatchRepositoryError::NotFound)) => Ok(()),
                    Ok(Err(_)) | Err(_) => Err(GameRuntimeError::MatchPersistence),
                }
            }
            LobbyStrokePersistence::Settlement { request, .. } => timeout(
                stroke.commit_timeout,
                self.repository.commit_stroke_match(request),
            )
            .await
            .map_err(|_| GameRuntimeError::MatchPersistence)?
            .map(drop)
            .map_err(|_| GameRuntimeError::MatchPersistence),
        }
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
        if self.config.retail_bootstrap {
            return self.send_retail_bootstrap(framed, snapshot).await;
        }
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

    /// Handles one retail match command by driving a durable match lifecycle.
    ///
    /// The retail wire differs from the synthetic one, but the underlying lifecycles —
    /// reserve, load, play, settle exactly once — are the same and already durable, so this
    /// maps retail signals onto them rather than duplicating a state machine.
    ///
    /// Which lifecycle depends on the room: a real client will not start a versus room that
    /// is not full, and the smallest capacity its Make Room dialog offers is two, so the
    /// ordinary retail start is a two-player one and runs on the stroke aggregate with its
    /// turn arbitration. A room holding one player still runs the solo lifecycle.
    ///
    /// Shot payloads are the client's own. Their content is relayed but never interpreted;
    /// what this server counts is strokes and whose turn it is.
    #[allow(clippy::too_many_arguments)]
    async fn handle_retail_match_command(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        state: GameState,
        identity: &RoomIdentity,
        room_id: Option<RoomId>,
        shutdown: &CancellationToken,
        idle_deadline: Instant,
        shots: &mut LocalRateWindow,
        strokes: &mut u32,
        opcode: u16,
        payload: &[u8],
    ) -> Result<GameState, GameRuntimeError> {
        if let Some(next) = self
            .handle_retail_stroke_command(
                framed,
                state,
                identity,
                room_id,
                shutdown,
                idle_deadline,
                shots,
                strokes,
                opcode,
                payload,
            )
            .await?
        {
            return Ok(next);
        }
        let solo = self
            .config
            .solo_practice
            .ok_or(GameRuntimeError::Protocol)?;
        match (state, opcode) {
            (GameState::InRoom, RETAIL_C2S_START_MATCH) => {
                *strokes = 0;
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
                    // A refused start leaves the client in the room rather than closing it.
                    Err(_) => return Ok(GameState::InRoom),
                };
                self.persist_and_confirm_begin(
                    identity.connection_id,
                    begin,
                    shutdown,
                    idle_deadline,
                )
                .await?;
                Ok(GameState::InRoom)
            }
            (GameState::InMatchLoading, RETAIL_C2S_HOLE_LOAD_FINISHED) => {
                let mark = match self
                    .lobby
                    .route_solo(
                        identity.connection_id,
                        LobbySoloCommand::LoadingComplete(
                            LoadingComplete::new(100)
                                .map_err(|_| GameRuntimeError::InvalidConfig)?,
                        ),
                    )
                    .await
                {
                    Ok(LobbySoloRouteResult::InGame(mark)) => mark,
                    Ok(_) => return Err(GameRuntimeError::Protocol),
                    Err(_) => return Ok(state),
                };
                self.persist_in_game(identity.connection_id, mark, shutdown, idle_deadline)
                    .await?;
                self.observer
                    .match_event(GameMatchObservation::LoadingComplete);
                Ok(state)
            }
            // A solo hole has no other participant, so there is nothing to echo this to.
            (GameState::InMatch, RETAIL_C2S_SHOT_SYNC) => {
                self.observer.unknown(GameUnknownObservation::Ignored);
                Ok(state)
            }
            (GameState::InMatch, RETAIL_C2S_SHOT_COMMIT | RETAIL_C2S_SHOT_END) => {
                if !shots.admit_count(solo.shot_packets_per_window) {
                    self.observer
                        .rate_limited(GameRateClass::ShotPacketsConnection);
                    self.observer.shot(GameShotObservation::RateLimited);
                    return Ok(state);
                }
                // The retail shot payload is the client's own and carries nothing the
                // server can trust, so a shot is recorded as one stroke rather than
                // interpreted. Scoring stays server-authoritative through the stroke count.
                self.record_retail_stroke(identity.connection_id, strokes, false)
                    .await?;
                self.send(
                    framed,
                    &RetailTurnEnd {
                        connection_id: u32::try_from(identity.connection_id.get()).unwrap_or(0),
                    },
                )
                .await?;
                self.send(
                    framed,
                    &RetailTurnStart {
                        connection_id: u32::try_from(identity.connection_id.get()).unwrap_or(0),
                    },
                )
                .await?;
                Ok(state)
            }
            (GameState::InMatch, RETAIL_C2S_HOLE_FINISH) => {
                // The finishing stroke is the one that holes out.
                self.record_retail_stroke(identity.connection_id, strokes, true)
                    .await?;
                let commit = match self
                    .lobby
                    .route_solo(identity.connection_id, LobbySoloCommand::PrepareFinish)
                    .await
                {
                    Ok(LobbySoloRouteResult::Commit(commit)) => commit,
                    Ok(_) => return Err(GameRuntimeError::Protocol),
                    Err(_) => return Ok(state),
                };
                self.persist_and_apply_commit(
                    identity.connection_id,
                    commit,
                    shutdown,
                    idle_deadline,
                )
                .await?;
                self.send(framed, &RetailFinishHole).await?;
                Ok(GameState::InRoom)
            }
            _ => Err(GameRuntimeError::Protocol),
        }
    }

    /// Handles a retail match command that belongs to the two-player stroke lifecycle.
    ///
    /// Returns `Ok(None)` when the command is not one of them, so the caller can fall through
    /// to the solo lifecycle. A command the aggregate refuses — most often one arriving from
    /// the participant whose turn it is not — leaves the connection where it was rather than
    /// closing it: both clients send the turn barrier, and only one of them owns the turn.
    #[allow(clippy::too_many_arguments)]
    async fn handle_retail_stroke_command(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        state: GameState,
        identity: &RoomIdentity,
        room_id: Option<RoomId>,
        shutdown: &CancellationToken,
        idle_deadline: Instant,
        shots: &mut LocalRateWindow,
        strokes: &mut u32,
        opcode: u16,
        payload: &[u8],
    ) -> Result<Option<GameState>, GameRuntimeError> {
        match (state, opcode) {
            (GameState::InRoom, RETAIL_C2S_START_MATCH) => {
                let Ok(LobbyRouteResult::Snapshot(snapshot)) = self
                    .lobby
                    .route(identity.connection_id, LobbyRoomCommand::GetState)
                    .await
                else {
                    return Ok(None);
                };
                if snapshot.members().len() != 2 {
                    return Ok(None);
                }
                if self.config.stroke_two.is_none() {
                    // Falling back to the solo lifecycle here is precisely the defect this
                    // path exists to remove: it would score one player and ignore the other.
                    tracing::debug!(
                        stage = "stroke_disabled",
                        "retail two-player match start refused"
                    );
                    return Ok(Some(GameState::InRoom));
                }
                *strokes = 0;
                self.begin_retail_stroke_match(
                    &snapshot,
                    room_id,
                    shutdown,
                    identity,
                    idle_deadline,
                )
                .await?;
                Ok(Some(GameState::InRoom))
            }
            (GameState::InStrokeLoading, RETAIL_C2S_HOLE_LOAD_FINISHED) => {
                let loading =
                    StrokeLoadingComplete::new(100).map_err(|_| GameRuntimeError::InvalidConfig)?;
                match self
                    .lobby
                    .route_stroke(
                        identity.connection_id,
                        LobbyStrokeCommand::LoadingComplete(loading),
                    )
                    .await
                {
                    Ok(LobbyStrokeRouteResult::Loading(
                        StrokeLoadingOutcome::Waiting | StrokeLoadingOutcome::Duplicate,
                    )) => {}
                    Ok(LobbyStrokeRouteResult::Loading(
                        StrokeLoadingOutcome::PersistenceRequired(mark),
                    )) => {
                        self.persist_stroke_in_game(
                            identity.connection_id,
                            room_id.ok_or(GameRuntimeError::Protocol)?,
                            mark,
                            shutdown,
                            idle_deadline,
                        )
                        .await?;
                        self.observer
                            .stroke_match_event(GameMatchObservation::LoadingComplete);
                    }
                    Ok(_) => return Err(GameRuntimeError::Protocol),
                    Err(_) => return Ok(Some(state)),
                }
                Ok(Some(state))
            }
            (GameState::InStrokeMatch, RETAIL_C2S_SHOT_COMMIT) => {
                if !self.admit_retail_stroke_shot(shots) {
                    return Ok(Some(state));
                }
                // The payload is the client's own shot. It is relayed unchanged and counted as
                // one action; nothing in it is trusted, so nothing in it is decoded.
                let sequence = strokes.saturating_add(1);
                let action = StrokeShotAction::new(sequence, 0, 1.0, 0.0, 0.0, 0.0)
                    .map_err(|_| GameRuntimeError::Protocol)?;
                if self
                    .lobby
                    .route_stroke(
                        identity.connection_id,
                        LobbyStrokeCommand::ShotAction(action),
                    )
                    .await
                    .is_err()
                {
                    return Ok(Some(state));
                }
                self.relay_retail_match_frame(
                    identity.connection_id,
                    RetailMatchRelay::Shot(payload.to_vec()),
                )
                .await;
                Ok(Some(state))
            }
            (GameState::InStrokeMatch, RETAIL_C2S_SHOT_SYNC) => {
                if !self.admit_retail_stroke_shot(shots) {
                    return Ok(Some(state));
                }
                self.relay_retail_match_frame(
                    identity.connection_id,
                    RetailMatchRelay::Sync(payload.to_vec()),
                )
                .await;
                Ok(Some(state))
            }
            (GameState::InStrokeMatch, RETAIL_C2S_SHOT_END) => {
                if !self.admit_retail_stroke_shot(shots) {
                    return Ok(Some(state));
                }
                // Both clients send this barrier after a shot; only the participant who owns
                // the turn ends it, and the aggregate is what decides that.
                let sequence = strokes.saturating_add(1);
                let result = StrokeShotResult::new(sequence, 0.0, 0.0, 0.0, Lie::Fairway, false)
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let routed = self
                    .lobby
                    .route_stroke(
                        identity.connection_id,
                        LobbyStrokeCommand::ShotResult(result),
                    )
                    .await;
                if let Ok(LobbyStrokeRouteResult::Result(outcome)) = routed
                    && outcome.disposition() == RelayDisposition::Accepted
                {
                    *strokes = sequence;
                    self.observer.stroke_shot(GameShotObservation::Accepted);
                }
                Ok(Some(state))
            }
            (GameState::InStrokeMatch, RETAIL_C2S_HOLE_FINISH) => {
                // The holing shot was already counted through the ordinary action/result pair,
                // so this completes the caller's hole without charging another stroke.
                let _routed = self
                    .lobby
                    .route_stroke(identity.connection_id, LobbyStrokeCommand::HoleOut)
                    .await;
                let _ = framed;
                Ok(Some(state))
            }
            _ => Ok(None),
        }
    }

    /// Applies the shared shot rate window, reporting a refusal the same way the solo path does.
    fn admit_retail_stroke_shot(&self, shots: &mut LocalRateWindow) -> bool {
        let Some(stroke) = self.config.stroke_two else {
            return false;
        };
        if shots.admit_count(stroke.shot_packets_per_window) {
            return true;
        }
        self.observer
            .rate_limited(GameRateClass::StrokePacketsConnection);
        self.observer.stroke_shot(GameShotObservation::RateLimited);
        false
    }

    /// Relays one client-authored in-match frame to the captured roster.
    ///
    /// A refused relay is not fatal: it means the hole moved on, and the frame is cosmetic.
    async fn relay_retail_match_frame(
        &self,
        connection_id: PlayerConnectionId,
        relay: RetailMatchRelay,
    ) {
        let _routed = self
            .lobby
            .route_stroke(connection_id, LobbyStrokeCommand::Relay(relay))
            .await;
    }

    /// Reserves, persists, and confirms a two-player retail match for the room's roster.
    async fn begin_retail_stroke_match(
        &self,
        snapshot: &RoomSnapshot,
        room_id: Option<RoomId>,
        shutdown: &CancellationToken,
        identity: &RoomIdentity,
        idle_deadline: Instant,
    ) -> Result<(), GameRuntimeError> {
        let stroke = self.config.stroke_two.ok_or(GameRuntimeError::Protocol)?;
        let members = snapshot.members();
        let [first, second] = [0_usize, 1_usize].map(|index| &members[index]);
        let participants = [
            StrokeParticipant::new(
                first.account_id(),
                StrokeRosterOrder::First,
                MatchResultKey::new(uuid::Uuid::new_v4()),
            ),
            StrokeParticipant::new(
                second.account_id(),
                StrokeRosterOrder::Second,
                MatchResultKey::new(uuid::Uuid::new_v4()),
            ),
        ];
        let mut seed_bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut seed_bytes);
        let seed = MatchSeed::new(seed_bytes);
        let (weather, wind) =
            deterministic_conditions(seed).map_err(|_| GameRuntimeError::InvalidConfig)?;
        let begin = BeginStrokeMatch::new(
            MatchId::new(uuid::Uuid::new_v4()),
            MatchResultKey::new(uuid::Uuid::new_v4()),
            participants,
            stroke.course,
            stroke.catalog_fingerprint,
            seed,
            weather,
            wind,
        )
        .map_err(|_| GameRuntimeError::InvalidConfig)?;
        let plan = StrokeStartPlan::new(
            begin,
            [first.connection_id(), second.connection_id()],
            stroke.loading_timeout,
            stroke.turn_timeout,
            stroke.game_timeout,
            stroke.max_strokes,
        )
        .map_err(|_| GameRuntimeError::InvalidConfig)?;
        let begin = match self
            .lobby
            .route_stroke(
                identity.connection_id,
                LobbyStrokeCommand::PrepareStart(plan),
            )
            .await
        {
            Ok(LobbyStrokeRouteResult::Begin(begin)) => begin,
            Ok(_) => return Err(GameRuntimeError::Protocol),
            // A refused start leaves both clients in the room rather than closing them.
            Err(_) => return Ok(()),
        };
        let room_id = room_id.ok_or(GameRuntimeError::Protocol)?;
        self.persist_and_confirm_stroke_begin(
            identity.connection_id,
            room_id,
            begin,
            shutdown,
            idle_deadline,
        )
        .await
    }

    /// Records one retail stroke against the durable solo match state.
    ///
    /// The client owns trajectory, so the coordinates are not meaningful here; what the
    /// match actor needs is the stroke sequence and whether the ball was holed.
    async fn record_retail_stroke(
        &self,
        connection_id: PlayerConnectionId,
        strokes: &mut u32,
        holed: bool,
    ) -> Result<(), GameRuntimeError> {
        *strokes = strokes.saturating_add(1);
        let sequence = *strokes;
        let action = ShotAction::new(sequence, 0, 1.0, 0.0, 0.0, 0.0)
            .map_err(|_| GameRuntimeError::Protocol)?;
        let _ = self
            .lobby
            .route_solo(connection_id, LobbySoloCommand::ShotAction(action))
            .await;
        let result = ShotResult::new(sequence, 0.0, 0.0, 0.0, Lie::Fairway, holed)
            .map_err(|_| GameRuntimeError::Protocol)?;
        let _ = self
            .lobby
            .route_solo(connection_id, LobbySoloCommand::ShotResult(result))
            .await;
        Ok(())
    }

    /// Handles one retail lobby/room command.
    ///
    /// Serves the lobby-side services a real client opens from its menu bar: the shop and the
    /// player's own room. Neither has durable content here yet, so both answer with the empty
    /// forms upstream sends rather than inventing furniture or stock.
    async fn handle_retail_lobby_service(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        identity: &RoomIdentity,
        opcode: u16,
        payload: &[u8],
    ) -> Result<(), GameRuntimeError> {
        let profile = &CompatibilityProfile::US_852;
        let account_id = u32::try_from(identity.account_id.get()).unwrap_or(0);
        match opcode {
            RetailShopJoin::OPCODE => self.send(framed, &RetailShopJoined).await,
            RetailLockerInventoryRequest::OPCODE => {
                self.send(framed, &RetailLockerInventoryResponse).await
            }
            RetailLockerCombinationAttempt::OPCODE => {
                self.send(framed, &RetailLockerCombinationResponse).await
            }
            RetailMyRoomEnter::OPCODE => {
                let request =
                    decode_packet_payload::<RetailMyRoomEnter>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                // Visiting another player's room is not implemented, and answering as though it
                // were would show this player their own room under someone else's name.
                if request.user_id != account_id || request.room_user_id != account_id {
                    return Err(GameRuntimeError::Protocol);
                }
                self.send(
                    framed,
                    &RetailMyRoomEntered {
                        user_id: account_id,
                    },
                )
                .await
            }
            _ => {
                let request = decode_packet_payload::<RetailMyRoomInventoryRequest>(
                    payload,
                    profile,
                    ServiceKind::Game,
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                if request.user_id != account_id {
                    return Err(GameRuntimeError::Protocol);
                }
                self.send(framed, &RetailMyRoomLayout).await?;
                self.send(
                    framed,
                    &RetailPlayerInfo {
                        player: RetailRoomPlayer {
                            connection_id: u32::try_from(identity.connection_id.get()).unwrap_or(0),
                            nickname: identity.nickname.display().as_bytes().to_vec(),
                            slot: 0,
                            character_uid: 0,
                            flags: RoomPlayerFlags::new(false, false),
                            level: 1,
                            user_id: account_id,
                        },
                    },
                )
                .await
            }
        }
    }

    /// Answers an equipment change with the equipment this server actually holds.
    ///
    /// A real client sends this repeatedly the moment My Room opens, and leaving it unanswered
    /// drops the session. What it deliberately does *not* do is acknowledge the requested change:
    /// character parts, caddies, consumables and decoration have no durable representation here,
    /// so echoing the request back would report a change that was never stored and contradict
    /// itself on the next login. Reporting the stored state instead is accurate, and a client
    /// that asked for something this server cannot keep simply sees it revert.
    async fn handle_retail_equipment_update(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        account_id: AccountId,
        payload: &[u8],
    ) -> Result<(), GameRuntimeError> {
        let profile = &CompatibilityProfile::US_852;
        let request =
            decode_packet_payload::<RetailEquipmentUpdate>(payload, profile, ServiceKind::Game)
                .map_err(|_| GameRuntimeError::Protocol)?;
        // Character parts and the two unclassified slots have no reply this server can form
        // honestly, so they are accepted and left alone rather than answered with a guess.
        let reply = match request.slot {
            RetailEquipmentSlot::CharacterParts
            | RetailEquipmentSlot::UnknownEight
            | RetailEquipmentSlot::UnknownNine => {
                self.observer.unknown(GameUnknownObservation::Ignored);
                return Ok(());
            }
            RetailEquipmentSlot::Caddie => RetailEquipmentUpdated::Caddie { caddie_id: 0 },
            RetailEquipmentSlot::Consumables => RetailEquipmentUpdated::Consumables {
                item_type_ids: [0; pangya_protocol::RETAIL_CONSUMABLE_SLOTS],
            },
            RetailEquipmentSlot::Decoration => {
                RetailEquipmentUpdated::Decoration { type_ids: [0; 6] }
            }
            RetailEquipmentSlot::Ball | RetailEquipmentSlot::Character => {
                let snapshot = self
                    .repository
                    .load_player_snapshot(account_id)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                if request.slot == RetailEquipmentSlot::Character {
                    RetailEquipmentUpdated::Character {
                        character_id: u32::try_from(snapshot.equipment.character_id.get())
                            .unwrap_or(0),
                    }
                } else {
                    let ball = snapshot.equipment.ball_item_id.and_then(|id| {
                        snapshot
                            .inventory
                            .iter()
                            .find(|item| item.id == id)
                            .map(|item| (id.get(), item.item_type_id.get()))
                    });
                    let (item_id, item_type_id) = ball.unwrap_or((0, 0));
                    RetailEquipmentUpdated::Ball {
                        item_id: u32::try_from(item_id).unwrap_or(0),
                        item_type_id,
                    }
                }
            }
        };
        self.send(framed, &reply).await
    }

    /// Buys the items a real client's shop asked for, priced from this server's catalog.
    ///
    /// The request carries the client's own idea of each price. Those fields are ignored: the
    /// definition, and therefore the cost, is resolved from the catalog, so a modified client
    /// cannot name its own price. Each line is committed through the same idempotent economy
    /// path the synthetic purchase uses, so a retried purchase replays rather than double-charges.
    async fn handle_retail_purchase(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        account_id: AccountId,
        payload: &[u8],
    ) -> Result<(), GameRuntimeError> {
        let profile = &CompatibilityProfile::US_852;
        let request =
            decode_packet_payload::<RetailPurchaseRequest>(payload, profile, ServiceKind::Game)
                .map_err(|_| GameRuntimeError::Protocol)?;
        let Some(economy) = self.config.economy else {
            tracing::debug!(stage = "economy_disabled", "retail purchase refused");
            return self.refuse_retail_purchase(framed, account_id).await;
        };
        if request.items.is_empty() {
            tracing::debug!(stage = "empty_request", "retail purchase refused");
            return self.refuse_retail_purchase(framed, account_id).await;
        }

        let mut spent = 0_u64;
        let mut pang_balance = None;
        for item in &request.items {
            if item.quantity == 0 || item.quantity > economy.max_purchase_quantity {
                tracing::debug!(
                    stage = "quantity",
                    quantity = item.quantity,
                    "retail purchase refused"
                );
                return self.refuse_retail_purchase(framed, account_id).await;
            }
            let Some(definition) = self
                .catalog
                .shop_offer(ItemTypeId::new(item.item_type_id))
                .copied()
            else {
                // The item id is catalog data, not player data, and without it a refusal is
                // indistinguishable from a pricing bug.
                tracing::debug!(
                    stage = "not_in_catalog",
                    item_type_id = item.item_type_id,
                    shop_offers = self.catalog.shop_offers().len(),
                    "retail purchase refused"
                );
                return self.refuse_retail_purchase(framed, account_id).await;
            };
            // The operation id makes a retried line replay its own commit instead of buying twice.
            let operation_id = retail_purchase_operation_id(account_id, item);
            let committed = timeout(
                economy.command_timeout,
                self.repository.purchase(PurchaseRequest {
                    account_id,
                    operation_id,
                    catalog: self.catalog.fingerprint(),
                    definition,
                    quantity: item.quantity,
                }),
            )
            .await;
            match committed {
                Ok(Ok(commit)) => {
                    let result = match &commit {
                        EconomyCommit::Committed(value) | EconomyCommit::Replayed(value) => value,
                    };
                    spent = spent.saturating_add(
                        definition
                            .pang_price()
                            .unwrap_or(0)
                            .saturating_mul(u64::from(item.quantity)),
                    );
                    pang_balance = Some(result.pang_balance);
                }
                Ok(Err(error)) => {
                    tracing::debug!(stage = "economy", %error, "retail purchase refused");
                    return self.refuse_retail_purchase(framed, account_id).await;
                }
                Err(_) => {
                    tracing::debug!(stage = "timeout", "retail purchase refused");
                    return self.refuse_retail_purchase(framed, account_id).await;
                }
            }
        }

        let balances = self.retail_balances(account_id).await?;
        let pang = pang_balance.unwrap_or(balances.0);
        self.send(
            framed,
            &RetailPangSpent {
                remaining: pang,
                spent,
            },
        )
        .await?;
        self.send(framed, &RetailPointBalance { points: balances.1 })
            .await?;
        self.send(
            framed,
            &RetailPurchaseResponse {
                status: 0,
                pang,
                points: balances.1,
            },
        )
        .await
    }

    /// Refuses a purchase without changing anything, reporting the untouched balances.
    async fn refuse_retail_purchase(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        account_id: AccountId,
    ) -> Result<(), GameRuntimeError> {
        let (pang, points) = self.retail_balances(account_id).await?;
        self.send(
            framed,
            &RetailPurchaseResponse {
                status: RetailPurchaseResponse::REFUSED,
                pang,
                points,
            },
        )
        .await
    }

    /// Reads an account's current balances straight from storage.
    async fn retail_balances(&self, account_id: AccountId) -> Result<(u64, u64), GameRuntimeError> {
        let snapshot = self
            .repository
            .load_player_snapshot(account_id)
            .await
            .map_err(|_| GameRuntimeError::Snapshot)?;
        Ok((snapshot.profile.pang, snapshot.profile.points))
    }

    /// Current rooms as retail list records, bounded by what one list frame can carry.
    async fn retail_room_list(&self) -> Result<Vec<RetailRoom>, GameRuntimeError> {
        let summaries = self
            .lobby
            .list()
            .await
            .map_err(|_| GameRuntimeError::Protocol)?;
        Ok(summaries
            .iter()
            .take(pangya_protocol::MAX_ROOMS_PER_LIST)
            .map(retail_room_from_summary_only)
            .collect())
    }

    /// The lobby actor is protocol-agnostic, so this is a wire translation only: retail
    /// requests map onto the same commands the synthetic path uses, and the actor's
    /// authoritative results map back onto retail replies.
    #[allow(clippy::too_many_arguments)]
    async fn handle_retail_room_command(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        state: GameState,
        identity: &RoomIdentity,
        outbound: mpsc::Sender<RoomEvent>,
        room_cancellation: CancellationToken,
        opcode: u16,
        payload: &[u8],
        room_id: &mut Option<RoomId>,
    ) -> Result<GameState, GameRuntimeError> {
        let profile = &CompatibilityProfile::US_852;
        match (state, opcode) {
            // Opening the room directory. The client wants the current room list before its
            // acknowledgement, which is what upstream sends on this transition.
            (GameState::InChannel, RETAIL_C2S_MULTIPLAYER_JOIN) => {
                if !payload.is_empty() {
                    return Err(GameRuntimeError::Protocol);
                }
                let rooms = self.retail_room_list().await?;
                self.send(
                    framed,
                    &RetailRoomList {
                        kind: RoomListKind::Initial,
                        rooms,
                    },
                )
                .await?;
                self.send(framed, &RetailMultiplayerJoined).await?;
                Ok(state)
            }
            (GameState::InChannel, RETAIL_C2S_MULTIPLAYER_LEAVE) => {
                if !payload.is_empty() {
                    return Err(GameRuntimeError::Protocol);
                }
                self.send(framed, &RetailMultiplayerLeft).await?;
                Ok(state)
            }
            (GameState::InChannel, RetailRoomCreate::OPCODE) => {
                let request =
                    decode_packet_payload::<RetailRoomCreate>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let Ok(name) = std::str::from_utf8(&request.name)
                    .map_err(drop)
                    .and_then(|value| RoomName::parse(value).map_err(drop))
                else {
                    return self.reject_retail_join(framed, state).await;
                };
                let password = if request.password.is_empty() {
                    None
                } else {
                    match std::str::from_utf8(&request.password)
                        .map_err(drop)
                        .and_then(|value| RoomPassword::parse(value).map_err(drop))
                    {
                        Ok(value) => Some(value),
                        Err(()) => return self.reject_retail_join(framed, state).await,
                    }
                };
                let Ok(settings) = RoomSettings::new(request.max_players) else {
                    return self.reject_retail_join(framed, state).await;
                };
                let created = self
                    .lobby
                    .create(
                        name,
                        password,
                        settings,
                        identity.clone(),
                        outbound,
                        room_cancellation,
                    )
                    .await;
                match created {
                    Ok(summary) => {
                        *room_id = Some(summary.id());
                        self.observer.room(GameRoomObservation::Created);
                        let room = retail_room_from_summary(&summary, &request);
                        self.send(framed, &RetailRoomJoinResult::Accepted(Box::new(room)))
                            .await?;
                        self.send_retail_census(framed, identity.connection_id)
                            .await?;
                        Ok(GameState::InRoom)
                    }
                    Err(_) => self.reject_retail_join(framed, state).await,
                }
            }
            (GameState::InChannel, RetailRoomJoin::OPCODE) => {
                let request =
                    decode_packet_payload::<RetailRoomJoin>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let Ok(target) = RoomId::new(u32::from(request.room_number)) else {
                    return self.reject_retail_join(framed, state).await;
                };
                let password = if request.password.is_empty() {
                    None
                } else {
                    match std::str::from_utf8(&request.password)
                        .map_err(drop)
                        .and_then(|value| RoomPassword::parse(value).map_err(drop))
                    {
                        Ok(value) => Some(value),
                        Err(()) => return self.reject_retail_join(framed, state).await,
                    }
                };
                let joined = self
                    .lobby
                    .join(
                        target,
                        identity.clone(),
                        password,
                        outbound,
                        room_cancellation,
                    )
                    .await;
                match joined {
                    Ok(snapshot) => {
                        *room_id = Some(snapshot.summary().id());
                        self.observer.room(GameRoomObservation::Joined);
                        self.send(
                            framed,
                            &RetailRoomJoinResult::Accepted(Box::new(retail_room_from_snapshot(
                                &snapshot,
                            ))),
                        )
                        .await?;
                        // No census here: joining mutates the room, so the actor broadcasts one
                        // to everyone in it, this connection included. Sending a second would
                        // hand the joiner the same roster twice.
                        Ok(GameState::InRoom)
                    }
                    Err(_) => self.reject_retail_join(framed, state).await,
                }
            }
            (GameState::InRoom, RETAIL_C2S_ROOM_READY) => {
                // A single byte, zero meaning ready. The client will not offer Start until the
                // roster comes back showing the change, so the census is the reply.
                let ready = payload.first().copied() == Some(0);
                let routed = self
                    .lobby
                    .route(identity.connection_id, LobbyRoomCommand::SetReady(ready))
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let LobbyRouteResult::Snapshot(snapshot) = routed else {
                    return Err(GameRuntimeError::Protocol);
                };
                self.send(framed, &retail_census_from_snapshot(&snapshot))
                    .await?;
                Ok(state)
            }
            (GameState::InRoom, RETAIL_C2S_ROOM_LEAVE) => {
                self.lobby
                    .leave(identity.connection_id)
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                *room_id = None;
                self.observer.room(GameRoomObservation::Left);
                self.send(framed, &RetailRoomLeave::to_lobby()).await?;
                self.send_retail_room_list(framed).await?;
                Ok(GameState::InChannel)
            }
            // A room opcode in the wrong state is a protocol violation, matching the
            // synthetic path rather than silently tolerating it.
            _ => Err(GameRuntimeError::Protocol),
        }
    }

    /// Refuses a create or join attempt without disclosing which check failed.
    async fn reject_retail_join(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        state: GameState,
    ) -> Result<GameState, GameRuntimeError> {
        self.send(
            framed,
            &RetailRoomJoinResult::Rejected(RoomJoinRejection::CannotCreate),
        )
        .await?;
        Ok(state)
    }

    /// Sends the current room roster, so the client can populate its member list.
    async fn send_retail_census(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        connection_id: PlayerConnectionId,
    ) -> Result<(), GameRuntimeError> {
        let snapshot = match self
            .lobby
            .route(connection_id, LobbyRoomCommand::GetState)
            .await
        {
            Ok(LobbyRouteResult::Snapshot(snapshot)) => snapshot,
            _ => return Ok(()),
        };
        self.send(framed, &retail_census_from_snapshot(&snapshot))
            .await
    }

    /// Sends the lobby's current room list.
    async fn send_retail_room_list(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
    ) -> Result<(), GameRuntimeError> {
        let rooms = self
            .lobby
            .list()
            .await
            .map_err(|_| GameRuntimeError::Protocol)?;
        let rooms = rooms.iter().map(retail_room_from_summary_only).collect();
        self.observer.room(GameRoomObservation::Listed);
        self.send(
            framed,
            &RetailRoomList {
                kind: RoomListKind::Initial,
                rooms,
            },
        )
        .await
    }

    /// Emits the reference-derived retail bootstrap sequence.
    ///
    /// Order is load-bearing: the client stays on its loading screen until the full
    /// handover reply arrives, and renders the lobby from the roster/equipment/inventory
    /// containers that follow. See `docs/protocol/US852_RETAIL_BOOTSTRAP.md`.
    async fn send_retail_bootstrap(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        snapshot: &PlayerSnapshot,
    ) -> Result<(), GameRuntimeError> {
        let narrow = |value: i64| u32::try_from(value).map_err(|_| GameRuntimeError::Snapshot);

        // Progress ticks release the client's loading bar before the large reply.
        for step in 0..=2 {
            self.send(framed, &HandoverControl::Progress(step)).await?;
        }

        let character = snapshot
            .characters
            .iter()
            .find(|value| value.id == snapshot.equipment.character_id)
            .ok_or(GameRuntimeError::Snapshot)?;
        let equipment = RetailEquipment {
            caddie_uid: 0,
            character_uid: narrow(snapshot.equipment.character_id.get())?,
            club_set_uid: snapshot
                .equipment
                .club_item_id
                .map(|id| narrow(id.get()))
                .transpose()?
                .unwrap_or(0),
            comet_iff_id: snapshot
                .equipment
                .ball_item_id
                .and_then(|id| {
                    snapshot
                        .inventory
                        .iter()
                        .find(|item| item.id == id)
                        .map(|item| item.item_type_id.get())
                })
                .unwrap_or(0),
            item_iff_ids: [0; EQUIPPED_ITEM_SLOTS],
        };
        let reply = HandoverReply {
            server_name: b"pangya-rs".to_vec(),
            identity: RetailPlayerIdentity {
                username: snapshot.account.username_display.as_bytes().to_vec(),
                nickname: snapshot
                    .profile
                    .nickname
                    .as_deref()
                    .ok_or(GameRuntimeError::Snapshot)?
                    .as_bytes()
                    .to_vec(),
                connection_id: 0,
                user_id: narrow(snapshot.account.id.get())?,
            },
            statistics: RetailPlayerStatistics {
                experience: u32::try_from(snapshot.profile.experience).unwrap_or(u32::MAX),
                pang: snapshot.profile.pang,
                ..RetailPlayerStatistics::default()
            },
            equipment,
            character: RetailCharacter {
                iff_id: character.item_type_id.get(),
                uid: narrow(character.id.get())?,
                hair_color: 0,
                part_iff_ids: [0; CHARACTER_PARTS],
                part_uids: [0; CHARACTER_PARTS],
                stats: [0; CHARACTER_STATS],
                mastery: 0,
            },
            caddie: RetailCaddie::default(),
            server_time: [0; 16],
            disabled_features: HandoverReply::DEFAULT_DISABLED_FEATURES,
        };
        self.send(framed, &reply).await?;

        let mut roster = Vec::with_capacity(snapshot.characters.len());
        for value in &snapshot.characters {
            let mut writer = PacketWriter::default();
            writer.u32_le(value.item_type_id.get());
            writer.u32_le(narrow(value.id.get())?);
            roster.push(writer.into_inner());
        }
        self.send_container(framed, IffContainerKind::CharacterRoster, roster)
            .await?;
        // No caddie support yet; the client still expects the container to arrive.
        self.send_container(framed, IffContainerKind::CaddieRoster, Vec::new())
            .await?;
        self.send(framed, &equipment).await?;

        let mut inventory = Vec::with_capacity(snapshot.inventory.len());
        for item in &snapshot.inventory {
            let mut writer = PacketWriter::default();
            writer.u32_le(narrow(item.id.get())?);
            writer.u32_le(item.item_type_id.get());
            writer.u32_le(item.quantity);
            inventory.push(writer.into_inner());
        }
        self.send_container(framed, IffContainerKind::Inventory, inventory)
            .await?;

        self.send(
            framed,
            &ServerChannelList {
                channels: vec![RetailChannel {
                    name: b"pangya-rs".to_vec(),
                    capacity: 200,
                    player_count: 0,
                    id: u16::try_from(self.config.channel_id)
                        .map_err(|_| GameRuntimeError::InvalidConfig)?,
                    restrictions: 0,
                }],
            },
        )
        .await?;

        // The lobby header reads its balances from these, not from the statistics block, so
        // without them a funded account still shows zero pang and zero cookies.
        self.send(
            framed,
            &RetailPangBalance {
                pang: snapshot.profile.pang,
            },
        )
        .await?;
        self.send(
            framed,
            &RetailPointBalance {
                points: snapshot.profile.points,
            },
        )
        .await
    }

    /// Sends one rostered container as its chunk sequence.
    async fn send_container(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        kind: IffContainerKind,
        entries: Vec<Vec<u8>>,
    ) -> Result<(), GameRuntimeError> {
        let chunks =
            IffContainerChunk::split(kind, entries).map_err(|_| GameRuntimeError::Protocol)?;
        for chunk in chunks {
            let mut writer = PacketWriter::default();
            chunk
                .encode_body(&mut writer, &CompatibilityProfile::US_852)
                .map_err(|_| GameRuntimeError::Protocol)?;
            self.send_raw(framed, chunk.opcode(), writer.into_inner())
                .await?;
        }
        Ok(())
    }

    /// Sends a pre-encoded body under a runtime-selected opcode.
    async fn send_raw(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        opcode: u16,
        payload: Vec<u8>,
    ) -> Result<(), GameRuntimeError> {
        let bytes = payload.len().saturating_add(2);
        timeout(
            self.config.limits.command_timeout,
            framed.send(OutboundFrame {
                opcode,
                payload: zeroize::Zeroizing::new(payload),
                salt: OsRng.next_u32() as u8,
            }),
        )
        .await
        .map_err(|_| GameRuntimeError::Timeout)?
        .map_err(|error| match error {
            PacketEncodeError::Io(_) => GameRuntimeError::Io,
            _ => GameRuntimeError::Protocol,
        })?;
        self.observer.frame("out", opcode, bytes);
        Ok(())
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
    EnterStrokeLoading,
    EnterStrokeMatch,
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

fn is_known_economy_opcode(opcode: u16) -> bool {
    matches!(
        opcode,
        SYNTHETIC_M7_C2S_SHOP_PAGE
            | SYNTHETIC_M7_C2S_PURCHASE
            | SYNTHETIC_M7_C2S_EQUIP
            | SYNTHETIC_M7_C2S_CONSUME
            | SYNTHETIC_M7_C2S_REPAIR
    )
}

fn economy_command_for_opcode(opcode: u16) -> Option<EconomyCommand> {
    match opcode {
        SYNTHETIC_M7_C2S_SHOP_PAGE => Some(EconomyCommand::ShopPage),
        SYNTHETIC_M7_C2S_PURCHASE => Some(EconomyCommand::Purchase),
        SYNTHETIC_M7_C2S_EQUIP => Some(EconomyCommand::Equip),
        SYNTHETIC_M7_C2S_CONSUME => Some(EconomyCommand::Consume),
        SYNTHETIC_M7_C2S_REPAIR => Some(EconomyCommand::Repair),
        _ => None,
    }
}

fn decode_economy_request_shape(opcode: u16, payload: &[u8]) -> Result<(), GameRuntimeError> {
    let profile = &CompatibilityProfile::US_852;
    match opcode {
        SYNTHETIC_M7_C2S_SHOP_PAGE => {
            decode_packet_payload::<ShopPageRequest>(payload, profile, ServiceKind::Game).map(drop)
        }
        SYNTHETIC_M7_C2S_PURCHASE => {
            decode_packet_payload::<PurchaseRequestPacket>(payload, profile, ServiceKind::Game)
                .map(drop)
        }
        SYNTHETIC_M7_C2S_EQUIP => {
            decode_packet_payload::<EquipRequest>(payload, profile, ServiceKind::Game).map(drop)
        }
        SYNTHETIC_M7_C2S_CONSUME => {
            decode_packet_payload::<ConsumeOneRequest>(payload, profile, ServiceKind::Game)
                .map(drop)
        }
        SYNTHETIC_M7_C2S_REPAIR => {
            decode_packet_payload::<RepairRequest>(payload, profile, ServiceKind::Game).map(drop)
        }
        _ => return Err(GameRuntimeError::Protocol),
    }
    .map_err(|_| GameRuntimeError::Protocol)
}

fn protocol_shop_offer(
    definition: pangya_domain::ItemDefinition,
) -> Result<ShopOffer, GameRuntimeError> {
    let kind = match definition.kind {
        ItemKind::ClubSet => EconomyItemKind::ClubSet,
        ItemKind::Ball => EconomyItemKind::Ball,
        ItemKind::Consumable => EconomyItemKind::Consumable,
        ItemKind::CharacterPart => EconomyItemKind::CharacterPart,
        ItemKind::Character => return Err(GameRuntimeError::Catalog),
    };
    let price = definition.pang_price().ok_or(GameRuntimeError::Catalog)?;
    let max_stack = match definition.stacking {
        ItemStacking::Unique => 1,
        ItemStacking::Stackable { max_stack } => max_stack,
    };
    let (max_durability, repair_rate) = match definition.durability {
        ItemDurability::Nondurable => (0, 0),
        ItemDurability::Durable {
            max,
            repair_pang_per_point,
        } => (max, repair_pang_per_point),
    };
    ShopOffer::new(
        definition.type_id.get(),
        kind,
        price,
        max_stack,
        max_durability,
        repair_rate,
    )
    .map_err(|_| GameRuntimeError::Catalog)
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

fn is_known_stroke_opcode(opcode: u16) -> bool {
    matches!(
        opcode,
        SYNTHETIC_M6_C2S_START_STROKE_TWO
            | SYNTHETIC_M6_C2S_LOADING_COMPLETE
            | SYNTHETIC_M6_C2S_SHOT_ACTION
            | SYNTHETIC_M6_C2S_SHOT_RESULT
            | SYNTHETIC_M6_C2S_GIVE_UP
    )
}

fn stroke_error_outcome(error: StrokeMatchError) -> StrokeCommandOutcome {
    match error {
        StrokeMatchError::InvalidSequence | StrokeMatchError::ConflictingReplay => {
            StrokeCommandOutcome::InvalidSequence
        }
        StrokeMatchError::InvalidTurn => StrokeCommandOutcome::InvalidTurn,
        StrokeMatchError::Timeout | StrokeMatchError::QueueFull | StrokeMatchError::Closed => {
            StrokeCommandOutcome::Timeout
        }
        StrokeMatchError::DeterministicConditionsInvariant
        | StrokeMatchError::InvalidPlan
        | StrokeMatchError::InvalidPhase
        | StrokeMatchError::IdentityMismatch
        | StrokeMatchError::NotParticipant
        | StrokeMatchError::InvalidProgress
        | StrokeMatchError::Invariant
        | StrokeMatchError::NotMember
        | StrokeMatchError::NotOwner
        | StrokeMatchError::NotExactlyTwo
        | StrokeMatchError::NotReady
        | StrokeMatchError::RosterMismatch => StrokeCommandOutcome::InvalidPhase,
    }
}

fn observe_stroke_relay(
    observer: &dyn GameObserver,
    result: &Result<LobbyStrokeRouteResult, StrokeMatchError>,
) -> StrokeCommandOutcome {
    match result {
        Ok(LobbyStrokeRouteResult::Relay(RelayDisposition::Accepted)) => {
            observer.stroke_shot(GameShotObservation::Accepted);
            StrokeCommandOutcome::Success
        }
        Ok(LobbyStrokeRouteResult::Relay(RelayDisposition::Duplicate)) => {
            observer.stroke_shot(GameShotObservation::Duplicate);
            StrokeCommandOutcome::Success
        }
        Ok(_) => {
            observer.stroke_shot(GameShotObservation::Rejected);
            StrokeCommandOutcome::InvalidPhase
        }
        Err(error) => {
            observer.stroke_shot(if *error == StrokeMatchError::InvalidTurn {
                GameShotObservation::OutOfTurn
            } else {
                GameShotObservation::Rejected
            });
            stroke_error_outcome(*error)
        }
    }
}

fn observe_stroke_result(
    observer: &dyn GameObserver,
    result: &Result<LobbyStrokeRouteResult, StrokeMatchError>,
) -> StrokeCommandOutcome {
    match result {
        Ok(LobbyStrokeRouteResult::Result(outcome)) => {
            observer.stroke_shot(match outcome.disposition() {
                RelayDisposition::Accepted => GameShotObservation::Accepted,
                RelayDisposition::Duplicate => GameShotObservation::Duplicate,
            });
            StrokeCommandOutcome::Success
        }
        Ok(_) => {
            observer.stroke_shot(GameShotObservation::Rejected);
            StrokeCommandOutcome::InvalidPhase
        }
        Err(error) => {
            observer.stroke_shot(if *error == StrokeMatchError::InvalidTurn {
                GameShotObservation::OutOfTurn
            } else {
                GameShotObservation::Rejected
            });
            stroke_error_outcome(*error)
        }
    }
}

const fn protocol_weather(weather: pangya_domain::Weather) -> ProtocolWeather {
    match weather {
        pangya_domain::Weather::Clear => ProtocolWeather::Clear,
        pangya_domain::Weather::Cloudy => ProtocolWeather::Cloudy,
        pangya_domain::Weather::Rain => ProtocolWeather::Rain,
    }
}

fn protocol_wind(wind: pangya_domain::WindConditions) -> Result<Wind, GameRuntimeError> {
    Wind::new(
        f32::from(wind.speed_tenths()) / 10.0,
        f32::from(wind.angle_degrees()),
    )
    .map_err(|_| GameRuntimeError::Protocol)
}

const fn protocol_stroke_completion(
    completion: DomainStrokeCompletion,
) -> ProtocolStrokeCompletion {
    match completion {
        DomainStrokeCompletion::Holed => ProtocolStrokeCompletion::Holed,
        DomainStrokeCompletion::StrokeCap => ProtocolStrokeCompletion::StrokeCap,
        DomainStrokeCompletion::WinnerByForfeit => ProtocolStrokeCompletion::WinnerByForfeit,
        DomainStrokeCompletion::GiveUp => ProtocolStrokeCompletion::GiveUp,
        DomainStrokeCompletion::Disconnect => ProtocolStrokeCompletion::Disconnect,
        DomainStrokeCompletion::TurnTimeout => ProtocolStrokeCompletion::TurnTimeout,
        DomainStrokeCompletion::GameTimeout => ProtocolStrokeCompletion::GameTimeout,
    }
}

const fn protocol_stroke_abort_reason(reason: MatchAbortReason) -> StrokeAbortReason {
    match reason {
        MatchAbortReason::LoadingTimeout => StrokeAbortReason::LoadingTimeout,
        MatchAbortReason::Disconnect => StrokeAbortReason::LoadingDisconnect,
        MatchAbortReason::Shutdown => StrokeAbortReason::ServerShutdown,
        MatchAbortReason::PersistenceFailure => StrokeAbortReason::PersistenceFailure,
        MatchAbortReason::StartupRecovery => StrokeAbortReason::StartupRecovery,
    }
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

/// Builds a retail census roster from a room's authoritative snapshot.
fn retail_census_from_snapshot(snapshot: &RoomSnapshot) -> RetailRoomCensus {
    let players = snapshot
        .members()
        .iter()
        .enumerate()
        .map(|(slot, member)| RetailRoomPlayer {
            connection_id: u32::try_from(member.connection_id().get()).unwrap_or(0),
            nickname: member.nickname().as_bytes().to_vec(),
            slot: u8::try_from(slot).unwrap_or(u8::MAX),
            character_uid: member
                .character_id()
                .and_then(|id| u32::try_from(id.get()).ok())
                .unwrap_or(0),
            flags: RoomPlayerFlags::new(member.is_owner(), member.is_ready()),
            level: 1,
            user_id: u32::try_from(member.account_id().get()).unwrap_or(0),
        })
        .collect();
    RetailRoomCensus::List(players)
}

/// Derives a stable economy operation key for one retail purchase line.
///
/// The retail purchase packet carries no operation identifier, so one is derived from the
/// account, the item and the quantity. A client that resends the same purchase replays the same
/// commit instead of buying twice; a genuinely new purchase of the same item differs by the
/// balance the replay check sees, and the economy path treats an exact replay as idempotent.
fn retail_purchase_operation_id(
    account_id: AccountId,
    item: &RetailPurchaseItem,
) -> EconomyOperationId {
    let mut hasher = Sha256::new();
    hasher.update(b"retail-purchase");
    hasher.update(account_id.get().to_le_bytes());
    hasher.update(item.item_type_id.to_le_bytes());
    hasher.update(item.quantity.to_le_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    EconomyOperationId::new(uuid::Uuid::from_bytes(bytes))
}

/// Retail room-leave client opcode.
const RETAIL_C2S_ROOM_LEAVE: u16 = 0x000f;
/// Retail multiplayer-mode enter client opcode, sent when the client opens the room directory.
const RETAIL_C2S_MULTIPLAYER_JOIN: u16 = 0x0081;
/// Retail multiplayer-mode leave client opcode, sent once the client leaves the directory.
const RETAIL_C2S_MULTIPLAYER_LEAVE: u16 = 0x0082;

/// Builds a retail room record from a lobby summary plus the settings the creator asked
/// for. The lobby model stores capacity and identity only, so course, timers, and hole
/// count are echoed from the request rather than invented.
fn retail_room_from_summary(summary: &RoomSummary, request: &RetailRoomCreate) -> RetailRoom {
    RetailRoom {
        name: summary.name().as_str().as_bytes().to_vec(),
        public: !summary.password_protected(),
        state: RetailRoomState::Lobby,
        max_players: summary.max_members(),
        player_count: summary.members(),
        hole_count: request.hole_count,
        room_type: RetailRoomType::from_wire(request.room_type).unwrap_or(RetailRoomType::Versus),
        id: u16::try_from(summary.id().get()).unwrap_or(u16::MAX),
        hole_progression: RetailHoleProgression::FrontStart,
        course: request.course,
        shot_timer_ms: request.shot_timer_ms,
        game_timer_ms: request.game_timer_ms,
        owner_uid: 0,
        natural_wind: false,
    }
}

/// Builds a retail room record from a summary alone, for the lobby list.
fn retail_room_from_summary_only(summary: &RoomSummary) -> RetailRoom {
    RetailRoom {
        name: summary.name().as_str().as_bytes().to_vec(),
        public: !summary.password_protected(),
        state: RetailRoomState::Lobby,
        max_players: summary.max_members(),
        player_count: summary.members(),
        hole_count: 1,
        room_type: RetailRoomType::Versus,
        id: u16::try_from(summary.id().get()).unwrap_or(u16::MAX),
        hole_progression: RetailHoleProgression::FrontStart,
        course: 0,
        shot_timer_ms: 30_000,
        game_timer_ms: 600_000,
        owner_uid: 0,
        natural_wind: false,
    }
}

/// Builds a retail room record from a joined room's authoritative snapshot.
fn retail_room_from_snapshot(snapshot: &RoomSnapshot) -> RetailRoom {
    let summary = snapshot.summary();
    RetailRoom {
        name: summary.name().as_str().as_bytes().to_vec(),
        public: !summary.password_protected(),
        state: RetailRoomState::Lobby,
        max_players: summary.max_members(),
        player_count: u8::try_from(snapshot.members().len()).unwrap_or(u8::MAX),
        hole_count: 1,
        room_type: RetailRoomType::Versus,
        id: u16::try_from(summary.id().get()).unwrap_or(u16::MAX),
        hole_progression: RetailHoleProgression::FrontStart,
        course: 0,
        shot_timer_ms: 30_000,
        game_timer_ms: 600_000,
        owner_uid: 0,
        natural_wind: false,
    }
}

/// Retail match client opcodes.
/// Retail room-ready client opcode. The client sends this before it will offer Start.
const RETAIL_C2S_ROOM_READY: u16 = 0x000d;
const RETAIL_C2S_START_MATCH: u16 = 0x000e;
const RETAIL_C2S_HOLE_LOAD_FINISHED: u16 = 0x0011;
const RETAIL_C2S_SHOT_COMMIT: u16 = 0x0012;
/// Shot and game timers a retail solo hole shows. Nothing enforces them: a practice hole has
/// no turn arbitration, and these are the client's own defaults.
const RETAIL_SOLO_SHOT_TIMER: Duration = Duration::from_secs(30);
const RETAIL_SOLO_GAME_TIMER: Duration = Duration::from_secs(600);

/// Post-shot ball state. A versus hole echoes it to both participants; a solo hole has
/// nobody to echo it to and accepts it without a reply.
const RETAIL_C2S_SHOT_SYNC: u16 = 0x001b;
const RETAIL_C2S_SHOT_END: u16 = 0x001c;
const RETAIL_C2S_HOLE_FINISH: u16 = 0x0031;

fn is_retail_match_opcode(opcode: u16) -> bool {
    matches!(
        opcode,
        RETAIL_C2S_START_MATCH
            | RETAIL_C2S_HOLE_LOAD_FINISHED
            | RETAIL_C2S_SHOT_COMMIT
            | RETAIL_C2S_SHOT_SYNC
            | RETAIL_C2S_SHOT_END
            | RETAIL_C2S_HOLE_FINISH
    )
}

/// Retail lobby/room client opcodes.
///
/// Deliberately disjoint from the synthetic family, so enabling the retail path cannot
/// silently reinterpret a synthetic frame.
fn is_retail_room_opcode(opcode: u16) -> bool {
    matches!(
        opcode,
        RetailRoomCreate::OPCODE
            | RetailRoomJoin::OPCODE
            | RETAIL_C2S_ROOM_LEAVE
            | RETAIL_C2S_ROOM_READY
            | RETAIL_C2S_MULTIPLAYER_JOIN
            | RETAIL_C2S_MULTIPLAYER_LEAVE
    )
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
        match lifecycle.mode {
            lobby::MatchLifecycleMode::SoloPractice => {
                observer.match_event(GameMatchObservation::Started);
            }
            lobby::MatchLifecycleMode::StrokeTwo => {
                observer.stroke_match_event(GameMatchObservation::Started);
            }
        }
    }
    observer.matches_active(lifecycle.solo_active);
    observer.stroke_matches_active(lifecycle.stroke_active);
}

fn observe_stroke_abort_terminal(observer: &dyn GameObserver, reason: MatchAbortReason) {
    let event = if reason == MatchAbortReason::LoadingTimeout {
        GameMatchObservation::LoadingTimeout
    } else {
        GameMatchObservation::Aborted
    };
    observer.stroke_match_event(event);
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
    use pangya_domain::StorageFault;
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
        stroke_commit_calls: AtomicUsize,
        stroke_abort_calls: AtomicUsize,
        begin_delay: Mutex<Duration>,
        mark_delay: Mutex<Duration>,
        commit_delay: Mutex<Duration>,
        abort_delay: Mutex<Duration>,
        stroke_commit_delay: Mutex<Duration>,
        stroke_abort_delay: Mutex<Duration>,
        begin_outcome: Mutex<Result<BeginSoloMatchOutcome, MatchRepositoryError>>,
        mark_outcome: Mutex<Result<MarkSoloInGameOutcome, MatchRepositoryError>>,
        commit_outcome: Mutex<Result<SoloMatchResult, MatchRepositoryError>>,
        abort_outcome: Mutex<Result<AbortMatchOutcome, MatchRepositoryError>>,
        stroke_commit_outcome: Mutex<Option<Result<StrokeMatchResult, MatchRepositoryError>>>,
        stroke_abort_outcome: Mutex<Option<Result<AbortStrokeMatchOutcome, MatchRepositoryError>>>,
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
                stroke_commit_calls: AtomicUsize::new(0),
                stroke_abort_calls: AtomicUsize::new(0),
                begin_delay: Mutex::new(Duration::ZERO),
                mark_delay: Mutex::new(Duration::ZERO),
                commit_delay: Mutex::new(Duration::ZERO),
                abort_delay: Mutex::new(Duration::ZERO),
                stroke_commit_delay: Mutex::new(Duration::ZERO),
                stroke_abort_delay: Mutex::new(Duration::ZERO),
                begin_outcome: Mutex::new(Ok(BeginSoloMatchOutcome::Begun)),
                mark_outcome: Mutex::new(Ok(MarkSoloInGameOutcome::Marked)),
                commit_outcome: Mutex::new(Err(MatchRepositoryError::Storage(StorageFault::Other))),
                abort_outcome: Mutex::new(Ok(AbortMatchOutcome::Aborted)),
                stroke_commit_outcome: Mutex::new(None),
                stroke_abort_outcome: Mutex::new(None),
            }
        }
    }

    impl HandoverRepository for FakeRepository {
        fn issue(&self, _handover: NewHandover) -> RepositoryFuture<'_, Result<(), HandoverError>> {
            Box::pin(async { Err(HandoverError::Storage(StorageFault::Other)) })
        }

        fn consume(
            &self,
            _request: ConsumeHandover,
        ) -> RepositoryFuture<'_, Result<AuthenticatedSession, HandoverError>> {
            Box::pin(async { Err(HandoverError::Storage(StorageFault::Other)) })
        }
    }

    impl PlayerRepository for FakeRepository {
        fn load_player_snapshot(
            &self,
            _account_id: AccountId,
        ) -> RepositoryFuture<'_, Result<PlayerSnapshot, RepositoryError>> {
            Box::pin(async { Err(RepositoryError::Storage(StorageFault::Other)) })
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
            let outcome = self.begin_outcome.lock().map_or(
                Err(MatchRepositoryError::Storage(StorageFault::Other)),
                |value| *value,
            );
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
            let outcome = self.mark_outcome.lock().map_or(
                Err(MatchRepositoryError::Storage(StorageFault::Other)),
                |value| *value,
            );
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
            let outcome = self.abort_outcome.lock().map_or(
                Err(MatchRepositoryError::Storage(StorageFault::Other)),
                |value| *value,
            );
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
            let outcome = self.commit_outcome.lock().map_or(
                Err(MatchRepositoryError::Storage(StorageFault::Other)),
                |value| *value,
            );
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                outcome
            })
        }

        fn commit_stroke_match(
            &self,
            _request: pangya_domain::CommitStrokeMatch,
        ) -> RepositoryFuture<'_, Result<StrokeMatchResult, MatchRepositoryError>> {
            self.stroke_commit_calls.fetch_add(1, Ordering::Relaxed);
            let delay = self
                .stroke_commit_delay
                .lock()
                .map_or(Duration::ZERO, |value| *value);
            let outcome = self
                .stroke_commit_outcome
                .lock()
                .ok()
                .and_then(|value| *value)
                .unwrap_or(Err(MatchRepositoryError::Storage(StorageFault::Other)));
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                outcome
            })
        }

        fn abort_stroke(
            &self,
            _request: AbortStrokeMatch,
        ) -> RepositoryFuture<'_, Result<AbortStrokeMatchOutcome, MatchRepositoryError>> {
            self.stroke_abort_calls.fetch_add(1, Ordering::Relaxed);
            let delay = self
                .stroke_abort_delay
                .lock()
                .map_or(Duration::ZERO, |value| *value);
            let outcome = self
                .stroke_abort_outcome
                .lock()
                .ok()
                .and_then(|value| *value)
                .unwrap_or(Err(MatchRepositoryError::Storage(StorageFault::Other)));
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

    impl EconomyRepository for FakeRepository {
        fn purchase(
            &self,
            _request: PurchaseRequest,
        ) -> RepositoryFuture<'_, Result<EconomyCommit<pangya_domain::PurchaseResult>, EconomyError>>
        {
            Box::pin(async { Err(EconomyError::Storage(StorageFault::Other)) })
        }
        fn equip(
            &self,
            _request: EquipmentChange,
        ) -> RepositoryFuture<
            '_,
            Result<EconomyCommit<pangya_domain::EquipmentChangeResult>, EconomyError>,
        > {
            Box::pin(async { Err(EconomyError::Storage(StorageFault::Other)) })
        }
        fn consume_one(
            &self,
            _request: ConsumeItem,
        ) -> RepositoryFuture<
            '_,
            Result<EconomyCommit<pangya_domain::ConsumeItemResult>, EconomyError>,
        > {
            Box::pin(async { Err(EconomyError::Storage(StorageFault::Other)) })
        }
        fn repair(
            &self,
            _request: RepairItem,
        ) -> RepositoryFuture<
            '_,
            Result<EconomyCommit<pangya_domain::RepairItemResult>, EconomyError>,
        > {
            Box::pin(async { Err(EconomyError::Storage(StorageFault::Other)) })
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
            character_id: None,
        }
    }

    fn second_test_identity() -> RoomIdentity {
        RoomIdentity {
            connection_id: PlayerConnectionId::new(2).unwrap_or_else(|_| unreachable!()),
            account_id: AccountId::new(8).unwrap_or_else(|_| unreachable!()),
            nickname: Nickname::parse("Second").unwrap_or_else(|_| unreachable!()),
            character_id: None,
        }
    }

    fn stroke_config(catalog: &Catalog, commit_timeout: Duration) -> StrokeRuntimeConfig {
        StrokeRuntimeConfig {
            course: catalog
                .one_hole_course(pangya_domain::CourseId::new(7).unwrap_or_else(|_| unreachable!()))
                .unwrap_or_else(|_| unreachable!()),
            catalog_fingerprint: catalog.fingerprint(),
            loading_timeout: Duration::from_secs(5),
            turn_timeout: Duration::from_secs(5),
            game_timeout: Duration::from_secs(30),
            commit_timeout,
            max_strokes: 9,
            startup_recovery_limit: IncompleteMatchAbortLimit::new(10)
                .unwrap_or_else(|_| unreachable!()),
            shot_packets_per_window: 80,
        }
    }

    fn test_stroke_service(
        repository: Arc<FakeRepository>,
        commit_timeout: Duration,
    ) -> GameService<FakeRepository> {
        let catalog = test_catalog();
        GameService::new(
            repository,
            catalog.clone(),
            GameRuntimeConfig {
                stroke_two: Some(stroke_config(&catalog, commit_timeout)),
                ..GameRuntimeConfig::default()
            },
            Arc::new(NoopGameObserver),
        )
        .unwrap_or_else(|_| unreachable!())
    }

    fn test_stroke_plan(service: &GameService<FakeRepository>, nonce: u128) -> StrokeStartPlan {
        let stroke = service.config.stroke_two.unwrap_or_else(|| unreachable!());
        let first = test_identity();
        let second = second_test_identity();
        let seed = MatchSeed::new([u8::try_from(nonce).unwrap_or(1); 32]);
        let (weather, wind) = deterministic_conditions(seed).unwrap_or_else(|_| unreachable!());
        let begin = BeginStrokeMatch::new(
            MatchId::new(uuid::Uuid::from_u128(nonce)),
            MatchResultKey::new(uuid::Uuid::from_u128(nonce.saturating_add(100))),
            [
                StrokeParticipant::new(
                    first.account_id,
                    StrokeRosterOrder::First,
                    MatchResultKey::new(uuid::Uuid::from_u128(nonce.saturating_add(101))),
                ),
                StrokeParticipant::new(
                    second.account_id,
                    StrokeRosterOrder::Second,
                    MatchResultKey::new(uuid::Uuid::from_u128(nonce.saturating_add(102))),
                ),
            ],
            stroke.course,
            stroke.catalog_fingerprint,
            seed,
            weather,
            wind,
        )
        .unwrap_or_else(|_| unreachable!());
        StrokeStartPlan::new(
            begin,
            [first.connection_id, second.connection_id],
            stroke.loading_timeout,
            stroke.turn_timeout,
            stroke.game_timeout,
            stroke.max_strokes,
        )
        .unwrap_or_else(|_| unreachable!())
    }

    async fn prepare_test_stroke_room(
        service: &GameService<FakeRepository>,
        plan: StrokeStartPlan,
    ) -> (RoomId, mpsc::Receiver<RoomEvent>, mpsc::Receiver<RoomEvent>) {
        let first = test_identity();
        let second = second_test_identity();
        let (first_outbound, first_events) = mpsc::channel(32);
        let (second_outbound, second_events) = mpsc::channel(32);
        let room = service
            .lobby
            .create(
                pangya_domain::RoomName::parse("stroke").unwrap_or_else(|_| unreachable!()),
                None,
                pangya_domain::RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                first.clone(),
                first_outbound,
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        service
            .lobby
            .join(
                room.id(),
                second.clone(),
                None,
                second_outbound,
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        service
            .lobby
            .route(first.connection_id, LobbyRoomCommand::SetReady(true))
            .await
            .unwrap_or_else(|_| unreachable!());
        service
            .lobby
            .route(second.connection_id, LobbyRoomCommand::SetReady(true))
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(matches!(
            service
                .lobby
                .route_stroke(
                    first.connection_id,
                    LobbyStrokeCommand::PrepareStart(plan.clone())
                )
                .await,
            Ok(LobbyStrokeRouteResult::Begin(_))
        ));
        service
            .lobby
            .route_stroke(
                first.connection_id,
                LobbyStrokeCommand::ConfirmBegin {
                    match_id: plan.begin().match_id(),
                    result_key: plan.begin().result_key(),
                },
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        (room.id(), first_events, second_events)
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
    async fn economy_composition_rejects_out_of_range_bounds_and_catalogs_without_consumables() {
        let economy_catalog = || {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../pangya-data/tests/fixtures/synthetic-catalog-v2");
            Catalog::load(&root, std::path::Path::new("manifest.toml"))
                .unwrap_or_else(|_| unreachable!())
        };
        let valid = EconomyRuntimeConfig {
            command_timeout: Duration::from_secs(2),
            commands_per_window: 30,
            page_size: 25,
            max_purchase_quantity: 50,
        };
        let compose = |catalog: Catalog, economy: EconomyRuntimeConfig| {
            GameService::new(
                Arc::new(FakeRepository::default()),
                catalog,
                GameRuntimeConfig {
                    economy: Some(economy),
                    ..GameRuntimeConfig::default()
                },
                Arc::new(NoopGameObserver),
            )
            .map(drop)
        };

        compose(economy_catalog(), valid).expect("valid economy composes");

        let grace = GameRuntimeLimits::default().shutdown_grace;
        for out_of_range in [
            EconomyRuntimeConfig {
                command_timeout: Duration::ZERO,
                ..valid
            },
            EconomyRuntimeConfig {
                command_timeout: grace + Duration::from_secs(1),
                ..valid
            },
            EconomyRuntimeConfig {
                commands_per_window: 0,
                ..valid
            },
            EconomyRuntimeConfig {
                commands_per_window: 1_000_001,
                ..valid
            },
            EconomyRuntimeConfig {
                page_size: 0,
                ..valid
            },
            EconomyRuntimeConfig {
                page_size: pangya_protocol::MAX_SHOP_PAGE_ENTRIES + 1,
                ..valid
            },
            EconomyRuntimeConfig {
                max_purchase_quantity: 0,
                ..valid
            },
            EconomyRuntimeConfig {
                max_purchase_quantity: pangya_protocol::MAX_PURCHASE_QUANTITY + 1,
                ..valid
            },
        ] {
            assert!(
                matches!(
                    compose(economy_catalog(), out_of_range),
                    Err(GameRuntimeError::InvalidConfig)
                ),
                "expected InvalidConfig for {out_of_range:?}"
            );
        }

        // The M3 catalog carries no shop offers at all, so it cannot price an economy.
        assert!(matches!(
            compose(test_catalog(), valid),
            Err(GameRuntimeError::Catalog)
        ));
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
                *configured = Err(MatchRepositoryError::Storage(StorageFault::Other));
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
            *outcome = Err(MatchRepositoryError::Storage(StorageFault::Other));
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
            *begin = Err(MatchRepositoryError::Storage(StorageFault::Other));
        }
        if let Ok(mut abort) = repository.abort_outcome.lock() {
            *abort = Err(MatchRepositoryError::Storage(StorageFault::Other));
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
    async fn priority_stroke_disconnect_persists_once_without_survivor_coordinator_work() {
        let repository = Arc::new(FakeRepository::default());
        let service = test_stroke_service(Arc::clone(&repository), Duration::from_millis(50));
        let plan = test_stroke_plan(&service, 40);
        let (room_id, mut owner_events, mut survivor_events) =
            prepare_test_stroke_room(&service, plan.clone()).await;
        while owner_events.try_recv().is_ok() {}
        while survivor_events.try_recv().is_ok() {}
        let outcome = service
            .lobby
            .disconnect_with_work(test_identity().connection_id, MatchAbortReason::Disconnect)
            .await
            .unwrap_or_else(|_| unreachable!());
        let room::RoomCloseOutcome::M6Abort { request: abort, .. } = outcome else {
            unreachable!()
        };
        assert!(
            std::iter::from_fn(|| survivor_events.try_recv().ok()).all(|event| !matches!(
                event,
                RoomEvent::StrokeAbortRequested(_) | RoomEvent::StrokeSettlementRequested(_)
            ))
        );
        if let Ok(mut value) = repository.stroke_abort_outcome.lock() {
            *value = Some(Ok(AbortStrokeMatchOutcome::Aborted));
        }
        assert_eq!(
            service
                .persist_stroke_abort_by_room(room_id, abort, true)
                .await,
            Ok(AbortResolution::Aborted)
        );
        assert_eq!(repository.stroke_abort_calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            survivor_events.recv().await,
            Some(RoomEvent::StrokeAborted(abort))
        );
        assert!(
            service
                .lobby
                .route(
                    second_test_identity().connection_id,
                    LobbyRoomCommand::GetState
                )
                .await
                .is_ok()
        );

        let repository = Arc::new(FakeRepository::default());
        let service = test_stroke_service(Arc::clone(&repository), Duration::from_millis(50));
        let plan = test_stroke_plan(&service, 50);
        let (room_id, mut owner_events, mut survivor_events) =
            prepare_test_stroke_room(&service, plan).await;
        let loading = StrokeLoadingComplete::new(100).unwrap_or_else(|_| unreachable!());
        assert_eq!(
            service
                .lobby
                .route_stroke(
                    second_test_identity().connection_id,
                    LobbyStrokeCommand::LoadingComplete(loading)
                )
                .await,
            Ok(LobbyStrokeRouteResult::Loading(
                StrokeLoadingOutcome::Waiting
            ))
        );
        let mark = match service
            .lobby
            .route_stroke(
                test_identity().connection_id,
                LobbyStrokeCommand::LoadingComplete(loading),
            )
            .await
        {
            Ok(LobbyStrokeRouteResult::Loading(StrokeLoadingOutcome::PersistenceRequired(
                mark,
            ))) => mark,
            _ => unreachable!(),
        };
        service
            .lobby
            .apply_stroke_in_game_by_room(room_id, mark)
            .await
            .unwrap_or_else(|_| unreachable!());
        while owner_events.try_recv().is_ok() {}
        while survivor_events.try_recv().is_ok() {}
        let outcome = service
            .lobby
            .disconnect_with_work(test_identity().connection_id, MatchAbortReason::Disconnect)
            .await
            .unwrap_or_else(|_| unreachable!());
        let room::RoomCloseOutcome::M6Settlement {
            request: commit, ..
        } = outcome
        else {
            unreachable!()
        };
        assert!(
            std::iter::from_fn(|| survivor_events.try_recv().ok()).all(|event| !matches!(
                event,
                RoomEvent::StrokeAbortRequested(_) | RoomEvent::StrokeSettlementRequested(_)
            ))
        );
        let players = [0_usize, 1_usize].map(|index| {
            let input = commit.players()[index];
            let reward = pangya_domain::synthetic_stroke_reward_v1(
                commit.config(),
                input.strokes(),
                input.completion(),
            )
            .unwrap_or_else(|_| unreachable!());
            pangya_domain::StrokePlayerResult::new(
                input,
                reward,
                pangya_domain::ServerBalances::from_persisted(100, 100),
            )
        });
        let committed = StrokeMatchResult::new(commit.match_id(), commit.result_key(), players);
        if let Ok(mut value) = repository.stroke_commit_outcome.lock() {
            *value = Some(Ok(committed));
        }
        assert_eq!(
            service.persist_stroke_commit_by_room(room_id, commit).await,
            Ok(())
        );
        assert_eq!(repository.stroke_commit_calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            survivor_events.recv().await,
            Some(RoomEvent::StrokeCommitted(committed))
        );
        assert!(
            service
                .lobby
                .route(
                    second_test_identity().connection_id,
                    LobbyRoomCommand::GetState
                )
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn concurrent_final_stroke_disconnect_retains_hidden_authority_until_one_apply() {
        let repository = Arc::new(FakeRepository::default());
        let service = test_stroke_service(Arc::clone(&repository), Duration::from_millis(50));
        let plan = test_stroke_plan(&service, 55);
        let (room_id, _owner_events, _peer_events) = prepare_test_stroke_room(&service, plan).await;
        let loading = StrokeLoadingComplete::new(100).unwrap_or_else(|_| unreachable!());
        assert!(matches!(
            service
                .lobby
                .route_stroke(
                    second_test_identity().connection_id,
                    LobbyStrokeCommand::LoadingComplete(loading),
                )
                .await,
            Ok(LobbyStrokeRouteResult::Loading(
                StrokeLoadingOutcome::Waiting
            ))
        ));
        let mark = match service
            .lobby
            .route_stroke(
                test_identity().connection_id,
                LobbyStrokeCommand::LoadingComplete(loading),
            )
            .await
        {
            Ok(LobbyStrokeRouteResult::Loading(StrokeLoadingOutcome::PersistenceRequired(
                mark,
            ))) => mark,
            _ => unreachable!(),
        };
        service
            .lobby
            .apply_stroke_in_game_by_room(room_id, mark)
            .await
            .unwrap_or_else(|_| unreachable!());
        let mut lifecycle = service.lobby.subscribe_match_lifecycle();
        let first = service
            .lobby
            .disconnect_with_work(test_identity().connection_id, MatchAbortReason::Disconnect);
        let second = service.lobby.disconnect_with_work(
            second_test_identity().connection_id,
            MatchAbortReason::Disconnect,
        );
        let (first, second) = tokio::join!(first, second);
        let outcomes = [
            first.unwrap_or_else(|_| unreachable!()),
            second.unwrap_or_else(|_| unreachable!()),
        ];
        let commits: Vec<_> = outcomes
            .into_iter()
            .filter_map(|outcome| match outcome {
                room::RoomCloseOutcome::M6Settlement { request, .. } => Some(request),
                room::RoomCloseOutcome::None => None,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            commits.len(),
            1,
            "only the first cleanup claims persistence"
        );
        assert!(service.lobby.list().await.unwrap_or_default().is_empty());
        let (outbound, _events) = mpsc::channel(8);
        assert_eq!(
            service
                .lobby
                .join(
                    room_id,
                    test_identity(),
                    None,
                    outbound,
                    CancellationToken::new(),
                )
                .await,
            Err(RoomError::RoomNotFound),
            "pending empty authority is not joinable",
        );

        let commit = commits[0];
        let players = [0_usize, 1_usize].map(|index| {
            let input = commit.players()[index];
            let reward = pangya_domain::synthetic_stroke_reward_v1(
                commit.config(),
                input.strokes(),
                input.completion(),
            )
            .unwrap_or_else(|_| unreachable!());
            pangya_domain::StrokePlayerResult::new(
                input,
                reward,
                pangya_domain::ServerBalances::from_persisted(100, 100),
            )
        });
        let committed = StrokeMatchResult::new(commit.match_id(), commit.result_key(), players);
        if let Ok(mut value) = repository.stroke_commit_outcome.lock() {
            *value = Some(Ok(committed));
        }
        assert_eq!(
            service.persist_stroke_commit_by_room(room_id, commit).await,
            Ok(())
        );
        assert_eq!(repository.stroke_commit_calls.load(Ordering::Relaxed), 1);
        assert!(matches!(
            lifecycle.recv().await,
            Ok(lobby::MatchLifecycle {
                stroke_active: 0,
                ..
            })
        ));
        assert_eq!(
            service
                .lobby
                .apply_stroke_commit_by_room(room_id, committed)
                .await,
            Err(StrokeMatchError::IdentityMismatch),
            "actor and registry close exactly once after apply",
        );
        assert!(service.lobby.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn fake_stroke_abort_timeout_and_already_committed_are_bounded_and_authoritative() {
        let repository = Arc::new(FakeRepository::default());
        if let Ok(mut delay) = repository.stroke_abort_delay.lock() {
            *delay = Duration::from_millis(50);
        }
        if let Ok(mut outcome) = repository.stroke_abort_outcome.lock() {
            *outcome = Some(Ok(AbortStrokeMatchOutcome::Aborted));
        }
        let service = test_stroke_service(Arc::clone(&repository), Duration::from_millis(5));
        let plan = test_stroke_plan(&service, 60);
        let (room_id, _owner_events, _survivor_events) =
            prepare_test_stroke_room(&service, plan).await;
        let outcome = service
            .lobby
            .disconnect_with_work(test_identity().connection_id, MatchAbortReason::Disconnect)
            .await
            .unwrap_or_else(|_| unreachable!());
        let room::RoomCloseOutcome::M6Abort { request: abort, .. } = outcome else {
            unreachable!()
        };
        assert_eq!(
            service
                .persist_stroke_abort_by_room(room_id, abort, true)
                .await,
            Err(GameRuntimeError::MatchPersistence)
        );
        assert_eq!(repository.stroke_abort_calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            service
                .lobby
                .disconnect_with_work(
                    second_test_identity().connection_id,
                    MatchAbortReason::Shutdown,
                )
                .await,
            Ok(room::RoomCloseOutcome::None),
            "an explicit persistence failure retains its claim for fatal/startup recovery"
        );
        assert!(service.lobby.shutdown().await.is_ok());
        assert_eq!(repository.stroke_abort_calls.load(Ordering::Relaxed), 1);

        let repository = Arc::new(FakeRepository::default());
        let service = test_stroke_service(Arc::clone(&repository), Duration::from_millis(50));
        let plan = test_stroke_plan(&service, 70);
        let (room_id, mut owner_events, mut survivor_events) =
            prepare_test_stroke_room(&service, plan.clone()).await;
        while owner_events.try_recv().is_ok() {}
        while survivor_events.try_recv().is_ok() {}
        let outcome = service
            .lobby
            .disconnect_with_work(test_identity().connection_id, MatchAbortReason::Disconnect)
            .await
            .unwrap_or_else(|_| unreachable!());
        let room::RoomCloseOutcome::M6Abort { request: abort, .. } = outcome else {
            unreachable!()
        };
        assert!(
            std::iter::from_fn(|| survivor_events.try_recv().ok()).all(|event| !matches!(
                event,
                RoomEvent::StrokeAbortRequested(_) | RoomEvent::StrokeSettlementRequested(_)
            ))
        );
        let players = [0_usize, 1_usize].map(|index| {
            let input = pangya_domain::StrokePlayerCommit::new(
                plan.begin().participants()[index],
                1,
                if index == 0 {
                    pangya_domain::StrokePlace::First
                } else {
                    pangya_domain::StrokePlace::Second
                },
                DomainStrokeCompletion::Holed,
            )
            .unwrap_or_else(|_| unreachable!());
            let reward = pangya_domain::synthetic_stroke_reward_v1(
                plan.begin().config(),
                1,
                DomainStrokeCompletion::Holed,
            )
            .unwrap_or_else(|_| unreachable!());
            pangya_domain::StrokePlayerResult::new(
                input,
                reward,
                pangya_domain::ServerBalances::from_persisted(100, 100),
            )
        });
        let committed = StrokeMatchResult::new(abort.match_id(), abort.result_key(), players);
        if let Ok(mut outcome) = repository.stroke_abort_outcome.lock() {
            *outcome = Some(Ok(AbortStrokeMatchOutcome::AlreadyCommitted(committed)));
        }
        assert_eq!(
            service
                .persist_stroke_abort_by_room(room_id, abort, true)
                .await,
            Ok(AbortResolution::Committed)
        );
        assert_eq!(repository.stroke_abort_calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            survivor_events.recv().await,
            Some(RoomEvent::StrokeCommitted(committed))
        );
        assert!(
            service
                .lobby
                .route(
                    second_test_identity().connection_id,
                    LobbyRoomCommand::GetState
                )
                .await
                .is_ok()
        );
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
            *abort = Err(MatchRepositoryError::Storage(StorageFault::Other));
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
            *abort = Err(MatchRepositoryError::Storage(StorageFault::Other));
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
                    GameState::InMatchLoading,
                    RoomEvent::AbortRequested(abort),
                    None,
                    test_identity().connection_id,
                    &mut None,
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

    #[test]
    fn stroke_runtime_rejects_invalid_course_before_listener_binding() {
        let catalog = test_catalog();
        let invalid_course = OneHoleConfig::new(
            pangya_domain::CourseId::new(99).unwrap_or_else(|_| unreachable!()),
            3,
        )
        .unwrap_or_else(|_| unreachable!());
        let stroke = StrokeRuntimeConfig {
            course: invalid_course,
            catalog_fingerprint: catalog.fingerprint(),
            loading_timeout: Duration::from_secs(5),
            turn_timeout: Duration::from_secs(5),
            game_timeout: Duration::from_secs(30),
            commit_timeout: Duration::from_secs(1),
            max_strokes: 9,
            startup_recovery_limit: IncompleteMatchAbortLimit::new(10)
                .unwrap_or_else(|_| unreachable!()),
            shot_packets_per_window: 80,
        };
        assert_eq!(
            GameService::new(
                Arc::new(FakeRepository::default()),
                catalog,
                GameRuntimeConfig {
                    stroke_two: Some(stroke),
                    ..GameRuntimeConfig::default()
                },
                Arc::new(NoopGameObserver),
            )
            .err(),
            Some(GameRuntimeError::Catalog)
        );
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
