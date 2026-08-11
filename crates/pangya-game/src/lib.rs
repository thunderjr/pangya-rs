#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Bounded synthetic GameService handover, bootstrap, lobby, and room runtime.

pub mod lobby;
pub mod match_state;
pub mod room;
pub mod stroke_state;

use crate::room::{RoomOutbound, TerminalDelivery, TerminalOutboxSender};
pub use lobby::{
    LobbyHandle, LobbyLimits, LobbyRoomCommand, LobbyRouteResult, LobbyShutdownError,
    LobbyShutdownOutcome, LobbySoloCommand, LobbySoloRouteResult, LobbyStrokeCommand,
    LobbyStrokePersistence, LobbyStrokeRouteResult, spawn_lobby,
};
pub use match_state::{
    LOADING_TIMEOUT_HARD_CAP, MAX_SOLO_STROKES, RelayDisposition, SoloMatchError, SoloMatchPhase,
    SoloMatchState, SoloStartPlan, deterministic_conditions, deterministic_conditions_for_gameplay,
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
    collections::{BTreeMap, HashMap, VecDeque},
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use chrono::{Datelike as _, Timelike as _};
use futures_util::{SinkExt as _, StreamExt as _};
use pangya_data::Catalog;
use pangya_domain::{
    AbortMatch, AbortMatchOutcome, AbortStrokeMatch, AbortStrokeMatchOutcome, AccountId,
    AdminItemGrant, BeginSoloMatch, BeginSoloMatchOutcome, BeginStrokeMatch,
    BeginStrokeMatchOutcome, CatalogFingerprint, CharacterId, ConsumeHandover, ConsumeItem,
    CourseId, EconomyCommit, EconomyError, EconomyItemSelector, EconomyOperationId,
    EconomyRepository, EquipmentChange, HandoverRepository, InventoryClass, InventoryItemId,
    ItemDefinition, ItemDurability, ItemKind, ItemStacking, ItemTypeId, LoginBonusReward,
    MarkSoloInGame, MarkSoloInGameOutcome, MarkStrokeInGame, MarkStrokeInGameOutcome,
    MascotMessageUpdate, MatchAbortReason, MatchId, MatchPlan, MatchRepository, MatchResultKey,
    MatchSeed, MemberCard, MemberSnapshot, Nickname, OfflineNoteClaim, OfflineNoteRequest,
    PlayerConnectionId, PlayerRepository, PlayerSnapshot, PurchaseRequest, RecentPlayer,
    RepairItem, RepositoryError, RetailEquipmentChange, RetailEquipmentState, RoomError, RoomId,
    RoomName, RoomPassword, RoomProfile, RoomSettings, RoomSnapshot, RoomSummary,
    ServiceKind as DomainServiceKind, ShopOverlay, SoloMatchResult, SourceAddressPrefix,
    StrokeCompletion as DomainStrokeCompletion, StrokeMatchResult, StrokeParticipant,
    StrokeRosterOrder,
};
use pangya_login::{
    CapacityRegistry, FixedWindowLimiter, KeyedCapacityGuard, KeyedCapacityRegistry, RateDecision,
    RegistryError, RegistryGuard, generate_handover, parse_handover,
};
use pangya_protocol::{
    BalanceUpdate, CHARACTER_PARTS, CHARACTER_STATS, ChannelJoined, CharacterBootstrap,
    CharacterInfo, CodecLimits, CompatibilityProfile, ConsumeOneRequest, DecodePacket,
    EQUIPPED_ITEM_SLOTS, EconomyCommand, EconomyCommandResult, EconomyItemKind, EconomyOutcome,
    EncodePacket, EquipRequest, EquipmentChanged, EquipmentInfo, FinishHole, FrameCodec,
    GAME_INVENTORY_SEGMENT_ITEMS, GameAuth, GameChat, GameChatResponse, GmRequest, GmSubcommand,
    HandoverControl, HandoverReply, HoleResult, IffContainerChunk, IffContainerKind,
    InventoryBootstrap, InventoryChanged, InventorySegment, Lie, LoadingComplete, LoungeAction,
    LoungeActionResponse, LoungeEnterRequest, LoungeEnterResponse, MacroUpdate,
    MatchAbortReason as ProtocolMatchAbortReason, MatchAborted, MatchPhase, MatchStarted,
    MessageServerList, MessageServerListRequest, NoteSend, OutboundFrame, PacketEncodeError,
    PacketWriter, PlayerInfo, PurchaseCommitted, PurchaseRequestPacket,
    RETAIL_C2S_FIRST_SHOT_READY, RETAIL_RECENT_PLAYERS, RepairCommitted, RepairRequest,
    RetailCaddie, RetailChannel, RetailChannelJoinNotice, RetailChannelJoined, RetailCharacter,
    RetailClientException, RetailDailyQuestDelta, RetailDailyQuestRequest, RetailDailyQuestState,
    RetailEquipment, RetailEquipmentAnnounce, RetailEquipmentRequested, RetailEquipmentSlot,
    RetailEquipmentUpdate, RetailEquipmentUpdated, RetailFinishHole, RetailFirstShotReady,
    RetailGameAuth, RetailHole, RetailHoleProgression, RetailHoleWeather, RetailHoleWind,
    RetailInventoryClass, RetailInventoryItem, RetailLoadProgress, RetailLobbyEquipmentUpdate,
    RetailLockerCombinationAttempt, RetailLockerCombinationResponse, RetailLockerInventoryRequest,
    RetailLockerInventoryResponse, RetailLoginBonusClaimRequest, RetailLoginBonusClaimResponse,
    RetailLoginBonusItemGrant, RetailLoginBonusRequest, RetailLoginBonusStatus,
    RetailMascotMessageResult, RetailMascotMessageUpdate, RetailMascotSeed, RetailMatchFinish,
    RetailMatchInfo, RetailMatchOpen, RetailMatchOpenAck, RetailMatchPlayer, RetailMatchStart,
    RetailMessageServerList, RetailMessageServerListRequest, RetailMultiplayerJoined,
    RetailMultiplayerLeft, RetailMyRoomEnter, RetailMyRoomEntered, RetailMyRoomFurniture,
    RetailMyRoomInventoryRequest, RetailMyRoomLayout, RetailNewSessionKey,
    RetailNewSessionKeyRequest, RetailPangBalance, RetailPangRate, RetailPangSpent,
    RetailPlayerData, RetailPlayerHistoryEntries, RetailPlayerHistoryRequest, RetailPlayerIdentity,
    RetailPlayerInfo, RetailPlayerStartHole, RetailPlayerStatistics, RetailPlayerStatisticsReport,
    RetailPointBalance, RetailPracticeShotSync, RetailPracticeShotSyncRequest, RetailPracticeStart,
    RetailPurchaseItem, RetailPurchaseRequest, RetailPurchaseResponse, RetailRateTable,
    RetailRecentPlayerSlot, RetailRoom, RetailRoomCensus, RetailRoomCreate,
    RetailRoomEquipmentUpdate, RetailRoomEquipmentUpdatePacket, RetailRoomInformationRequest,
    RetailRoomInformationResponse, RetailRoomInformationUser, RetailRoomInvite,
    RetailRoomInviteInfo, RetailRoomInviteInfoResponse, RetailRoomInviteNotification,
    RetailRoomInviteResponse, RetailRoomJoin, RetailRoomJoinResult, RetailRoomKick,
    RetailRoomLeave, RetailRoomList, RetailRoomPlayer, RetailRoomResync, RetailRoomSettingChange,
    RetailRoomSettingsUpdate, RetailRoomState, RetailRoomStatus, RetailRoomType,
    RetailSelectChannel, RetailServerEntry, RetailServerList, RetailServerListRequest,
    RetailServerTime, RetailServerTimeRequest, RetailShopJoin, RetailShopJoined,
    RetailShotCommitRelay, RetailShotSync, RetailStanding, RetailSubServerConnect,
    RetailSubServerEntry, RetailTeamChange, RetailTeamChangeAnnounce, RetailTurnEnd,
    RetailTurnStart, RetailUccUploadKeyRefusal, RetailWeather, RoomChatEvent, RoomChatRequest,
    RoomCommand, RoomCommandResult, RoomCommandResultResponse, RoomCreateRequest,
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
    StrokeStandings, StrokeTurnStarted, TypingIndicator, TypingIndicatorResponse, UnknownBytes,
    UserCharacterInfoResponse, UserCourseRecordsInfoResponse, UserEquipmentInfoResponse,
    UserGrandPrixTrophiesInfoResponse, UserGuildInfoResponse, UserInfoRequest, UserInfoResponse,
    UserNameInfoResponse, UserRelatedInfoResponse, UserSpecialTrophiesInfoResponse,
    UserStatisticsInfoResponse, UserStatusRequest, UserStatusResponse, UserTrophiesInfoResponse,
    Weather as ProtocolWeather, Whisper, WhisperRefusalResponse, WhisperResponse, Wind,
    authorize_gm_request, decode_gm_request, decode_packet_payload, encode_packet_payload,
    is_retail_accepted_match_opcode, is_retail_accepted_session_opcode,
    is_retail_explicit_social_refusal, packed_system_time, synthetic_game_hello, us852_game_hello,
};
use rand::{RngCore as _, rngs::OsRng};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::{
    io::AsyncWriteExt as _,
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore, broadcast, watch},
    task::JoinSet,
    time::{sleep_until, timeout},
};
use tokio_util::{codec::Framed, sync::CancellationToken};
use tracing::Instrument as _;

#[cfg(test)]
use tokio::sync::mpsc;

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
    /// Identity of a durable stroke settlement commit, for correlation without payload bodies.
    fn stroke_commit_identity(&self, _match_id: MatchId, _result_key: MatchResultKey) {}
    /// Identity of a terminal payload marker consumed by a connection.
    fn stroke_terminal_payload(
        &self,
        _connection_id: GameConnectionId,
        _match_id: MatchId,
        _result_key: MatchResultKey,
        _generation: u64,
    ) {
    }
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
    /// Catalog-validated course and whole-card plan.
    pub course: MatchPlan,
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
    /// Catalog-validated course and whole-card plan.
    pub course: MatchPlan,
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

/// Checked daily login bonus configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoginBonusRuntimeConfig {
    /// One catalog-defined reward used for each server day.
    pub reward: LoginBonusReward,
    /// Number of calendar days before the display day wraps to one.
    pub calendar_days: u32,
}

impl LoginBonusRuntimeConfig {
    /// Validates the bounded calendar and configured reward quantity.
    fn validate(self, catalog: &Catalog) -> Result<(), GameRuntimeError> {
        // The whole definition is authoritative, not just the type ID. Comparing every field
        // prevents an operator from changing the family, stacking cap, durability, sale policy,
        // or character compatibility while retaining a valid-looking catalog identifier.
        if self.calendar_days == 0
            || self.reward.quantity == 0
            || catalog.item_definition(self.reward.definition.type_id)
                != Some(&self.reward.definition)
        {
            return Err(GameRuntimeError::Catalog);
        }
        match self.reward.definition.stacking {
            ItemStacking::Unique if self.reward.quantity != 1 => {
                return Err(GameRuntimeError::Catalog);
            }
            ItemStacking::Stackable { max_stack }
                if max_stack == 0 || self.reward.quantity > max_stack =>
            {
                return Err(GameRuntimeError::Catalog);
            }
            ItemStacking::Unique | ItemStacking::Stackable { .. } => {}
        }
        if self.reward.definition.kind == ItemKind::Character {
            return Err(GameRuntimeError::Catalog);
        }
        Ok(())
    }
}

/// Immutable GameService composition.
#[derive(Clone, Debug)]
pub struct GameRuntimeConfig {
    /// Sole locally configured channel ID used by the listener's initial channel.
    pub channel_id: u32,
    /// Every retail sub-server ID this service advertises and accepts for transitions.
    ///
    /// The initial channel must be present. Keeping this topology explicit prevents `0x0083`
    /// from treating an arbitrary byte (or the current channel) as a successful move.
    pub advertised_channel_ids: Vec<u8>,
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
    /// Optional daily login bonus. The reward is resolved against the immutable catalog at startup.
    pub login_bonus: Option<LoginBonusRuntimeConfig>,
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
            advertised_channel_ids: vec![1],
            unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
            limits: GameRuntimeLimits::default(),
            solo_practice: None,
            stroke_two: None,
            economy: None,
            login_bonus: None,
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

/// Everything a retail connection remembers about the hole it is in.
#[derive(Clone, Copy, Debug, Default)]
struct ConnectionMatchContext {
    stroke: Option<ConnectionStrokeContext>,
    /// One-based hole currently loading/playing in a retail card.
    solo_hole: u8,
    /// Weather and wind for the hole being loaded, withheld until the room actor hands out
    /// the first turn.
    ///
    /// No reference server sends either before every client has reported its hole loaded, and
    /// the retail client crashes during the load ramp when it is told early. See
    /// `docs/protocol/US852_RETAIL_BOOTSTRAP.md`.
    atmosphere: Option<RetailHoleAtmosphere>,
}

/// The weather and wind frames a hole is introduced with, once its players are all loaded.
#[derive(Clone, Copy, Debug)]
struct RetailHoleAtmosphere {
    weather: RetailWeather,
    wind: pangya_domain::WindConditions,
}

/// Connection-local order state for a retail stroke's opaque wire phases.
///
/// PacketDoc models `0x0012` as the committed shot (`gameservice/client/0012.ksy:21-64`),
/// describes `0x001c` after the undocumented `0x001b` sync
/// (`gameservice/client/001c.ksy:10-40`), and documents `0x0031` as the cumulative hole
/// statistics submission (`gameservice/client/0031.ksy:10-27`). A real U.S. 851 trace at
/// `/private/tmp/issue45-server-4f64913-restart.log` reverses the latter two barriers: accepted
/// `0x0012` at `2026-08-11T13:23:49.906575`, `0x001b` at `13:23:50.206577`, `0x0031` at
/// `13:23:54.356862`, then `0x001c` at `13:23:59.328564`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RetailStrokeSequence {
    awaiting_result: bool,
    early_hole_finish: bool,
}

impl RetailStrokeSequence {
    fn accepted_action(&mut self) {
        self.awaiting_result = true;
        self.early_hole_finish = false;
    }

    fn remember_early_hole_finish(&mut self) -> bool {
        if !self.awaiting_result {
            return false;
        }
        self.early_hole_finish = true;
        true
    }

    fn accepted_result(&mut self) -> bool {
        if !self.awaiting_result {
            self.clear();
            return false;
        }
        self.awaiting_result = false;
        std::mem::take(&mut self.early_hole_finish)
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug)]
struct ConnectionStrokeContext {
    match_id: MatchId,
    roster: [PlayerConnectionId; 2],
    seed: pangya_domain::MatchSeed,
    natural_wind: bool,
    /// One-based hole whose opening frame was last emitted.
    hole: u8,
    /// The participant this connection was last told owns the turn, so a handover can name
    /// the player whose turn ended. Retail has no phase frame that carries it.
    active: Option<PlayerConnectionId>,
}

struct Admission {
    source: SourceAddressPrefix,
    _global: OwnedSemaphorePermit,
    _source: KeyedCapacityGuard<SourceAddressPrefix>,
}

#[derive(Clone, Debug)]
enum SocialEvent {
    Notice {
        nickname: Vec<u8>,
        message: Vec<u8>,
    },
    Chat {
        targets: Vec<PlayerConnectionId>,
        nickname: Vec<u8>,
        message: Vec<u8>,
    },
    OfflineNote {
        target: PlayerConnectionId,
        claim: OfflineNoteClaim,
        nickname: Vec<u8>,
        message: Vec<u8>,
    },
    Whisper {
        target: PlayerConnectionId,
        status: u8,
        nickname: Vec<u8>,
        message: Vec<u8>,
    },
    Typing {
        targets: Vec<PlayerConnectionId>,
        connection_id: u32,
        typing: bool,
    },
    Lounge {
        targets: Vec<PlayerConnectionId>,
        connection_id: u32,
        action: Vec<u8>,
    },
    UserInfo {
        target: PlayerConnectionId,
        user_id: u32,
        request_type: u8,
        nickname: Vec<u8>,
        card: MemberCard,
    },
}

#[derive(Clone)]
struct SocialMember {
    account_id: AccountId,
    /// The OID assigned by the authoritative live-session registry, not a cast from a GM packet.
    oid: u32,
    nickname: Vec<u8>,
    card: MemberCard,
    room: Option<RoomId>,
    /// Retail sub-server presence; absent until the initial channel selection succeeds.
    channel: Option<u8>,
    whisper_accept: bool,
    /// Cancels the owning connection task for a real GM disconnect.
    cancellation: CancellationToken,
}

#[derive(Clone)]
struct SocialHub {
    members: Arc<Mutex<BTreeMap<PlayerConnectionId, SocialMember>>>,
    /// Process-authoritative OID to durable account mapping. Entries outlive a live connection so
    /// a catalog-valid GM grant can still address an account immediately after logout.
    oid_accounts: Arc<Mutex<BTreeMap<u32, AccountId>>>,
    oid_capacity: usize,
    events: broadcast::Sender<SocialEvent>,
}

impl SocialHub {
    fn new(capacity: usize) -> Self {
        let (events, _) = broadcast::channel(capacity.max(1));
        Self {
            members: Arc::new(Mutex::new(BTreeMap::new())),
            oid_accounts: Arc::new(Mutex::new(BTreeMap::new())),
            oid_capacity: capacity.max(1),
            events,
        }
    }
    fn subscribe(&self) -> broadcast::Receiver<SocialEvent> {
        self.events.subscribe()
    }
    #[cfg(test)]
    fn register(
        &self,
        id: PlayerConnectionId,
        account_id: AccountId,
        nickname: Vec<u8>,
        card: MemberCard,
    ) {
        let oid = u32::try_from(id.get()).unwrap_or(u32::MAX);
        self.register_with_oid(id, oid, account_id, nickname, card);
    }
    #[cfg(test)]
    fn register_with_oid(
        &self,
        id: PlayerConnectionId,
        oid: u32,
        account_id: AccountId,
        nickname: Vec<u8>,
        card: MemberCard,
    ) {
        self.register_with_oid_and_cancellation(
            id,
            oid,
            account_id,
            nickname,
            card,
            CancellationToken::new(),
        );
    }
    fn register_with_oid_and_cancellation(
        &self,
        id: PlayerConnectionId,
        oid: u32,
        account_id: AccountId,
        nickname: Vec<u8>,
        card: MemberCard,
        cancellation: CancellationToken,
    ) {
        if let Ok(mut accounts) = self.oid_accounts.lock() {
            accounts.insert(oid, account_id);
            while accounts.len() > self.oid_capacity {
                let Some(oldest) = accounts.keys().next().copied() else {
                    break;
                };
                accounts.remove(&oldest);
            }
        }
        if let Ok(mut members) = self.members.lock() {
            members.insert(
                id,
                SocialMember {
                    account_id,
                    oid,
                    nickname,
                    card,
                    room: None,
                    channel: None,
                    whisper_accept: true,
                    cancellation,
                },
            );
        }
    }
    fn remove(&self, id: PlayerConnectionId) {
        if let Ok(mut members) = self.members.lock() {
            members.remove(&id);
        }
    }
    fn set_channel(&self, id: PlayerConnectionId, channel: Option<u8>) {
        if let Ok(mut members) = self.members.lock()
            && let Some(member) = members.get_mut(&id)
        {
            member.channel = channel;
        }
    }
    fn set_room(&self, id: PlayerConnectionId, room: Option<RoomId>) {
        if let Ok(mut members) = self.members.lock()
            && let Some(member) = members.get_mut(&id)
        {
            member.room = room;
        }
    }
    fn set_whisper_accept(&self, id: PlayerConnectionId, accept: bool) {
        if let Ok(mut members) = self.members.lock()
            && let Some(member) = members.get_mut(&id)
        {
            member.whisper_accept = accept;
        }
    }
    fn contains_connection(&self, id: u32) -> bool {
        let Ok(members) = self.members.lock() else {
            return false;
        };
        members
            .keys()
            .any(|member| u32::try_from(member.get()).ok() == Some(id))
    }
    fn account_for_connection(&self, id: PlayerConnectionId) -> Option<AccountId> {
        self.members
            .lock()
            .ok()?
            .get(&id)
            .map(|member| member.account_id)
    }
    fn connection_for_oid(&self, oid: u32) -> Option<PlayerConnectionId> {
        self.members
            .lock()
            .ok()?
            .iter()
            .find(|(_, member)| member.oid == oid)
            .map(|(connection, _)| *connection)
    }
    fn account_for_oid(&self, oid: u32) -> Option<AccountId> {
        self.oid_accounts.lock().ok()?.get(&oid).copied()
    }
    fn cancel_connection_for_oid(&self, oid: u32) -> bool {
        let Ok(members) = self.members.lock() else {
            return false;
        };
        let Some(member) = members.values().find(|member| member.oid == oid) else {
            return false;
        };
        member.cancellation.cancel();
        true
    }
    fn contains_account(&self, id: u32) -> bool {
        let Ok(members) = self.members.lock() else {
            return false;
        };
        members
            .values()
            .any(|member| u32::try_from(member.account_id.get()).ok() == Some(id))
    }
    fn deliver_offline_note(
        &self,
        recipient: AccountId,
        claim: OfflineNoteClaim,
        sender_nickname: Vec<u8>,
        message: Vec<u8>,
    ) {
        let Ok(members) = self.members.lock() else {
            return;
        };
        let Some(target) = members
            .iter()
            .find(|(_, member)| member.account_id == recipient)
        else {
            return;
        };
        let _ = self.events.send(SocialEvent::OfflineNote {
            target: *target.0,
            claim,
            nickname: sender_nickname,
            message,
        });
    }
    fn update_card(&self, id: PlayerConnectionId, card: MemberCard) {
        if let Ok(mut members) = self.members.lock()
            && let Some(member) = members.get_mut(&id)
        {
            member.card = card;
        }
    }
    fn scoped_targets(&self, id: PlayerConnectionId) -> Vec<PlayerConnectionId> {
        let Ok(members) = self.members.lock() else {
            return Vec::new();
        };
        let Some(requester) = members.get(&id) else {
            return Vec::new();
        };
        let room = requester.room;
        let channel = requester.channel;
        members
            .iter()
            .filter_map(|(target, member)| {
                (member.channel == channel && member.room == room).then_some(*target)
            })
            .collect()
    }
    fn notice(&self, nickname: Vec<u8>, message: Vec<u8>) {
        let _ = self.events.send(SocialEvent::Notice { nickname, message });
    }

    fn chat(&self, id: PlayerConnectionId, nickname: Vec<u8>, message: Vec<u8>) {
        let targets = self.scoped_targets(id);
        let _ = self.events.send(SocialEvent::Chat {
            targets,
            nickname,
            message,
        });
    }
    fn typing(&self, id: PlayerConnectionId, typing: bool) {
        let targets = self.scoped_targets(id);
        let _ = self.events.send(SocialEvent::Typing {
            targets,
            connection_id: u32::try_from(id.get()).unwrap_or(u32::MAX),
            typing,
        });
    }
    fn lounge(&self, id: PlayerConnectionId, action: Vec<u8>) {
        let targets = self.scoped_targets(id);
        let _ = self.events.send(SocialEvent::Lounge {
            targets,
            connection_id: u32::try_from(id.get()).unwrap_or(u32::MAX),
            action,
        });
    }
    fn whisper(&self, id: PlayerConnectionId, target_name: &[u8], message: Vec<u8>) {
        let Ok(members) = self.members.lock() else {
            return;
        };
        let sender = members.get(&id);
        let Some(sender) = sender else { return };
        let target = members
            .iter()
            .find(|(_, member)| member.nickname.as_slice() == target_name)
            .map(|(id, member)| (*id, member));
        let Some((target_id, target)) = target else {
            let _ = self.events.send(SocialEvent::Whisper {
                target: id,
                status: 5,
                nickname: target_name.to_vec(),
                message: Vec::new(),
            });
            return;
        };
        if !target.whisper_accept {
            let _ = self.events.send(SocialEvent::Whisper {
                target: id,
                status: 4,
                nickname: target_name.to_vec(),
                message: Vec::new(),
            });
            return;
        }
        let _ = self.events.send(SocialEvent::Whisper {
            target: target_id,
            status: 0,
            nickname: sender.nickname.clone(),
            message: message.clone(),
        });
        let _ = self.events.send(SocialEvent::Whisper {
            target: id,
            status: 1,
            nickname: target.nickname.clone(),
            message,
        });
    }
    fn user_info(
        &self,
        requester: PlayerConnectionId,
        target_id: u32,
        request_type: u8,
        card: MemberCard,
    ) {
        let Ok(members) = self.members.lock() else {
            return;
        };
        let target = members
            .values()
            .find(|m| u32::try_from(m.account_id.get()).ok() == Some(target_id));
        let (nickname, card) = target.map_or((Vec::new(), card), |member| {
            (member.nickname.clone(), member.card.clone())
        });
        let _ = self.events.send(SocialEvent::UserInfo {
            target: requester,
            user_id: target_id,
            request_type,
            nickname,
            card,
        });
    }
}

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

/// Clock used for wall-clock server-day and packet timestamps.
///
/// Production uses [`SystemGameClock`]. Tests and deterministic integrations can inject a fixed
/// implementation at the service boundary without changing the persisted server-day contract.
pub trait GameClock: Send + Sync {
    /// Returns the current wall-clock instant.
    fn now(&self) -> SystemTime;
}

/// Production wall clock for [`GameService`].
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemGameClock;

impl GameClock for SystemGameClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Generic bounded GameService over domain repositories and an immutable catalog.
pub struct GameService<R>
where
    R: HandoverRepository + PlayerRepository + MatchRepository + EconomyRepository + 'static,
{
    repository: Arc<R>,
    catalog: Catalog,
    /// Operator overrides layered on the immutable catalog at purchase time.
    ///
    /// A `watch` receiver rather than a repository query: a purchase is on the player's
    /// critical path and the overlay changes at operator speed, so the admin surface
    /// republishes a whole snapshot and every connection reads it without touching the
    /// database. Defaults to empty, which resolves to exactly the catalog's own answers.
    shop_overlay: watch::Receiver<ShopOverlay>,
    config: GameRuntimeConfig,
    clock: Arc<dyn GameClock>,
    message_server: pangya_protocol::MessageServerEntry,
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
    social: SocialHub,
    /// Authenticated channel connections eligible for direct room invites.
    invite_targets: Arc<Mutex<HashMap<AccountId, RoomOutbound>>>,
    pending_invites: Arc<Mutex<HashMap<AccountId, RoomId>>>,
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
    /// Subscribes this service to a live operator shop overlay.
    ///
    /// Kept out of [`Self::new`] so the dozens of existing composition sites and tests keep
    /// working with the empty default, which resolves to exactly the catalog's own answers.
    #[must_use]
    pub fn with_shop_overlay(mut self, overlay: watch::Receiver<ShopOverlay>) -> Self {
        self.shop_overlay = overlay;
        self
    }
    /// Injects the wall clock used by login-bonus server-day calculations and timestamps.
    #[must_use]
    pub fn with_clock<C>(mut self, clock: C) -> Self
    where
        C: GameClock + 'static,
    {
        self.clock = Arc::new(clock);
        self
    }

    /// Sets the configured MessageService endpoint used by 0x008b responses.
    #[must_use]
    pub fn with_message_server(
        mut self,
        message_server: pangya_protocol::MessageServerEntry,
    ) -> Self {
        self.message_server = message_server;
        self
    }

    /// Resolves what the server will actually sell one item for, right now.
    ///
    /// Starts from **any** catalog record rather than only the already-sold ones, because an
    /// overlay's whole purpose is to be able to offer something the client's own tables mark
    /// as unavailable. It still cannot reach a type the catalog has never heard of, so the
    /// client's data remains the outer bound on what exists.
    fn resolve_offer(&self, type_id: ItemTypeId) -> Option<ItemDefinition> {
        let definition = self.catalog.item_definition(type_id).copied()?;
        self.shop_overlay.borrow().resolve(definition)
    }

    /// Returns every item the server currently sells, in deterministic type-ID order.
    fn resolved_offers(&self) -> Vec<ItemDefinition> {
        let overlay = self.shop_overlay.borrow();
        if overlay.is_empty() {
            // The overwhelmingly common case: no overrides, so the catalog's precomputed
            // sorted slice is already the answer and there is nothing to rebuild.
            return self.catalog.shop_offers().to_vec();
        }
        let mut offers = self
            .catalog
            .records()
            .filter_map(|(_, record)| record.definition().copied())
            .filter_map(|definition| overlay.resolve(definition))
            .collect::<Vec<_>>();
        offers.sort_by_key(|definition| definition.type_id);
        offers
    }

    /// Creates a GameService after validating every direct and actor bound.
    pub fn new(
        repository: Arc<R>,
        catalog: Catalog,
        config: GameRuntimeConfig,
        observer: Arc<dyn GameObserver>,
    ) -> Result<Self, GameRuntimeError> {
        let limits = &config.limits;
        let invalid = config.channel_id == 0
            || config.advertised_channel_ids.is_empty()
            || config.advertised_channel_ids.len() > 255
            || !config
                .advertised_channel_ids
                .contains(&u8::try_from(config.channel_id).unwrap_or(0))
            || config
                .advertised_channel_ids
                .iter()
                .enumerate()
                .any(|(index, id)| config.advertised_channel_ids[index + 1..].contains(id))
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
            || config
                .login_bonus
                .is_some_and(|bonus| bonus.calendar_days == 0 || bonus.reward.quantity == 0)
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
        if let Some(login_bonus) = config.login_bonus
            && login_bonus.validate(&catalog).is_err()
        {
            return Err(GameRuntimeError::Catalog);
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
                .declared_course_plan(
                    course.course_id(),
                    course.hole_count(),
                    course.hole_mode(),
                    course.par(),
                )
                .map_err(|_| GameRuntimeError::Catalog)?;
            if catalog_course != course || catalog.fingerprint() != fingerprint {
                return Err(GameRuntimeError::Catalog);
            }
            if let Ok(derived) =
                catalog.course_plan(course.course_id(), course.hole_count(), course.hole_mode())
                && derived != course
            {
                return Err(GameRuntimeError::Catalog);
            }
        }
        let lobby = spawn_lobby(limits.lobby);
        Ok(Self {
            repository,
            catalog,
            clock: Arc::new(SystemGameClock),
            // Empty by default; composition swaps in a live receiver when the operator
            // admin surface is enabled.
            shop_overlay: watch::channel(ShopOverlay::default()).1,
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
            social: SocialHub::new(limits.outbound_room_event_capacity),
            invite_targets: Arc::new(Mutex::new(HashMap::new())),
            pending_invites: Arc::new(Mutex::new(HashMap::new())),
            captures: CaptureSink::new(limits.unknown_capture_capacity),
            config,
            message_server: pangya_protocol::MessageServerEntry {
                name: b"PangYa-RS Message".to_vec(),
                id: 1,
                max_users: 200,
                num_users: 0,
                ip_address: b"127.0.0.1".to_vec(),
                port: 30303,
                unknown2: pangya_protocol::UnknownBytes([0; 2]),
                flags: pangya_protocol::UnknownBytes([0; 2]),
                unknown3: pangya_protocol::UnknownBytes([0; 14]),
                char_icon: 0,
            },
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
        // Distinct from the authenticated account lease: this is the current retail sub-server
        // and is changed only after a valid transition has completed its room cleanup.
        let mut current_channel: Option<u8> = None;
        // Target account selected by the last 0x00b5 My Room open. It is connection-local and
        // never trusted as authorization for mutable state; every projection is loaded from DB.
        let mut my_room_target: Option<AccountId> = None;
        let mut local = LocalRateWindow::new(self.config.limits.rate_window);
        let mut commands = LocalRateWindow::new(self.config.limits.rate_window);
        let mut chats = LocalRateWindow::new(self.config.limits.rate_window);
        let mut whispers = LocalRateWindow::new(self.config.limits.rate_window);
        let mut typing_events = LocalRateWindow::new(self.config.limits.rate_window);
        let mut lounge_actions = LocalRateWindow::new(self.config.limits.rate_window);
        let mut lounge_enters = LocalRateWindow::new(self.config.limits.rate_window);
        let mut user_info_requests = LocalRateWindow::new(self.config.limits.rate_window);
        let mut whisper_accept_updates = LocalRateWindow::new(self.config.limits.rate_window);
        let mut macro_updates = LocalRateWindow::new(self.config.limits.rate_window);

        let mut shots = LocalRateWindow::new(self.config.limits.rate_window);
        let mut economy_commands = LocalRateWindow::new(self.config.limits.rate_window);
        // A retail purchase has no application operation ID. Scope generated IDs to this
        // authenticated transport and retain a bounded salt+payload replay window: an exact frame
        // replay reuses its commit, while a later intentional purchase (new salt) is a new command.
        let retail_purchase_scope = uuid::Uuid::new_v4();
        let mut retail_purchase_replays = RetailWireReplayWindow::new();
        // Social state updates are also replay-keyed by the frame salt and digest. This keeps a
        // transport retry from repeating a durable write while the bounded windows still allow
        // intentional later updates.
        let mut social_replays = RetailWireReplayWindow::new();
        // Retail shots are opaque client payloads, so the server counts strokes itself.
        let mut retail_strokes = 0_u32;
        let mut retail_stroke_sequence = RetailStrokeSequence::default();
        let mut unknown_strikes = 0_u32;
        let (outbound, mut room_events) =
            RoomOutbound::ordered(self.config.limits.outbound_room_event_capacity);
        let terminal_outbound = outbound.clone();
        let mut persistence_events = outbound.take_persistence_receiver();
        let room_cancellation = CancellationToken::new();
        let mut room_id: Option<RoomId> = None;
        let mut match_context = ConnectionMatchContext::default();
        let mut terminal_generation = 0_u64;
        let mut terminal_identity: Option<(MatchId, MatchResultKey)> = None;
        let mut social_events = self.social.subscribe();

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
                social_event = social_events.recv(), if !matches!(state, GameState::AwaitHandover | GameState::AwaitChannel | GameState::Closed) => {
                    let event = match social_event {
                        Ok(event) => event,
                        // A broadcast receiver cannot reconstruct missing social packets. Do not
                        // silently continue with a stale roster/chat view: boundedly disconnect
                        // so the client can reconnect and receive a fresh projection.
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::debug!(skipped, "social event receiver lagged; disconnecting for resync");
                            break Err(GameRuntimeError::Limited);
                        }
                        Err(broadcast::error::RecvError::Closed) => break Err(GameRuntimeError::Limited),
                    };
                    if let Err(error) = self.handle_social_event(&mut framed, connection_id, event).await { break Err(error); }
                }
                event = async {
                    tokio::select! {
                        biased;
                        event = async {
                            match persistence_events.as_mut() {
                                Some(receiver) => receiver.recv().await,
                                None => std::future::pending().await,
                            }
                        } => event,
                        event = room_events.recv() => event,
                    }
                }, if matches!(state, GameState::InChannel | GameState::InRoom | GameState::InMatchLoading | GameState::InMatch | GameState::InStrokeLoading | GameState::InStrokeMatch) => {
                    let Some(event) = event else { break Err(GameRuntimeError::Limited); };
                    let handled = tokio::select! {
                        biased;
                        () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                        handled = async {
                            match event {
                                RoomEvent::StrokeSettlementRequested(commit) => {
                                    terminal_outbound.begin_persistence_delivery();
                                    self.persist_stroke_commit_by_room(
                                        room_id.ok_or(GameRuntimeError::Protocol)?,
                                        commit,
                                    )
                                    .await
                                    .map(|()| RoomEventEffect::Remain)
                                }
                                RoomEvent::StrokeCommittedWithGeneration { result, generation } => {
                                    terminal_outbound.begin_terminal_delivery(generation);
                                    self.handle_terminal_delivery(
                                        &mut framed,
                                        state,
                                        TerminalDelivery { result, generation },
                                        room_id,
                                        connection_id,
                                        &mut match_context,
                                        &mut terminal_generation,
                                        &mut terminal_identity,
                                        &terminal_outbound,
                                    ).await
                                }
                                event => self.handle_room_event(
                                    &mut framed,
                                    state,
                                    event,
                                    room_id,
                                    connection_id,
                                    &mut match_context,
                                ).await,
                            }
                        } => handled,
                    };
                    match handled {
                        Ok(RoomEventEffect::Remain) => {}
                        Ok(RoomEventEffect::EnterChannel) => {
                            retail_stroke_sequence.clear();
                            state = GameState::InChannel;
                            room_id = None;
                            self.social.set_room(connection_id, None);
                        }
                        Ok(RoomEventEffect::EnterRoom) => {
                            retail_stroke_sequence.clear();
                            state = GameState::InRoom;
                            match_context = ConnectionMatchContext::default();
                        }
                        Ok(RoomEventEffect::EnterLoading) => {
                            retail_stroke_sequence.clear();
                            state = GameState::InMatchLoading;
                        }
                        Ok(RoomEventEffect::EnterMatch) => {
                            retail_stroke_sequence.clear();
                            state = GameState::InMatch;
                        }
                        Ok(RoomEventEffect::EnterStrokeLoading) => {
                            retail_stroke_sequence.clear();
                            state = GameState::InStrokeLoading;
                        }
                        Ok(RoomEventEffect::EnterStrokeMatch) => {
                            retail_stroke_sequence.clear();
                            state = GameState::InStrokeMatch;
                        }
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
                                    let oid = u32::try_from(connection_id.get())
                                        .map_err(|_| GameRuntimeError::Limited)?;
                                    self.social.register_with_oid_and_cancellation(
                                        connection_id,
                                        oid,
                                        established.account_id,
                                        established.nickname.display().as_bytes().to_vec(),
                                        established.card.clone(),
                                        room_cancellation.clone(),
                                    );
                                    // Claim pending durable notes only after authentication. The
                                    // lease is not acknowledged until the outbound write succeeds,
                                    // so disconnects and crashes leave the note retryable.
                                    let pending = self
                                        .repository
                                        .claim_offline_notes(established.account_id)
                                        .await
                                        .map_err(|_| GameRuntimeError::Snapshot)?;
                                    for note in pending {
                                        let _ = self.social.events.send(SocialEvent::OfflineNote {
                                            target: connection_id,
                                            claim: OfflineNoteClaim {
                                                id: note.id,
                                                lease_token: note.lease_token,
                                            },
                                            nickname: note.sender_nickname,
                                            message: note.message,
                                        });
                                    }
                                    presence = Some(guard);
                                    identity = Some(established.clone());
                                    if let Ok(mut targets) = self.invite_targets.lock() {
                                        targets.insert(established.account_id, outbound.clone());
                                    }
                                    state = GameState::AwaitChannel;
                                }
                                Err(error) => break Err(error),
                            }
                        }
                        GameState::AwaitChannel if frame.opcode == RetailClientException::OPCODE => {
                            // Authenticated clients may report an exception before choosing a
                            // channel. The report is fire-and-forget and must not become an
                            // unknown-opcode strike or a response-producing command.
                            observe_retail_client_exception(&frame.payload);
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
                            let Some(channel) = u8::try_from(channel_id).ok() else {
                                break Err(GameRuntimeError::Protocol);
                            };
                            if !self.config.advertised_channel_ids.contains(&channel)
                                || identity.is_none()
                                || presence.is_none()
                                || current_channel.is_some()
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
                            current_channel = Some(channel);
                            self.social.set_channel(connection_id, current_channel);
                            state = GameState::InChannel;
                        }
                        GameState::InChannel | GameState::InRoom | GameState::InMatchLoading | GameState::InMatch | GameState::InStrokeLoading | GameState::InStrokeMatch => {
                            if is_gm_opcode(frame.opcode) {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                if let Err(error) = authorize_gm_request(
                                    established.game_master,
                                    frame.opcode,
                                ) {
                                    tracing::warn!(
                                        account_id = established.account_id.get(),
                                        opcode = frame.opcode,
                                        error = ?error,
                                        "unauthorized GM request refused"
                                    );
                                    break Err(GameRuntimeError::Protocol);
                                }
                                match decode_gm_request(frame.opcode, &frame.payload) {
                                    Ok(GmRequest::Command(command)) => {
                                        if let Err(error) = self
                                            .handle_gm_command(
                                                connection_id,
                                                room_id,
                                                command,
                                            )
                                            .await
                                        {
                                            break Err(error);
                                        }
                                    }
                                    Ok(GmRequest::Notice(message)) => {
                                        if message.is_empty() {
                                            break Err(GameRuntimeError::Protocol);
                                        }
                                        self.social.notice(
                                            established.nickname.display().as_bytes().to_vec(),
                                            message,
                                        );
                                    }
                                    Ok(GmRequest::Identity { .. } | GmRequest::Refused { .. }) => {
                                        tracing::debug!(opcode = frame.opcode, "GM request refused: unresolved layout");
                                    }
                                    Err(error) => {
                                        tracing::warn!(opcode = frame.opcode, error = ?error, "malformed GM request refused");
                                        break Err(GameRuntimeError::Protocol);
                                    }
                                }
                            } else if matches!(frame.opcode, GameAuth::OPCODE | SelectChannel::OPCODE) {
                                break Err(GameRuntimeError::Protocol);
                            } else if frame.opcode == RetailClientException::OPCODE {
                                // This report is fire-and-forget. Decode failures are ignored
                                // after the bounded frame has been consumed: a client diagnostic
                                // must never turn into a second disconnect or stall the session.
                                observe_retail_client_exception(&frame.payload);
                            } else if self.config.retail_bootstrap && frame.opcode == RetailServerListRequest::OPCODE {
                                if !frame.payload.is_empty() { break Err(GameRuntimeError::Protocol); }
                                let server = RetailServerEntry {
                                    name: b"PangYa-RS".to_vec(), id: self.config.channel_id,
                                    user_max: 200, user_count: 0, ip: Vec::new(), port: 0,
                                    unknown_c: UnknownBytes([0; 2]), flags: UnknownBytes([0; 2]),
                                    unknown_d: UnknownBytes([0; 14]), icon: 1,
                                };
                                let channels = self
                                    .config
                                    .advertised_channel_ids
                                    .iter()
                                    .map(|&id| RetailSubServerEntry {
                                        name: format!("Channel {id}").into_bytes(),
                                        unknown_a: UnknownBytes([0; 47]),
                                        id,
                                        unknown_b: UnknownBytes([0; 8]),
                                    })
                                    .collect();
                                self.send(&mut framed, &RetailServerList { servers: vec![server], sub_servers: channels }).await?;
                            } else if self.config.retail_bootstrap && frame.opcode == RetailServerTimeRequest::OPCODE {
                                if !frame.payload.is_empty() { break Err(GameRuntimeError::Protocol); }
                                let now = chrono::Local::now();
                                self.send(&mut framed, &RetailServerTime {
                                    year: u16::try_from(now.year()).map_err(|_| GameRuntimeError::Protocol)?,
                                    month: u16::try_from(now.month()).map_err(|_| GameRuntimeError::Protocol)?,
                                    weekday: u16::try_from(now.weekday().num_days_from_sunday()).map_err(|_| GameRuntimeError::Protocol)?,
                                    day: u16::try_from(now.day()).map_err(|_| GameRuntimeError::Protocol)?,
                                    hour: u16::try_from(now.hour()).map_err(|_| GameRuntimeError::Protocol)?,
                                    minute: u16::try_from(now.minute()).map_err(|_| GameRuntimeError::Protocol)?,
                                    second: u16::try_from(now.second()).map_err(|_| GameRuntimeError::Protocol)?,
                                    millisecond: u16::try_from(now.nanosecond() / 1_000_000).map_err(|_| GameRuntimeError::Protocol)?,
                                }).await?;
                            } else if self.config.retail_bootstrap && frame.opcode == RetailSubServerConnect::OPCODE {
                                let request = decode_packet_payload::<RetailSubServerConnect>(&frame.payload, &CompatibilityProfile::US_852, ServiceKind::Game).map_err(|_| GameRuntimeError::Protocol)?;
                                let Some(previous) = current_channel else { break Err(GameRuntimeError::Protocol); };
                                // 0x0083 is a real move, not an idempotent re-join. Validate the
                                // destination before touching the room actor or presence map so
                                // malformed/unknown requests cannot strand a player half-moved.
                                if !matches!(state, GameState::InChannel | GameState::InRoom)
                                    || request.sub_server_id == previous
                                    || !self.config.advertised_channel_ids.contains(&request.sub_server_id)
                                {
                                    break Err(GameRuntimeError::Protocol);
                                }
                                if state == GameState::InRoom {
                                    match self.lobby.disconnect(connection_id).await {
                                        Ok(_) | Err(RoomError::NotMember | RoomError::RoomNotFound) => {}
                                        Err(_) => break Err(GameRuntimeError::Protocol),
                                    }
                                    room_id = None;
                                    self.social.set_room(connection_id, None);
                                    // The room actor may have queued its final census before the
                                    // leave reply. It must not leak into a later room entered on
                                    // the destination channel.
                                    while room_events.try_recv().is_ok() {}
                                }
                                // Reference 0x004e/0x01f6 carries no destination ID. Commit the
                                // session/lobby presence only after both response frames are sent.
                                self.send(&mut framed, &RetailChannelJoined).await?;
                                self.send(&mut framed, &RetailChannelJoinNotice).await?;
                                current_channel = Some(request.sub_server_id);
                                self.social.set_channel(connection_id, current_channel);
                                state = GameState::InChannel;
                            } else if self.config.retail_bootstrap && frame.opcode == RetailNewSessionKeyRequest::OPCODE {
                                let request = decode_packet_payload::<RetailNewSessionKeyRequest>(&frame.payload, &CompatibilityProfile::US_852, ServiceKind::Game).map_err(|_| GameRuntimeError::Protocol)?;
                                let Some(established) = identity.as_ref() else { break Err(GameRuntimeError::Protocol); };
                                // A destination that is not this configured service is refused;
                                // issuing a key for an unknown topology would create an unusable
                                // bearer and, worse, widen the handover trust boundary.
                                if request.server_id != self.config.channel_id { continue; }
                                let generated = generate_handover(established.account_id, DomainServiceKind::Game, source.clone(), SystemTime::now()).map_err(|_| GameRuntimeError::Authentication)?;
                                self.repository.issue(generated.record).await.map_err(|_| GameRuntimeError::Authentication)?;
                                self.send(&mut framed, &RetailNewSessionKey { unknown: UnknownBytes([0; 4]), session_key: generated.token.expose_secret().as_bytes().to_vec().into() }).await?;
                            } else if frame.opcode == RetailMessageServerListRequest::OPCODE {
                                if decode_packet_payload::<RetailMessageServerListRequest>(
                                    &frame.payload,
                                    &CompatibilityProfile::US_852,
                                    ServiceKind::Game,
                                ).is_err() {
                                    break Err(GameRuntimeError::Protocol);
                                }
                                let response = RetailMessageServerList {
                                    servers: vec![self.message_server.clone()],
                                };
                                if let Err(error) = self.send(&mut framed, &response).await {
                                    break Err(error);
                                }
                            } else if self.config.retail_bootstrap
                                && frame.opcode == RetailLoginBonusRequest::OPCODE
                            {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                if decode_packet_payload::<RetailLoginBonusRequest>(
                                    &frame.payload,
                                    &CompatibilityProfile::US_852,
                                    ServiceKind::Game,
                                )
                                .is_err()
                                {
                                    break Err(GameRuntimeError::Protocol);
                                }
                                let status = self
                                    .retail_login_bonus_status(established.account_id)
                                    .await?;
                                self.send(&mut framed, &status).await?;
                            } else if self.config.retail_bootstrap
                                && frame.opcode == RetailLoginBonusClaimRequest::OPCODE
                            {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                if decode_packet_payload::<RetailLoginBonusClaimRequest>(
                                    &frame.payload,
                                    &CompatibilityProfile::US_852,
                                    ServiceKind::Game,
                                )
                                .is_err()
                                {
                                    break Err(GameRuntimeError::Protocol);
                                }
                                self.retail_login_bonus_claim(
                                    &mut framed,
                                    established.account_id,
                                )
                                .await?;
                            } else if self.config.retail_bootstrap
                                && frame.opcode == RetailPlayerHistoryRequest::OPCODE
                            {
                                if !frame.payload.is_empty() {
                                    break Err(GameRuntimeError::Protocol);
                                }
                                let sent = tokio::select! {
                                    biased;
                                    () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                                    sent = async {
                                        let Some(established) = identity.as_ref() else { return Err(GameRuntimeError::Protocol); };
                                        let recent = self.repository.load_recent_players(established.account_id).await.map_err(|_| GameRuntimeError::Snapshot)?;
                                        let entries = recent.into_iter().take(RETAIL_RECENT_PLAYERS).filter_map(|player: RecentPlayer| {
                                            let account_id = u32::try_from(player.account_id.get()).ok()?;
                                            let nickname = player.nickname.as_bytes().get(..player.nickname.len().min(21))?.to_vec();
                                            Some(RetailRecentPlayerSlot { account_id, secondary_name: nickname.clone(), nickname, unknown: 0 })
                                        }).collect::<Vec<_>>();
                                        self.send(&mut framed, &RetailPlayerHistoryEntries { entries }).await
                                    } => sent,
                                };
                                if let Err(error) = sent {
                                    break Err(error);
                                }
                            } else if self.config.retail_bootstrap
                                && matches!(frame.opcode, RETAIL_C2S_EQUIPMENT_LOBBY | RETAIL_C2S_EQUIPMENT_ROOM)
                                // In-match 0x000b/0x000c are loading-equipment synchronization,
                                // explicitly accepted without a reply by the retail match
                                // allowlist. Let them reach that handler rather than rejecting a
                                // valid five-byte room body because the actor has entered its
                                // loading state. PacketDoc `gameservice/client/000c.ksy:24-80`
                                // and the real-client sequence in
                                // `docs/evidence/REAL_CLIENT_PRACTICE_2026-08-09.md:32-42`.
                                && !matches!(
                                    state,
                                    GameState::InMatchLoading
                                        | GameState::InMatch
                                        | GameState::InStrokeLoading
                                        | GameState::InStrokeMatch
                                )
                                // The existing retail practice start also uses 0x000c with the
                                // reference-defined type-7 four-word body; preserve that full
                                // lifecycle path rather than stealing its start frame.
                                && !(frame.opcode == RETAIL_C2S_EQUIPMENT_ROOM
                                    && frame.payload.first() == Some(&7)
                                    && frame.payload.len() == 17)
                            {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                if (frame.opcode == RETAIL_C2S_EQUIPMENT_LOBBY && state != GameState::InChannel)
                                    || (frame.opcode == RETAIL_C2S_EQUIPMENT_ROOM && state != GameState::InRoom)
                                {
                                    break Err(GameRuntimeError::Protocol);
                                }
                                let changed = match self
                                    .handle_retail_room_equipment_update(
                                        &mut framed,
                                        state,
                                        established,
                                        frame.opcode,
                                        &frame.payload,
                                    )
                                    .await
                                {
                                    Ok(changed) => changed,
                                    Err(error) => break Err(error),
                                };
                                if changed {
                                    let fresh = self
                                        .refresh_social_projection(
                                            connection_id,
                                            established.account_id,
                                        )
                                        .await
                                        .map_err(|_| GameRuntimeError::Snapshot)?;
                                    if let Some(established) = identity.as_mut() {
                                        established.character_id =
                                            Some(fresh.equipment.character_id);
                                        established.character_iff_id = fresh
                                            .characters
                                            .iter()
                                            .find(|value| value.id == fresh.equipment.character_id)
                                            .map(|value| value.item_type_id.get());
                                        established.card = member_card(&fresh);
                                    }
                                }
                            } else if self.config.retail_bootstrap
                                && frame.opcode == RetailEquipmentUpdate::OPCODE
                            {
                                let Some(established) = identity.as_mut() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                match self
                                    .handle_retail_equipment_update(
                                        &mut framed,
                                        established.account_id,
                                        &frame.payload,
                                    )
                                    .await
                                {
                                    Ok(Some(_card)) => {
                                        // Re-read all durable fields after the mutation. The
                                        // acknowledgement card is intentionally only a wire
                                        // projection; room census and match bootstrap need the
                                        // complete coherent snapshot.
                                        let account_id = established.account_id;
                                        let fresh = self
                                            .refresh_social_projection(connection_id, account_id)
                                            .await?;
                                        established.character_id = Some(fresh.equipment.character_id);
                                        established.character_iff_id = fresh
                                            .characters
                                            .iter()
                                            .find(|value| value.id == fresh.equipment.character_id)
                                            .map(|value| value.item_type_id.get());
                                        established.card = member_card(&fresh);
                                    }
                                    Ok(None) => {}
                                    Err(GameRuntimeError::EconomyPersistence) => {
                                        if let Some(slot) = frame
                                            .payload
                                            .first()
                                            .and_then(|tag| RetailEquipmentSlot::from_tag(*tag))
                                        {
                                            self.send(
                                                &mut framed,
                                                &RetailEquipmentUpdated::Rejected { slot },
                                            )
                                            .await?;
                                        } else {
                                            break Err(GameRuntimeError::Protocol);
                                        }
                                    }
                                    Err(error) => break Err(error),
                                }
                            } else if self.config.retail_bootstrap && frame.opcode == <GameChat as DecodePacket>::OPCODE {
                                let Some(established) = identity.as_ref() else { break Err(GameRuntimeError::Protocol); };
                                let chat = decode_packet_payload::<GameChat>(&frame.payload, &CompatibilityProfile::US_852, ServiceKind::Game).map_err(|_| GameRuntimeError::Protocol)?;
                                if chat.nickname != established.nickname.display().as_bytes() { break Err(GameRuntimeError::Protocol); }
                                if !chats.admit_count(self.config.limits.chat_messages_per_window) { break Err(GameRuntimeError::Limited); }
                                self.social.chat(connection_id, chat.nickname, chat.message);
                            } else if self.config.retail_bootstrap && frame.opcode == <Whisper as DecodePacket>::OPCODE {
                                let whisper = decode_packet_payload::<Whisper>(&frame.payload, &CompatibilityProfile::US_852, ServiceKind::Game).map_err(|_| GameRuntimeError::Protocol)?;
                                if !whispers.admit_count(self.config.limits.chat_messages_per_window) { break Err(GameRuntimeError::Limited); }
                                self.social.whisper(connection_id, &whisper.nickname, whisper.message);
                            } else if self.config.retail_bootstrap && frame.opcode == <TypingIndicator as DecodePacket>::OPCODE {
                                let typing = decode_packet_payload::<TypingIndicator>(&frame.payload, &CompatibilityProfile::US_852, ServiceKind::Game).map_err(|_| GameRuntimeError::Protocol)?;
                                if !typing_events.admit_count(self.config.limits.chat_messages_per_window) { break Err(GameRuntimeError::Limited); }
                                self.social.typing(connection_id, typing.typing);
                            } else if self.config.retail_bootstrap && frame.opcode == <MacroUpdate as DecodePacket>::OPCODE {
                                let macros = decode_packet_payload::<MacroUpdate>(&frame.payload, &CompatibilityProfile::US_852, ServiceKind::Game).map_err(|_| GameRuntimeError::Protocol)?;
                                if !macro_updates.admit_count(self.config.limits.chat_messages_per_window) { break Err(GameRuntimeError::Limited); }
                                let Some(established) = identity.as_ref() else { break Err(GameRuntimeError::Protocol); };
                                let digest: [u8; 32] = Sha256::digest(&frame.payload).into();
                                let replay = social_replays.is_replay(frame.metadata.salt, &digest);
                                let _sequence = social_replays.sequence(frame.metadata.salt, digest);
                                if !replay {
                                    self.repository.save_chat_macros(established.account_id, macros.values).await.map_err(|_| GameRuntimeError::Snapshot)?;
                                }
                            } else if self.config.retail_bootstrap && frame.opcode == <UserInfoRequest as DecodePacket>::OPCODE {
                                let request = decode_packet_payload::<UserInfoRequest>(&frame.payload, &CompatibilityProfile::US_852, ServiceKind::Game).map_err(|_| GameRuntimeError::Protocol)?;
                                if !user_info_requests.admit_count(self.config.limits.chat_messages_per_window) { break Err(GameRuntimeError::Limited); }
                                if let Some(target) = self.social.members.lock().ok().and_then(|members| members.iter().find(|(_, member)| u32::try_from(member.account_id.get()).ok() == Some(request.user_id)).map(|(_, member)| member.clone())) {
                                    self.social.user_info(connection_id, request.user_id, request.request_type, MemberCard { username: String::new(), character_iff_id: 0, character_uid: 0, caddie_uid: 0, club_set_uid: 0, club_set_iff_id: 0, comet_iff_id: 0, experience: 0, pang: 0 });
                                    let _ = target;
                                } else { let _ = self.social.events.send(SocialEvent::UserInfo { target: connection_id, user_id: request.user_id, request_type: request.request_type, nickname: Vec::new(), card: MemberCard::default() }); }
                            } else if self.config.retail_bootstrap && frame.opcode == 0x0055 {
                                if frame.payload.len() != 1 || frame.payload[0] > 1 { break Err(GameRuntimeError::Protocol); }
                                if !whisper_accept_updates.admit_count(self.config.limits.chat_messages_per_window) { break Err(GameRuntimeError::Limited); }
                                self.social.set_whisper_accept(connection_id, frame.payload[0] == 1);
                            } else if self.config.retail_bootstrap && frame.opcode == <LoungeEnterRequest as DecodePacket>::OPCODE {
                                let request = decode_packet_payload::<LoungeEnterRequest>(&frame.payload, &CompatibilityProfile::US_852, ServiceKind::Game).map_err(|_| GameRuntimeError::Protocol)?;
                                // 0x00EB queries the selected occupant, not necessarily the sender.
                                // The target must be an authenticated member of this process-wide
                                // lounge projection; accepting arbitrary IDs would expose a fake
                                // presence response and self-only validation breaks multi-client UI.
                                if !self.social.contains_connection(request.connection_id) {
                                    break Err(GameRuntimeError::Protocol);
                                }
                                if !lounge_enters.admit_count(self.config.limits.chat_messages_per_window) { break Err(GameRuntimeError::Limited); }
                                self.send(&mut framed, &LoungeEnterResponse { connection_id: request.connection_id }).await?;
                            } else if self.config.retail_bootstrap && frame.opcode == <UserStatusRequest as DecodePacket>::OPCODE {
                                let _request = decode_packet_payload::<UserStatusRequest>(&frame.payload, &CompatibilityProfile::US_852, ServiceKind::Game).map_err(|_| GameRuntimeError::Protocol)?;
                                self.send(&mut framed, &UserStatusResponse).await?;
                            } else if self.config.retail_bootstrap && frame.opcode == <NoteSend as DecodePacket>::OPCODE {
                                let request = decode_packet_payload::<NoteSend>(&frame.payload, &CompatibilityProfile::US_852, ServiceKind::Game).map_err(|_| GameRuntimeError::Protocol)?;
                                if request.subtype != 0x0111 {
                                    break Err(GameRuntimeError::Protocol);
                                }
                                let Some(established) = identity.as_ref() else { break Err(GameRuntimeError::Protocol); };
                                if !chats.admit_count(self.config.limits.chat_messages_per_window) { break Err(GameRuntimeError::Limited); }
                                let recipient_id = AccountId::new(i64::from(request.user_id)).map_err(|_| GameRuntimeError::Protocol)?;
                                let digest: [u8; 32] = Sha256::digest(&frame.payload).into();
                                if social_replays.is_replay(frame.metadata.salt, &digest) {
                                    // A transport retry must still receive the money update, but
                                    // must never debit the account a second time.
                                    let (pang, _) = self.retail_balances(established.account_id).await?;
                                    self.send(&mut framed, &RetailPangBalance { pang }).await?;
                                    continue;
                                }
                                match self.repository.accept_offline_note(OfflineNoteRequest {
                                    sender_id: established.account_id,
                                    recipient_id,
                                    operation_id: digest,
                                    message: request.message.clone(),
                                }).await {
                                    Ok(commit) => {
                                        let _ = social_replays.sequence(frame.metadata.salt, digest);
                                        if self.social.contains_account(request.user_id) {
                                            for note in self
                                                .repository
                                                .claim_offline_notes(recipient_id)
                                                .await
                                                .map_err(|_| GameRuntimeError::Snapshot)?
                                            {
                                                self.social.deliver_offline_note(
                                                    recipient_id,
                                                    OfflineNoteClaim {
                                                        id: note.id,
                                                        lease_token: note.lease_token,
                                                    },
                                                    note.sender_nickname,
                                                    note.message,
                                                );
                                            }
                                        }
                                        self.refresh_social_projection(connection_id, established.account_id).await?;
                                        self.send(&mut framed, &RetailPangBalance { pang: commit.pang }).await?;
                                    }
                                    Err(RepositoryError::BalanceInsufficient | RepositoryError::NotFound | RepositoryError::AccountInactive) => {
                                        // Rejection is side-effect free. Keep the transport usable
                                        // and report the unchanged authoritative balance.
                                        let (pang, _) = self.retail_balances(established.account_id).await?;
                                        self.send(&mut framed, &RetailPangBalance { pang }).await?;
                                    }
                                    Err(_) => break Err(GameRuntimeError::Snapshot),
                                }
                            } else if self.config.retail_bootstrap && frame.opcode == <MessageServerListRequest as DecodePacket>::OPCODE {
                                if !frame.payload.is_empty() { break Err(GameRuntimeError::Protocol); }
                                self.send(&mut framed, &MessageServerList).await?;
                            } else if self.config.retail_bootstrap && frame.opcode == <LoungeAction as DecodePacket>::OPCODE {
                                let action = decode_packet_payload::<LoungeAction>(&frame.payload, &CompatibilityProfile::US_852, ServiceKind::Game).map_err(|_| GameRuntimeError::Protocol)?;
                                if !lounge_actions.admit_count(self.config.limits.chat_messages_per_window) { break Err(GameRuntimeError::Limited); }
                                let mut payload = Vec::with_capacity(action.action_payload.len() + 1);
                                payload.push(action.action_type); payload.extend_from_slice(&action.action_payload);
                                self.social.lounge(connection_id, payload);
                            } else if self.config.retail_bootstrap
                                && frame.opcode == RetailPurchaseRequest::OPCODE
                            {
                                let Some(established) = identity.as_ref() else {
                                    break Err(GameRuntimeError::Protocol);
                                };
                                let account_id = established.account_id;
                                let payload_digest: [u8; 32] = Sha256::digest(&frame.payload).into();
                                let purchase_sequence = retail_purchase_replays
                                    .sequence(frame.metadata.salt, payload_digest);
                                if let Err(error) = self
                                    .handle_retail_purchase(
                                        &mut framed,
                                        account_id,
                                        &frame.payload,
                                        retail_purchase_scope,
                                        purchase_sequence,
                                    )
                                    .await
                                {
                                    break Err(error);
                                }
                                let fresh = self
                                    .refresh_social_projection(connection_id, account_id)
                                    .await
                                    .map_err(|_| GameRuntimeError::Snapshot)?;
                                if let Some(established) = identity.as_mut() {
                                    established.character_id = Some(fresh.equipment.character_id);
                                    established.character_iff_id = fresh
                                        .characters
                                        .iter()
                                        .find(|value| value.id == fresh.equipment.character_id)
                                        .map(|value| value.item_type_id.get());
                                    established.card = member_card(&fresh);
                                }
                            } else if self.config.retail_bootstrap
                                && matches!(
                                    frame.opcode,
                                    RetailShopJoin::OPCODE
                                        | RetailDailyQuestRequest::OPCODE
                                        | RetailMyRoomEnter::OPCODE
                                        | RetailMyRoomInventoryRequest::OPCODE
                                        | <RetailMascotMessageUpdate as DecodePacket>::OPCODE
                                        | RetailLockerInventoryRequest::OPCODE
                                        | 0x00b9
                                        | 0x00c9
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
                                        &mut my_room_target,
                                        frame.opcode,
                                        &frame.payload,
                                    )
                                    .await;
                                if let Err(error) = handled {
                                    break Err(error);
                                }
                            } else if self.config.retail_bootstrap && frame.opcode == RetailClientException::OPCODE {
                                let report = decode_packet_payload::<RetailClientException>(
                                    &frame.payload,
                                    &CompatibilityProfile::US_852,
                                    ServiceKind::Game,
                                ).map_err(|_| GameRuntimeError::Protocol)?;
                                tracing::warn!(message = %report.sanitized(), "client reported an exception");
                            } else if self.config.retail_bootstrap && frame.opcode == 0x004f {
                                // `pangbox/server` Client004F has no fields: it is the client's
                                // local chat-gag notification. Validate the exact empty body
                                // rather than treating arbitrary bytes as harmless chatter.
                                if !frame.payload.is_empty() {
                                    break Err(GameRuntimeError::Protocol);
                                }
                                self.observer.unknown(GameUnknownObservation::Ignored);
                            } else if self.config.retail_bootstrap
                                && is_retail_explicit_social_refusal(frame.opcode)
                            {
                                // The reference corpus does not establish a response layout for
                                // these AFK/report/team/ticker states. Keep the client alive, but
                                // record an explicit refusal rather than silently treating a
                                // stateful request as implemented.
                                tracing::debug!(opcode = frame.opcode, "retail social request refused");
                                self.observer.unknown(GameUnknownObservation::Ignored);
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
                                self.refresh_social_projection(connection_id, established.account_id)
                                    .await
                                    .map_err(|_| GameRuntimeError::Snapshot)?;
                            } else if self.config.retail_bootstrap
                                && state == GameState::InRoom
                                && frame.opcode == RETAIL_C2S_ROOM_RESYNC
                            {
                                // 0x001c is also the in-match shot-end opcode. Room resync
                                // must win while the connection is still in the room; routing
                                // it through the match allowlist would reject a perfectly valid
                                // roster refresh before the room handler can answer it.
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
                                        terminal_outbound.clone(),
                                        room_cancellation.clone(),
                                        current_channel.ok_or(GameRuntimeError::Protocol)?,
                                        frame.opcode,
                                        &frame.payload,
                                        &mut room_id,
                                    ) => handled,
                                };
                                match handled {
                                    Ok(next) => state = next,
                                    Err(error) => break Err(error),
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
                                        &mut retail_stroke_sequence,
                                        &mut match_context,
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
                                        terminal_outbound.clone(),
                                        room_cancellation.clone(),
                                        current_channel.ok_or(GameRuntimeError::Protocol)?,
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
                                        terminal_outbound.clone(),
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
        // Release any room terminal-delivery wait before asking the lobby actor to remove this
        // connection. A full outbound queue is ordinary backpressure, not a failed member; the
        // room's retained terminal result will retry for surviving roster members during apply or
        // failover.
        room_cancellation.cancel();
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
        self.social.remove(connection_id);
        if let Some(identity) = identity.as_ref()
            && let Ok(mut targets) = self.invite_targets.lock()
        {
            targets.remove(&identity.account_id);
        }
        drop(presence);
        let _terminal_state = state;
        match cleanup_result {
            Err(error) => Err(error),
            Ok(()) => result,
        }
    }

    async fn handle_gm_command(
        &self,
        connection_id: PlayerConnectionId,
        room_id: Option<RoomId>,
        command: GmSubcommand,
    ) -> Result<(), GameRuntimeError> {
        match command {
            GmSubcommand::Kick { oid, .. } => {
                let target = self
                    .social
                    .connection_for_oid(oid)
                    .ok_or(GameRuntimeError::Protocol)?;
                self.lobby
                    .kick(target)
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
            }
            GmSubcommand::Disconnect { oid } => {
                if !self.social.cancel_connection_for_oid(oid) {
                    return Err(GameRuntimeError::Protocol);
                }
            }
            GmSubcommand::Destroy { room } => {
                let room = RoomId::new(u32::from(room)).map_err(|_| GameRuntimeError::Protocol)?;
                self.lobby
                    .destroy_room(room)
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
            }
            GmSubcommand::Wind { speed, direction } => {
                if room_id.is_none() {
                    return Err(GameRuntimeError::Protocol);
                };
                self.lobby
                    .route(
                        connection_id,
                        LobbyRoomCommand::Atmosphere {
                            weather: None,
                            wind: Some((speed, direction)),
                        },
                    )
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
            }
            GmSubcommand::Weather { weather } => {
                if room_id.is_none() {
                    return Err(GameRuntimeError::Protocol);
                };
                self.lobby
                    .route(
                        connection_id,
                        LobbyRoomCommand::Atmosphere {
                            weather: Some(weather),
                            wind: None,
                        },
                    )
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
            }
            GmSubcommand::GiveItem {
                oid,
                item_type_id,
                quantity,
            } => {
                if quantity == 0 || quantity > 20_000 {
                    return Err(GameRuntimeError::Protocol);
                }
                let type_id = ItemTypeId::new(item_type_id);
                let Some(definition) = self.catalog.item_definition(type_id).copied() else {
                    tracing::warn!(
                        item_type_id,
                        "GM give-item refused: item absent from catalog"
                    );
                    return Ok(());
                };
                let class = match definition.kind {
                    ItemKind::ClubSet => InventoryClass::ClubSet,
                    ItemKind::Ball => InventoryClass::Ball,
                    ItemKind::Consumable => InventoryClass::Consumable,
                    ItemKind::CharacterPart => InventoryClass::CharacterPart,
                    ItemKind::Caddie => InventoryClass::Caddie,
                    ItemKind::CaddieItem => InventoryClass::CaddieItem,
                    ItemKind::Mascot => InventoryClass::Mascot,
                    ItemKind::Card => InventoryClass::Card,
                    ItemKind::Furniture => InventoryClass::Furniture,
                    ItemKind::Skin => InventoryClass::Skin,
                    ItemKind::HairStyle => InventoryClass::HairStyle,
                    ItemKind::SetItem => InventoryClass::SetItem,
                    ItemKind::Character => {
                        tracing::warn!(
                            item_type_id,
                            "GM give-item refused: character is not inventory"
                        );
                        return Ok(());
                    }
                };
                let target = self
                    .social
                    .account_for_oid(oid)
                    .ok_or(GameRuntimeError::Protocol)?;
                self.repository
                    .gm_grant_item(AdminItemGrant {
                        account_id: target,
                        item_type_id: type_id,
                        class,
                        quantity,
                        durability: match definition.durability {
                            ItemDurability::Nondurable => None,
                            ItemDurability::Durable { max, .. } => Some(max),
                        },
                    })
                    .await
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
            }
            GmSubcommand::Refused { subcommand } => {
                tracing::debug!(subcommand, "unresolved GM subcommand refused");
            }
        }
        Ok(())
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
                let offers = self.resolved_offers();
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
                let Some(definition) = self.resolve_offer(type_id) else {
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
            Ok(remaining) => timeout(
                remaining,
                self.send_bootstrap(framed, &loaded, connection_id),
            )
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
                    RegistryError::Duplicate(_) | RegistryError::Stale(_) => {
                        self.observer.authentication("duplicate")
                    }
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
                game_master: loaded.account.game_master,
                nickname,
                // Carried from the authenticated snapshot so a room roster can render the
                // player's character instead of an empty slot. Both halves travel: the client
                // renders by catalog id and has no way to resolve an inventory one.
                character_id: Some(loaded.equipment.character_id),
                character_iff_id: loaded
                    .characters
                    .iter()
                    .find(|value| value.id == loaded.equipment.character_id)
                    .map(|value| value.item_type_id.get()),
                card: member_card(&loaded),
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_room_command(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        state: GameState,
        identity: &RoomIdentity,
        outbound: RoomOutbound,
        terminal_outbound: TerminalOutboxSender,
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
                    .create_with_terminal_outbox(
                        request.name,
                        request.password,
                        request.settings,
                        identity.clone(),
                        outbound,
                        terminal_outbound.clone(),
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
                                self.social.set_room(identity.connection_id, *room_id);
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
                    .join_with_terminal_outbox(
                        requested_room,
                        identity.clone(),
                        request.password,
                        outbound,
                        terminal_outbound.clone(),
                        room_cancellation,
                    )
                    .await;
                match result {
                    Ok(snapshot) => {
                        *room_id = Some(requested_room);
                        self.social.set_room(identity.connection_id, *room_id);
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
                        self.social.set_room(identity.connection_id, None);
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
                let prepared = self
                    .lobby
                    .route_solo(identity.connection_id, LobbySoloCommand::PrepareFinish)
                    .await;
                let commit = match prepared {
                    Ok(LobbySoloRouteResult::Commit(commit)) => commit,
                    Ok(LobbySoloRouteResult::Applied) => {
                        self.send_solo_result(
                            framed,
                            SoloCommand::FinishHole,
                            SoloCommandOutcome::Success,
                        )
                        .await?;
                        return Ok(state);
                    }
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
        self.observer
            .stroke_commit_identity(committed.match_id(), committed.result_key());
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
    ) -> Result<SoloMatchResult, GameRuntimeError> {
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
                Ok(committed)
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

    async fn handle_social_event(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        connection_id: PlayerConnectionId,
        event: SocialEvent,
    ) -> Result<(), GameRuntimeError> {
        match event {
            SocialEvent::Notice { nickname, message } => {
                self.send(framed, &GameChatResponse { nickname, message })
                    .await?;
            }
            SocialEvent::Chat {
                targets,
                nickname,
                message,
            } if targets.contains(&connection_id) => {
                self.send(framed, &GameChatResponse { nickname, message })
                    .await?;
            }
            SocialEvent::OfflineNote {
                target,
                claim,
                nickname,
                message,
            } if target == connection_id => {
                self.send(
                    framed,
                    &WhisperResponse {
                        status: 0,
                        nickname,
                        message,
                    },
                )
                .await?;
                self.repository
                    .ack_offline_note(claim)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
            }
            SocialEvent::Whisper {
                target,
                status,
                nickname,
                message,
            } if target == connection_id => {
                if matches!(status, 4 | 5) {
                    self.send(framed, &WhisperRefusalResponse { status, nickname })
                        .await?;
                } else {
                    self.send(
                        framed,
                        &WhisperResponse {
                            status,
                            nickname,
                            message,
                        },
                    )
                    .await?;
                }
            }
            SocialEvent::Typing {
                targets,
                connection_id: sender,
                typing,
            } if targets.contains(&connection_id) => {
                self.send(
                    framed,
                    &TypingIndicatorResponse {
                        connection_id: sender,
                        typing,
                    },
                )
                .await?;
            }
            SocialEvent::Lounge {
                targets,
                connection_id: sender,
                action,
            } if targets.contains(&connection_id) => {
                self.send(framed, &LoungeActionResponse::new(sender, action))
                    .await?;
            }
            SocialEvent::UserInfo {
                target,
                user_id,
                request_type,
                nickname,
                card,
            } if target == connection_id => {
                let status = u32::from(!nickname.is_empty());
                if status == 1 {
                    self.send(
                        framed,
                        &UserNameInfoResponse {
                            request_type,
                            user_id,
                            username: card.username.as_bytes().to_vec(),
                            nickname,
                        },
                    )
                    .await?;
                    self.send(
                        framed,
                        &UserCharacterInfoResponse {
                            user_id,
                            character_iff_id: card.character_iff_id,
                            character_uid: card.character_uid,
                        },
                    )
                    .await?;
                    self.send(
                        framed,
                        &UserEquipmentInfoResponse {
                            request_type,
                            user_id,
                            character_uid: card.character_uid,
                            comet_iff_id: card.comet_iff_id,
                        },
                    )
                    .await?;
                    self.send(
                        framed,
                        &UserStatisticsInfoResponse {
                            request_type,
                            user_id,
                            experience: card.experience,
                            pang: card.pang,
                        },
                    )
                    .await?;
                    self.send(framed, &UserGuildInfoResponse { user_id })
                        .await?;
                    let natural_type = if request_type == 5 { 0x33 } else { 0x0a };
                    let grand_prix_type = if request_type == 5 { 0x34 } else { 0x0b };
                    for record_type in [natural_type, grand_prix_type] {
                        self.send(
                            framed,
                            &UserCourseRecordsInfoResponse {
                                request_type: record_type,
                                user_id,
                            },
                        )
                        .await?;
                    }
                    self.send(
                        framed,
                        &UserRelatedInfoResponse {
                            request_type,
                            user_id,
                        },
                    )
                    .await?;
                    self.send(
                        framed,
                        &UserSpecialTrophiesInfoResponse {
                            request_type,
                            user_id,
                        },
                    )
                    .await?;
                    self.send(
                        framed,
                        &UserTrophiesInfoResponse {
                            request_type,
                            user_id,
                        },
                    )
                    .await?;
                    self.send(
                        framed,
                        &UserCourseRecordsInfoResponse {
                            request_type,
                            user_id,
                        },
                    )
                    .await?;
                    self.send(
                        framed,
                        &UserGrandPrixTrophiesInfoResponse {
                            request_type,
                            user_id,
                        },
                    )
                    .await?;
                }
                // PacketDoc/K4T/SuperSS place the acknowledgement after the complete fan-out.
                self.send(
                    framed,
                    &UserInfoResponse {
                        status: if status == 1 { 1 } else { 2 },
                        request_type,
                        user_id,
                    },
                )
                .await?;
            }
            _ => {}
        }
        Ok(())
    }

    fn accepts_terminal_generation(generation: u64, last_generation: u64) -> bool {
        generation != 0 && generation >= last_generation
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_terminal_delivery(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        state: GameState,
        delivery: TerminalDelivery,
        room_id: Option<RoomId>,
        connection_id: PlayerConnectionId,
        match_context: &mut ConnectionMatchContext,
        terminal_generation: &mut u64,
        terminal_identity: &mut Option<(MatchId, MatchResultKey)>,
        terminal_outbound: &RoomOutbound,
    ) -> Result<RoomEventEffect, GameRuntimeError> {
        let Some(context) = match_context.stroke else {
            return Ok(RoomEventEffect::Remain);
        };
        // A terminal mailbox is session-local, but a late item must still be rejected when the
        // connection has already advanced to another card. Match ID, durable settlement key, and
        // generation are all checked before any wire frame is emitted.
        let identity = (delivery.result.match_id(), delivery.result.result_key());
        if context.match_id != delivery.result.match_id()
            || !Self::accepts_terminal_generation(delivery.generation, *terminal_generation)
            || terminal_identity.is_some_and(|current| current == identity)
        {
            return Ok(RoomEventEffect::Remain);
        }
        // Log only an accepted terminal payload. A retained item from an older generation or
        // another match is rejected above and must not appear as a valid registration in logs.
        self.observer.stroke_terminal_payload(
            GameConnectionId(connection_id.get()),
            identity.0,
            identity.1,
            delivery.generation,
        );
        let handled = self
            .handle_room_event(
                framed,
                state,
                RoomEvent::StrokeCommitted(delivery.result),
                room_id,
                connection_id,
                match_context,
            )
            .await;
        if handled.is_ok() {
            // Only a completed socket write is an ACK. Keep identity/generation untouched on a
            // failed write so reconnect/replay remains eligible, and clear only this generation
            // so a stale queued event cannot drop a newer terminal reservation.
            *terminal_generation = delivery.generation;
            *terminal_identity = Some(identity);
            terminal_outbound.acknowledge_terminal_delivery(delivery.generation);
        }
        handled
    }

    async fn handle_room_event(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        state: GameState,
        event: RoomEvent,
        room_id: Option<RoomId>,
        connection_id: PlayerConnectionId,
        match_context: &mut ConnectionMatchContext,
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
                | RoomEvent::RetailLoadProgress { .. }
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
                RoomEvent::Invite {
                    channel_id,
                    room_id,
                    inviter_id,
                    inviter_nickname,
                    invitee_id,
                } => {
                    if state == GameState::InChannel {
                        self.send(
                            framed,
                            &RetailRoomInviteNotification {
                                server_id: 0,
                                channel_id: *channel_id,
                                room_id: u16::try_from(room_id.get()).unwrap_or(u16::MAX),
                                inviter_id: u32::try_from(inviter_id.get()).unwrap_or(u32::MAX),
                                inviter_nickname: inviter_nickname.clone(),
                                invitee_id: u32::try_from(invitee_id.get()).unwrap_or(u32::MAX),
                            },
                        )
                        .await?;
                    }
                    return Ok(RoomEventEffect::Remain);
                }
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
                RoomEvent::SettingsChanged(room) => {
                    if state == GameState::InRoom {
                        self.send(
                            framed,
                            &RetailRoomStatus {
                                room: retail_room_from_snapshot(room),
                            },
                        )
                        .await?;
                    }
                    return Ok(RoomEventEffect::Remain);
                }
                RoomEvent::Chat { .. } => {
                    return Ok(RoomEventEffect::Remain);
                }
                RoomEvent::EquipmentAnnounce(announce) => {
                    if state == GameState::InRoom {
                        self.send(framed, announce).await?;
                    }
                    return Ok(RoomEventEffect::Remain);
                }
                RoomEvent::Atmosphere { weather, wind } => {
                    if let Some(weather) = weather {
                        let weather = match weather {
                            0 => RetailWeather::Clear,
                            1 => RetailWeather::Cloudy,
                            2 => RetailWeather::Raining,
                            _ => return Err(GameRuntimeError::Protocol),
                        };
                        self.send(framed, &RetailHoleWeather { weather }).await?;
                    }
                    if let Some((strength, direction)) = wind {
                        self.send(
                            framed,
                            &RetailHoleWind {
                                strength: *strength,
                                direction: u16::from(*direction),
                            },
                        )
                        .await?;
                    }
                    return Ok(RoomEventEffect::Remain);
                }
                RoomEvent::SoloStarted(plan) => {
                    let begin = plan.begin();
                    // A solo hole has no turn arbitration, so the client's own timers are the
                    // only ones, and it is told the same defaults it shows for practice.
                    let roster = self.retail_match_roster(connection_id).await;
                    match_context.solo_hole = 1;
                    match_context.atmosphere = Some(
                        self.send_retail_hole_intro(
                            framed,
                            connection_id,
                            &roster,
                            RetailHoleIntro {
                                course_id: begin.config().course_id().get(),
                                hole_count: begin.config().hole_count(),
                                hole_mode: begin.config().hole_mode(),
                                weather: begin.weather(),
                                seed: begin.seed(),
                                natural_wind: self.retail_natural_wind(connection_id).await,
                                shot_timer: RETAIL_SOLO_SHOT_TIMER,
                                game_timer: RETAIL_SOLO_GAME_TIMER,
                            },
                        )
                        .await?,
                    );
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
                        if let Some(atmosphere) = match_context.atmosphere {
                            self.send_retail_hole_atmosphere(framed, atmosphere).await?;
                        }
                        // `0x0053` already names whose turn the hole opens on, so no
                        // `0x0063` follows it. See the stroke path for why sending one here
                        // is fatal.
                        self.send(
                            framed,
                            &RetailPlayerStartHole {
                                connection_id: connection,
                            },
                        )
                        .await?;
                        self.send_retail_hole_rate_tables(framed).await?;
                        return Ok(RoomEventEffect::EnterMatch);
                    }
                    return Ok(RoomEventEffect::Remain);
                }
                RoomEvent::TeamChanged {
                    connection_id,
                    team,
                } => {
                    self.send(
                        framed,
                        &RetailTeamChangeAnnounce {
                            connection_id: u32::try_from(connection_id.get()).unwrap_or(u32::MAX),
                            team: match team {
                                0 => pangya_protocol::RetailTeam::Red,
                                1 => pangya_protocol::RetailTeam::Blue,
                                _ => return Err(GameRuntimeError::Protocol),
                            },
                        },
                    )
                    .await?;
                    return Ok(RoomEventEffect::Remain);
                }
                RoomEvent::Kicked { .. } => {
                    self.send(framed, &RetailRoomLeave::to_lobby()).await?;
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
                    let roster = self.retail_match_roster(connection_id).await;
                    let atmosphere = self
                        .send_retail_hole_intro(
                            framed,
                            connection_id,
                            &roster,
                            RetailHoleIntro {
                                course_id: begin.config().course_id().get(),
                                hole_count: begin.config().hole_count(),
                                hole_mode: begin.config().hole_mode(),
                                weather: begin.weather(),
                                seed: begin.seed(),
                                natural_wind: self.retail_natural_wind(connection_id).await,
                                shot_timer: plan.turn_timeout(),
                                game_timer: plan.game_timeout(),
                            },
                        )
                        .await?;
                    match_context.atmosphere = Some(atmosphere);
                    match_context.stroke = Some(ConnectionStrokeContext {
                        match_id: begin.match_id(),
                        roster: *plan.roster(),
                        seed: begin.seed(),
                        natural_wind: self.retail_natural_wind(connection_id).await,
                        hole: 1,
                        active: None,
                    });
                    return Ok(RoomEventEffect::EnterStrokeLoading);
                }
                // Phase is carried by the turn frames a retail client actually reads.
                RoomEvent::StrokePhase { .. } => return Ok(RoomEventEffect::Remain),
                RoomEvent::StrokeTurn(phase) => {
                    let StrokeMatchPhase::AwaitAction { active, hole, .. } = *phase else {
                        return Err(GameRuntimeError::Protocol);
                    };
                    let mut context = match_context.stroke.ok_or(GameRuntimeError::Protocol)?;
                    let connection = |id: PlayerConnectionId| u32::try_from(id.get()).unwrap_or(0);
                    // A completed hole resets the aggregate's active player to the first seat.
                    // That reset is a new hole introduction, not an ordinary turn handover.
                    // Keep this check before the active-player comparison so every subsequent
                    // hole receives the required 0x0053 and never an early 0x0063.
                    if context.active.is_some() && hole != context.hole {
                        let (weather, wind) = deterministic_conditions_for_gameplay(
                            context.seed,
                            context.natural_wind,
                            hole,
                        )
                        .map_err(|_| GameRuntimeError::InvalidConfig)?;
                        let weather = match weather {
                            pangya_domain::Weather::Clear => RetailWeather::Clear,
                            pangya_domain::Weather::Cloudy => RetailWeather::Cloudy,
                            pangya_domain::Weather::Rain => RetailWeather::Raining,
                        };
                        self.send_retail_hole_atmosphere(
                            framed,
                            RetailHoleAtmosphere { weather, wind },
                        )
                        .await?;
                        self.send(
                            framed,
                            &RetailPlayerStartHole {
                                connection_id: connection(active),
                            },
                        )
                        .await?;
                        self.send_retail_hole_rate_tables(framed).await?;
                    } else {
                        match context.active {
                            // The first turn of the hole is introduced rather than handed over.
                            //
                            // `0x0053` carries the connection whose turn opens the hole, and that
                            // is the whole announcement: no reference server sends a `0x0063`
                            // here. Sending one is fatal rather than merely redundant. The
                            // client's `0x0063` handler walks its per-player in-game array to
                            // mark the active player, and while the course loading screen is
                            // still up that array holds no scene objects yet — so it dereferences
                            // a null model and the process exits. `0x0063` belongs only to a turn
                            // *change*, after a full shot cycle.
                            //
                            // `pangbox/server` `game/room/room.go`: `startHole` sends `0x009e`,
                            // `0x005b` and `0x0053` and no `0x0063`; `0x0063` is emitted only by
                            // `nextTurn`, reached only from `endTurn`. `Acrisio-Filho/SuperSS-Dev`
                            // `GAME/versus_base.cpp`: `sendReplyFinishLoadHole` sends `0x9e`,
                            // `0x5b`, `0x53`; `0x63` lives only in `sendPlayerTurn`, called only
                            // from `changeTurn`. `hsreina/pangya-server` `Game.pas`
                            // `HandlePlayerLoadOk` likewise sends no `0x63`.
                            None => {
                                if let Some(atmosphere) = match_context.atmosphere.take() {
                                    self.send_retail_hole_atmosphere(framed, atmosphere).await?;
                                }
                                self.send(
                                    framed,
                                    &RetailPlayerStartHole {
                                        connection_id: connection(active),
                                    },
                                )
                                .await?;
                                self.send_retail_hole_rate_tables(framed).await?;
                            }
                            Some(previous) if previous != active => {
                                self.send(
                                    framed,
                                    &RetailTurnEnd {
                                        connection_id: connection(previous),
                                    },
                                )
                                .await?;
                                self.send(
                                    framed,
                                    &RetailTurnStart {
                                        connection_id: connection(active),
                                    },
                                )
                                .await?;
                            }
                            Some(_) => {
                                self.send(
                                    framed,
                                    &RetailTurnStart {
                                        connection_id: connection(active),
                                    },
                                )
                                .await?;
                            }
                        }
                    }
                    context.hole = hole;
                    context.active = Some(active);
                    match_context.stroke = Some(context);
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
                // The loading screen draws a bar per player and waits on all of them, so each
                // client has to hear about the others'. Upstream broadcasts one of these for
                // every progress frame it receives, including back to the sender.
                RoomEvent::RetailLoadProgress { from, progress } => {
                    self.send(
                        framed,
                        &RetailLoadProgress {
                            connection_id: u32::try_from(from.get()).unwrap_or(0),
                            progress: *progress,
                        },
                    )
                    .await?;
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
                RoomEvent::StrokeCommitted(result)
                | RoomEvent::StrokeCommittedWithGeneration { result, .. } => {
                    // The actor emitted this only after durable settlement. Recording before the
                    // completion frames keeps a client-visible completed match and its history
                    // atomic from an observer's point of view.
                    self.record_retail_match_history(result).await?;
                    self.send_retail_stroke_committed(framed, *result, match_context.stroke)
                        .await?;
                    match_context.stroke = None;
                    return Ok(RoomEventEffect::EnterRoom);
                }
                // An abort pays nothing, and there is no retail frame that says so. The room
                // is what the client returns to either way.
                RoomEvent::StrokeAborted(_) => {
                    match_context.stroke = None;
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
            RoomEvent::EquipmentAnnounce(announce) => {
                self.send(framed, &announce).await?;
                Ok(RoomEventEffect::Remain)
            }
            RoomEvent::SettingsChanged(room) => {
                self.send(framed, &RoomStateResponse { room }).await?;
                self.observer.room(GameRoomObservation::StateSent);
                Ok(RoomEventEffect::Remain)
            }
            RoomEvent::Invite { .. } => Ok(RoomEventEffect::Remain),
            RoomEvent::Atmosphere { .. } => Ok(RoomEventEffect::Remain),
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
            RoomEvent::TeamChanged {
                connection_id,
                team,
            } => {
                self.send(
                    framed,
                    &RetailTeamChangeAnnounce {
                        connection_id: u32::try_from(connection_id.get()).unwrap_or(u32::MAX),
                        team: match team {
                            0 => pangya_protocol::RetailTeam::Red,
                            1 => pangya_protocol::RetailTeam::Blue,
                            _ => return Err(GameRuntimeError::Protocol),
                        },
                    },
                )
                .await?;
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
                // Projection refreshes are a retail room contract. Synthetic M5 settlement has
                // its own terminal packet order and must not enqueue a retail census frame.
                if self.config.retail_bootstrap
                    && let Some(account_id) = self.social.account_for_connection(connection_id)
                {
                    self.refresh_social_projection(connection_id, account_id)
                        .await?;
                }
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
                match_context.stroke = Some(ConnectionStrokeContext {
                    match_id: begin.match_id(),
                    roster,
                    seed: begin.seed(),
                    natural_wind: false,
                    hole: 1,
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
                    hole: _,
                } = phase
                else {
                    return Err(GameRuntimeError::Protocol);
                };
                let context = match_context.stroke.ok_or(GameRuntimeError::Protocol)?;
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
            RoomEvent::RetailRelay { .. } | RoomEvent::RetailLoadProgress { .. } => {
                Err(GameRuntimeError::Protocol)
            }
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
            RoomEvent::StrokeCommitted(result)
            | RoomEvent::StrokeCommittedWithGeneration { result, .. } => {
                // Projection refreshes are a retail room contract. Synthetic M6 settlement has
                // its own terminal packet order and must not enqueue a retail census frame.
                if self.config.retail_bootstrap
                    && let Some(account_id) = self.social.account_for_connection(connection_id)
                {
                    self.refresh_social_projection(connection_id, account_id)
                        .await?;
                }
                self.send_stroke_committed(framed, connection_id, result, match_context.stroke)
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

    /// Derives the authoritative match plan from the room's complete profile.
    fn retail_match_plan(&self, profile: RoomProfile) -> Result<MatchPlan, GameRuntimeError> {
        // Retail course ordinals are zero-based on the wire; the durable domain ID is
        // positive and therefore starts one above the wire value.
        let course_id = CourseId::new(u32::from(profile.course).saturating_add(1))
            .map_err(|_| GameRuntimeError::InvalidConfig)?;
        // The room's course identity is authoritative. Some local test/catalog bundles only
        // carry one par declaration, so use that checked par as the metadata fallback while
        // preserving the client-selected course ID rather than silently reverting the course.
        let par = self
            .catalog
            .course_plan(course_id, profile.hole_count, profile.hole_progression)
            .map(|declared| declared.par())
            .or_else(|_| {
                self.config
                    .solo_practice
                    .map(|configured| configured.course.par())
                    .or_else(|| {
                        self.config
                            .stroke_two
                            .map(|configured| configured.course.par())
                    })
                    .ok_or(())
            })
            .map_err(|_| GameRuntimeError::InvalidConfig)?;
        MatchPlan::with_holes(course_id, profile.hole_count, profile.hole_progression, par)
            .map_err(|_| GameRuntimeError::InvalidConfig)
    }

    /// Reads the room's natural-wind switch for gameplay generation.
    async fn retail_natural_wind(&self, connection_id: PlayerConnectionId) -> bool {
        match self
            .lobby
            .route(connection_id, LobbyRoomCommand::GetState)
            .await
        {
            Ok(LobbyRouteResult::Snapshot(snapshot)) => snapshot.summary().profile().natural_wind,
            _ => false,
        }
    }

    /// Builds a deterministic full-course hole order for the retail card.
    ///
    /// Front/back use the contiguous ranges from the retail UI. RandomStart is a deterministic
    /// rotation (the room/course pair is its stable seed), while ShuffleAll uses a tiny local
    /// Fisher-Yates pass. Keeping all 18 source holes available before truncating preserves the
    /// semantics for every supported card size instead of accidentally treating a 9-hole card as
    /// a different course.
    fn retail_hole_order(course_id: u32, hole_count: u8, hole_mode: u8) -> Vec<u8> {
        let mut holes: Vec<u8> = match hole_mode {
            1 => (19_u8.saturating_sub(hole_count)..=18).collect(),
            _ => (1..=18).collect(),
        };
        let count = usize::from(hole_count);
        match hole_mode {
            2 => {
                let offset = (course_id
                    .wrapping_mul(31)
                    .wrapping_add(u32::from(hole_count).wrapping_mul(17))
                    % 18) as usize;
                holes.rotate_left(offset);
            }
            3 => {
                let mut state = course_id
                    .wrapping_mul(0x9e37_79b9)
                    .wrapping_add(u32::from(hole_count).wrapping_mul(0x85eb_ca6b));
                for index in (1..holes.len()).rev() {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let swap = (state as usize) % (index + 1);
                    holes.swap(index, swap);
                }
            }
            _ => {}
        }
        holes.truncate(count);
        holes
    }

    /// The room's members, in seat order, for a match roster.
    ///
    /// Read from the room rather than from the match plan: the plan names connections, and the
    /// roster has to describe whole players. An empty answer sends an empty roster, which the
    /// client renders as a match with nobody in it rather than misreading the frame.
    async fn retail_match_roster(&self, connection_id: PlayerConnectionId) -> Vec<MemberSnapshot> {
        match self
            .lobby
            .route(connection_id, LobbyRoomCommand::GetState)
            .await
        {
            Ok(LobbyRouteResult::Snapshot(snapshot)) => snapshot.members().to_vec(),
            _ => Vec::new(),
        }
    }

    /// Sends the frames a retail client needs before it will load a hole.
    ///
    /// The framing pair and the pang rate come first, then the roster carrying every player
    /// whole, because the client builds each of them from it; the plan follows with the room's
    /// complete card shape. The client reads the whole plan up front, so an incomplete card
    /// strands it, and the mascot seed closes the sequence.
    ///
    /// Weather and wind are deliberately not here. They are returned for the caller to hold
    /// until every player has reported its hole loaded — no reference server sends either
    /// before that point.
    async fn send_retail_hole_intro(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        connection_id: PlayerConnectionId,
        roster: &[MemberSnapshot],
        hole: RetailHoleIntro,
    ) -> Result<RetailHoleAtmosphere, GameRuntimeError> {
        let RetailHoleIntro {
            course_id,
            hole_count,
            hole_mode,
            weather,
            seed,
            natural_wind,
            shot_timer,
            game_timer,
        } = hole;
        let (_, wind) = deterministic_conditions_for_gameplay(seed, natural_wind, 1)
            .map_err(|_| GameRuntimeError::InvalidConfig)?;
        let millis = |duration: Duration| {
            u32::try_from(duration.as_millis()).map_err(|_| GameRuntimeError::InvalidConfig)
        };
        let weather = match weather {
            pangya_domain::Weather::Clear => RetailWeather::Clear,
            pangya_domain::Weather::Cloudy => RetailWeather::Cloudy,
            pangya_domain::Weather::Rain => RetailWeather::Raining,
        };
        let course = u8::try_from(course_id.saturating_sub(1)).unwrap_or(0);
        self.send(framed, &RetailMatchOpen).await?;
        self.send(framed, &RetailMatchOpenAck).await?;
        self.send(framed, &RetailPangRate::default()).await?;
        let seats: Vec<_> = roster
            .iter()
            .enumerate()
            .map(|(slot, member)| retail_match_player(slot, member))
            .collect();
        tracing::info!(players = seats.len(), "retail match roster");
        self.send(framed, &RetailMatchStart::Roster(seats)).await?;
        // Each player's own statistics, which the client asks for by starting the game and
        // waits on before it will finish building the hole. Sent between the roster and the
        // plan, where both reference servers put it.
        if let Some(member) = roster
            .iter()
            .find(|member| member.connection_id() == connection_id)
        {
            let card = member.card();
            self.send(
                framed,
                &RetailPlayerStatisticsReport {
                    statistics: RetailPlayerStatistics {
                        experience: card.experience,
                        pang: card.pang,
                        ..RetailPlayerStatistics::default()
                    },
                },
            )
            .await?;
        }
        self.send(
            framed,
            &RetailMatchInfo {
                course,
                room_ui_type: 0,
                hole_mode,
                hole_count,
                shot_timer_ms: millis(shot_timer)?,
                game_timer_ms: millis(game_timer)?,
                holes: Self::retail_hole_order(course_id, hole_count, hole_mode)
                    .into_iter()
                    .map(|number| RetailHole {
                        // The seed is fixed for this deterministic room plan; the hole
                        // sequence itself is authoritative and is retained in the card.
                        random_id: u32::from(number),
                        pin: 0,
                        course,
                        number,
                    })
                    .collect(),
                random_seed: 1,
            },
        )
        .await?;
        self.send(framed, &RetailMascotSeed::default()).await?;
        // The census modification that closes the start sequence. `SuperSS-Dev`
        // (`GAME/room.cpp` `room::startGame`) sends exactly this for a stroke or match room,
        // right after the initial data, addressed to the player who started.
        if let Some((slot, member)) = roster
            .iter()
            .enumerate()
            .find(|(_, member)| member.connection_id() == connection_id)
        {
            self.send(
                framed,
                &RetailRoomCensus::Update(Box::new(retail_room_player(slot, member))),
            )
            .await?;
        }
        Ok(RetailHoleAtmosphere { weather, wind })
    }

    /// Sends the three voice and effect rate tables that follow `0x0053`.
    ///
    /// Every reference server sends exactly these three, in this order, immediately after the
    /// hole's opening player is announced and before it waits on the clients.
    async fn send_retail_hole_rate_tables(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
    ) -> Result<(), GameRuntimeError> {
        for table in RetailRateTable::hole_tables() {
            self.send(framed, &table).await?;
        }
        Ok(())
    }

    /// Sends the weather and wind the hole is played under, once its players are all loaded.
    ///
    /// Upstream never sends a wind strength of zero, so a still hole is reported as the
    /// weakest breeze rather than as no wind at all.
    async fn send_retail_hole_atmosphere(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        atmosphere: RetailHoleAtmosphere,
    ) -> Result<(), GameRuntimeError> {
        let RetailHoleAtmosphere { weather, wind } = atmosphere;
        self.send(framed, &RetailHoleWeather { weather }).await?;
        self.send(
            framed,
            &RetailHoleWind {
                strength: u8::try_from(wind.speed_tenths() / 10)
                    .unwrap_or(u8::MAX)
                    .max(1),
                direction: wind.angle_degrees(),
            },
        )
        .await
    }

    /// Puts the durable settlement of a two-player hole on a retail client's results screen.
    ///
    /// Every figure here is the committed server-side result. A forfeit has no golf score, so
    /// its line reports zero rather than inventing one.
    /// Records only players in the durable completed-match result. The result carries account
    /// identity from `match_players`, so a disconnect between settlement and this frame cannot
    /// remove a participant from history; nicknames are loaded from durable player projections.
    async fn record_retail_match_history(
        &self,
        result: &StrokeMatchResult,
    ) -> Result<(), GameRuntimeError> {
        let participants = result
            .players()
            .map(|player| player.participant().account_id());
        let mut snapshots = Vec::with_capacity(participants.len());
        for account_id in participants {
            let snapshot = self
                .repository
                .load_player_snapshot(account_id)
                .await
                .map_err(|_| GameRuntimeError::Snapshot)?;
            let nickname = snapshot
                .profile
                .nickname
                .ok_or(GameRuntimeError::Snapshot)?;
            snapshots.push((account_id, nickname));
        }
        for (owner, _) in &snapshots {
            for (recent, nickname) in &snapshots {
                if owner == recent {
                    continue;
                }
                self.repository
                    .record_recent_player(
                        *owner,
                        RecentPlayer {
                            account_id: *recent,
                            nickname: nickname.clone(),
                            seen_at: SystemTime::now(),
                        },
                    )
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
            }
        }
        Ok(())
    }

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
        // A terminal room event is delivered once to each captured roster member, including the
        // last finisher's opponent. Emit the terminal hole marker here (rather than in the command
        // path) so every player gets exactly one `0x0065`, then one authoritative `0x0066`.
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
        connection_id: PlayerConnectionId,
    ) -> Result<(), GameRuntimeError> {
        if self.config.retail_bootstrap {
            return self
                .send_retail_bootstrap(framed, snapshot, connection_id)
                .await;
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
        retail_sequence: &mut RetailStrokeSequence,
        match_context: &mut ConnectionMatchContext,
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
                retail_sequence,
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
            (GameState::InRoom, RETAIL_C2S_START_MATCH | RetailPracticeStart::OPCODE) => {
                let Ok(LobbyRouteResult::Snapshot(snapshot)) = self
                    .lobby
                    .route(identity.connection_id, LobbyRoomCommand::GetState)
                    .await
                else {
                    return Ok(GameState::InRoom);
                };
                let profile = snapshot.summary().profile();
                let course = self.retail_match_plan(profile)?;
                if opcode == RetailPracticeStart::OPCODE {
                    let _request = decode_packet_payload::<RetailPracticeStart>(
                        payload,
                        &CompatibilityProfile::US_852,
                        ServiceKind::Game,
                    )
                    .map_err(|_| GameRuntimeError::Protocol)?;
                    let Ok(LobbyRouteResult::Snapshot(snapshot)) = self
                        .lobby
                        .route(identity.connection_id, LobbyRoomCommand::GetState)
                        .await
                    else {
                        return Ok(GameState::InRoom);
                    };
                    if snapshot.members().len() != 1
                        || snapshot.summary().profile().mode != RetailRoomType::Practice as u8
                    {
                        return Ok(GameState::InRoom);
                    }
                    // The submission is a UI barrier, not equipment authority. The durable
                    // equipment aggregate already supplied the roster and remains unchanged even
                    // when these client-claimed ids are stale.
                }
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
                    course,
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
            // SuperSS documents silent Practice Init Shot, while pangbox and alter broadcast an
            // acknowledgement. The real U.S. 852 client exercised here follows the latter path:
            // without `0x0055` it remains frozen at Impact and never begins ball animation. This
            // echo is only an animation barrier; the stroke is counted later at sync.
            (GameState::InMatch, RETAIL_C2S_SHOT_COMMIT) => {
                if !shots.admit_count(solo.shot_packets_per_window) {
                    self.observer
                        .rate_limited(GameRateClass::ShotPacketsConnection);
                    self.observer.shot(GameShotObservation::RateLimited);
                    return Ok(state);
                }
                self.send(
                    framed,
                    &RetailShotCommitRelay {
                        connection_id: u32::try_from(identity.connection_id.get())
                            .unwrap_or_default(),
                        shot: retail_shot_announce_payload(payload)?,
                    },
                )
                .await?;
                Ok(state)
            }
            // Practice answers sync immediately with its reduced `0x006e` body. It is also the
            // point at which the reference increments the authoritative stroke count.
            (GameState::InMatch, RETAIL_C2S_SHOT_SYNC) => {
                if !shots.admit_count(solo.shot_packets_per_window) {
                    self.observer
                        .rate_limited(GameRateClass::ShotPacketsConnection);
                    self.observer.shot(GameShotObservation::RateLimited);
                    return Ok(state);
                }
                let sync = decode_packet_payload::<RetailPracticeShotSyncRequest>(
                    payload,
                    &CompatibilityProfile::US_852,
                    ServiceKind::Game,
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                let expected_connection =
                    u32::try_from(identity.connection_id.get()).unwrap_or_default();
                if sync.connection_id != expected_connection {
                    return Err(GameRuntimeError::Protocol);
                }
                self.record_retail_stroke(identity.connection_id, strokes, false)
                    .await?;
                self.send(
                    framed,
                    &RetailPracticeShotSync {
                        connection_id: sync.connection_id,
                        hole: match_context.solo_hole,
                        x: sync.x,
                        z: sync.z,
                        shot_state: sync.shot_state,
                        shot_time: sync.shot_time,
                    },
                )
                .await?;
                Ok(state)
            }
            // This U.S. 852 flow uses the versus-shaped hole intro, so the end-shot barrier must
            // hand the perpetual one-player turn back as well. Without `0x0063` the client waits
            // exactly sixty seconds after landing, reports an exception, and disconnects.
            (GameState::InMatch, RETAIL_C2S_SHOT_END) => {
                let connection_id = u32::try_from(identity.connection_id.get()).unwrap_or_default();
                self.send(framed, &RetailTurnEnd { connection_id }).await?;
                self.send(framed, &RetailTurnStart { connection_id })
                    .await?;
                Ok(state)
            }
            (GameState::InMatch, RETAIL_C2S_HOLE_FINISH) => {
                // The client announces hole completion after the already-counted sync/end pair.
                // Mark that stroke holed without fabricating another one.
                match self
                    .lobby
                    .route_solo(identity.connection_id, LobbySoloCommand::HoleOut)
                    .await
                {
                    Ok(LobbySoloRouteResult::Relay(_)) => {}
                    Ok(_) => return Err(GameRuntimeError::Protocol),
                    Err(_) => return Ok(state),
                }
                let prepared = self
                    .lobby
                    .route_solo(identity.connection_id, LobbySoloCommand::PrepareFinish)
                    .await;
                self.send(framed, &RetailFinishHole).await?;
                let commit = match prepared {
                    Ok(LobbySoloRouteResult::Commit(commit)) => commit,
                    Ok(LobbySoloRouteResult::Applied) => {
                        // The actor has advanced to the next hole. The wire has no new
                        // MatchInfo frame here: 0x0053 starts the next card entry and the
                        // atmosphere/rate frames follow it, exactly as on the first hole.
                        match_context.solo_hole = match_context.solo_hole.saturating_add(1);
                        // The actor's per-hole sequence restarts at one; this counter mirrors
                        // that sequence for the retail practice adapter, not the card total.
                        *strokes = 0;
                        let connection = u32::try_from(identity.connection_id.get()).unwrap_or(0);
                        if let Some(atmosphere) = match_context.atmosphere {
                            self.send_retail_hole_atmosphere(framed, atmosphere).await?;
                        }
                        self.send(
                            framed,
                            &RetailPlayerStartHole {
                                connection_id: connection,
                            },
                        )
                        .await?;
                        self.send_retail_hole_rate_tables(framed).await?;
                        return Ok(state);
                    }
                    Ok(_) => return Err(GameRuntimeError::Protocol),
                    Err(_) => return Ok(state),
                };
                let committed = self
                    .persist_and_apply_commit(
                        identity.connection_id,
                        commit,
                        shutdown,
                        idle_deadline,
                    )
                    .await?;
                self.send(
                    framed,
                    &RetailMatchFinish {
                        standings: vec![RetailStanding {
                            connection_id: u32::try_from(identity.connection_id.get())
                                .unwrap_or_default(),
                            place: 1,
                            score: i8::try_from(committed.score()).unwrap_or(
                                if committed.score() < 0 {
                                    i8::MIN
                                } else {
                                    i8::MAX
                                },
                            ),
                            experience: u16::try_from(committed.experience_reward())
                                .unwrap_or(u16::MAX),
                            pang: committed.pang_reward(),
                            bonus_pang: 0,
                        }],
                    },
                )
                .await?;
                self.send(
                    framed,
                    &RetailPangBalance {
                        pang: committed.pang_balance(),
                    },
                )
                .await?;
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
        retail_sequence: &mut RetailStrokeSequence,
        opcode: u16,
        payload: &[u8],
    ) -> Result<Option<GameState>, GameRuntimeError> {
        // The cosmetic half of a hole: where a player is aiming, how far the power meter has
        // travelled, which club is in hand. None of it changes a stroke, a turn, or a score.
        // What matters is that it is answered by an explicit allowlist rather than by the
        // unknown-opcode policy, which under the shipped `disconnect` would end the session
        // mid-hole.
        if is_retail_accepted_match_opcode(opcode)
            && !(state == GameState::InRoom && opcode == RetailPracticeStart::OPCODE)
        {
            self.observer.unknown(GameUnknownObservation::Ignored);
            return Ok(Some(state));
        }
        if opcode == RETAIL_C2S_LOAD_PROGRESS {
            // The client draws a loading bar per player, so its own progress is republished to
            // everyone in the match. Ignoring it leaves every other client's bar stuck.
            let progress = payload.first().copied().unwrap_or(0);
            let _ignored = self
                .lobby
                .route_stroke(
                    identity.connection_id,
                    LobbyStrokeCommand::LoadProgress(progress),
                )
                .await;
            return Ok(Some(state));
        }
        if opcode == RETAIL_C2S_FIRST_SHOT_READY {
            // The client announces it is ready for the first shot and waits to be told to go.
            // The reply carries nothing; it is the fact of it that the client reads.
            self.send(framed, &RetailFirstShotReady).await?;
            return Ok(Some(state));
        }
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
                // The client's own button reads Start for the room master and Ready for
                // everyone else, so a master never sends `0x000d`: pressing Start *is* the
                // master declaring itself ready. Without this the aggregate refuses every real
                // start with `NotReady`, because the one player who cannot say so is the only
                // one who can begin the match.
                if !matches!(
                    self.lobby
                        .route(identity.connection_id, LobbyRoomCommand::SetReady(true))
                        .await,
                    Ok(LobbyRouteResult::Snapshot(_))
                ) {
                    return Ok(Some(GameState::InRoom));
                }
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
                    retail_sequence.clear();
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
                    retail_sequence.clear();
                    return Ok(Some(state));
                }
                retail_sequence.accepted_action();
                self.relay_retail_match_frame(
                    identity.connection_id,
                    RetailMatchRelay::Shot(retail_shot_announce_payload(payload)?),
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
                    retail_sequence.clear();
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
                    if retail_sequence.accepted_result() {
                        self.complete_retail_stroke_hole(framed, identity.connection_id, strokes)
                            .await?;
                    }
                } else {
                    // A declined/duplicate result cannot complete an earlier client claim.
                    retail_sequence.clear();
                }
                Ok(Some(state))
            }
            (GameState::InStrokeMatch, RETAIL_C2S_HOLE_FINISH) => {
                // U.S. 851 may send its cumulative `0x0031` during ball flight, before the
                // ordinary `0x001c` result barrier. Retain one claim locally until that already
                // accepted action is actually committed; replayed early claims remain one bit.
                if retail_sequence.remember_early_hole_finish() {
                    return Ok(Some(state));
                }
                self.complete_retail_stroke_hole(framed, identity.connection_id, strokes)
                    .await?;
                Ok(Some(state))
            }
            _ => Ok(None),
        }
    }

    /// Completes a hole after its ordinary accepted result, regardless of which side of that
    /// result barrier carried the opaque retail `0x0031` body.
    async fn complete_retail_stroke_hole(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        connection_id: PlayerConnectionId,
        strokes: &mut u32,
    ) -> Result<(), GameRuntimeError> {
        // The holing shot was already counted through the ordinary action/result pair, so this
        // completes the caller's hole without charging another stroke. The finish frame precedes
        // the next 0x0053/turn sequence for a multi-hole card.
        let routed = self
            .lobby
            .route_stroke(connection_id, LobbyStrokeCommand::HoleOut)
            .await;
        match routed {
            Ok(LobbyStrokeRouteResult::HoleOut(StrokeHoleOutOutcome::Waiting)) => {
                // Each connection's counter mirrors that participant's per-hole sequence;
                // finishing this participant resets it while another may still finish this card
                // entry.
                *strokes = 0;
                self.send(framed, &RetailFinishHole).await?;
            }
            Ok(LobbyStrokeRouteResult::HoleOut(StrokeHoleOutOutcome::Settlement(_))) => {
                // The terminal room event sends one 0x0065 and one 0x0066 to every captured
                // roster member. Sending 0x0065 here would duplicate the final completion.
                *strokes = 0;
            }
            Ok(LobbyStrokeRouteResult::HoleOut(StrokeHoleOutOutcome::Duplicate)) | Err(_) => {
                // Replayed `0x0031` is idempotent and has no wire reply. A transport race after
                // settlement is handled by the retained terminal room event.
            }
            Ok(_) => return Err(GameRuntimeError::Protocol),
        }
        Ok(())
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
        let course = self.retail_match_plan(snapshot.summary().profile())?;
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
            course,
            stroke.catalog_fingerprint,
            seed,
            weather,
            wind,
        )
        .map_err(|_| GameRuntimeError::InvalidConfig)?;
        // The live room timer is authoritative. Versus and chat carry a whole-game timer on
        // the wire, but PacketDoc identifies it as unused; use the checked stroke configuration
        // for that actor deadline instead of turning the retained zero wire value into an invalid
        // duration. Room announcements continue to use the untouched profile value.
        let profile = snapshot.summary().profile();
        let shot_timeout = Duration::from_millis(u64::from(profile.shot_timer_ms));
        let game_timeout = retail_stroke_game_timeout(profile, stroke.game_timeout);
        let plan = StrokeStartPlan::new(
            begin,
            [first.connection_id(), second.connection_id()],
            stroke.loading_timeout,
            shot_timeout,
            game_timeout,
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

    fn deliver_retail_invite(
        &self,
        room_id: RoomId,
        inviter: &RoomIdentity,
        invitee_id: AccountId,
    ) {
        let sender = self
            .invite_targets
            .lock()
            .ok()
            .and_then(|targets| targets.get(&invitee_id).cloned());
        if let Ok(mut pending) = self.pending_invites.lock() {
            pending.insert(invitee_id, room_id);
        }
        if let Some(sender) = sender {
            let _ignored = sender.try_send(RoomEvent::Invite {
                channel_id: 0,
                room_id,
                inviter_id: inviter.account_id,
                inviter_nickname: inviter.nickname.display().as_bytes().to_vec(),
                invitee_id,
            });
        }
    }

    fn retail_server_day(now: SystemTime) -> i64 {
        now.duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| {
                i64::try_from(duration.as_secs() / 86_400).unwrap_or(i64::MAX)
            })
    }

    async fn retail_login_bonus_status(
        &self,
        account_id: AccountId,
    ) -> Result<RetailLoginBonusStatus, GameRuntimeError> {
        let Some(config) = self.config.login_bonus else {
            return Ok(RetailLoginBonusStatus::Collected {
                unknown_a: [0; 4],
                current_item_id: 0,
                current_item_quantity: 0,
                future_item_id: 0,
                future_item_quantity: 0,
                future_bonus_day: 0,
            });
        };
        let server_day = Self::retail_server_day(self.clock.now());
        let claimed = self
            .repository
            .login_bonus_claimed(account_id, server_day)
            .await
            .map_err(|_| GameRuntimeError::Snapshot)?;
        let day = u32::try_from(server_day.rem_euclid(i64::from(config.calendar_days)))
            .map_err(|_| GameRuntimeError::Catalog)?
            .saturating_add(1);
        let item_id = config.reward.definition.type_id.get();
        if claimed {
            Ok(RetailLoginBonusStatus::Collected {
                unknown_a: [0; 4],
                current_item_id: item_id,
                current_item_quantity: config.reward.quantity,
                future_item_id: item_id,
                future_item_quantity: config.reward.quantity,
                future_bonus_day: if day == config.calendar_days {
                    1
                } else {
                    day + 1
                },
            })
        } else {
            Ok(RetailLoginBonusStatus::Uncollected {
                unknown_a: [0; 4],
                current_item_id: item_id,
                current_item_quantity: config.reward.quantity,
                padding_a: [0; 8],
                current_bonus_day: day,
            })
        }
    }

    async fn retail_login_bonus_claim(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        account_id: AccountId,
    ) -> Result<(), GameRuntimeError> {
        let Some(config) = self.config.login_bonus else {
            return Err(GameRuntimeError::Protocol);
        };
        let server_day = Self::retail_server_day(self.clock.now());
        let calendar_day = u32::try_from(server_day.rem_euclid(i64::from(config.calendar_days)))
            .map_err(|_| GameRuntimeError::Catalog)?
            .saturating_add(1);
        let claim = self
            .repository
            .claim_login_bonus(account_id, server_day, calendar_day, config.reward)
            .await
            .map_err(|_| GameRuntimeError::EconomyPersistence)?;
        let item_id = config.reward.definition.type_id.get();
        if !claim.already_claimed {
            let old = claim
                .quantity_after
                .checked_sub(config.reward.quantity)
                .ok_or(GameRuntimeError::Snapshot)?;
            let unix_time = self
                .clock
                .now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| {
                    u32::try_from(duration.as_secs()).unwrap_or(u32::MAX)
                });
            self.send(
                framed,
                &RetailLoginBonusItemGrant {
                    status_date_unix_time: unix_time,
                    item_id,
                    inventory_slot: u32::try_from(claim.inventory_item_id.get())
                        .map_err(|_| GameRuntimeError::Snapshot)?,
                    quantity_old: old,
                    quantity_new: claim.quantity_after,
                },
            )
            .await?;
        }
        self.send(
            framed,
            &RetailLoginBonusClaimResponse {
                unknown_a: [0; 5],
                current_item_id: item_id,
                current_item_quantity: config.reward.quantity,
                future_item_id: item_id,
                future_item_quantity: config.reward.quantity,
                current_bonus_day: calendar_day,
            },
        )
        .await
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
        my_room_target: &mut Option<AccountId>,
        opcode: u16,
        payload: &[u8],
    ) -> Result<(), GameRuntimeError> {
        let profile = &CompatibilityProfile::US_852;
        let account_id = u32::try_from(identity.account_id.get()).unwrap_or(0);
        match opcode {
            RetailShopJoin::OPCODE => self.send(framed, &RetailShopJoined).await,
            RetailDailyQuestRequest::OPCODE => {
                decode_packet_payload::<RetailDailyQuestRequest>(
                    payload,
                    profile,
                    ServiceKind::Game,
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                let server_time = SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |duration| {
                        u32::try_from(duration.as_secs()).unwrap_or(u32::MAX)
                    });
                self.send(framed, &RetailDailyQuestDelta { server_time })
                    .await?;
                self.send(framed, &RetailDailyQuestState).await
            }
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
                if request.user_id != account_id {
                    return Err(GameRuntimeError::Protocol);
                }
                let target = AccountId::new(i64::from(request.room_user_id))
                    .map_err(|_| GameRuntimeError::Protocol)?;
                // Loading the target snapshot is the authorization/existence check. Do not echo
                // the visitor's own state under the target's name.
                self.repository
                    .load_player_snapshot(target)
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                *my_room_target = Some(target);
                self.send(
                    framed,
                    &RetailMyRoomEntered {
                        user_id: request.room_user_id,
                    },
                )
                .await
            }
            <RetailMascotMessageUpdate as DecodePacket>::OPCODE => {
                let update = decode_packet_payload::<RetailMascotMessageUpdate>(
                    payload,
                    profile,
                    ServiceKind::Game,
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                let mascot_id = InventoryItemId::new(i64::from(update.mascot_id))
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let result = self
                    .repository
                    .save_mascot_message(
                        identity.account_id,
                        MascotMessageUpdate {
                            inventory_item_id: mascot_id,
                            message: update.message.clone(),
                        },
                    )
                    .await;
                let (status, mascot_id, message) = if result.is_ok() {
                    (4, update.mascot_id, update.message)
                } else {
                    // SuperSS sends -1 for both the error status and mascot id.
                    (255, u32::MAX, Vec::new())
                };
                let snapshot = self
                    .repository
                    .load_player_snapshot(identity.account_id)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                self.send(
                    framed,
                    &RetailMascotMessageResult {
                        status,
                        mascot_id,
                        message,
                        pang: snapshot.profile.pang,
                    },
                )
                .await
            }
            0x00b9 => {
                // PacketDoc defines option + inventory slot + trailing option. The upload
                // service/HTTP endpoint is intentionally absent, so answer explicitly rather
                // than inventing an URL or silently dropping the client's request.
                if payload.len() != 6 {
                    return Err(GameRuntimeError::Protocol);
                }
                self.send(framed, &RetailUccUploadKeyRefusal::unsupported())
                    .await
            }
            0x00c9 => {
                // SuperSS's packed request is opt/u32 owner/u8 sequence/i32 item id (10 bytes).
                // We do not issue upload keys without a configured authenticated upload service.
                if payload.len() != 10 {
                    return Err(GameRuntimeError::Protocol);
                }
                self.send(framed, &RetailUccUploadKeyRefusal::unsupported())
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
                let target = my_room_target.unwrap_or(identity.account_id);
                let snapshot = self
                    .repository
                    .load_player_snapshot(target)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                let state = self
                    .repository
                    .load_retail_equipment(target)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                let room = self
                    .repository
                    .load_my_room(target)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                let furniture = room
                    .furniture
                    .into_iter()
                    .map(|entry| RetailMyRoomFurniture {
                        unknown_prefix: entry.unknown_prefix,
                        item_type_id: entry.item_type_id,
                        unknown_suffix: entry.unknown_suffix,
                    })
                    .collect();
                self.send(framed, &RetailMyRoomLayout::new(furniture))
                    .await?;
                let character = retail_character_from_snapshot(&snapshot, &state)?;
                self.send(
                    framed,
                    &RetailPlayerInfo {
                        player: RetailRoomPlayer {
                            connection_id: 0,
                            nickname: snapshot
                                .profile
                                .nickname
                                .clone()
                                .unwrap_or_default()
                                .as_bytes()
                                .to_vec(),
                            slot: 0,
                            character_iff_id: character.iff_id,
                            flags: RoomPlayerFlags::new(false, false),
                            level: 1,
                            user_id: u32::try_from(target.get()).unwrap_or(0),
                            character,
                        },
                    },
                )
                .await?;
                // PacketDoc defines the room visit response through 0x012d and 0x0168 only.
                // Mascot 0x00e2 is emitted by the established 0x0073 update flow below, not as
                // an unsolicited visitor-room projection.
                Ok(())
            }
        }
    }

    async fn handle_retail_room_equipment_update(
        &self,
        _framed: &mut Framed<TcpStream, FrameCodec>,
        state: GameState,
        identity: &RoomIdentity,
        opcode: u16,
        payload: &[u8],
    ) -> Result<bool, GameRuntimeError> {
        let profile = &CompatibilityProfile::US_852;
        let update = if opcode == RETAIL_C2S_EQUIPMENT_LOBBY {
            decode_packet_payload::<RetailLobbyEquipmentUpdate>(payload, profile, ServiceKind::Game)
                .map_err(|_| GameRuntimeError::Protocol)?
                .0
        } else {
            decode_packet_payload::<RetailRoomEquipmentUpdatePacket>(
                payload,
                profile,
                ServiceKind::Game,
            )
            .map_err(|_| GameRuntimeError::Protocol)?
            .0
        };
        let snapshot = self
            .repository
            .load_player_snapshot(identity.account_id)
            .await
            .map_err(|_| GameRuntimeError::Snapshot)?;
        // A replay returns the durable projection but must not fan out a second room event. The
        // wire packet has no operation result bit, so suppress announcements when the requested
        // selection already equals the coherent pre-update projection.
        let should_announce;
        // Room equipment packets have no application operation id. Derive a stable key from the
        // authenticated account and exact frame so transport retries replay the same economy
        // operation instead of incrementing the equipment version again.
        // The exact authenticated frame includes its opcode. Without it, identical lobby and
        // room bodies (for example `0x000b` and `0x000c` character changes) would alias one
        // durable operation and incorrectly suppress the room mutation as a replay.
        let mut operation_scope = Vec::with_capacity(2 + payload.len());
        operation_scope.extend_from_slice(&opcode.to_le_bytes());
        operation_scope.extend_from_slice(payload);
        let mut scope_bytes = [0_u8; 16];
        scope_bytes.copy_from_slice(&Sha256::digest(operation_scope)[..16]);
        let scope = uuid::Uuid::from_bytes(scope_bytes);
        let announce = match update {
            RetailRoomEquipmentUpdate::Caddie(item_id) => {
                let result = self
                    .repository
                    .update_retail_equipment(
                        identity.account_id,
                        retail_equipment_operation_id(
                            identity.account_id,
                            RetailEquipmentSlot::Caddie,
                            item_id,
                            0,
                            scope,
                            0,
                        ),
                        snapshot.equipment.version,
                        RetailEquipmentChange::Caddie(item_id),
                    )
                    .await
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                let (state, announce) = result.into_parts();
                should_announce = announce;
                let (uid, type_id) = state
                    .caddie
                    .map(|(id, type_id)| (u32::try_from(id.get()).unwrap_or(0), type_id))
                    .unwrap_or((0, 0));
                RetailEquipmentAnnounce::Caddie {
                    connection_id: u32::try_from(identity.connection_id.get()).unwrap_or(0),
                    caddie_uid: uid,
                    caddie_type_id: type_id,
                }
            }
            RetailRoomEquipmentUpdate::Ball(ball_type_id) => {
                let ball = snapshot
                    .inventory
                    .iter()
                    .find(|item| item.item_type_id.get() == ball_type_id)
                    .and_then(|item| {
                        self.catalog
                            .item_definition(item.item_type_id)
                            .filter(|d| d.kind == ItemKind::Ball)
                            .map(|definition| EconomyItemSelector {
                                inventory_id: item.id,
                                definition: *definition,
                            })
                    });
                let club = snapshot
                    .equipment
                    .club_item_id
                    .and_then(|id| snapshot.inventory.iter().find(|item| item.id == id))
                    .and_then(|item| {
                        self.catalog
                            .item_definition(item.item_type_id)
                            .filter(|d| d.kind == ItemKind::ClubSet)
                            .map(|definition| EconomyItemSelector {
                                inventory_id: item.id,
                                definition: *definition,
                            })
                    });
                if ball_type_id != 0 && ball.is_none() {
                    return Err(GameRuntimeError::EconomyPersistence);
                }
                let result = self
                    .repository
                    .equip(EquipmentChange {
                        account_id: identity.account_id,
                        operation_id: retail_equipment_operation_id(
                            identity.account_id,
                            RetailEquipmentSlot::Ball,
                            ball_type_id,
                            0,
                            scope,
                            0,
                        ),
                        catalog: self.catalog.fingerprint(),
                        expected_version: snapshot.equipment.version,
                        character_id: snapshot.equipment.character_id,
                        character_type_id: snapshot
                            .characters
                            .iter()
                            .find(|c| c.id == snapshot.equipment.character_id)
                            .map(|c| c.item_type_id)
                            .ok_or(GameRuntimeError::Snapshot)?,
                        club,
                        ball,
                    })
                    .await
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                should_announce = result.was_applied();
                RetailEquipmentAnnounce::Ball {
                    connection_id: u32::try_from(identity.connection_id.get()).unwrap_or(0),
                    ball_type_id,
                }
            }
            RetailRoomEquipmentUpdate::ClubSet(club_item_id) => {
                let club = snapshot
                    .inventory
                    .iter()
                    .find(|item| item.id.get() == i64::from(club_item_id))
                    .and_then(|item| {
                        self.catalog
                            .item_definition(item.item_type_id)
                            .filter(|d| d.kind == ItemKind::ClubSet)
                            .map(|definition| EconomyItemSelector {
                                inventory_id: item.id,
                                definition: *definition,
                            })
                    });
                let ball = snapshot
                    .equipment
                    .ball_item_id
                    .and_then(|id| snapshot.inventory.iter().find(|item| item.id == id))
                    .and_then(|item| {
                        self.catalog
                            .item_definition(item.item_type_id)
                            .filter(|d| d.kind == ItemKind::Ball)
                            .map(|definition| EconomyItemSelector {
                                inventory_id: item.id,
                                definition: *definition,
                            })
                    });
                let club_type_id = club
                    .map(|v| v.definition.type_id.get())
                    .ok_or(GameRuntimeError::EconomyPersistence)?;
                let result = self
                    .repository
                    .equip(EquipmentChange {
                        account_id: identity.account_id,
                        operation_id: retail_equipment_operation_id(
                            identity.account_id,
                            RetailEquipmentSlot::Ball,
                            club_item_id,
                            club_type_id,
                            scope,
                            0,
                        ),
                        catalog: self.catalog.fingerprint(),
                        expected_version: snapshot.equipment.version,
                        character_id: snapshot.equipment.character_id,
                        character_type_id: snapshot
                            .characters
                            .iter()
                            .find(|c| c.id == snapshot.equipment.character_id)
                            .map(|c| c.item_type_id)
                            .ok_or(GameRuntimeError::Snapshot)?,
                        club,
                        ball,
                    })
                    .await
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                should_announce = result.was_applied();
                RetailEquipmentAnnounce::ClubSet {
                    connection_id: u32::try_from(identity.connection_id.get()).unwrap_or(0),
                    club_item_id,
                    club_type_id,
                }
            }
            RetailRoomEquipmentUpdate::Character(character_uid) => {
                let character_id = CharacterId::new(i64::from(character_uid))
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                let character = snapshot
                    .characters
                    .iter()
                    .find(|value| value.id == character_id)
                    .ok_or(GameRuntimeError::EconomyPersistence)?;
                let club = snapshot
                    .equipment
                    .club_item_id
                    .and_then(|id| snapshot.inventory.iter().find(|item| item.id == id))
                    .and_then(|item| {
                        self.catalog
                            .item_definition(item.item_type_id)
                            .filter(|d| d.kind == ItemKind::ClubSet)
                            .map(|definition| EconomyItemSelector {
                                inventory_id: item.id,
                                definition: *definition,
                            })
                    });
                let ball = snapshot
                    .equipment
                    .ball_item_id
                    .and_then(|id| snapshot.inventory.iter().find(|item| item.id == id))
                    .and_then(|item| {
                        self.catalog
                            .item_definition(item.item_type_id)
                            .filter(|d| d.kind == ItemKind::Ball)
                            .map(|definition| EconomyItemSelector {
                                inventory_id: item.id,
                                definition: *definition,
                            })
                    });
                let result = self
                    .repository
                    .equip(EquipmentChange {
                        account_id: identity.account_id,
                        operation_id: retail_equipment_operation_id(
                            identity.account_id,
                            RetailEquipmentSlot::Character,
                            character_uid,
                            0,
                            scope,
                            0,
                        ),
                        catalog: self.catalog.fingerprint(),
                        expected_version: snapshot.equipment.version,
                        character_id,
                        character_type_id: character.item_type_id,
                        club,
                        ball,
                    })
                    .await
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                should_announce = result.was_applied();
                RetailEquipmentAnnounce::Character {
                    connection_id: u32::try_from(identity.connection_id.get()).unwrap_or(0),
                    character_type_id: character.item_type_id.get(),
                    character_uid,
                }
            }
            RetailRoomEquipmentUpdate::UnknownSeven {
                character,
                caddie,
                club_set,
                ball,
            } => {
                let _ = (character, caddie, club_set, ball);
                return Err(GameRuntimeError::Protocol);
            }
        };
        if state == GameState::InRoom && should_announce {
            let _ = self
                .lobby
                .route(
                    identity.connection_id,
                    LobbyRoomCommand::EquipmentAnnounce(announce),
                )
                .await
                .map_err(|_| GameRuntimeError::Protocol)?;
        }
        Ok(should_announce)
    }

    /// Applies a tagged retail `0x0020` update and reports the transaction's stored projection.
    ///
    /// A real client sends this repeatedly in My Room. Every modeled family is ownership-checked
    /// and persisted before the `0x006b` acknowledgement; character and ball continue to use the
    /// proven minimum equipment transaction.
    async fn handle_retail_equipment_update(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        account_id: AccountId,
        payload: &[u8],
    ) -> Result<Option<MemberCard>, GameRuntimeError> {
        let profile = &CompatibilityProfile::US_852;
        let request =
            decode_packet_payload::<RetailEquipmentUpdate>(payload, profile, ServiceKind::Game)
                .map_err(|_| GameRuntimeError::Protocol)?;
        tracing::debug!(
            slot = ?request.slot,
            requested = ?request.requested,
            "retail equipment update decoded"
        );
        let operation_id = retail_equipment_update_operation_id(account_id, payload);
        // Every non-minimum slot goes through one ownership-checked transaction. The returned
        // projection is the acknowledgement, never the packet's untrusted values.
        let persisted = match request.requested {
            RetailEquipmentRequested::CharacterParts(parts) => {
                let character_id = CharacterId::new(i64::from(parts.character_id))
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                let snapshot = self
                    .repository
                    .load_player_snapshot(account_id)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                let character = snapshot
                    .characters
                    .iter()
                    .find(|character| character.id == character_id)
                    .ok_or(GameRuntimeError::EconomyPersistence)?;
                if character.item_type_id.get() != parts.character_type_id {
                    return Err(GameRuntimeError::EconomyPersistence);
                }
                let result = self
                    .repository
                    .update_retail_equipment(
                        account_id,
                        operation_id,
                        snapshot.equipment.version,
                        RetailEquipmentChange::CharacterParts {
                            character_id,
                            type_ids: parts.part_type_ids,
                            inventory_ids: parts.part_uids,
                            hair_color: parts.hair_color,
                        },
                    )
                    .await
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                let (state, _was_applied) = result.into_parts();
                let (durable_id, durable_types, durable_uids) = state
                    .character_parts
                    .filter(|(id, _, _)| *id == character_id)
                    .ok_or(GameRuntimeError::EconomyPersistence)?;
                Some(RetailEquipmentUpdated::CharacterFull(RetailCharacter {
                    iff_id: character.item_type_id.get(),
                    uid: u32::try_from(durable_id.get()).unwrap_or(0),
                    hair_color: u32::from(state.character_hair_color),
                    part_iff_ids: durable_types,
                    part_uids: durable_uids,
                    stats: [0; CHARACTER_STATS],
                    mastery: 0,
                }))
            }
            RetailEquipmentRequested::Caddie(item_id) => {
                let snapshot = self
                    .repository
                    .load_player_snapshot(account_id)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                let result = self
                    .repository
                    .update_retail_equipment(
                        account_id,
                        operation_id,
                        snapshot.equipment.version,
                        RetailEquipmentChange::Caddie(item_id),
                    )
                    .await
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                let (state, _was_applied) = result.into_parts();
                Some(RetailEquipmentUpdated::Caddie {
                    caddie_id: state
                        .caddie
                        .map(|(id, _)| u32::try_from(id.get()).unwrap_or(0))
                        .unwrap_or(0),
                })
            }
            RetailEquipmentRequested::Consumables(values) => {
                let snapshot = self
                    .repository
                    .load_player_snapshot(account_id)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                let result = self
                    .repository
                    .update_retail_equipment(
                        account_id,
                        operation_id,
                        snapshot.equipment.version,
                        RetailEquipmentChange::Consumables(values),
                    )
                    .await
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                let (state, _was_applied) = result.into_parts();
                Some(RetailEquipmentUpdated::Consumables {
                    item_type_ids: state.consumables,
                })
            }
            RetailEquipmentRequested::Decoration(values) => {
                let snapshot = self
                    .repository
                    .load_player_snapshot(account_id)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                let result = self
                    .repository
                    .update_retail_equipment(
                        account_id,
                        operation_id,
                        snapshot.equipment.version,
                        RetailEquipmentChange::Decoration(values),
                    )
                    .await
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                let (state, _was_applied) = result.into_parts();
                Some(RetailEquipmentUpdated::Decoration {
                    type_ids: state.decoration,
                })
            }
            RetailEquipmentRequested::Mascot(item_id) => {
                let snapshot = self
                    .repository
                    .load_player_snapshot(account_id)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                let _state = self
                    .repository
                    .update_retail_equipment(
                        account_id,
                        operation_id,
                        snapshot.equipment.version,
                        RetailEquipmentChange::Mascot(item_id),
                    )
                    .await
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                Some(RetailEquipmentUpdated::Mascot { data: [0; 62] })
            }
            RetailEquipmentRequested::CutIn { character_id, data } => {
                let snapshot = self
                    .repository
                    .load_player_snapshot(account_id)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                let character_id = CharacterId::new(i64::from(character_id))
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                if !snapshot
                    .characters
                    .iter()
                    .any(|character| character.id == character_id)
                {
                    return Err(GameRuntimeError::EconomyPersistence);
                }
                let result = self
                    .repository
                    .update_retail_equipment(
                        account_id,
                        operation_id,
                        snapshot.equipment.version,
                        RetailEquipmentChange::CutIn { character_id, data },
                    )
                    .await
                    .map_err(|_| GameRuntimeError::EconomyPersistence)?;
                let (state, _was_applied) = result.into_parts();
                let (durable_character_id, durable_data) =
                    state.cut_in.ok_or(GameRuntimeError::EconomyPersistence)?;
                Some(RetailEquipmentUpdated::CutIn {
                    character_id: u32::try_from(durable_character_id.get()).unwrap_or(0),
                    data: durable_data,
                })
            }
            _ => None,
        };
        if let Some(reply) = persisted {
            self.send(framed, &reply).await?;
            return Ok(None);
        }
        let reply = match request.requested {
            RetailEquipmentRequested::CharacterParts(_)
            | RetailEquipmentRequested::Mascot(_)
            | RetailEquipmentRequested::CutIn { .. } => return Ok(None),
            RetailEquipmentRequested::Caddie(_) => RetailEquipmentUpdated::Caddie { caddie_id: 0 },
            RetailEquipmentRequested::Consumables(_) => RetailEquipmentUpdated::Consumables {
                item_type_ids: [0; pangya_protocol::RETAIL_CONSUMABLE_SLOTS],
            },
            RetailEquipmentRequested::Decoration(_) => {
                RetailEquipmentUpdated::Decoration { type_ids: [0; 6] }
            }
            RetailEquipmentRequested::BallAndClub {
                ball_type_id,
                club_item_id: _,
            }
            | RetailEquipmentRequested::Character(ball_type_id) => {
                let Some(economy) = self.config.economy else {
                    return Err(GameRuntimeError::Catalog);
                };
                let mut snapshot = self
                    .repository
                    .load_player_snapshot(account_id)
                    .await
                    .map_err(|_| GameRuntimeError::Snapshot)?;
                let mut character_id = snapshot.equipment.character_id;
                let mut ball_item_id = snapshot.equipment.ball_item_id;
                let mut club_item_id = snapshot.equipment.club_item_id;
                let mut valid = true;
                let requested_aux = match request.requested {
                    RetailEquipmentRequested::BallAndClub {
                        ball_type_id: 0,
                        club_item_id: raw_club,
                    } => {
                        ball_item_id = None;
                        club_item_id = InventoryItemId::new(i64::from(raw_club)).ok();
                        raw_club
                    }
                    RetailEquipmentRequested::BallAndClub {
                        ball_type_id: type_id,
                        club_item_id: raw_club,
                    } => {
                        ball_item_id = snapshot
                            .inventory
                            .iter()
                            .find(|item| {
                                item.item_type_id.get() == type_id
                                    && self
                                        .catalog
                                        .item_definition(item.item_type_id)
                                        .is_some_and(|definition| definition.kind == ItemKind::Ball)
                            })
                            .map(|item| item.id);
                        club_item_id = InventoryItemId::new(i64::from(raw_club)).ok();
                        valid = ball_item_id.is_some();
                        raw_club
                    }
                    RetailEquipmentRequested::Character(raw_id) => {
                        if let Ok(requested) = CharacterId::new(i64::from(raw_id)) {
                            character_id = requested;
                            valid = snapshot
                                .characters
                                .iter()
                                .any(|character| character.id == character_id);
                        } else {
                            valid = false;
                        }
                        0
                    }
                    _ => return Err(GameRuntimeError::Protocol),
                };

                let selector = |id: Option<InventoryItemId>, expected: ItemKind| {
                    id.and_then(|inventory_id| {
                        snapshot
                            .inventory
                            .iter()
                            .find(|item| item.id == inventory_id)
                            .and_then(|item| {
                                self.catalog.item_definition(item.item_type_id).and_then(
                                    |definition| {
                                        (definition.kind == expected).then_some(
                                            EconomyItemSelector {
                                                inventory_id,
                                                definition: *definition,
                                            },
                                        )
                                    },
                                )
                            })
                    })
                };
                let club = selector(club_item_id, ItemKind::ClubSet);
                let ball = selector(ball_item_id, ItemKind::Ball);
                if matches!(
                    request.requested,
                    RetailEquipmentRequested::BallAndClub { .. }
                ) {
                    // Zero clears a slot; a nonzero club row must be an owned catalog club set.
                    valid &= requested_aux == 0 || club.is_some();
                }
                if !valid {
                    return Err(GameRuntimeError::EconomyPersistence);
                }
                let changed = match request.requested {
                    RetailEquipmentRequested::BallAndClub { .. } => {
                        ball_item_id != snapshot.equipment.ball_item_id
                            || club_item_id != snapshot.equipment.club_item_id
                    }
                    RetailEquipmentRequested::Character(_) => {
                        character_id != snapshot.equipment.character_id
                    }
                    _ => false,
                };

                if valid && changed {
                    let character = snapshot
                        .characters
                        .iter()
                        .find(|value| value.id == character_id)
                        .ok_or(GameRuntimeError::Snapshot)?;
                    let operation_id = retail_equipment_operation_id(
                        account_id,
                        request.slot,
                        ball_type_id,
                        requested_aux,
                        operation_id.get(),
                        0,
                    );
                    match timeout(
                        economy.command_timeout,
                        self.repository.equip(EquipmentChange {
                            account_id,
                            operation_id,
                            catalog: self.catalog.fingerprint(),
                            expected_version: snapshot.equipment.version,
                            character_id,
                            character_type_id: character.item_type_id,
                            club,
                            ball,
                        }),
                    )
                    .await
                    {
                        Err(_) => return Err(GameRuntimeError::Timeout),
                        Ok(Err(
                            EconomyError::ArithmeticOverflow
                            | EconomyError::CorruptData
                            | EconomyError::Storage(_),
                        )) => return Err(GameRuntimeError::EconomyPersistence),
                        Ok(Err(error)) => {
                            tracing::debug!(%error, "retail equipment update reverted");
                        }
                        Ok(Ok(EconomyCommit::Committed(_) | EconomyCommit::Replayed(_))) => {
                            snapshot = self
                                .repository
                                .load_player_snapshot(account_id)
                                .await
                                .map_err(|_| GameRuntimeError::Snapshot)?;
                        }
                    }
                } else if !valid {
                    tracing::debug!(
                        ball_type_id,
                        requested_aux,
                        "retail equipment item is not owned"
                    );
                }

                if request.slot == RetailEquipmentSlot::Character {
                    RetailEquipmentUpdated::Character {
                        character_id: u32::try_from(snapshot.equipment.character_id.get())
                            .unwrap_or(0),
                    }
                } else {
                    let ball_type_id = snapshot
                        .equipment
                        .ball_item_id
                        .and_then(|id| {
                            snapshot
                                .inventory
                                .iter()
                                .find(|item| item.id == id)
                                .map(|item| item.item_type_id.get())
                        })
                        .unwrap_or(0);
                    let club_item_id = snapshot
                        .equipment
                        .club_item_id
                        .and_then(|id| u32::try_from(id.get()).ok())
                        .unwrap_or(0);
                    let reply = RetailEquipmentUpdated::BallAndClub {
                        ball_type_id,
                        club_item_id,
                    };
                    self.send(framed, &reply).await?;
                    return Ok(Some(member_card(&snapshot)));
                }
            }
        };
        self.send(framed, &reply).await?;
        if matches!(request.slot, RetailEquipmentSlot::Character) {
            let snapshot = self
                .repository
                .load_player_snapshot(account_id)
                .await
                .map_err(|_| GameRuntimeError::Snapshot)?;
            Ok(Some(member_card(&snapshot)))
        } else {
            Ok(None)
        }
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
        purchase_scope: uuid::Uuid,
        purchase_sequence: u64,
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
        for (line_index, item) in request.items.iter().enumerate() {
            if item.quantity == 0 || item.quantity > economy.max_purchase_quantity {
                tracing::debug!(
                    stage = "quantity",
                    quantity = item.quantity,
                    "retail purchase refused"
                );
                return self.refuse_retail_purchase(framed, account_id).await;
            }
            let Some(definition) = self.resolve_offer(ItemTypeId::new(item.item_type_id)) else {
                // The item id is catalog data, not player data, and without it a refusal is
                // indistinguishable from a pricing bug.
                tracing::debug!(
                    stage = "not_in_catalog",
                    item_type_id = item.item_type_id,
                    shop_offers = self.resolved_offers().len(),
                    "retail purchase refused"
                );
                return self.refuse_retail_purchase(framed, account_id).await;
            };
            // Retail balls are sold as packs: the packet quantity is the pack's displayed ball
            // count (for example 50), while the durable inventory/equipment model owns one ball
            // definition. Other kinds use the packet quantity as their purchase quantity.
            let committed_quantity = if matches!(
                (definition.kind, definition.stacking),
                (ItemKind::Ball, pangya_domain::ItemStacking::Unique)
            ) {
                1
            } else {
                item.quantity
            };
            // These client assertions are safe to log and are indispensable when a retail row's
            // package semantics disagree with the server definition.
            tracing::debug!(
                item_type_id = item.item_type_id,
                retail_quantity = item.quantity,
                committed_quantity,
                claimed_cost_pang = item.claimed_cost_pang,
                claimed_cost_point = item.claimed_cost_point,
                kind = ?definition.kind,
                stacking = ?definition.stacking,
                "retail purchase line decoded"
            );
            // The scoped operation id makes an exact encrypted-frame replay reuse its commit while
            // a later intentional purchase with a new client salt receives a new sequence.
            let operation_id = retail_purchase_operation_id(
                account_id,
                item,
                purchase_scope,
                purchase_sequence,
                line_index,
            );
            let committed = timeout(
                economy.command_timeout,
                self.repository.purchase(PurchaseRequest {
                    account_id,
                    operation_id,
                    catalog: self.catalog.fingerprint(),
                    definition,
                    quantity: committed_quantity,
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
                            .saturating_mul(u64::from(committed_quantity)),
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

    /// Rebuilds every process-local public projection after a durable economy mutation.
    ///
    /// The room actor owns immutable member identities, so updating only the social card leaves
    /// `0x0048` and `0x0076` stale until relog. Load one bounded coherent snapshot, update social
    /// state, and replace the room member in actor order; a room update is also broadcast as the
    /// next census to every current client.
    async fn refresh_social_projection(
        &self,
        connection_id: PlayerConnectionId,
        account_id: AccountId,
    ) -> Result<PlayerSnapshot, GameRuntimeError> {
        let snapshot = self
            .repository
            .load_player_snapshot(account_id)
            .await
            .map_err(|_| GameRuntimeError::Snapshot)?;
        let card = member_card(&snapshot);
        let character = snapshot
            .characters
            .iter()
            .find(|value| value.id == snapshot.equipment.character_id);
        self.social.update_card(connection_id, card.clone());
        // Synthetic M5/M6 settlement also reloads this helper's durable snapshot, but its
        // generated terminal contract has no retail census/projection frame. Keep the social card
        // coherent for internal state while limiting room events to retail sessions.
        if self.config.retail_bootstrap {
            // Not being in a room is the normal path for a shop mutation; do not turn that into a
            // failed purchase. Once admitted, the actor serializes this replacement with room
            // commands and publishes one coherent snapshot to all members.
            match self
                .lobby
                .route(
                    connection_id,
                    LobbyRoomCommand::UpdateMemberProjection {
                        card,
                        character_id: Some(snapshot.equipment.character_id),
                        character_iff_id: character.map(|value| value.item_type_id.get()),
                    },
                )
                .await
            {
                Ok(LobbyRouteResult::Snapshot(_))
                | Err(RoomError::NotMember | RoomError::RoomNotFound) => {}
                Ok(_) => return Err(GameRuntimeError::Protocol),
                Err(_) => return Err(GameRuntimeError::Protocol),
            }
        }
        Ok(snapshot)
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
    async fn retail_room_list(&self, channel: u8) -> Result<Vec<RetailRoom>, GameRuntimeError> {
        let summaries = self
            .lobby
            .list_on_channel(channel)
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
        outbound: RoomOutbound,
        terminal_outbound: TerminalOutboxSender,
        room_cancellation: CancellationToken,
        channel: u8,
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
                let rooms = self.retail_room_list(channel).await?;
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
            (GameState::InChannel, RETAIL_C2S_GM_ENTER_ROOM) => {
                if !payload.is_empty() {
                    return Err(GameRuntimeError::Protocol);
                }
                self.observer.unknown(GameUnknownObservation::Ignored);
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
                // Preserve the complete client-selected room shape. The match plan is derived
                // from this profile when Start is accepted; no configured-course fallback may
                // overwrite the requested course or hole card.
                if !matches!(request.hole_count, 1 | 3 | 6 | 9 | 18)
                    || !is_retail_course_value(request.course)
                    || !retail_room_create_has_valid_timers(&request)
                {
                    return self.reject_retail_join(framed, state).await;
                }
                let settings = settings.with_profile(RoomProfile {
                    mode: request.room_type,
                    course: request.course,
                    hole_count: request.hole_count,
                    hole_progression: 0,
                    shot_timer_ms: request.shot_timer_ms,
                    game_timer_ms: request.game_timer_ms,
                    artifact_id: 0,
                    natural_wind: false,
                });
                let created = self
                    .lobby
                    .create_on_channel_with_terminal_outbox(
                        name,
                        password,
                        settings,
                        identity.clone(),
                        channel,
                        outbound,
                        terminal_outbound.clone(),
                        room_cancellation,
                    )
                    .await;
                match created {
                    Ok(summary) => {
                        *room_id = Some(summary.id());
                        self.social
                            .set_room(identity.connection_id, Some(summary.id()));
                        self.observer.room(GameRoomObservation::Created);
                        // The number is on the room's own header in the client, and an
                        // operator driving a second seat into it has otherwise no way to learn
                        // it: the room directory does not list rooms yet. It is a room number,
                        // not an identity — nothing secret travels here.
                        tracing::info!(room = summary.id().get(), "retail room created");
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
                    .join_on_channel_with_terminal_outbox(
                        target,
                        identity.clone(),
                        password,
                        channel,
                        outbound,
                        terminal_outbound.clone(),
                        room_cancellation,
                    )
                    .await;
                match joined {
                    Ok(snapshot) => {
                        *room_id = Some(snapshot.summary().id());
                        self.social
                            .set_room(identity.connection_id, Some(snapshot.summary().id()));
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
                // The room actor broadcasts the post-ready census to every member.
                let _ = snapshot;
                Ok(state)
            }
            (GameState::InRoom, RETAIL_C2S_ROOM_EDIT) => {
                let request = decode_packet_payload::<RetailRoomSettingsUpdate>(
                    payload,
                    profile,
                    ServiceKind::Game,
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                let routed = self
                    .lobby
                    .route(identity.connection_id, LobbyRoomCommand::GetState)
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let LobbyRouteResult::Snapshot(snapshot) = routed else {
                    return Err(GameRuntimeError::Protocol);
                };
                let current = snapshot.summary().profile();
                let mut profile_update = current;
                let mut capacity = snapshot.summary().max_members();
                let mut name = None;
                let mut password = None;
                for change in request.changes {
                    match change {
                        RetailRoomSettingChange::Name(value) => {
                            let text = std::str::from_utf8(&value)
                                .map_err(|_| GameRuntimeError::Protocol)?;
                            name = Some(
                                RoomName::parse(text).map_err(|_| GameRuntimeError::Protocol)?,
                            );
                        }
                        RetailRoomSettingChange::Password(value) => {
                            password = if value.is_empty() {
                                Some(None)
                            } else {
                                let text = std::str::from_utf8(&value)
                                    .map_err(|_| GameRuntimeError::Protocol)?;
                                Some(Some(
                                    RoomPassword::parse(text)
                                        .map_err(|_| GameRuntimeError::Protocol)?,
                                ))
                            };
                        }
                        RetailRoomSettingChange::Mode(mode) => profile_update.mode = mode as u8,
                        RetailRoomSettingChange::Course(course) => profile_update.course = course,
                        RetailRoomSettingChange::HoleCount(count) => {
                            profile_update.hole_count = count
                        }
                        RetailRoomSettingChange::HoleProgression(progression) => {
                            profile_update.hole_progression = progression as u8
                        }
                        RetailRoomSettingChange::ShotTimerSeconds(seconds) => {
                            profile_update.shot_timer_ms = u32::from(seconds) * 1000
                        }
                        RetailRoomSettingChange::PlayerCount(count) => capacity = count,
                        RetailRoomSettingChange::GameTimerMinutes(minutes) => {
                            profile_update.game_timer_ms = u32::from(minutes) * 60_000
                        }
                        RetailRoomSettingChange::NaturalWind(enabled) => {
                            profile_update.natural_wind = enabled
                        }
                        RetailRoomSettingChange::StateAfk(_is_afk) => {
                            // SuperSS-Dev room.cpp:1450-1535 stores STATE_FLAG in its room
                            // aggregate, but this service has no corresponding aggregate field
                            // or authoritative response shape. Observe and ignore it rather than
                            // disconnecting the retail client or claiming the state was applied.
                            self.observer.unknown(GameUnknownObservation::Ignored);
                            return Ok(state);
                        }
                        RetailRoomSettingChange::Artifact(_artifact_id) => {
                            // PacketDoc carries the catalog id, but the checked gameplay
                            // references provide no authoritative effect or reward semantics.
                            // Refuse it without mutating the room rather than advertising a
                            // cosmetic setting the match would silently discard.
                            self.observer.unknown(GameUnknownObservation::Ignored);
                            return Ok(state);
                        }
                        RetailRoomSettingChange::RepeatHole(_)
                        | RetailRoomSettingChange::FixedRepeatHole(_) => {
                            // No checked reference defines these tags or their reward/card
                            // semantics. Refuse deterministically by leaving the room unchanged;
                            // do not disconnect a real client for an unsupported optional edit.
                            self.observer.unknown(GameUnknownObservation::Ignored);
                            return Ok(state);
                        }
                    }
                }
                if !matches!(profile_update.hole_count, 1 | 3 | 6 | 9 | 18)
                    || !is_retail_course_value(profile_update.course)
                    || !retail_room_has_valid_timers(
                        profile_update.mode,
                        profile_update.shot_timer_ms,
                        profile_update.game_timer_ms,
                    )
                {
                    return Err(GameRuntimeError::Protocol);
                }
                let settings = RoomSettings::new(capacity)
                    .map_err(|_| GameRuntimeError::Protocol)?
                    .with_profile(profile_update);
                let routed = self
                    .lobby
                    .route(
                        identity.connection_id,
                        LobbyRoomCommand::UpdateRoom {
                            settings,
                            name,
                            password,
                        },
                    )
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let LobbyRouteResult::Snapshot(snapshot) = routed else {
                    return Err(GameRuntimeError::Protocol);
                };
                // The room actor broadcasts the post-settings census to every member.
                let _ = snapshot;
                Ok(state)
            }
            (GameState::InRoom, RETAIL_C2S_TEAM_CHANGE) => {
                let request =
                    decode_packet_payload::<RetailTeamChange>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let routed = self
                    .lobby
                    .route(
                        identity.connection_id,
                        LobbyRoomCommand::ChangeTeam(request.team as u8),
                    )
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let LobbyRouteResult::Snapshot(snapshot) = routed else {
                    return Err(GameRuntimeError::Protocol);
                };
                // TeamChanged and the resulting census are both broadcast by the room actor;
                // do not echo either frame directly to the requester.
                let _ = snapshot;
                Ok(state)
            }
            (GameState::InRoom, RETAIL_C2S_ROOM_RESYNC) => {
                let _request =
                    decode_packet_payload::<RetailRoomResync>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                self.send_retail_census(framed, identity.connection_id)
                    .await?;
                Ok(state)
            }
            (GameState::InChannel, RETAIL_C2S_REJOIN_INVITED) => {
                let target = self
                    .pending_invites
                    .lock()
                    .ok()
                    .and_then(|mut pending| pending.remove(&identity.account_id));
                let Some(target) = target else {
                    return Err(GameRuntimeError::Protocol);
                };
                let snapshot = self
                    .lobby
                    .join_with_terminal_outbox(
                        target,
                        identity.clone(),
                        None,
                        outbound,
                        terminal_outbound.clone(),
                        room_cancellation,
                    )
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                *room_id = Some(target);
                self.send(
                    framed,
                    &RetailRoomJoinResult::Accepted(Box::new(retail_room_from_snapshot(&snapshot))),
                )
                .await?;
                Ok(GameState::InRoom)
            }
            (GameState::InChannel, RETAIL_C2S_ROOM_INFO) => {
                let request = decode_packet_payload::<RetailRoomInformationRequest>(
                    payload,
                    profile,
                    ServiceKind::Game,
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                let room_id = RoomId::new(u32::from(request.room_number))
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let snapshot = self
                    .lobby
                    .room_info(room_id)
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let players = snapshot
                    .members()
                    .iter()
                    .map(retail_room_information_user)
                    .collect();
                self.send(framed, &RetailRoomInformationResponse { players })
                    .await?;
                Ok(state)
            }
            (GameState::InRoom, RETAIL_C2S_ROOM_INFO) => {
                let _request = decode_packet_payload::<RetailRoomInformationRequest>(
                    payload,
                    profile,
                    ServiceKind::Game,
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                let routed = self
                    .lobby
                    .route(identity.connection_id, LobbyRoomCommand::GetState)
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let LobbyRouteResult::Snapshot(snapshot) = routed else {
                    return Err(GameRuntimeError::Protocol);
                };
                let players = snapshot
                    .members()
                    .iter()
                    .map(retail_room_information_user)
                    .collect();
                self.send(framed, &RetailRoomInformationResponse { players })
                    .await?;
                Ok(state)
            }
            (GameState::InRoom, RETAIL_C2S_ROOM_KICK) => {
                let request =
                    decode_packet_payload::<RetailRoomKick>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let Ok(target) = PlayerConnectionId::new(u64::from(request.connection_id)) else {
                    return Err(GameRuntimeError::Protocol);
                };
                let routed = self
                    .lobby
                    .route(identity.connection_id, LobbyRoomCommand::Kick(target))
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let LobbyRouteResult::Snapshot(snapshot) = routed else {
                    return Err(GameRuntimeError::Protocol);
                };
                // The room actor broadcasts the post-kick census to the surviving members;
                // the removed member receives the 0x004c leave acknowledgement from its own
                // Kicked event. Do not echo a second status/census from the requester path.
                let _ = snapshot;
                Ok(state)
            }
            (GameState::InRoom, RETAIL_C2S_ROOM_INVITE_INFO) => {
                let request = decode_packet_payload::<RetailRoomInviteInfo>(
                    payload,
                    profile,
                    ServiceKind::Game,
                )
                .map_err(|_| GameRuntimeError::Protocol)?;
                self.lobby
                    .route(identity.connection_id, LobbyRoomCommand::GetState)
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                // 0x0029 is the invitee lookup leg. It only receives the 0x0130
                // acknowledgement; the actual invitation is sent by the paired 0x00ba frame.
                self.send(
                    framed,
                    &RetailRoomInviteInfoResponse {
                        account_id: request.account_id,
                    },
                )
                .await?;
                Ok(state)
            }
            (GameState::InRoom, RETAIL_C2S_ROOM_INVITE) => {
                let request =
                    decode_packet_payload::<RetailRoomInvite>(payload, profile, ServiceKind::Game)
                        .map_err(|_| GameRuntimeError::Protocol)?;
                let routed = self
                    .lobby
                    .route(identity.connection_id, LobbyRoomCommand::GetState)
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                let LobbyRouteResult::Snapshot(snapshot) = routed else {
                    return Err(GameRuntimeError::Protocol);
                };
                let room_id = snapshot.summary().id();
                self.deliver_retail_invite(
                    room_id,
                    identity,
                    AccountId::new(i64::from(request.account_id))
                        .map_err(|_| GameRuntimeError::Protocol)?,
                );
                let nickname = identity.nickname.display().as_bytes().to_vec();
                self.send(
                    framed,
                    &RetailRoomInviteResponse {
                        server_id: 0,
                        channel_id: 0,
                        room_id: u16::try_from(room_id.get()).unwrap_or(u16::MAX),
                        inviter_id: u32::try_from(identity.account_id.get()).unwrap_or(u32::MAX),
                        inviter_nickname: nickname,
                        invitee_id: request.account_id,
                    },
                )
                .await?;
                Ok(state)
            }
            (GameState::InRoom, RETAIL_C2S_GM_ENTER_ROOM | RETAIL_C2S_REJOIN_INVITED) => {
                // 0x003e is a privileged observer entry and 0x00b4 is a reference stub. No
                // unauthenticated elevation or guessed rejoin body is safe; consume only an
                // empty frame and keep this connection alive for the client-safe no-op.
                if !payload.is_empty() {
                    return Err(GameRuntimeError::Protocol);
                }
                self.observer.unknown(GameUnknownObservation::Ignored);
                Ok(state)
            }
            (GameState::InRoom, RETAIL_C2S_LOUNGE_ACTION) => {
                // The room UI emits this for avatar-stage clicks. Pangbox relays rotations as
                // `0x00c4`; ignoring the cosmetic action is honest until that projection exists,
                // and prevents one missed Start-button click from becoming a protocol disconnect
                // (`pangbox/server`, `game/server/conn.go:224-231`).
                self.observer.unknown(GameUnknownObservation::Ignored);
                Ok(state)
            }
            (GameState::InRoom, RETAIL_C2S_ROOM_LEAVE) => {
                self.lobby
                    .leave(identity.connection_id)
                    .await
                    .map_err(|_| GameRuntimeError::Protocol)?;
                *room_id = None;
                self.social.set_room(identity.connection_id, None);
                self.observer.room(GameRoomObservation::Left);
                self.send(framed, &RetailRoomLeave::to_lobby()).await?;
                self.send_retail_room_list(framed, channel).await?;
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
        channel: u8,
    ) -> Result<(), GameRuntimeError> {
        let rooms = self
            .lobby
            .list_on_channel(channel)
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
        connection_id: PlayerConnectionId,
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
        let retail_state = self
            .repository
            .load_retail_equipment(snapshot.account.id)
            .await
            .map_err(|_| GameRuntimeError::Snapshot)?;
        let equipment = RetailEquipment {
            caddie_uid: retail_state
                .caddie
                .map(|(id, _)| narrow(id.get()))
                .transpose()?
                .unwrap_or(0),
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
            item_iff_ids: retail_state.consumables,
            decoration_slots: retail_state.decoration_slots,
            decoration_iff_ids: retail_state.decoration,
            furniture_ids: [0; 2],
        };
        let reply = HandoverReply {
            server_name: b"pangya-rs".to_vec(),
            player: RetailPlayerData {
                identity: RetailPlayerIdentity {
                    username: snapshot.account.username_display.as_bytes().to_vec(),
                    nickname: snapshot
                        .profile
                        .nickname
                        .as_deref()
                        .ok_or(GameRuntimeError::Snapshot)?
                        .as_bytes()
                        .to_vec(),
                    // The client matches this against the connection id on every room census
                    // record to find its own. Told zero, it never recognises itself, so it
                    // never learns it is the room master and its Start button stays an inert
                    // Ready.
                    connection_id: u32::try_from(connection_id.get()).unwrap_or(0),
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
                    hair_color: u32::from(retail_state.character_hair_color),
                    part_iff_ids: retail_state
                        .character_parts
                        .filter(|(id, _, _)| *id == character.id)
                        .map(|(_, types, _)| types)
                        .unwrap_or([0; CHARACTER_PARTS]),
                    part_uids: retail_state
                        .character_parts
                        .filter(|(id, _, _)| *id == character.id)
                        .map(|(_, _, ids)| ids)
                        .unwrap_or([0; CHARACTER_PARTS]),
                    stats: [0; CHARACTER_STATS],
                    mastery: 0,
                },
                caddie: retail_state
                    .caddie
                    .map(|(id, type_id)| RetailCaddie {
                        uid: narrow(id.get()).unwrap_or(0),
                        iff_id: type_id,
                        level: 0,
                        experience: 0,
                    })
                    .unwrap_or_default(),
                club_set_iff_id: club_set_iff_id(snapshot),
            },
            server_time: retail_now(),
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
            let class = match self
                .catalog
                .item_definition(item.item_type_id)
                .map(|definition| definition.kind)
            {
                Some(ItemKind::ClubSet | ItemKind::Ball) => RetailInventoryClass::Equipment,
                Some(ItemKind::Consumable) => RetailInventoryClass::Consumable,
                Some(ItemKind::CharacterPart) => RetailInventoryClass::Miscellaneous,
                // Everything the widened catalog added is Miscellaneous on the wire: the retail
                // class byte only separates equipment from consumables from the rest, and none
                // of these are equipment or consumables as the client counts them.
                Some(
                    ItemKind::Caddie
                    | ItemKind::CaddieItem
                    | ItemKind::Mascot
                    | ItemKind::Card
                    | ItemKind::Furniture
                    | ItemKind::Skin
                    | ItemKind::HairStyle
                    | ItemKind::SetItem,
                ) => RetailInventoryClass::Miscellaneous,
                Some(ItemKind::Character) | None => return Err(GameRuntimeError::Catalog),
            };
            let mut writer = PacketWriter::default();
            RetailInventoryItem {
                item_id: narrow(item.id.get())?,
                item_type_id: item.item_type_id.get(),
                quantity: item.quantity,
                class,
            }
            .encode_body(&mut writer);
            inventory.push(writer.into_inner());
        }
        self.send_container(framed, IffContainerKind::Inventory, inventory)
            .await?;

        self.send(
            framed,
            &ServerChannelList {
                channels: self
                    .config
                    .advertised_channel_ids
                    .iter()
                    .map(|&id| RetailChannel {
                        name: b"pangya-rs".to_vec(),
                        capacity: 200,
                        player_count: 0,
                        id: u16::from(id),
                        restrictions: 0,
                    })
                    .collect(),
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

fn is_gm_opcode(opcode: u16) -> bool {
    matches!(opcode, 0x003e | 0x0041 | 0x0057 | 0x0060 | 0x0061 | 0x008f)
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
        // This is the *synthetic* M7 shop's own kind vocabulary, which predates the widened
        // families and has no value for them. That path is local-protocol only — the retail
        // client never reaches it — so the widened families are simply not offered there rather
        // than being mapped onto a kind that means something else.
        ItemKind::Caddie
        | ItemKind::CaddieItem
        | ItemKind::Mascot
        | ItemKind::Card
        | ItemKind::Furniture
        | ItemKind::Skin
        | ItemKind::HairStyle
        | ItemKind::SetItem
        | ItemKind::Character => return Err(GameRuntimeError::Catalog),
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

/// Everything about the hole a retail client is about to load, other than who is in it.
#[derive(Clone, Copy, Debug)]
struct RetailHoleIntro {
    course_id: u32,
    hole_count: u8,
    hole_mode: u8,
    weather: pangya_domain::Weather,
    seed: pangya_domain::MatchSeed,
    natural_wind: bool,
    shot_timer: Duration,
    game_timer: Duration,
}

/// Projects the part of an authenticated player that the rest of a room is allowed to see.
fn member_card(snapshot: &PlayerSnapshot) -> MemberCard {
    let character = snapshot
        .characters
        .iter()
        .find(|value| value.id == snapshot.equipment.character_id);
    MemberCard {
        username: snapshot.account.username_display.clone(),
        character_iff_id: character.map(|value| value.item_type_id.get()).unwrap_or(0),
        character_uid: character
            .and_then(|value| u32::try_from(value.id.get()).ok())
            .unwrap_or(0),
        // No caddie is equippable yet, so the roster reports none rather than inventing one.
        caddie_uid: 0,
        club_set_uid: snapshot
            .equipment
            .club_item_id
            .and_then(|id| u32::try_from(id.get()).ok())
            .unwrap_or(0),
        club_set_iff_id: club_set_iff_id(snapshot),
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
        experience: u32::try_from(snapshot.profile.experience).unwrap_or(u32::MAX),
        pang: snapshot.profile.pang,
    }
}

/// Rebuilds one player's record for a match roster from what the room holds of them.
///
/// The client reads the same record here as in the handover reply that admitted it, so a
/// roster that describes players any less completely is one it cannot build models from.
/// The catalog id of a player's equipped club set.
///
/// Zero when nothing is equipped, which is what a player with no clubs honestly has.
fn club_set_iff_id(snapshot: &PlayerSnapshot) -> u32 {
    snapshot
        .equipment
        .club_item_id
        .and_then(|id| {
            snapshot
                .inventory
                .iter()
                .find(|item| item.id == id)
                .map(|item| item.item_type_id.get())
        })
        .unwrap_or(0)
}

/// The current wall clock, packed the way the client reads it.
///
/// Anything the clock cannot supply packs as the Unix epoch rather than as zeros: a zeroed
/// block names no date — month zero and day zero exist on no calendar — and the client cannot
/// convert one back into a time.
fn retail_now() -> [u8; 16] {
    let since_epoch = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    packed_system_time(
        i64::try_from(since_epoch.as_secs()).unwrap_or(0),
        u16::try_from(since_epoch.subsec_millis()).unwrap_or(0),
    )
}

fn retail_character_from_snapshot(
    snapshot: &PlayerSnapshot,
    state: &RetailEquipmentState,
) -> Result<RetailCharacter, GameRuntimeError> {
    let character = snapshot
        .characters
        .iter()
        .find(|character| character.id == snapshot.equipment.character_id)
        .ok_or(GameRuntimeError::Snapshot)?;
    let (part_iff_ids, part_uids) = state
        .character_parts
        .filter(|(id, _, _)| *id == character.id)
        .map(|(_, types, ids)| (types, ids))
        .unwrap_or(([0; CHARACTER_PARTS], [0; CHARACTER_PARTS]));
    Ok(RetailCharacter {
        iff_id: character.item_type_id.get(),
        uid: u32::try_from(character.id.get()).map_err(|_| GameRuntimeError::Snapshot)?,
        hair_color: u32::from(state.character_hair_color),
        part_iff_ids,
        part_uids,
        stats: [0; CHARACTER_STATS],
        mastery: 0,
    })
}

fn retail_match_player(slot: usize, member: &MemberSnapshot) -> RetailMatchPlayer {
    let card = member.card();
    let start_time = retail_now();
    RetailMatchPlayer {
        slot: u16::try_from(slot.saturating_add(1)).unwrap_or(u16::MAX),
        player: RetailPlayerData {
            identity: RetailPlayerIdentity {
                username: card.username.as_bytes().to_vec(),
                nickname: member.nickname().as_bytes().to_vec(),
                connection_id: u32::try_from(member.connection_id().get()).unwrap_or(0),
                user_id: u32::try_from(member.account_id().get()).unwrap_or(0),
            },
            statistics: RetailPlayerStatistics {
                experience: card.experience,
                pang: card.pang,
                ..RetailPlayerStatistics::default()
            },
            equipment: RetailEquipment {
                caddie_uid: card.caddie_uid,
                character_uid: card.character_uid,
                club_set_uid: card.club_set_uid,
                comet_iff_id: card.comet_iff_id,
                item_iff_ids: [0; EQUIPPED_ITEM_SLOTS],
                decoration_slots: [0; 6],
                decoration_iff_ids: [0; 6],
                furniture_ids: [0; 2],
            },
            character: RetailCharacter {
                iff_id: card.character_iff_id,
                uid: card.character_uid,
                hair_color: 0,
                part_iff_ids: [0; CHARACTER_PARTS],
                part_uids: [0; CHARACTER_PARTS],
                stats: [0; CHARACTER_STATS],
                mastery: 0,
            },
            caddie: RetailCaddie::default(),
            club_set_iff_id: card.club_set_iff_id,
        },
        start_time,
    }
}

/// Builds a retail census roster from a room's authoritative snapshot.
/// Builds one room census record.
///
/// `slot` is one-based: `pangbox/packetdoc` (`gameservice/server/0048.ksy`, `room_user_slot`)
/// documents it as "from 1 to the user_max", and the client numbers the seats it draws from it.
fn retail_room_information_user(member: &MemberSnapshot) -> RetailRoomInformationUser {
    RetailRoomInformationUser::new(
        u32::try_from(member.connection_id().get()).unwrap_or(0),
        1,
        0,
    )
}

fn retail_room_player(slot: usize, member: &MemberSnapshot) -> RetailRoomPlayer {
    RetailRoomPlayer {
        connection_id: u32::try_from(member.connection_id().get()).unwrap_or(0),
        nickname: member.nickname().as_bytes().to_vec(),
        slot: u8::try_from(slot.saturating_add(1)).unwrap_or(u8::MAX),
        character_iff_id: member.character_iff_id().unwrap_or(0),
        flags: RoomPlayerFlags::new(member.is_owner(), member.is_ready()),
        level: 1,
        user_id: u32::try_from(member.account_id().get()).unwrap_or(0),
        // The client builds every player's model from this block when the hole loads, so
        // the catalog id has to be one its own Character.iff holds. Fitted parts are not
        // carried by a room member and stay zero: they change how a character looks, not
        // whether it can be instantiated.
        character: RetailCharacter {
            iff_id: member.character_iff_id().unwrap_or(0),
            uid: member
                .character_id()
                .and_then(|id| u32::try_from(id.get()).ok())
                .unwrap_or(0),
            hair_color: 0,
            part_iff_ids: [0; CHARACTER_PARTS],
            part_uids: [0; CHARACTER_PARTS],
            stats: [0; CHARACTER_STATS],
            mastery: 0,
        },
    }
}

fn retail_census_from_snapshot(snapshot: &RoomSnapshot) -> RetailRoomCensus {
    let players = snapshot
        .members()
        .iter()
        .enumerate()
        .map(|(slot, member)| retail_room_player(slot, member))
        .collect();
    RetailRoomCensus::List(players)
}

const RETAIL_PURCHASE_REPLAY_WINDOW: usize = 128;

/// Bounded, connection-local translation from wire replay identity to a purchase sequence.
///
/// The client's salt plus the full plaintext digest distinguishes ordinary repeated purchases
/// while retaining an exact frame replay long enough to cover transport retries. Bounded storage
/// prevents an authenticated client from growing per-connection state without limit.
#[derive(Debug)]
struct RetailWireReplayWindow {
    next_sequence: u64,
    entries: VecDeque<(u8, [u8; 32], u64)>,
}

impl RetailWireReplayWindow {
    fn new() -> Self {
        Self {
            next_sequence: 1,
            entries: VecDeque::with_capacity(RETAIL_PURCHASE_REPLAY_WINDOW),
        }
    }

    fn sequence(&mut self, salt: u8, payload_digest: [u8; 32]) -> u64 {
        if let Some((_, _, sequence)) = self.entries.iter().find(|(seen_salt, seen_digest, _)| {
            *seen_salt == salt && *seen_digest == payload_digest
        }) {
            return *sequence;
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        if self.entries.len() == RETAIL_PURCHASE_REPLAY_WINDOW {
            self.entries.pop_front();
        }
        self.entries.push_back((salt, payload_digest, sequence));
        sequence
    }

    fn is_replay(&self, salt: u8, payload_digest: &[u8; 32]) -> bool {
        self.entries
            .iter()
            .any(|(seen_salt, seen_digest, _)| *seen_salt == salt && seen_digest == payload_digest)
    }
}

/// Derives a restart-stable operation key for one exact retail `0x0020` frame.
///
/// The packet has no application operation identifier, so the authenticated account and complete
/// wire payload form its durable identity. This intentionally does not include connection state.
fn retail_equipment_update_operation_id(
    account_id: AccountId,
    payload: &[u8],
) -> EconomyOperationId {
    let mut hasher = Sha256::new();
    hasher.update(b"retail-equipment-update-v1");
    hasher.update(account_id.get().to_le_bytes());
    hasher.update(payload);
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    EconomyOperationId::new(uuid::Uuid::from_bytes(bytes))
}

/// Derives a stable operation key for one retail equipment frame.
fn retail_equipment_operation_id(
    account_id: AccountId,
    slot: RetailEquipmentSlot,
    requested: u32,
    requested_aux: u32,
    operation_scope: uuid::Uuid,
    operation_sequence: u64,
) -> EconomyOperationId {
    let mut hasher = Sha256::new();
    hasher.update(b"retail-equipment");
    hasher.update(operation_scope.as_bytes());
    hasher.update(account_id.get().to_le_bytes());
    hasher.update(operation_sequence.to_le_bytes());
    hasher.update([slot.tag()]);
    hasher.update(requested.to_le_bytes());
    hasher.update(requested_aux.to_le_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    EconomyOperationId::new(uuid::Uuid::from_bytes(bytes))
}

/// Derives a stable economy operation key for one retail purchase line.
///
/// The retail purchase packet carries no application operation identifier. The connection loop
/// assigns a sequence per distinct bounded `(salt, payload)` replay key, scoped by a fresh UUID for
/// that transport. Exact frame replays therefore derive the same key; intentional later purchases
/// with a new client salt derive a different key, including when the item and quantity are equal.
fn retail_purchase_operation_id(
    account_id: AccountId,
    item: &RetailPurchaseItem,
    purchase_scope: uuid::Uuid,
    purchase_sequence: u64,
    line_index: usize,
) -> EconomyOperationId {
    let mut hasher = Sha256::new();
    hasher.update(b"retail-purchase");
    hasher.update(purchase_scope.as_bytes());
    hasher.update(account_id.get().to_le_bytes());
    hasher.update(purchase_sequence.to_le_bytes());
    hasher.update(line_index.to_le_bytes());
    hasher.update(item.item_type_id.to_le_bytes());
    hasher.update(item.quantity.to_le_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    EconomyOperationId::new(uuid::Uuid::from_bytes(bytes))
}

/// Retail equipment change in channel/lobby, packetdoc client opcode `0x000b`.
const RETAIL_C2S_EQUIPMENT_LOBBY: u16 = 0x000b;
/// Retail equipment change in a room, packetdoc client opcode `0x000c`.
const RETAIL_C2S_EQUIPMENT_ROOM: u16 = 0x000c;
/// Retail room-leave client opcode.
const RETAIL_C2S_ROOM_LEAVE: u16 = 0x000f;
/// Retail multiplayer-mode enter client opcode, sent when the client opens the room directory.
const RETAIL_C2S_MULTIPLAYER_JOIN: u16 = 0x0081;
/// Retail multiplayer-mode leave client opcode, sent once the client leaves the directory.
const RETAIL_C2S_MULTIPLAYER_LEAVE: u16 = 0x0082;

/// Builds a retail room record from a lobby summary plus the settings the creator asked for.
/// The room profile is authoritative, including its course and whole-card shape.
fn retail_room_from_summary(summary: &RoomSummary, request: &RetailRoomCreate) -> RetailRoom {
    let (mode, play_mode) = retail_room_wire_modes(request.room_type);
    RetailRoom {
        name: summary.name().as_str().as_bytes().to_vec(),
        public: !summary.password_protected(),
        state: RetailRoomState::Lobby,
        max_players: summary.max_members(),
        player_count: summary.members(),
        hole_count: request.hole_count,
        mode,
        play_mode,
        id: u16::try_from(summary.id().get()).unwrap_or(u16::MAX),
        hole_progression: RetailHoleProgression::FrontStart,
        course: request.course,
        shot_timer_ms: request.shot_timer_ms,
        game_timer_ms: request.game_timer_ms,
        owner_uid: 0,
        artifact_id: 0,
        natural_wind: false,
    }
}

/// Builds a retail room record from a summary alone, for the lobby list.
fn retail_room_from_summary_only(summary: &RoomSummary) -> RetailRoom {
    retail_room_from_parts(summary, summary.members())
}

/// Builds a retail room record from a joined room's authoritative snapshot.
fn retail_room_from_snapshot(snapshot: &RoomSnapshot) -> RetailRoom {
    retail_room_from_parts(
        snapshot.summary(),
        u8::try_from(snapshot.members().len()).unwrap_or(u8::MAX),
    )
}

/// Describes a room the way the room itself describes it.
///
/// Everything but the occupancy comes from the room's own profile, which is the shape its
/// creator asked for. Rebuilding it as a default versus room instead is what left a client
/// sitting in its own practice room told it was somewhere else, with Start disabled.
fn retail_room_from_parts(summary: &RoomSummary, player_count: u8) -> RetailRoom {
    let profile = summary.profile();
    let (mode, play_mode) = retail_room_wire_modes(profile.mode);
    RetailRoom {
        name: summary.name().as_str().as_bytes().to_vec(),
        public: !summary.password_protected(),
        state: RetailRoomState::Lobby,
        max_players: summary.max_members(),
        player_count,
        hole_count: profile.hole_count,
        mode,
        play_mode,
        id: u16::try_from(summary.id().get()).unwrap_or(u16::MAX),
        hole_progression: match profile.hole_progression {
            1 => RetailHoleProgression::BackStart,
            2 => RetailHoleProgression::RandomStart,
            3 => RetailHoleProgression::ShuffleAll,
            _ => RetailHoleProgression::FrontStart,
        },
        course: profile.course,
        shot_timer_ms: profile.shot_timer_ms,
        game_timer_ms: profile.game_timer_ms,
        owner_uid: 0,
        artifact_id: profile.artifact_id,
        natural_wind: profile.natural_wind,
    }
}

/// The room record carries a UI-family mode first and the semantic room type later. They are
/// equal for the minimum versus path, but retail Practice is semantic type 19 in UI family 4.
/// `alter-pangya` spells this out as `PRACTICE(19, uiType = 4)` and serializes both fields
/// (`RoomType.kt:8-28`, `Room.kt:145-174`).
fn is_retail_course_value(course: u8) -> bool {
    matches!(course, 0x00..=0x0b | 0x0d..=0x10 | 0x12..=0x14 | 0x7f)
}

/// Validates the timer which is live for the requested retail room family.
///
/// Versus and chat use the shot timer, while tournament-shaped rooms use the whole-game
/// timer. Both wire fields remain bounded when present; retaining the former requirement for
/// unknown room types avoids assigning semantics that the profile does not define.
fn retail_room_create_has_valid_timers(request: &RetailRoomCreate) -> bool {
    retail_room_has_valid_timers(
        request.room_type,
        request.shot_timer_ms,
        request.game_timer_ms,
    )
}

fn retail_room_has_valid_timers(room_type: u8, shot_timer_ms: u32, game_timer_ms: u32) -> bool {
    const MAX_RETAIL_ROOM_TIMER_MS: u32 = 3_600_000;
    let timer_is_bounded = |timer_ms: u32| timer_ms <= MAX_RETAIL_ROOM_TIMER_MS;
    if !timer_is_bounded(shot_timer_ms) || !timer_is_bounded(game_timer_ms) {
        return false;
    }
    match RetailRoomType::from_wire(room_type) {
        Some(RetailRoomType::Versus | RetailRoomType::Chat) => shot_timer_ms != 0,
        Some(RetailRoomType::Tournament | RetailRoomType::Battle | RetailRoomType::Practice) => {
            game_timer_ms != 0
        }
        None => shot_timer_ms != 0 && game_timer_ms != 0,
    }
}

/// Selects the actor's whole-game deadline without changing the room's announced wire profile.
fn retail_stroke_game_timeout(profile: RoomProfile, configured_game_timeout: Duration) -> Duration {
    match RetailRoomType::from_wire(profile.mode) {
        Some(RetailRoomType::Versus | RetailRoomType::Chat) => configured_game_timeout,
        Some(RetailRoomType::Tournament | RetailRoomType::Battle | RetailRoomType::Practice)
        | None => Duration::from_millis(u64::from(profile.game_timer_ms)),
    }
}

fn retail_room_wire_modes(semantic_mode: u8) -> (u8, u8) {
    if semantic_mode == RetailRoomType::Practice as u8 {
        (
            RetailRoomType::Tournament as u8,
            RetailRoomType::Practice as u8,
        )
    } else {
        (semantic_mode, 0)
    }
}

/// Retail match client opcodes.
/// Retail room-ready client opcode. The client sends this before it will offer Start.
const RETAIL_C2S_ROOM_READY: u16 = 0x000d;
/// Retail room-edit client opcode. The client sends it whenever the room master touches the
/// room's settings, and it sends one on the way into a match.
const RETAIL_C2S_ROOM_EDIT: u16 = 0x000a;
const RETAIL_C2S_TEAM_CHANGE: u16 = 0x0010;
const RETAIL_C2S_ROOM_RESYNC: u16 = 0x001c;
const RETAIL_C2S_ROOM_KICK: u16 = 0x0026;
const RETAIL_C2S_ROOM_INVITE_INFO: u16 = 0x0029;
const RETAIL_C2S_ROOM_INFO: u16 = 0x002d;
const RETAIL_C2S_GM_ENTER_ROOM: u16 = 0x003e;
const RETAIL_C2S_ROOM_INVITE: u16 = 0x00ba;
const RETAIL_C2S_REJOIN_INVITED: u16 = 0x00b4;
/// Cosmetic lounge/avatar action sent by clicks in the room's character stage.
const RETAIL_C2S_LOUNGE_ACTION: u16 = 0x0063;
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
/// Removes the client-only shot subtype before relaying a committed shot as server `0x0055`.
fn retail_shot_announce_payload(payload: &[u8]) -> Result<Vec<u8>, GameRuntimeError> {
    let subtype = payload
        .get(..2)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or(GameRuntimeError::Protocol)?;
    tracing::debug!(
        shot_subtype = subtype,
        shot_payload_bytes = payload.len(),
        "retail shot commit shape"
    );
    let prefix = match subtype {
        0 => 2,
        1 => 11, // putt carries nine subtype bytes
        _ => return Err(GameRuntimeError::Protocol),
    };
    let shot = payload
        .get(prefix..)
        // The references disagree on the opaque tail width (PacketDoc and pangbox model distinct
        // revisions). The frame is already hard-capped; preserve every supplied byte instead of
        // guessing one width, while still refusing an empty trajectory.
        .filter(|shot| !shot.is_empty())
        .ok_or(GameRuntimeError::Protocol)?;
    Ok(shot.to_vec())
}
const RETAIL_C2S_LOAD_PROGRESS: u16 = 0x0048;

/// Observes a fire-and-forget retail client exception without making it fatal.
///
/// The frame has already been bounded by the codec. Decode errors are deliberately redacted and
/// ignored: the client is reporting a failure, and a malformed report must not cause a second
/// disconnect or prevent it from sending whatever diagnostic follows.
fn observe_retail_client_exception(payload: &[u8]) {
    match decode_packet_payload::<RetailClientException>(
        payload,
        &CompatibilityProfile::US_852,
        ServiceKind::Game,
    ) {
        Ok(report) => {
            tracing::warn!(message = %report.sanitized(), "client reported an exception");
        }
        Err(error) => {
            tracing::debug!(%error, "malformed client exception report");
        }
    }
}

fn is_retail_match_opcode(opcode: u16) -> bool {
    matches!(
        opcode,
        RETAIL_C2S_START_MATCH
            | RetailPracticeStart::OPCODE
            | RETAIL_C2S_HOLE_LOAD_FINISHED
            | RETAIL_C2S_SHOT_COMMIT
            | RETAIL_C2S_SHOT_SYNC
            | RETAIL_C2S_SHOT_END
            | RETAIL_C2S_HOLE_FINISH
            | RETAIL_C2S_FIRST_SHOT_READY
            | RETAIL_C2S_LOAD_PROGRESS
    ) || is_retail_accepted_match_opcode(opcode)
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
            | RETAIL_C2S_EQUIPMENT_LOBBY
            | RETAIL_C2S_EQUIPMENT_ROOM
            | RETAIL_C2S_ROOM_LEAVE
            | RETAIL_C2S_ROOM_READY
            | RETAIL_C2S_ROOM_EDIT
            | RETAIL_C2S_TEAM_CHANGE
            | RETAIL_C2S_ROOM_RESYNC
            | RETAIL_C2S_ROOM_KICK
            | RETAIL_C2S_ROOM_INVITE_INFO
            | RETAIL_C2S_ROOM_INFO
            | RETAIL_C2S_GM_ENTER_ROOM
            | RETAIL_C2S_ROOM_INVITE
            | RETAIL_C2S_REJOIN_INVITED
            | RETAIL_C2S_LOUNGE_ACTION
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
        RoomError::MatchActive | RoomError::InvalidTeam => RoomCommandResult::Closed,
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

    /// Decodes the documented `0x0008` layout so timer validation covers the client wire shape,
    /// rather than a hand-built request.
    fn decoded_retail_room_create(
        room_type: RetailRoomType,
        shot_timer_ms: u32,
        game_timer_ms: u32,
    ) -> RetailRoomCreate {
        let mut writer = PacketWriter::default();
        writer.u8(0);
        writer.u32_le(shot_timer_ms);
        writer.u32_le(game_timer_ms);
        writer.u8(4);
        writer.u8(room_type as u8);
        writer.u8(3);
        writer.u8(1);
        writer.bytes(&[0; 5]);
        writer.pstring(b"Retail Stroke", 64).expect("name");
        writer.pstring(b"", 64).expect("password");
        decode_packet_payload::<RetailRoomCreate>(
            &writer.into_inner(),
            &CompatibilityProfile::US_852,
            ServiceKind::Game,
        )
        .expect("documented retail room-create layout")
    }

    #[test]
    fn retail_stroke_sequence_defers_one_early_hole_finish_until_accepted_result() {
        let mut sequence = RetailStrokeSequence::default();
        assert!(!sequence.remember_early_hole_finish());

        sequence.accepted_action();
        assert!(sequence.remember_early_hole_finish());
        assert!(
            sequence.remember_early_hole_finish(),
            "a duplicate early 0031 remains one pending claim"
        );
        assert!(sequence.accepted_result());
        assert_eq!(sequence, RetailStrokeSequence::default());
        assert!(
            !sequence.accepted_result(),
            "a replayed result cannot complete the remembered claim twice"
        );
    }

    #[test]
    fn retail_stroke_sequence_leaves_post_result_hole_finish_for_normal_routing() {
        let mut sequence = RetailStrokeSequence::default();
        sequence.accepted_action();
        assert!(
            !sequence.accepted_result(),
            "without early 0031, the normal post-result 0031 still routes HoleOut"
        );
        assert_eq!(sequence, RetailStrokeSequence::default());
        sequence.accepted_action();
        sequence.clear();
        assert_eq!(sequence, RetailStrokeSequence::default());
    }

    #[test]
    fn retail_room_create_accepts_reference_stroke_without_game_timer() {
        // `US852_TOURNAMENT_MODE.md` §1.3, citing PacketDoc `client/0008.ksy:28-33`,
        // identifies shot_timer_ms as the live timer for versus. These are the observed
        // US852 UI values for a three-hole Stroke room with the 120-second selection.
        let request = decoded_retail_room_create(RetailRoomType::Versus, 120_000, 0);

        assert!(retail_room_create_has_valid_timers(&request));
    }

    #[test]
    fn retail_room_create_rejects_zero_or_out_of_range_live_shot_timer() {
        for shot_timer_ms in [0, 3_600_001] {
            let request = decoded_retail_room_create(RetailRoomType::Versus, shot_timer_ms, 0);
            assert!(
                !retail_room_create_has_valid_timers(&request),
                "invalid live shot timer {shot_timer_ms} must be rejected"
            );
        }
    }

    #[test]
    fn retail_room_create_preserves_tournament_game_timer_requirement() {
        let valid = decoded_retail_room_create(RetailRoomType::Tournament, 0, 900_000);
        assert!(retail_room_create_has_valid_timers(&valid));

        let missing_game_timer = decoded_retail_room_create(RetailRoomType::Tournament, 120_000, 0);
        assert!(!retail_room_create_has_valid_timers(&missing_game_timer));
    }

    #[tokio::test]
    async fn retail_stroke_plan_uses_configured_game_timeout_for_versus_zero_wire_timer() {
        let profile = RoomProfile {
            mode: RetailRoomType::Versus as u8,
            course: 1,
            hole_count: 3,
            shot_timer_ms: 120_000,
            game_timer_ms: 0,
            ..RoomProfile::default()
        };
        assert!(retail_room_has_valid_timers(
            profile.mode,
            profile.shot_timer_ms,
            profile.game_timer_ms,
        ));
        let service =
            test_stroke_service(Arc::new(FakeRepository::default()), Duration::from_secs(1));
        let stroke = service.config.stroke_two.unwrap_or_else(|| unreachable!());
        let base = test_stroke_plan(&service, 1);
        let game_timeout = retail_stroke_game_timeout(profile, stroke.game_timeout);
        let plan = StrokeStartPlan::new(
            base.begin().clone(),
            *base.roster(),
            base.loading_timeout(),
            Duration::from_millis(u64::from(profile.shot_timer_ms)),
            game_timeout,
            base.max_strokes(),
        )
        .expect("real three-hole versus profile prepares with configured game fallback");

        assert_eq!(profile.game_timer_ms, 0, "retained room-announcement value");
        assert_eq!(plan.turn_timeout(), Duration::from_secs(120));
        assert_eq!(plan.game_timeout(), stroke.game_timeout);

        let tournament = RoomProfile {
            mode: RetailRoomType::Tournament as u8,
            game_timer_ms: 900_000,
            ..profile
        };
        assert_eq!(
            retail_stroke_game_timeout(tournament, stroke.game_timeout),
            Duration::from_secs(900),
            "Tournament retains its live whole-game timer"
        );
    }

    #[test]
    fn gm_oid_resolves_authoritative_live_connection_and_cancels_only_that_token() {
        let hub = SocialHub::new(16);
        let connection = PlayerConnectionId::new(7).unwrap_or(PlayerConnectionId::new(1).unwrap());
        let target_oid = 42;
        let token = CancellationToken::new();
        let account = AccountId::new(99).unwrap_or(AccountId::new(1).unwrap());
        hub.register_with_oid_and_cancellation(
            connection,
            target_oid,
            account,
            b"target".to_vec(),
            MemberCard::default(),
            token.clone(),
        );

        assert_eq!(hub.connection_for_oid(target_oid), Some(connection));
        assert_eq!(hub.account_for_oid(target_oid), Some(account));
        assert!(!token.is_cancelled());
        assert!(hub.cancel_connection_for_oid(target_oid));
        assert!(token.is_cancelled());
        assert_eq!(hub.connection_for_oid(target_oid), Some(connection));
        assert!(
            !hub.cancel_connection_for_oid(7),
            "wire OID must not be cast to a connection ID"
        );
    }

    #[test]
    fn gm_oid_keeps_durable_account_target_after_connection_leaves() {
        let hub = SocialHub::new(16);
        let connection = PlayerConnectionId::new(7).unwrap_or(PlayerConnectionId::new(1).unwrap());
        let target_oid = 42;
        let account = AccountId::new(99).unwrap_or(AccountId::new(1).unwrap());
        hub.register_with_oid(
            connection,
            target_oid,
            account,
            b"target".to_vec(),
            MemberCard::default(),
        );
        hub.remove(connection);

        assert_eq!(hub.connection_for_oid(target_oid), None);
        assert_eq!(hub.account_for_oid(target_oid), Some(account));
    }

    #[test]
    fn issue12_social_hub_scopes_ordered_chat_typing_and_lounge_to_room_members() {
        let hub = SocialHub::new(16);
        let first = PlayerConnectionId::new(1).unwrap_or(PlayerConnectionId::new(2).unwrap());
        let second = PlayerConnectionId::new(2).unwrap_or(PlayerConnectionId::new(3).unwrap());
        let outsider = PlayerConnectionId::new(3).unwrap_or(PlayerConnectionId::new(4).unwrap());
        let room = RoomId::new(9).unwrap_or(RoomId::new(10).unwrap());
        for (id, account, name) in [
            (first, 11, b"one".to_vec()),
            (second, 12, b"two".to_vec()),
            (outsider, 13, b"three".to_vec()),
        ] {
            hub.register(
                id,
                AccountId::new(account).unwrap_or(AccountId::new(1).unwrap()),
                name,
                MemberCard::default(),
            );
        }
        hub.set_room(first, Some(room));
        hub.set_room(second, Some(room));
        let mut first_events = hub.subscribe();
        let mut outsider_events = hub.subscribe();
        hub.chat(first, b"one".to_vec(), b"hello".to_vec());
        assert!(
            matches!(first_events.try_recv(), Ok(SocialEvent::Chat { ref targets, .. }) if targets.contains(&second) && !targets.contains(&outsider))
        );
        assert!(
            matches!(outsider_events.try_recv(), Ok(SocialEvent::Chat { ref targets, .. }) if !targets.contains(&outsider))
        );
        hub.typing(first, true);
        assert!(matches!(
            first_events.try_recv(),
            Ok(SocialEvent::Typing { typing: true, .. })
        ));
        hub.lounge(second, vec![7, 1, 2]);
        assert!(
            matches!(first_events.try_recv(), Ok(SocialEvent::Lounge { action, .. }) if action == vec![7, 1, 2])
        );
    }

    #[test]
    fn retail_hole_orders_are_deterministic_and_cover_requested_card() {
        assert_eq!(
            GameService::<FakeRepository>::retail_hole_order(1, 3, 0),
            vec![1, 2, 3]
        );
        assert_eq!(
            GameService::<FakeRepository>::retail_hole_order(1, 9, 1),
            vec![10, 11, 12, 13, 14, 15, 16, 17, 18]
        );
        let random = GameService::<FakeRepository>::retail_hole_order(7, 18, 2);
        assert_eq!(
            random,
            GameService::<FakeRepository>::retail_hole_order(7, 18, 2)
        );
        assert_eq!(random.len(), 18);
        assert_eq!(
            random
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            18
        );
        let shuffle = GameService::<FakeRepository>::retail_hole_order(7, 18, 3);
        assert_eq!(
            shuffle,
            GameService::<FakeRepository>::retail_hole_order(7, 18, 3)
        );
        assert_eq!(
            shuffle
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            18
        );
    }

    #[test]
    fn issue12_equipment_projection_refreshes_social_card() {
        let hub = SocialHub::new(8);
        let member = PlayerConnectionId::new(1).unwrap();
        hub.register(
            member,
            AccountId::new(1).unwrap(),
            b"member".to_vec(),
            MemberCard::default(),
        );
        let mut events = hub.subscribe();
        let updated = MemberCard {
            character_uid: 99,
            ..MemberCard::default()
        };
        hub.update_card(member, updated);
        hub.user_info(member, 1, 5, MemberCard::default());
        assert!(
            matches!(events.try_recv(), Ok(SocialEvent::UserInfo { card, request_type: 5, .. }) if card.character_uid == 99)
        );
    }

    #[test]
    fn issue12_whisper_accept_state_refuses_blocked_and_offline_targets() {
        let hub = SocialHub::new(16);
        let sender = PlayerConnectionId::new(1).unwrap_or(PlayerConnectionId::new(2).unwrap());
        let target = PlayerConnectionId::new(2).unwrap_or(PlayerConnectionId::new(3).unwrap());
        hub.register(
            sender,
            AccountId::new(1).unwrap(),
            b"sender".to_vec(),
            MemberCard::default(),
        );
        hub.register(
            target,
            AccountId::new(2).unwrap(),
            b"target".to_vec(),
            MemberCard::default(),
        );
        let mut sender_events = hub.subscribe();
        let mut target_events = hub.subscribe();
        hub.set_whisper_accept(target, false);
        hub.whisper(sender, b"target", b"blocked".to_vec());
        assert!(matches!(
            sender_events.try_recv(),
            Ok(SocialEvent::Whisper { status: 4, .. })
        ));
        assert!(
            matches!(target_events.try_recv(), Ok(SocialEvent::Whisper { target: recipient, status: 4, .. }) if recipient == sender)
        );
        hub.whisper(sender, b"missing", b"offline".to_vec());
        assert!(matches!(
            sender_events.try_recv(),
            Ok(SocialEvent::Whisper { status: 5, .. })
        ));
    }

    #[test]
    fn retail_purchase_replay_window_reuses_only_exact_bounded_wire_keys() {
        let mut window = RetailWireReplayWindow::new();
        let digest = [0x42; 32];
        let first = window.sequence(7, digest);
        assert_eq!(window.sequence(7, digest), first, "exact frame replay");
        assert_ne!(window.sequence(8, digest), first, "new client salt");
        assert_ne!(window.sequence(7, [0x43; 32]), first, "new payload");

        for value in 0..RETAIL_PURCHASE_REPLAY_WINDOW {
            let mut other = [0_u8; 32];
            other[..8].copy_from_slice(&(value as u64).to_le_bytes());
            window.sequence(99, other);
        }
        assert_ne!(
            window.sequence(7, digest),
            first,
            "an evicted replay key becomes a new bounded command"
        );
    }

    #[test]
    fn retail_equipment_operation_id_is_restart_stable_and_payload_bound() {
        let account = AccountId::new(7).unwrap_or_else(|_| unreachable!());
        let payload = [RetailEquipmentSlot::Consumables.tag(), 1, 2, 3];
        let same = retail_equipment_update_operation_id(account, &payload);
        assert_eq!(
            same,
            retail_equipment_update_operation_id(account, &payload)
        );
        assert_ne!(
            same,
            retail_equipment_update_operation_id(account, &[payload[0], 1, 2, 4])
        );
        assert_ne!(
            same,
            retail_equipment_update_operation_id(
                AccountId::new(8).unwrap_or_else(|_| unreachable!()),
                &payload
            )
        );
    }

    #[test]
    fn retail_purchase_operation_id_scopes_replays_and_distinct_lines() {
        let account = AccountId::new(7).unwrap_or_else(|_| unreachable!());
        let item = RetailPurchaseItem {
            item_type_id: 0x1400_00c9,
            quantity: 50,
            claimed_cost_pang: 77,
            claimed_cost_point: 0,
        };
        let scope = uuid::Uuid::from_u128(9);
        let replay = retail_purchase_operation_id(account, &item, scope, 3, 0);
        assert_eq!(
            retail_purchase_operation_id(account, &item, scope, 3, 0),
            replay
        );
        assert_ne!(
            retail_purchase_operation_id(account, &item, scope, 4, 0),
            replay,
            "a later intentional purchase has a distinct sequence"
        );
        assert_ne!(
            retail_purchase_operation_id(account, &item, scope, 3, 1),
            replay,
            "equal line items in one basket remain distinct"
        );
    }

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

    fn catalog_v2() -> Catalog {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../pangya-data/tests/fixtures/synthetic-catalog-v2");
        Catalog::load(&root, std::path::Path::new("manifest.toml"))
            .unwrap_or_else(|_| unreachable!())
    }

    fn solo_config(catalog: &Catalog, commit_timeout: Duration) -> SoloRuntimeConfig {
        SoloRuntimeConfig {
            course: catalog
                .course_plan(
                    pangya_domain::CourseId::new(7).unwrap_or_else(|_| unreachable!()),
                    1,
                    0,
                )
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

    #[tokio::test]
    async fn login_bonus_requires_the_exact_catalog_definition() {
        let catalog = catalog_v2();
        let definition = catalog
            .item_definition(ItemTypeId::new(0x1a00_0001))
            .copied()
            .unwrap_or_else(|| unreachable!());
        let reward = LoginBonusReward {
            definition,
            quantity: 1,
        };
        let valid = GameRuntimeConfig {
            login_bonus: Some(LoginBonusRuntimeConfig {
                reward,
                calendar_days: 30,
            }),
            ..GameRuntimeConfig::default()
        };
        assert!(
            GameService::new(
                Arc::new(FakeRepository::default()),
                catalog.clone(),
                valid,
                Arc::new(NoopGameObserver),
            )
            .is_ok()
        );

        for changed in [
            ItemDefinition {
                kind: ItemKind::Ball,
                ..definition
            },
            ItemDefinition {
                stacking: ItemStacking::Stackable { max_stack: 98 },
                ..definition
            },
            ItemDefinition {
                durability: ItemDurability::Durable {
                    max: 10,
                    repair_pang_per_point: 1,
                },
                ..definition
            },
        ] {
            assert_eq!(
                GameService::new(
                    Arc::new(FakeRepository::default()),
                    catalog.clone(),
                    GameRuntimeConfig {
                        login_bonus: Some(LoginBonusRuntimeConfig {
                            reward: LoginBonusReward {
                                definition: changed,
                                quantity: 1,
                            },
                            calendar_days: 30,
                        }),
                        ..GameRuntimeConfig::default()
                    },
                    Arc::new(NoopGameObserver),
                )
                .err(),
                Some(GameRuntimeError::Catalog)
            );
        }
    }

    #[test]
    fn login_bonus_server_day_changes_only_at_utc_epoch_boundaries() {
        let boundary = SystemTime::UNIX_EPOCH + Duration::from_secs(86_400 * 30);
        assert_eq!(
            GameService::<FakeRepository>::retail_server_day(boundary - Duration::from_secs(1)),
            29
        );
        assert_eq!(
            GameService::<FakeRepository>::retail_server_day(boundary),
            30
        );
    }

    fn test_identity() -> RoomIdentity {
        RoomIdentity {
            connection_id: PlayerConnectionId::new(1).unwrap_or_else(|_| unreachable!()),
            account_id: AccountId::new(7).unwrap_or_else(|_| unreachable!()),
            game_master: false,
            nickname: Nickname::parse("Tester").unwrap_or_else(|_| unreachable!()),
            character_id: None,
            character_iff_id: None,
            card: MemberCard::default(),
        }
    }

    fn second_test_identity() -> RoomIdentity {
        RoomIdentity {
            connection_id: PlayerConnectionId::new(2).unwrap_or_else(|_| unreachable!()),
            account_id: AccountId::new(8).unwrap_or_else(|_| unreachable!()),
            game_master: false,
            nickname: Nickname::parse("Second").unwrap_or_else(|_| unreachable!()),
            character_id: None,
            character_iff_id: None,
            card: MemberCard::default(),
        }
    }

    fn stroke_config(catalog: &Catalog, commit_timeout: Duration) -> StrokeRuntimeConfig {
        StrokeRuntimeConfig {
            course: catalog
                .course_plan(
                    pangya_domain::CourseId::new(7).unwrap_or_else(|_| unreachable!()),
                    1,
                    0,
                )
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

    #[test]
    fn retail_equipment_change_opcodes_are_routed_as_room_commands() {
        assert!(is_retail_room_opcode(0x000b));
        assert!(is_retail_room_opcode(0x000c));
    }

    #[test]
    fn retail_equipment_announce_reaches_room_peers() {
        // The room actor must fan this event to every member; a self-only reply leaves peers
        // stale until they reconnect.
        let _ = RoomEvent::EquipmentAnnounce(RetailEquipmentAnnounce::Ball {
            connection_id: 7,
            ball_type_id: 0x1400_0001,
        });
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
    fn retail_client_exception_is_a_handled_session_not_match_opcode() {
        assert!(is_retail_accepted_session_opcode(
            RetailClientException::OPCODE
        ));
        assert!(!is_retail_match_opcode(RetailClientException::OPCODE));
    }

    #[test]
    fn retail_client_exception_log_value_is_bounded_and_redacted() {
        let report = RetailClientException {
            message: [b"safe", &[b'\n', 0, 0xff][..], &[b'x'; 300][..]].concat(),
        };
        let sanitized = report.sanitized();
        assert_eq!(sanitized.len(), 256);
        assert_eq!(&sanitized[..7], "safe...");
        assert!(
            sanitized
                .chars()
                .all(|character| { character == '.' || (' '..='~').contains(&character) })
        );
    }

    #[test]
    fn retail_client_exception_uses_reference_body_and_rejects_truncation_or_trailing_bytes() {
        // pangbox's ClientException is exactly one filler byte followed by one PString. The
        // references do not define an extension tail, so an exact body is the safe policy.
        let valid = [0, 4, 0, b's', b'a', b'f', b'e'];
        let report = decode_packet_payload::<RetailClientException>(
            &valid,
            &CompatibilityProfile::US_852,
            ServiceKind::Game,
        )
        .unwrap_or_else(|_| unreachable!());
        assert_eq!(report.message, b"safe");

        for malformed in [[0, 1, 0].as_slice(), &[0, 0, 0, 0][..]] {
            assert!(
                decode_packet_payload::<RetailClientException>(
                    malformed,
                    &CompatibilityProfile::US_852,
                    ServiceKind::Game,
                )
                .is_err()
            );
        }
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
                    &mut ConnectionMatchContext::default(),
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

    #[test]
    fn terminal_outbox_rejects_stale_generation() {
        assert!(!GameService::<FakeRepository>::accepts_terminal_generation(
            0, 0
        ));
        assert!(!GameService::<FakeRepository>::accepts_terminal_generation(
            1, 2
        ));
        assert!(GameService::<FakeRepository>::accepts_terminal_generation(
            2, 2
        ));
        assert!(GameService::<FakeRepository>::accepts_terminal_generation(
            3, 2
        ));
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
        let invalid_course = MatchPlan::with_holes(
            pangya_domain::CourseId::new(99).unwrap_or_else(|_| unreachable!()),
            1,
            0,
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
            .course_plan(
                pangya_domain::CourseId::new(7).unwrap_or_else(|_| unreachable!()),
                1,
                0,
            )
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
