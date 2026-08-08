//! Bounded LoginService TCP runtime generic over domain repository traits.

use std::{
    future::Future,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use futures_util::{SinkExt, StreamExt};
use pangya_domain::{
    AccountId, AccountRepository, AccountStatus, AuthenticationRecord, HandoverRepository,
    MAX_STARTER_ITEMS, NewAccount, Nickname, NormalizedUsername, RepositoryError,
    ServiceKind as DomainServiceKind, SetupState, SourceAddressPrefix, StarterGrant, Username,
};
use pangya_protocol::{
    ChatMacros, CheckNickname, CodecLimits, CompatibilityProfile, DecodePacket,
    EmptyMessageServerList, EncodePacket, ErrorClass, FrameCodec, GameServerEntry, GameServerList,
    InboundFrame, LOGIN_ERROR_DUPLICATE_CONNECTION, LOGIN_ERROR_INVALID_CREDENTIALS, LoginKey,
    LoginResult, LoginSuccess, NicknameCheckResult, OutboundFrame, PacketEncodeError, PacketReader,
    SelectCharacter, SelectServer, ServiceKind, SessionKey, SetNickname, UnknownBytes,
    encode_packet_payload, us852_login_hello,
};
use rand::{RngCore, rngs::OsRng};
use thiserror::Error;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinSet,
    time::timeout,
};
use tokio_util::{codec::Framed, sync::CancellationToken};
use tracing::Instrument as _;
use zeroize::Zeroizing;

use crate::{
    BoundedCredentialExecutor, CanonicalTransportSecret, CapacityRegistry, CredentialError,
    CredentialExecutorError, FixedWindowLimiter, KeyedCapacityGuard, KeyedCapacityRegistry,
    LoginEvent, LoginState, LoginStateMachine, RateDecision, RegistryError, RegistryGuard,
    generate_handover,
};

/// Generated process-local connection identifier safe for logs and metrics.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConnectionId(u64);

impl ConnectionId {
    /// Returns the process-local numeric representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Fixed redacted terminal outcomes for an accepted LoginService connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTermination {
    /// The full login/handover/server-selection state machine completed.
    Completed,
    /// A bounded friendly rejection closed the connection.
    Rejected,
    /// Service cancellation closed an incomplete connection.
    Cancelled,
    /// The peer closed an incomplete connection.
    PeerClosed,
    /// A configured login or idle timeout elapsed.
    Timeout,
    /// A fixed resource/rate limit closed the connection.
    Limited,
    /// Protocol validation closed the connection.
    Protocol,
    /// Another redacted runtime error closed the connection.
    Error,
}

impl ConnectionTermination {
    /// Returns the fixed metrics/tracing label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Completed => "complete",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
            Self::PeerClosed => "peer_closed",
            Self::Timeout => "timeout",
            Self::Limited => "limited",
            Self::Protocol => "protocol",
            Self::Error => "error",
        }
    }
}

/// Fixed protocol error metric classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolMetricClass {
    /// Semantic packet validation failure outside the wire reader.
    Decode,
    /// Wire input ended before a field completed.
    DecodeTruncated,
    /// Wire length/count exceeded a configured bound.
    DecodeLimit,
    /// Wire value could not be represented.
    DecodeOverflow,
    /// Required wire NUL terminator was absent.
    DecodeMissingTerminator,
    /// Wire packet was otherwise invalid.
    DecodeInvalid,
    /// Framed transport I/O failure.
    Io,
    /// Transport cryptography failure.
    Crypto,
    /// Packet encoding or compression failure.
    EncodeOrCompress,
    /// Known opcode used in the wrong state.
    InvalidState,
    /// Opcode outside the implemented bounded set.
    UnknownOpcode,
}

/// Fixed true-unknown opcode buckets that prevent cardinality growth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnknownOpcodeBucket {
    /// Unknown opcode in `0x0000..=0x00ff`.
    Low,
    /// Unknown opcode outside the low bounded range.
    Other,
}

/// Fixed credential worker metric outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialWorkerOutcome {
    /// Admission queue was saturated.
    Overload,
    /// Admitted operation exceeded its timeout.
    Timeout,
    /// Stored credential policy or worker failed operationally.
    OperationalError,
}

/// Fixed repository latency result classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DbQueryClass {
    /// Successful query at or under the low-latency threshold.
    Fast,
    /// Successful query above the low-latency threshold.
    Slow,
    /// Repository error.
    Error,
}

/// Fixed rate-limit metric classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateLimitClass {
    /// Global accept budget.
    AcceptGlobal,
    /// Masked-source accept budget.
    AcceptSource,
    /// Global concurrent connections.
    ConnectionGlobal,
    /// Masked-source concurrent connections.
    ConnectionSource,
    /// Global login attempts.
    LoginGlobal,
    /// Masked-source login attempts.
    LoginSource,
    /// Normalized-username login attempts.
    LoginUsername,
    /// Global packet count.
    PacketGlobal,
    /// Masked-source packet count.
    PacketSource,
    /// Per-connection packet/byte budget.
    PacketOrBytesConnection,
    /// Global plaintext bytes.
    BytesGlobal,
    /// Masked-source plaintext bytes.
    BytesSource,
}

/// Low-cardinality runtime observation boundary.
pub trait LoginObserver: Send + Sync + 'static {
    /// Records an accepted connection using only a generated ID and masked prefix.
    fn accepted(&self, _connection_id: ConnectionId, _source: &SourceAddressPrefix) {}
    /// Records a fixed redacted terminal outcome.
    fn closed(&self, _outcome: ConnectionTermination) {}
    /// Records frame direction/opcode/byte count without packet bodies.
    fn frame(&self, _direction: &'static str, _opcode: u16, _bytes: usize) {}
    /// Records a bounded login outcome.
    fn login(&self, _outcome: &'static str) {}
    /// Records an authenticated numeric account ID.
    fn authenticated(&self, _account_id: AccountId) {}
    /// Records credential worker outcome without secret material.
    fn credential_worker(&self, _outcome: CredentialWorkerOutcome) {}
    /// Records a typed protocol failure class.
    fn protocol_error(&self, _class: ProtocolMetricClass) {}
    /// Records a true-unknown opcode in a bounded bucket.
    fn unknown_opcode(&self, _bucket: UnknownOpcodeBucket) {}
    /// Records repository latency.
    fn db_query(&self, _class: DbQueryClass) {}
    /// Records a bounded rate-limit class.
    fn rate_limited(&self, _class: RateLimitClass) {}
}

/// No-op observer suitable for tests and embeddings.
#[derive(Debug, Default)]
pub struct NoopLoginObserver;
impl LoginObserver for NoopLoginObserver {}

/// Configured GameService advertisement used only by LoginService.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvertisedGameServer {
    /// Protocol server identifier.
    pub id: u16,
    /// Fixed-width server display name.
    pub name: String,
    /// IPv4 text validated by composition.
    pub ipv4: String,
    /// TCP port.
    pub port: u16,
    /// Advertised capacity.
    pub capacity: u32,
}

/// Hard resource and timeout limits for the LoginService runtime.
#[derive(Clone, Debug)]
pub struct LoginRuntimeLimits {
    /// Total concurrent TCP connections.
    pub global_connections: usize,
    /// Concurrent connections per privacy-masked source prefix.
    pub connections_per_source: usize,
    /// Tracked source-prefix capacity.
    pub source_capacity: usize,
    /// Global accepts/window.
    pub global_accepts_per_window: u32,
    /// Accepts per source/window.
    pub accepts_per_window: u32,
    /// Global login attempts/window.
    pub global_logins_per_window: u32,
    /// Login attempts per source/window.
    pub logins_per_window: u32,
    /// Login attempts per normalized username/window.
    pub username_logins_per_window: u32,
    /// Accept/login fixed window.
    pub rate_window: Duration,
    /// Global plaintext packets/window.
    pub global_packets_per_window: u32,
    /// Global plaintext packet bytes/window.
    pub global_bytes_per_window: u64,
    /// Plaintext packets per source/window.
    pub source_packets_per_window: u32,
    /// Plaintext packet bytes per source/window.
    pub source_bytes_per_window: u64,
    /// Plaintext packets per connection/window.
    pub packets_per_window: u32,
    /// Plaintext packet bytes per connection/window.
    pub bytes_per_window: u64,
    /// Transport malformed/unknown strike cap; M2 intentionally supports only one.
    pub malformed_strike_cap: u8,
    /// Maximum friendly state-machine retries.
    pub max_retries: u8,
    /// Time allowed to finish login.
    pub login_timeout: Duration,
    /// Idle interval between packets.
    pub idle_timeout: Duration,
    /// Graceful connection-task drain bound.
    pub shutdown_grace: Duration,
    /// Bounded codec limits.
    pub codec: CodecLimits,
}

impl Default for LoginRuntimeLimits {
    fn default() -> Self {
        Self {
            global_connections: 256,
            connections_per_source: 8,
            source_capacity: 1_024,
            global_accepts_per_window: 1_000,
            accepts_per_window: 30,
            global_logins_per_window: 1_000,
            logins_per_window: 10,
            username_logins_per_window: 10,
            rate_window: Duration::from_secs(60),
            global_packets_per_window: 10_000,
            global_bytes_per_window: 16 * 1024 * 1024,
            source_packets_per_window: 600,
            source_bytes_per_window: 2 * 1024 * 1024,
            packets_per_window: 60,
            bytes_per_window: 256 * 1024,
            malformed_strike_cap: 1,
            max_retries: 3,
            login_timeout: Duration::from_secs(15),
            idle_timeout: Duration::from_secs(120),
            shutdown_grace: Duration::from_secs(10),
            codec: CodecLimits::default(),
        }
    }
}

/// Complete runtime policy supplied by validated composition.
#[derive(Clone, Debug)]
pub struct LoginRuntimeConfig {
    /// Local-profile-only account auto-create switch.
    pub auto_create_accounts: bool,
    /// Configured starter aggregate; IFF validation begins in M3.
    pub starter: StarterGrant,
    /// Provisional bounded character type allowlist.
    pub allowed_character_types: Vec<u32>,
    /// Sole advertised GameService endpoint.
    pub game_server: AdvertisedGameServer,
    /// Resource limits.
    pub limits: LoginRuntimeLimits,
}

/// Redacted LoginService runtime failure.
#[derive(Debug, Error)]
pub enum LoginRuntimeError {
    /// Listener accept failed.
    #[error("LoginService listener failed")]
    Accept,
    /// Hello or framed I/O failed.
    #[error("LoginService connection I/O failed")]
    Io,
    /// Packet was malformed or invalid for state.
    #[error("LoginService protocol rejected the connection")]
    Protocol,
    /// Repository operation failed.
    #[error("LoginService repository operation failed")]
    Repository,
    /// Credential worker rejected the operation.
    #[error("LoginService credential operation failed")]
    Credential,
    /// Generated handover could not be issued.
    #[error("LoginService handover failed")]
    Handover,
    /// Login or idle timeout elapsed.
    #[error("LoginService connection timed out")]
    Timeout,
    /// Runtime policy is internally inconsistent or unsupported.
    #[error("LoginService runtime configuration is invalid")]
    InvalidConfig,
    /// Connection or credential workers exceeded graceful shutdown.
    #[error("LoginService graceful shutdown timed out")]
    ShutdownTimeout,
    /// Configured limit rejected the connection.
    #[error("LoginService resource limit reached")]
    Limited,
}

/// Generic LoginService composition with no SQLx dependency.
pub struct LoginService<R>
where
    R: AccountRepository + HandoverRepository + 'static,
{
    repository: Arc<R>,
    credentials: BoundedCredentialExecutor,
    config: LoginRuntimeConfig,
    observer: Arc<dyn LoginObserver>,
    connection_ids: AtomicU64,
    global_connections: Arc<Semaphore>,
    source_connections: KeyedCapacityRegistry<SourceAddressPrefix>,
    global_accepts: FixedWindowLimiter<()>,
    accepts: FixedWindowLimiter<SourceAddressPrefix>,
    global_logins: FixedWindowLimiter<()>,
    source_logins: FixedWindowLimiter<SourceAddressPrefix>,
    username_logins: FixedWindowLimiter<NormalizedUsername>,
    global_packets: FixedWindowLimiter<()>,
    global_bytes: FixedWindowLimiter<()>,
    source_packets: FixedWindowLimiter<SourceAddressPrefix>,
    source_bytes: FixedWindowLimiter<SourceAddressPrefix>,
    active_accounts: CapacityRegistry<AccountId>,
}

struct Admission {
    prefix: SourceAddressPrefix,
    _global: OwnedSemaphorePermit,
    _source: KeyedCapacityGuard<SourceAddressPrefix>,
}

impl<R> std::fmt::Debug for LoginService<R>
where
    R: AccountRepository + HandoverRepository + 'static,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoginService")
            .field("config", &self.config)
            .field("credentials", &self.credentials)
            .finish_non_exhaustive()
    }
}

impl<R> LoginService<R>
where
    R: AccountRepository + HandoverRepository + 'static,
{
    /// Creates a service from validated limits and repository composition.
    ///
    /// # Errors
    /// M2 rejects a malformed strike cap other than exactly one because encrypted
    /// transport frames cannot be safely resynchronized after a decode failure.
    pub fn new(
        repository: Arc<R>,
        credentials: BoundedCredentialExecutor,
        config: LoginRuntimeConfig,
        observer: Arc<dyn LoginObserver>,
    ) -> Result<Self, LoginRuntimeError> {
        let limits = &config.limits;
        let invalid = config.allowed_character_types.len() > 64
            || config.starter.items.len() > MAX_STARTER_ITEMS
            || limits.malformed_strike_cap != 1
            || limits.global_connections == 0
            || limits.global_connections > 10_000
            || limits.connections_per_source == 0
            || limits.connections_per_source > limits.global_connections
            || limits.source_capacity == 0
            || limits.source_capacity > 65_536
            || limits.global_accepts_per_window == 0
            || limits.global_accepts_per_window > 1_000_000
            || limits.accepts_per_window == 0
            || limits.accepts_per_window > 1_000_000
            || limits.global_logins_per_window == 0
            || limits.global_logins_per_window > 1_000_000
            || limits.logins_per_window == 0
            || limits.logins_per_window > 1_000_000
            || limits.username_logins_per_window == 0
            || limits.username_logins_per_window > 1_000_000
            || limits.global_packets_per_window == 0
            || limits.global_packets_per_window > 1_000_000
            || limits.source_packets_per_window == 0
            || limits.source_packets_per_window > 1_000_000
            || limits.packets_per_window == 0
            || limits.packets_per_window > 1_000_000
            || limits.global_bytes_per_window == 0
            || limits.global_bytes_per_window > 1024 * 1024 * 1024
            || limits.source_bytes_per_window == 0
            || limits.source_bytes_per_window > 1024 * 1024 * 1024
            || limits.bytes_per_window == 0
            || limits.bytes_per_window > 1024 * 1024 * 1024
            || limits.rate_window.is_zero()
            || limits.rate_window > Duration::from_secs(3_600)
            || limits.max_retries == 0
            || limits.max_retries > 10
            || limits.login_timeout.is_zero()
            || limits.login_timeout > Duration::from_secs(3_600)
            || limits.idle_timeout.is_zero()
            || limits.idle_timeout > Duration::from_secs(3_600)
            || limits.shutdown_grace.is_zero()
            || limits.shutdown_grace > Duration::from_secs(300)
            || limits.codec.max_client_frame_bytes < 5
            || limits.codec.max_client_frame_bytes > 65_535
            || limits.codec.max_server_plaintext_bytes < 2
            || limits.codec.max_server_plaintext_bytes > 64 * 1024 * 1024
            || limits.codec.max_expansion_ratio == 0
            || limits.codec.max_expansion_ratio > 1_024;
        if invalid {
            return Err(LoginRuntimeError::InvalidConfig);
        }
        Ok(Self {
            repository,
            credentials,
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
            accepts: FixedWindowLimiter::new(
                limits.source_capacity,
                limits.accepts_per_window,
                limits.rate_window,
            ),
            global_logins: FixedWindowLimiter::new(
                1,
                limits.global_logins_per_window,
                limits.rate_window,
            ),
            source_logins: FixedWindowLimiter::new(
                limits.source_capacity,
                limits.logins_per_window,
                limits.rate_window,
            ),
            username_logins: FixedWindowLimiter::new(
                limits.source_capacity,
                limits.username_logins_per_window,
                limits.rate_window,
            ),
            global_packets: FixedWindowLimiter::new(
                1,
                limits.global_packets_per_window,
                limits.rate_window,
            ),
            global_bytes: FixedWindowLimiter::new_weighted(
                1,
                limits.global_bytes_per_window,
                limits.rate_window,
            ),
            source_packets: FixedWindowLimiter::new(
                limits.source_capacity,
                limits.source_packets_per_window,
                limits.rate_window,
            ),
            source_bytes: FixedWindowLimiter::new_weighted(
                limits.source_capacity,
                limits.source_bytes_per_window,
                limits.rate_window,
            ),
            active_accounts: CapacityRegistry::new(limits.global_connections),
            config,
            observer,
        })
    }

    /// Runs accepts until cancellation, then supervises all connection tasks to a bounded drain.
    ///
    /// # Errors
    /// Returns an actionable redacted listener failure.
    pub async fn serve(
        self: Arc<Self>,
        listener: TcpListener,
        shutdown: CancellationToken,
    ) -> Result<(), LoginRuntimeError> {
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                accepted = listener.accept() => {
                    let (stream, peer) = accepted.map_err(|_| LoginRuntimeError::Accept)?;
                    match self.admit(peer) {
                        Ok(admission) => {
                            while tasks.len() >= self.config.limits.global_connections {
                                let _ = tasks.join_next().await;
                            }
                            let service = Arc::clone(&self);
                            let child_shutdown = shutdown.child_token();
                            tasks.spawn(async move {
                                let _ = service.run_admitted(stream, admission, child_shutdown).await;
                            });
                        }
                        Err(_) => drop(stream),
                    }
                }
                joined = tasks.join_next(), if !tasks.is_empty() => {
                    let _ = joined;
                }
            }
        }
        drop(listener);
        let deadline = tokio::time::Instant::now() + self.config.limits.shutdown_grace;
        let drain = async { while tasks.join_next().await.is_some() {} };
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if timeout(remaining, drain).await.is_err() {
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if !self.credentials.wait_idle(remaining).await {
            return Err(LoginRuntimeError::ShutdownTimeout);
        }
        Ok(())
    }

    fn admit(&self, peer: SocketAddr) -> Result<Admission, LoginRuntimeError> {
        let prefix = SourceAddressPrefix::from_ip(peer.ip());
        let admission_time = Instant::now();
        if self.global_accepts.check((), admission_time) != RateDecision::Allowed {
            self.observer.rate_limited(RateLimitClass::AcceptGlobal);
            return Err(LoginRuntimeError::Limited);
        }
        if self.accepts.check(prefix.clone(), admission_time) != RateDecision::Allowed {
            self.observer.rate_limited(RateLimitClass::AcceptSource);
            return Err(LoginRuntimeError::Limited);
        }
        let global = Arc::clone(&self.global_connections)
            .try_acquire_owned()
            .map_err(|_| {
                self.observer.rate_limited(RateLimitClass::ConnectionGlobal);
                LoginRuntimeError::Limited
            })?;
        let source = self
            .source_connections
            .acquire(prefix.clone())
            .map_err(|_| {
                self.observer.rate_limited(RateLimitClass::ConnectionSource);
                LoginRuntimeError::Limited
            })?;
        Ok(Admission {
            prefix,
            _global: global,
            _source: source,
        })
    }

    async fn run_admitted(
        self: Arc<Self>,
        mut stream: TcpStream,
        admission: Admission,
        shutdown: CancellationToken,
    ) -> Result<(), LoginRuntimeError> {
        let prefix = admission.prefix.clone();
        let connection_id = ConnectionId(self.connection_ids.fetch_add(1, Ordering::Relaxed));
        self.observer.accepted(connection_id, &prefix);
        let span = tracing::info_span!(
            "connection",
            connection_id = connection_id.get(),
            service = "login",
            client_profile = "us_852",
            source_prefix = %prefix,
            account_id = tracing::field::Empty,
        );
        let result = async {
            let key = (OsRng.next_u32() & 0x0f) as u8;
            let hello = us852_login_hello(key).map_err(|error| {
                self.observer.protocol_error(encode_error_class(&error));
                LoginRuntimeError::Protocol
            })?;
            stream
                .write_all(&hello)
                .await
                .map_err(|_| LoginRuntimeError::Io)?;
            let codec = FrameCodec::new(key, ServiceKind::Login, self.config.limits.codec);
            let framed = Framed::new(stream, codec);
            let connection_shutdown = shutdown.clone();
            tokio::select! {
                biased;
                result = timeout(
                    self.config.limits.login_timeout,
                    self.run_connection(framed, prefix, connection_shutdown),
                ) => result.map_err(|_| LoginRuntimeError::Timeout)?,
                () = shutdown.cancelled() => Ok(ConnectionTermination::Cancelled),
            }
        }
        .instrument(span)
        .await;
        drop(admission);
        let outcome = match &result {
            Ok(outcome) => *outcome,
            Err(LoginRuntimeError::Timeout) => ConnectionTermination::Timeout,
            Err(LoginRuntimeError::Limited) => ConnectionTermination::Limited,
            Err(LoginRuntimeError::Protocol) => ConnectionTermination::Protocol,
            Err(_) => ConnectionTermination::Error,
        };
        self.observer.closed(outcome);
        result.map(drop)
    }

    async fn run_connection(
        &self,
        mut framed: Framed<TcpStream, FrameCodec>,
        source: SourceAddressPrefix,
        shutdown: CancellationToken,
    ) -> Result<ConnectionTermination, LoginRuntimeError> {
        let started = Instant::now();
        let mut packet_window = LocalRateWindow::new(self.config.limits.rate_window);
        let mut machine = LoginStateMachine::new(self.config.limits.max_retries)
            .map_err(|_| LoginRuntimeError::Protocol)?;
        let mut account: Option<AuthenticationRecord> = None;
        let mut _presence: Option<RegistryGuard<AccountId>> = None;
        let mut handover_token: Option<Zeroizing<Vec<u8>>> = None;

        loop {
            match machine.state() {
                LoginState::Complete => return Ok(ConnectionTermination::Completed),
                LoginState::Closed => return Ok(ConnectionTermination::Rejected),
                _ => {}
            }
            let login_remaining = self
                .config
                .limits
                .login_timeout
                .checked_sub(started.elapsed())
                .ok_or(LoginRuntimeError::Timeout)?;
            let wait = login_remaining.min(self.config.limits.idle_timeout);
            let frame = tokio::select! {
                biased;
                () = shutdown.cancelled() => {
                    let _ = machine.apply(LoginEvent::Disconnect);
                    return Ok(ConnectionTermination::Cancelled);
                }
                result = timeout(wait, framed.next()) => {
                    result.map_err(|_| LoginRuntimeError::Timeout)?
                }
            };
            let Some(frame) = frame else {
                let _ = machine.apply(LoginEvent::Disconnect);
                return Ok(ConnectionTermination::PeerClosed);
            };
            let frame = match frame {
                Ok(frame) => frame,
                Err(error) => {
                    self.observer.protocol_error(decode_error_class(&error));
                    return Err(LoginRuntimeError::Protocol);
                }
            };
            let plaintext_bytes = frame.payload.len().saturating_add(2);
            let weighted_bytes = u64::try_from(plaintext_bytes).map_or(u64::MAX, |value| value);
            let rate_time = Instant::now();
            if self.global_packets.check((), rate_time) != RateDecision::Allowed {
                self.observer.rate_limited(RateLimitClass::PacketGlobal);
                return Err(LoginRuntimeError::Limited);
            }
            if self
                .global_bytes
                .check_weighted((), rate_time, weighted_bytes)
                != RateDecision::Allowed
            {
                self.observer.rate_limited(RateLimitClass::BytesGlobal);
                return Err(LoginRuntimeError::Limited);
            }
            if self.source_packets.check(source.clone(), rate_time) != RateDecision::Allowed {
                self.observer.rate_limited(RateLimitClass::PacketSource);
                return Err(LoginRuntimeError::Limited);
            }
            if self
                .source_bytes
                .check_weighted(source.clone(), rate_time, weighted_bytes)
                != RateDecision::Allowed
            {
                self.observer.rate_limited(RateLimitClass::BytesSource);
                return Err(LoginRuntimeError::Limited);
            }
            if !packet_window.admit(
                plaintext_bytes,
                self.config.limits.packets_per_window,
                self.config.limits.bytes_per_window,
            ) {
                self.observer
                    .rate_limited(RateLimitClass::PacketOrBytesConnection);
                return Err(LoginRuntimeError::Limited);
            }
            self.observer.frame("in", frame.opcode, plaintext_bytes);

            match machine.state() {
                LoginState::AwaitLogin if frame.opcode == 0x0001 => {
                    let authenticated = tokio::select! {
                        biased;
                        () = shutdown.cancelled() => {
                            let _ = machine.apply(LoginEvent::Disconnect);
                            return Ok(ConnectionTermination::Cancelled);
                        }
                        result = self.authenticate(&frame, &source) => result?,
                    };
                    let Some(authenticated) = authenticated else {
                        self.observer.login("rejected");
                        let next = machine
                            .apply(LoginEvent::AuthenticationRejected)
                            .map_err(|_| LoginRuntimeError::Protocol)?;
                        self.send(
                            &mut framed,
                            &LoginResult::Error(LOGIN_ERROR_INVALID_CREDENTIALS),
                        )
                        .await?;
                        if next == LoginState::Closed {
                            return Ok(ConnectionTermination::Rejected);
                        }
                        continue;
                    };
                    let guard = match self.active_accounts.acquire(authenticated.account.id) {
                        Ok(guard) => guard,
                        Err(RegistryError::Duplicate) => {
                            self.observer.login("duplicate");
                            self.send(
                                &mut framed,
                                &LoginResult::Error(LOGIN_ERROR_DUPLICATE_CONNECTION),
                            )
                            .await?;
                            return Ok(ConnectionTermination::Rejected);
                        }
                        Err(RegistryError::Capacity) => return Err(LoginRuntimeError::Limited),
                    };
                    self.observer.authenticated(authenticated.account.id);
                    self.observer.login("success");
                    let setup = authenticated.setup_state;
                    machine
                        .apply(LoginEvent::Authenticated(setup))
                        .map_err(|_| LoginRuntimeError::Protocol)?;
                    self.send_login_result(&mut framed, &authenticated).await?;
                    account = Some(authenticated);
                    _presence = Some(guard);
                    if machine.state() == LoginState::IssueHandover {
                        let token = self
                            .issue_handover(&mut framed, &mut machine, account.as_ref(), &source)
                            .await?;
                        handover_token = Some(token);
                    }
                }
                LoginState::AwaitNicknameCheckOrSet if frame.opcode == 0x0007 => {
                    let packet = self.decode_packet::<CheckNickname>(&frame)?;
                    let nickname = nickname_from_wire(&packet.nickname, self.observer.as_ref())?;
                    let available = self
                        .repository_call(self.repository.nickname_available(nickname.normalized()))
                        .await
                        .map_err(|_| LoginRuntimeError::Repository)?;
                    let next = machine
                        .apply(LoginEvent::NicknameChecked { available })
                        .map_err(|_| LoginRuntimeError::Protocol)?;
                    self.send(
                        &mut framed,
                        &NicknameCheckResult {
                            unknown_result: u32::from(!available),
                            nickname: packet.nickname,
                        },
                    )
                    .await?;
                    if next == LoginState::Closed {
                        return Ok(ConnectionTermination::Rejected);
                    }
                }
                LoginState::AwaitNicknameCheckOrSet if frame.opcode == 0x0006 => {
                    let packet = self.decode_packet::<SetNickname>(&frame)?;
                    let nickname = nickname_from_wire(&packet.nickname, self.observer.as_ref())?;
                    let account_id = account
                        .as_ref()
                        .map(|record| record.account.id)
                        .ok_or(LoginRuntimeError::Protocol)?;
                    match self
                        .repository_call(self.repository.set_nickname(account_id, nickname))
                        .await
                    {
                        Ok(()) => {}
                        Err(RepositoryError::DuplicateNickname) => {
                            let next = machine
                                .apply(LoginEvent::NicknameRejected)
                                .map_err(|_| LoginRuntimeError::Protocol)?;
                            self.send(
                                &mut framed,
                                &NicknameCheckResult {
                                    unknown_result: 1,
                                    nickname: packet.nickname,
                                },
                            )
                            .await?;
                            if next == LoginState::Closed {
                                return Ok(ConnectionTermination::Rejected);
                            }
                            continue;
                        }
                        Err(_) => return Err(LoginRuntimeError::Repository),
                    }
                    machine
                        .apply(LoginEvent::NicknameSet {
                            needs_character: false,
                        })
                        .map_err(|_| LoginRuntimeError::Protocol)?;
                    // A successful set is answered with the login result, not another nickname
                    // check response: upstream breaks out of its nickname loop straight into the
                    // common success tail. Answering with `0x000e` leaves a real client sitting on
                    // an inert server list until the login deadline closes the connection.
                    let record = account.as_ref().ok_or(LoginRuntimeError::Protocol)?;
                    self.send(
                        &mut framed,
                        &LoginResult::Success(LoginSuccess {
                            username: record.account.username_display.as_bytes().to_vec(),
                            user_id: u32::try_from(record.account.id.get())
                                .map_err(|_| LoginRuntimeError::Protocol)?,
                            unknown: UnknownBytes([0; 14]),
                            // Upstream echoes the nickname it just persisted.
                            nickname: packet.nickname.clone(),
                        }),
                    )
                    .await?;
                    let token = self
                        .issue_handover(&mut framed, &mut machine, account.as_ref(), &source)
                        .await?;
                    handover_token = Some(token);
                }
                LoginState::AwaitCharacterSelect if frame.opcode == 0x0008 => {
                    let packet = self.decode_packet::<SelectCharacter>(&frame)?;
                    if !self
                        .config
                        .allowed_character_types
                        .contains(&packet.character_id)
                    {
                        // The identifier is catalog data, not credential or personal data, and
                        // an operator whose allowlist is narrower than the client's roster has
                        // no other way to see which selection was refused.
                        tracing::debug!(
                            character_id = packet.character_id,
                            "refused a character selection outside the configured allowlist"
                        );
                        return Err(LoginRuntimeError::Protocol);
                    }
                    let account_id = account
                        .as_ref()
                        .map(|record| record.account.id)
                        .ok_or(LoginRuntimeError::Protocol)?;
                    let mut starter = self.config.starter.clone();
                    starter.character.item_type_id = packet.character_id.into();
                    // An account that reaches setup without a starter — an operator-created one,
                    // say — is granted it here, which is the original path. An auto-created
                    // account already holds a provisionally granted starter, and replaying the
                    // grant with the character the player actually picked is a drift error by
                    // design. So on drift, repoint the provisional character and replay: the
                    // second grant succeeding is what proves the whole aggregate now agrees with
                    // the selection, rather than assuming the repoint was sufficient.
                    let granted = self
                        .repository_call(self.repository.grant_starter(account_id, starter.clone()))
                        .await;
                    match granted {
                        Ok(_) => {}
                        Err(RepositoryError::InvalidStarterGrant) => {
                            self.repository_call(
                                self.repository.select_starter_character(
                                    account_id,
                                    packet.character_id.into(),
                                ),
                            )
                            .await
                            .map_err(|_| LoginRuntimeError::Repository)?;
                            self.repository_call(
                                self.repository.grant_starter(account_id, starter),
                            )
                            .await
                            .map_err(|_| LoginRuntimeError::Repository)?;
                        }
                        Err(_) => return Err(LoginRuntimeError::Repository),
                    }
                    machine
                        .apply(LoginEvent::CharacterSelected)
                        .map_err(|_| LoginRuntimeError::Protocol)?;
                    // Upstream documents the login packet being resent with `success` once the
                    // character is selected, and a real client blocks on "Waiting for server's
                    // response." until it arrives. The nickname is deliberately empty: upstream
                    // records official servers returning an empty one here even when set.
                    let record = account.as_ref().ok_or(LoginRuntimeError::Protocol)?;
                    self.send(
                        &mut framed,
                        &LoginResult::Success(LoginSuccess {
                            username: record.account.username_display.as_bytes().to_vec(),
                            user_id: u32::try_from(record.account.id.get())
                                .map_err(|_| LoginRuntimeError::Protocol)?,
                            unknown: UnknownBytes([0; 14]),
                            nickname: Vec::new(),
                        }),
                    )
                    .await?;
                    let token = self
                        .issue_handover(&mut framed, &mut machine, account.as_ref(), &source)
                        .await?;
                    handover_token = Some(token);
                }
                LoginState::AwaitServerSelect if frame.opcode == 0x0003 => {
                    let packet = self.decode_packet::<SelectServer>(&frame)?;
                    if packet.server_id != self.config.game_server.id {
                        return Err(LoginRuntimeError::Protocol);
                    }
                    let token = handover_token.take().ok_or(LoginRuntimeError::Handover)?;
                    self.send(
                        &mut framed,
                        &SessionKey {
                            unknown: UnknownBytes([0; 4]),
                            session_key: token.to_vec(),
                        },
                    )
                    .await?;
                    machine
                        .apply(LoginEvent::ServerSelected)
                        .map_err(|_| LoginRuntimeError::Protocol)?;
                }
                _ => {
                    if is_known_opcode(frame.opcode) {
                        self.observer
                            .protocol_error(ProtocolMetricClass::InvalidState);
                    } else {
                        self.observer
                            .protocol_error(ProtocolMetricClass::UnknownOpcode);
                        self.observer
                            .unknown_opcode(unknown_opcode_bucket(frame.opcode));
                    }
                    return Err(LoginRuntimeError::Protocol);
                }
            }
        }
    }

    async fn authenticate(
        &self,
        frame: &InboundFrame,
        source: &SourceAddressPrefix,
    ) -> Result<Option<AuthenticationRecord>, LoginRuntimeError> {
        let now = Instant::now();
        if self.global_logins.check((), now) != RateDecision::Allowed {
            self.observer.rate_limited(RateLimitClass::LoginGlobal);
            return Err(LoginRuntimeError::Limited);
        }
        if self.source_logins.check(source.clone(), now) != RateDecision::Allowed {
            self.observer.rate_limited(RateLimitClass::LoginSource);
            return Err(LoginRuntimeError::Limited);
        }
        let packet = self.decode_packet::<pangya_protocol::LoginRequest>(frame)?;
        let username_text = std::str::from_utf8(&packet.username).map_err(|_| {
            self.observer.protocol_error(ProtocolMetricClass::Decode);
            LoginRuntimeError::Protocol
        })?;
        let secret_text = std::str::from_utf8(&packet.password).map_err(|_| {
            self.observer.protocol_error(ProtocolMetricClass::Decode);
            LoginRuntimeError::Protocol
        })?;
        let username = Username::parse(username_text).map_err(|_| {
            self.observer.protocol_error(ProtocolMetricClass::Decode);
            LoginRuntimeError::Protocol
        })?;
        let secret = CanonicalTransportSecret::parse(secret_text).map_err(|_| {
            self.observer.protocol_error(ProtocolMetricClass::Decode);
            LoginRuntimeError::Protocol
        })?;
        if self
            .username_logins
            .check(username.normalized().clone(), now)
            != RateDecision::Allowed
        {
            self.observer.rate_limited(RateLimitClass::LoginUsername);
            return Err(LoginRuntimeError::Limited);
        }
        let loaded = self
            .repository_call(self.repository.load_authentication(username.normalized()))
            .await
            .map_err(|_| LoginRuntimeError::Repository)?;
        if let Some(record) = loaded {
            if record.account.status != AccountStatus::Active {
                return Ok(None);
            }
            return match self
                .credentials
                .verify(secret, record.credential_hash.clone())
                .await
            {
                Ok(()) => Ok(Some(record)),
                Err(CredentialExecutorError::Credential(CredentialError::Verification)) => Ok(None),
                Err(CredentialExecutorError::Credential(
                    CredentialError::UnsupportedPolicy | CredentialError::Hashing,
                )) => {
                    self.observer
                        .credential_worker(CredentialWorkerOutcome::OperationalError);
                    Err(LoginRuntimeError::Credential)
                }
                Err(CredentialExecutorError::Overloaded) => {
                    self.observer
                        .credential_worker(CredentialWorkerOutcome::Overload);
                    Err(LoginRuntimeError::Credential)
                }
                Err(CredentialExecutorError::Timeout) => {
                    self.observer
                        .credential_worker(CredentialWorkerOutcome::Timeout);
                    Err(LoginRuntimeError::Credential)
                }
                Err(CredentialExecutorError::Worker) => {
                    self.observer
                        .credential_worker(CredentialWorkerOutcome::OperationalError);
                    Err(LoginRuntimeError::Credential)
                }
            };
        }
        if !self.config.auto_create_accounts {
            return Ok(None);
        }
        let hash = match self.credentials.hash(secret).await {
            Ok(hash) => hash,
            Err(CredentialExecutorError::Overloaded) => {
                self.observer
                    .credential_worker(CredentialWorkerOutcome::Overload);
                return Err(LoginRuntimeError::Credential);
            }
            Err(CredentialExecutorError::Timeout) => {
                self.observer
                    .credential_worker(CredentialWorkerOutcome::Timeout);
                return Err(LoginRuntimeError::Credential);
            }
            Err(CredentialExecutorError::Worker) => {
                self.observer
                    .credential_worker(CredentialWorkerOutcome::OperationalError);
                return Err(LoginRuntimeError::Credential);
            }
            Err(CredentialExecutorError::Credential(_)) => {
                self.observer
                    .credential_worker(CredentialWorkerOutcome::OperationalError);
                return Err(LoginRuntimeError::Credential);
            }
        };
        let aggregate = self
            .repository_call(self.repository.create_account(NewAccount {
                username,
                credential_hash: hash.clone(),
                nickname: None,
                starter: self.config.starter.clone(),
            }))
            .await
            .map_err(|_| LoginRuntimeError::Repository)?;
        Ok(Some(AuthenticationRecord {
            account: aggregate.account,
            credential_hash: hash,
            setup_state: aggregate.profile.setup_state,
        }))
    }

    async fn send_login_result(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        record: &AuthenticationRecord,
    ) -> Result<(), LoginRuntimeError> {
        let result = match record.setup_state {
            SetupState::NeedsNickname => LoginResult::NeedSetNickname,
            SetupState::NeedsStarter => LoginResult::NeedSelectCharacter,
            SetupState::Complete => LoginResult::Success(LoginSuccess {
                username: record.account.username_display.as_bytes().to_vec(),
                user_id: u32::try_from(record.account.id.get())
                    .map_err(|_| LoginRuntimeError::Protocol)?,
                unknown: UnknownBytes([0; 14]),
                nickname: Vec::new(),
            }),
        };
        self.send(framed, &result).await
    }

    async fn issue_handover(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        machine: &mut LoginStateMachine,
        account: Option<&AuthenticationRecord>,
        source: &SourceAddressPrefix,
    ) -> Result<Zeroizing<Vec<u8>>, LoginRuntimeError> {
        let account = account.ok_or(LoginRuntimeError::Protocol)?;
        let generated = generate_handover(
            account.account.id,
            DomainServiceKind::Game,
            source.clone(),
            SystemTime::now(),
        )
        .map_err(|_| LoginRuntimeError::Handover)?;
        self.repository_call(self.repository.issue(generated.record))
            .await
            .map_err(|_| LoginRuntimeError::Handover)?;
        machine
            .apply(LoginEvent::HandoverIssued)
            .map_err(|_| LoginRuntimeError::Protocol)?;
        let bearer = Zeroizing::new(generated.token.expose_secret().as_bytes().to_vec());
        // `0x0010` is what the client remembers and later echoes back to GameService as its
        // login key; `0x0003` after server selection carries the same value again. Sending only
        // `0x0003` leaves the client with nothing to echo, and its GameService auth then arrives
        // with an empty key. Upstream sends both, this one first.
        self.send(
            framed,
            &LoginKey {
                login_key: bearer.to_vec(),
            },
        )
        .await?;
        self.send(
            framed,
            &ChatMacros {
                values: std::array::from_fn(|_| Vec::new()),
            },
        )
        .await?;
        self.send(framed, &EmptyMessageServerList).await?;
        self.send(framed, &self.server_list()).await?;
        Ok(bearer)
    }

    fn server_list(&self) -> GameServerList {
        GameServerList {
            servers: vec![GameServerEntry {
                name: self.config.game_server.name.as_bytes().to_vec(),
                id: u32::from(self.config.game_server.id),
                max_users: self.config.game_server.capacity,
                num_users: 0,
                ip_address: self.config.game_server.ipv4.as_bytes().to_vec(),
                port: self.config.game_server.port,
                unknown2: UnknownBytes([0; 2]),
                flags: UnknownBytes([0; 2]),
                unknown3: UnknownBytes([0; 6]),
                boosts: 0,
                unknown4: UnknownBytes([0; 6]),
                char_icon: 0,
                // Upstream LoginService advertises no channels here and lets GameService send its
                // own channel list after the client connects; only the count byte is on the wire.
                channels: Vec::new(),
            }],
        }
    }

    async fn repository_call<T, E, F>(&self, future: F) -> Result<T, E>
    where
        F: Future<Output = Result<T, E>>,
    {
        let started = Instant::now();
        let result = future.await;
        let class = if result.is_err() {
            DbQueryClass::Error
        } else if started.elapsed() <= Duration::from_millis(50) {
            DbQueryClass::Fast
        } else {
            DbQueryClass::Slow
        };
        self.observer.db_query(class);
        result
    }

    fn decode_packet<T: DecodePacket>(&self, frame: &InboundFrame) -> Result<T, LoginRuntimeError> {
        let mut reader = PacketReader::new(
            &frame.payload,
            pangya_protocol::Direction::ClientToServer,
            ServiceKind::Login,
            Some(frame.opcode),
        );
        T::decode(&mut reader, &CompatibilityProfile::US_852).map_err(|error| {
            self.observer.protocol_error(decode_error_class(&error));
            LoginRuntimeError::Protocol
        })
    }

    async fn send<T: EncodePacket>(
        &self,
        framed: &mut Framed<TcpStream, FrameCodec>,
        packet: &T,
    ) -> Result<(), LoginRuntimeError> {
        let payload =
            encode_packet_payload(packet, &CompatibilityProfile::US_852).map_err(|error| {
                self.observer.protocol_error(encode_error_class(&error));
                LoginRuntimeError::Protocol
            })?;
        let payload_len = payload.len();
        let salt = OsRng.next_u32() as u8;
        framed
            .send(OutboundFrame {
                opcode: T::OPCODE,
                payload,
                salt,
            })
            .await
            .map_err(|error| {
                self.observer.protocol_error(encode_error_class(&error));
                LoginRuntimeError::Io
            })?;
        self.observer.frame("out", T::OPCODE, payload_len + 2);
        Ok(())
    }
}

fn decode_error_class(error: &pangya_protocol::PacketDecodeError) -> ProtocolMetricClass {
    match error {
        pangya_protocol::PacketDecodeError::Io(_) => ProtocolMetricClass::Io,
        pangya_protocol::PacketDecodeError::Context { class, .. } => match class {
            ErrorClass::Truncated => ProtocolMetricClass::DecodeTruncated,
            ErrorClass::Limit => ProtocolMetricClass::DecodeLimit,
            ErrorClass::Overflow => ProtocolMetricClass::DecodeOverflow,
            ErrorClass::MissingTerminator => ProtocolMetricClass::DecodeMissingTerminator,
            ErrorClass::Crypto => ProtocolMetricClass::Crypto,
            ErrorClass::Invalid => ProtocolMetricClass::DecodeInvalid,
        },
    }
}

fn encode_error_class(error: &PacketEncodeError) -> ProtocolMetricClass {
    match error {
        PacketEncodeError::Io(_) => ProtocolMetricClass::Io,
        PacketEncodeError::Crypto(_) => ProtocolMetricClass::Crypto,
        PacketEncodeError::Invalid { .. }
        | PacketEncodeError::Limit { .. }
        | PacketEncodeError::Overflow(_)
        | PacketEncodeError::Profile(_) => ProtocolMetricClass::EncodeOrCompress,
    }
}

fn is_known_opcode(opcode: u16) -> bool {
    matches!(opcode, 0x0001 | 0x0003 | 0x0006 | 0x0007 | 0x0008)
}

fn unknown_opcode_bucket(opcode: u16) -> UnknownOpcodeBucket {
    if opcode <= 0x00ff {
        UnknownOpcodeBucket::Low
    } else {
        UnknownOpcodeBucket::Other
    }
}

fn nickname_from_wire(
    bytes: &[u8],
    observer: &dyn LoginObserver,
) -> Result<Nickname, LoginRuntimeError> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        observer.protocol_error(ProtocolMetricClass::Decode);
        LoginRuntimeError::Protocol
    })?;
    Nickname::parse(text).map_err(|_| {
        observer.protocol_error(ProtocolMetricClass::Decode);
        LoginRuntimeError::Protocol
    })
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

    fn admit(&mut self, bytes: usize, packet_limit: u32, byte_limit: u64) -> bool {
        let now = Instant::now();
        if now.saturating_duration_since(self.started) >= self.interval {
            self.started = now;
            self.packets = 0;
            self.bytes = 0;
        }
        self.packets = self.packets.saturating_add(1);
        let converted = u64::try_from(bytes).map_or(u64::MAX, |value| value);
        self.bytes = self.bytes.saturating_add(converted);
        self.packets <= packet_limit && self.bytes <= byte_limit
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn framed_io_errors_have_the_fixed_io_metric_class() {
        let decode = pangya_protocol::PacketDecodeError::Io(io::Error::other("synthetic"));
        let encode = PacketEncodeError::Io(io::Error::other("synthetic"));
        assert_eq!(decode_error_class(&decode), ProtocolMetricClass::Io);
        assert_eq!(encode_error_class(&encode), ProtocolMetricClass::Io);
    }

    #[test]
    fn wire_decode_classes_preserve_fixed_detail() {
        for (class, expected) in [
            (ErrorClass::Truncated, ProtocolMetricClass::DecodeTruncated),
            (ErrorClass::Limit, ProtocolMetricClass::DecodeLimit),
            (ErrorClass::Overflow, ProtocolMetricClass::DecodeOverflow),
            (
                ErrorClass::MissingTerminator,
                ProtocolMetricClass::DecodeMissingTerminator,
            ),
            (ErrorClass::Crypto, ProtocolMetricClass::Crypto),
            (ErrorClass::Invalid, ProtocolMetricClass::DecodeInvalid),
        ] {
            let error = pangya_protocol::PacketDecodeError::Context {
                direction: pangya_protocol::Direction::ClientToServer,
                service: pangya_protocol::ServiceKind::Login,
                opcode: Some(1),
                offset: 0,
                class,
                detail: "synthetic".to_owned(),
            };
            assert_eq!(decode_error_class(&error), expected);
        }
    }
}
