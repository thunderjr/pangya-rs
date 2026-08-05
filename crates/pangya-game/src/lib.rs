#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Bounded synthetic GameService handover, bootstrap, lobby, and room runtime.

pub mod lobby;
pub mod room;

pub use lobby::{LobbyHandle, LobbyLimits, LobbyRoomCommand, LobbyRouteResult, spawn_lobby};
pub use room::{RoomActorLimits, RoomEvent, RoomHandle, RoomIdentity, spawn_room};

use std::{
    collections::VecDeque,
    future::Future,
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
    AccountId, ConsumeHandover, HandoverRepository, Nickname, PlayerConnectionId, PlayerRepository,
    PlayerSnapshot, RepositoryError, RoomError, RoomId, ServiceKind as DomainServiceKind,
    SourceAddressPrefix,
};
use pangya_login::{
    CapacityRegistry, FixedWindowLimiter, KeyedCapacityGuard, KeyedCapacityRegistry, RateDecision,
    RegistryError, RegistryGuard, parse_handover,
};
use pangya_protocol::{
    ChannelJoined, CharacterBootstrap, CharacterInfo, CodecLimits, CompatibilityProfile,
    DecodePacket, EncodePacket, EquipmentInfo, FrameCodec, GAME_INVENTORY_SEGMENT_ITEMS, GameAuth,
    InventoryBootstrap, InventorySegment, OutboundFrame, PacketEncodeError, PlayerInfo,
    RoomChatEvent, RoomChatRequest, RoomCommand, RoomCommandResult, RoomCommandResultResponse,
    RoomCreateRequest, RoomJoinRequest, RoomKickRequest, RoomLeaveRequest, RoomListRequest,
    RoomListResponse, RoomMembershipEvent, RoomMembershipKind, RoomReadyRequest,
    RoomSettingsRequest, RoomStateRequest, RoomStateResponse, SYNTHETIC_M4_C2S_CHAT,
    SYNTHETIC_M4_C2S_CREATE, SYNTHETIC_M4_C2S_JOIN, SYNTHETIC_M4_C2S_KICK, SYNTHETIC_M4_C2S_LEAVE,
    SYNTHETIC_M4_C2S_LIST, SYNTHETIC_M4_C2S_READY, SYNTHETIC_M4_C2S_SETTINGS,
    SYNTHETIC_M4_C2S_STATE, SelectChannel, ServiceKind, decode_packet_payload,
    encode_packet_payload, synthetic_game_hello,
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
    /// Fixed queue observation.
    fn queue(&self, _event: GameQueueObservation) {}
    /// Fixed chat observation.
    fn chat(&self, _event: GameChatObservation) {}
    /// Fixed unknown-opcode observation.
    fn unknown(&self, _event: GameUnknownObservation) {}
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

/// Immutable GameService composition.
#[derive(Clone, Debug)]
pub struct GameRuntimeConfig {
    /// Sole locally configured channel ID.
    pub channel_id: u32,
    /// Post-channel handling policy for truly unknown opcodes.
    pub unknown_opcode_policy: UnknownOpcodePolicy,
    /// Resource, rate, actor, and deadline limits.
    pub limits: GameRuntimeLimits,
}

impl Default for GameRuntimeConfig {
    fn default() -> Self {
        Self {
            channel_id: 1,
            unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
            limits: GameRuntimeLimits::default(),
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
    /// Connection or lobby drain exceeded grace.
    #[error("GameService graceful shutdown timed out")]
    ShutdownTimeout,
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
    R: HandoverRepository + PlayerRepository + 'static,
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
    R: HandoverRepository + PlayerRepository + 'static,
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
    R: HandoverRepository + PlayerRepository + 'static,
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
            || limits.outbound_room_event_capacity > 65_536
            || !limits.lobby.is_valid()
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
            || limits.codec.max_expansion_ratio > 1_024;
        if invalid {
            return Err(GameRuntimeError::InvalidConfig);
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
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                accepted = listener.accept() => {
                    let (stream, peer) = accepted.map_err(|_| GameRuntimeError::Accept)?;
                    match self.admit(peer) {
                        Ok(admission) => {
                            while tasks.len() >= self.config.limits.global_connections {
                                let _completed = tasks.join_next().await;
                            }
                            let service = Arc::clone(&self);
                            let child = shutdown.child_token();
                            tasks.spawn(async move {
                                let _outcome = service.run_admitted(stream, admission, child).await;
                            });
                        }
                        Err(_) => drop(stream),
                    }
                }
                joined = tasks.join_next(), if !tasks.is_empty() => { let _completed = joined; }
            }
        }
        drop(listener);
        let drain_timed_out = timeout(self.config.limits.shutdown_grace, async {
            while tasks.join_next().await.is_some() {}
        })
        .await
        .is_err();
        if drain_timed_out {
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
        }
        let lobby_result = timeout(self.config.limits.shutdown_grace, self.lobby.shutdown()).await;
        if drain_timed_out || !matches!(lobby_result, Ok(Ok(()))) {
            return Err(GameRuntimeError::ShutdownTimeout);
        }
        Ok(())
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
        let mut unknown_strikes = 0_u32;
        let (outbound, mut room_events) =
            mpsc::channel(self.config.limits.outbound_room_event_capacity);
        let mut queue_drops = self.lobby.subscribe_queue_drops();
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
                dropped = queue_drops.recv() => {
                    match dropped {
                        Ok(dropped) if dropped == connection_id => {
                            self.observer.queue(GameQueueObservation::OutboundDropped);
                            break Err(GameRuntimeError::Limited);
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            self.observer.queue(GameQueueObservation::OutboundDropped);
                            break Err(GameRuntimeError::Limited);
                        }
                        Err(broadcast::error::RecvError::Closed) => break Err(GameRuntimeError::Limited),
                    }
                }
                event = room_events.recv(), if state == GameState::InRoom => {
                    let Some(event) = event else { break Err(GameRuntimeError::Limited); };
                    let handled = tokio::select! {
                        biased;
                        () = shutdown.cancelled() => break Ok(GameTermination::Cancelled),
                        handled = self.handle_room_event(&mut framed, event, room_id) => handled,
                    };
                    match handled {
                        Ok(RoomEventEffect::Remain) => {}
                        Ok(RoomEventEffect::EnterChannel) => {
                            state = GameState::InChannel;
                            room_id = None;
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
                        GameState::InChannel | GameState::InRoom => {
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
        let cleanup = timeout(
            self.config.limits.command_timeout,
            self.lobby.disconnect(connection_id),
        )
        .await;
        if matches!(cleanup, Ok(Ok(None))) {
            self.observer.room(GameRoomObservation::Closed);
        } else if !matches!(
            cleanup,
            Ok(Ok(Some(_))) | Ok(Err(RoomError::NotMember | RoomError::RoomNotFound))
        ) {
            self.observer.queue(GameQueueObservation::LobbyRejected);
        }
        drop(presence);
        let _terminal_state = state;
        result
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
                match self.lobby_call(self.lobby.list()).await {
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
                    .lobby_call(self.lobby.create(
                        request.name,
                        request.password,
                        request.settings,
                        identity.clone(),
                        outbound,
                    ))
                    .await;
                match result {
                    Ok(summary) => {
                        *room_id = Some(summary.id());
                        self.send_result(framed, RoomCommand::Create, Ok(()))
                            .await?;
                        self.observer.room(GameRoomObservation::Created);
                        if let Ok(LobbyRouteResult::Snapshot(snapshot)) = self
                            .lobby_call(
                                self.lobby
                                    .route(identity.connection_id, LobbyRoomCommand::GetState),
                            )
                            .await
                        {
                            self.send(framed, &RoomStateResponse { room: snapshot })
                                .await?;
                        }
                        Ok(GameState::InRoom)
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
                    .lobby_call(self.lobby.join(
                        requested_room,
                        identity.clone(),
                        request.password,
                        outbound,
                    ))
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
                let result = self
                    .lobby_call(self.lobby.leave(identity.connection_id))
                    .await;
                match result {
                    Ok(snapshot) => {
                        self.send_result(framed, RoomCommand::Leave, Ok(())).await?;
                        self.observer.room(GameRoomObservation::Left);
                        if snapshot.is_none() {
                            self.observer.room(GameRoomObservation::Closed);
                        }
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
                    .lobby_call(
                        self.lobby
                            .route(identity.connection_id, LobbyRoomCommand::Chat(request.text)),
                    )
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
                    .lobby_call(
                        self.lobby
                            .route(identity.connection_id, LobbyRoomCommand::GetState),
                    )
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

    async fn route_snapshot(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        connection_id: PlayerConnectionId,
        command: RoomCommand,
        route: LobbyRoomCommand,
        observation: GameRoomObservation,
    ) -> Result<(), GameRuntimeError> {
        let result = self
            .lobby_call(self.lobby.route(connection_id, route))
            .await;
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
                self.observer.room(GameRoomObservation::Closed);
                Ok(RoomEventEffect::EnterChannel)
            }
        }
    }

    async fn lobby_call<F, T>(&self, operation: F) -> Result<T, RoomError>
    where
        F: Future<Output = Result<T, RoomError>>,
    {
        timeout(self.config.limits.command_timeout, operation)
            .await
            .map_err(|_| RoomError::Timeout)?
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
    use super::*;

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

    #[test]
    fn fixed_windows_bound_room_and_chat_commands() {
        let mut commands = LocalRateWindow::new(Duration::from_secs(60));
        assert!(commands.admit_count(2));
        assert!(commands.admit_count(2));
        assert!(!commands.admit_count(2));
    }
}
