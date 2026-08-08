//! Local synthetic M3 LoginService-to-GameService real-PostgreSQL acceptance.

use std::{
    io,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use pangya_data::Catalog;
use pangya_domain::{
    AccountAggregate, AccountId, AccountRepository as _, AccountStatus, ChatText, CourseId,
    CredentialHash, HandoverRepository as _, IncompleteMatchAbortLimit, ItemTypeId, MatchSeed,
    MemberSnapshot, NewAccount, Nickname, PlayerConnectionId, RoomId, RoomName, RoomPassword,
    RoomSettings, RoomSnapshot, RoomSummary, ServiceKind, SourceAddressPrefix, StarterCharacter,
    StarterGrant, StarterItem, StarterKey, Username, Weather,
};
use pangya_game::{
    EconomyRuntimeConfig, GameRuntimeConfig, GameRuntimeLimits, GameService, SoloRuntimeConfig,
    StrokeRuntimeConfig, UnknownOpcodePolicy, deterministic_conditions,
};
use pangya_login::{
    AdvertisedGameServer, BoundedCredentialExecutor, CredentialPolicy, LoginRuntimeConfig,
    LoginRuntimeLimits, LoginService, generate_handover,
};
use pangya_observability::M2Metrics;
use pangya_protocol::{
    BalanceUpdate, CompatibilityProfile, ConsumeOneRequest, DecodePacket, EconomyCommand,
    EconomyCommandResult, EconomyOutcome, EncodePacket, EquipRequest, EquipmentChanged, FinishHole,
    HoleResult, InventoryChanged, Lie, LoadingComplete, MatchAbortReason, MatchAborted, MatchPhase,
    MatchStarted, PacketWriter, PurchaseCommitted, PurchaseRequestPacket, RepairCommitted,
    RepairRequest, RoomChatEvent, RoomChatRequest, RoomCommand, RoomCommandResult,
    RoomCommandResultResponse, RoomCreateRequest, RoomJoinRequest, RoomKickRequest,
    RoomLeaveRequest, RoomListRequest, RoomListResponse, RoomMembershipEvent, RoomMembershipKind,
    RoomReadyRequest, RoomSettingsRequest, RoomStateRequest, RoomStateResponse,
    ServiceKind as ProtocolServiceKind, ShopPage, ShopPageRequest, ShotAction, ShotActionRelay,
    ShotResult, ShotResultRelay, SoloCommand, SoloCommandOutcome, SoloCommandResult, SoloPhase,
    StartSolo, StartStrokeTwo, StrokeAbortReason, StrokeActionRelay, StrokeBalanceUpdate,
    StrokeCommand, StrokeCommandOutcome, StrokeCommandResult, StrokeCompletion, StrokeGiveUp,
    StrokeLoadingComplete, StrokeMatchAborted, StrokeMatchStarted, StrokePhase, StrokePhaseKind,
    StrokeResultRelay, StrokeShotAction, StrokeShotResult, StrokeStandings, StrokeTurnStarted,
    Weather as ProtocolWeather, decode_packet_payload, encode_packet_payload,
};
use pangya_storage::{MIGRATOR, PgRepository};
use sqlx::PgPool;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    sync::Notify,
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::fmt::MakeWriter;

const SECRET: &str = "0123456789abcdef0123456789abcdef";
/// Generous packet deadline for ordinary E2E assertions. Timeout-path tests use their own short
/// product deadlines, so a missing expected packet fails deterministically instead of hanging.
const E2E_RECEIVE_TIMEOUT: Duration = Duration::from_secs(10);

struct BlockingStrokeCommitRepository {
    inner: PgRepository,
    commit_started: Notify,
    commit_calls: AtomicUsize,
    abort_calls: AtomicUsize,
    /// When set, economy purchases stall past any sane command deadline.
    stall_economy: bool,
}

impl BlockingStrokeCommitRepository {
    fn new(pool: PgPool) -> Self {
        Self {
            inner: PgRepository::new(pool),
            commit_started: Notify::new(),
            commit_calls: AtomicUsize::new(0),
            abort_calls: AtomicUsize::new(0),
            stall_economy: false,
        }
    }

    fn stalling_economy(pool: PgPool) -> Self {
        Self {
            stall_economy: true,
            ..Self::new(pool)
        }
    }
}

impl pangya_domain::HandoverRepository for BlockingStrokeCommitRepository {
    fn issue(
        &self,
        handover: pangya_domain::NewHandover,
    ) -> pangya_domain::RepositoryFuture<'_, Result<(), pangya_domain::HandoverError>> {
        pangya_domain::HandoverRepository::issue(&self.inner, handover)
    }

    fn consume(
        &self,
        request: pangya_domain::ConsumeHandover,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<pangya_domain::AuthenticatedSession, pangya_domain::HandoverError>,
    > {
        pangya_domain::HandoverRepository::consume(&self.inner, request)
    }
}

impl pangya_domain::PlayerRepository for BlockingStrokeCommitRepository {
    fn load_player_snapshot(
        &self,
        account_id: AccountId,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<pangya_domain::PlayerSnapshot, pangya_domain::RepositoryError>,
    > {
        pangya_domain::PlayerRepository::load_player_snapshot(&self.inner, account_id)
    }
}

impl pangya_domain::MatchRepository for BlockingStrokeCommitRepository {
    fn begin_stroke(
        &self,
        request: pangya_domain::BeginStrokeMatch,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<pangya_domain::BeginStrokeMatchOutcome, pangya_domain::MatchRepositoryError>,
    > {
        pangya_domain::MatchRepository::begin_stroke(&self.inner, request)
    }

    fn mark_stroke_in_game(
        &self,
        request: pangya_domain::MarkStrokeInGame,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<pangya_domain::MarkStrokeInGameOutcome, pangya_domain::MatchRepositoryError>,
    > {
        pangya_domain::MatchRepository::mark_stroke_in_game(&self.inner, request)
    }

    fn abort_stroke(
        &self,
        request: pangya_domain::AbortStrokeMatch,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<pangya_domain::AbortStrokeMatchOutcome, pangya_domain::MatchRepositoryError>,
    > {
        self.abort_calls.fetch_add(1, Ordering::Relaxed);
        pangya_domain::MatchRepository::abort_stroke(&self.inner, request)
    }

    fn commit_stroke_match(
        &self,
        _request: pangya_domain::CommitStrokeMatch,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<pangya_domain::StrokeMatchResult, pangya_domain::MatchRepositoryError>,
    > {
        self.commit_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async {
            self.commit_started.notify_one();
            std::future::pending().await
        })
    }

    fn begin_solo(
        &self,
        request: pangya_domain::BeginSoloMatch,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<pangya_domain::BeginSoloMatchOutcome, pangya_domain::MatchRepositoryError>,
    > {
        pangya_domain::MatchRepository::begin_solo(&self.inner, request)
    }

    fn mark_solo_in_game(
        &self,
        request: pangya_domain::MarkSoloInGame,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<pangya_domain::MarkSoloInGameOutcome, pangya_domain::MatchRepositoryError>,
    > {
        pangya_domain::MatchRepository::mark_solo_in_game(&self.inner, request)
    }

    fn abort(
        &self,
        request: pangya_domain::AbortMatch,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<pangya_domain::AbortMatchOutcome, pangya_domain::MatchRepositoryError>,
    > {
        pangya_domain::MatchRepository::abort(&self.inner, request)
    }

    fn commit_solo_hole(
        &self,
        request: pangya_domain::CommitSoloHole,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<pangya_domain::SoloMatchResult, pangya_domain::MatchRepositoryError>,
    > {
        pangya_domain::MatchRepository::commit_solo_hole(&self.inner, request)
    }

    fn abort_incomplete_matches(
        &self,
        limit: IncompleteMatchAbortLimit,
    ) -> pangya_domain::RepositoryFuture<'_, Result<u32, pangya_domain::MatchRepositoryError>> {
        pangya_domain::MatchRepository::abort_incomplete_matches(&self.inner, limit)
    }
}

impl pangya_domain::EconomyRepository for BlockingStrokeCommitRepository {
    fn purchase(
        &self,
        request: pangya_domain::PurchaseRequest,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<
            pangya_domain::EconomyCommit<pangya_domain::PurchaseResult>,
            pangya_domain::EconomyError,
        >,
    > {
        if self.stall_economy {
            return Box::pin(async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                Err(pangya_domain::EconomyError::Storage(
                    pangya_domain::StorageFault::Other,
                ))
            });
        }
        pangya_domain::EconomyRepository::purchase(&self.inner, request)
    }
    fn equip(
        &self,
        request: pangya_domain::EquipmentChange,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<
            pangya_domain::EconomyCommit<pangya_domain::EquipmentChangeResult>,
            pangya_domain::EconomyError,
        >,
    > {
        pangya_domain::EconomyRepository::equip(&self.inner, request)
    }
    fn consume_one(
        &self,
        request: pangya_domain::ConsumeItem,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<
            pangya_domain::EconomyCommit<pangya_domain::ConsumeItemResult>,
            pangya_domain::EconomyError,
        >,
    > {
        pangya_domain::EconomyRepository::consume_one(&self.inner, request)
    }
    fn repair(
        &self,
        request: pangya_domain::RepairItem,
    ) -> pangya_domain::RepositoryFuture<
        '_,
        Result<
            pangya_domain::EconomyCommit<pangya_domain::RepairItemResult>,
            pangya_domain::EconomyError,
        >,
    > {
        pangya_domain::EconomyRepository::repair(&self.inner, request)
    }
}

#[derive(Clone)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);
struct CaptureGuard(Arc<Mutex<Vec<u8>>>);
impl io::Write for CaptureGuard {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("capture"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl<'a> MakeWriter<'a> for CaptureWriter {
    type Writer = CaptureGuard;
    fn make_writer(&'a self) -> Self::Writer {
        CaptureGuard(Arc::clone(&self.0))
    }
}

fn tracing_capture() -> Arc<Mutex<Vec<u8>>> {
    static CAPTURE: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    Arc::clone(CAPTURE.get_or_init(|| {
        let output = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(CaptureWriter(Arc::clone(&output)))
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("capture subscriber");
        output
    }))
}

fn key(value: &str) -> StarterKey {
    StarterKey::parse(value).expect("key")
}

fn starter(item_count: usize, item_type: u32) -> StarterGrant {
    StarterGrant {
        character: StarterCharacter {
            key: key("starter.character"),
            item_type_id: ItemTypeId::new(0x0400_0000),
        },
        items: (0..item_count)
            .map(|index| StarterItem {
                key: key(&format!("starter.item.{index}")),
                item_type_id: ItemTypeId::new(item_type),
                quantity: u32::try_from(index + 1).expect("quantity"),
            })
            .collect(),
        equipped_club_key: (item_count > 0).then(|| key("starter.item.0")),
        equipped_ball_key: None,
    }
}

async fn create_account(
    pool: &PgPool,
    username: &str,
    item_count: usize,
    item_type: u32,
) -> AccountAggregate {
    PgRepository::new(pool.clone())
        .create_account(NewAccount {
            username: Username::parse(username).expect("username"),
            credential_hash: CredentialHash::new("synthetic-game-only".to_owned()),
            nickname: Some(Nickname::parse(&format!("N{username}")).expect("nickname")),
            starter: starter(item_count, item_type),
        })
        .await
        .expect("account")
}

fn catalog() -> Catalog {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pangya-data/tests/fixtures/synthetic-catalog");
    Catalog::load(&root, std::path::Path::new("manifest.toml")).expect("catalog")
}

fn economy_catalog() -> Catalog {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pangya-data/tests/fixtures/synthetic-catalog-v2");
    Catalog::load(&root, std::path::Path::new("manifest.toml")).expect("M7 catalog")
}

fn economy_service(pool: PgPool, metrics: Arc<M2Metrics>) -> Arc<GameService<PgRepository>> {
    economy_service_with(pool, metrics, Some(default_economy()))
}

fn default_economy() -> EconomyRuntimeConfig {
    EconomyRuntimeConfig {
        command_timeout: Duration::from_secs(2),
        commands_per_window: 50,
        page_size: 50,
        max_purchase_quantity: 99,
    }
}

fn economy_service_with(
    pool: PgPool,
    metrics: Arc<M2Metrics>,
    economy: Option<EconomyRuntimeConfig>,
) -> Arc<GameService<PgRepository>> {
    Arc::new(
        GameService::new(
            Arc::new(PgRepository::new(pool)),
            economy_catalog(),
            GameRuntimeConfig {
                channel_id: 1,
                unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
                limits: GameRuntimeLimits {
                    packets_per_window: 200,
                    ..GameRuntimeLimits::default()
                },
                solo_practice: None,
                stroke_two: None,
                economy,
                retail_bootstrap: false,
            },
            metrics,
        )
        .expect("economy service"),
    )
}

/// Authenticates a funded account and enters the channel, ready for economy commands.
async fn connect_economy_client(
    pool: &PgPool,
    address: std::net::SocketAddr,
    account_id: pangya_domain::AccountId,
) -> (TcpStream, u8) {
    let token = issue_token(pool, account_id, SystemTime::now(), ServiceKind::Game).await;
    let (mut stream, key) = connect_game(address).await;
    send_packet(
        &mut stream,
        key,
        1,
        2,
        &auth_payload(account_id.get(), &token),
    )
    .await;
    read_bootstrap(&mut stream, key, 1).await;
    send_packet(&mut stream, key, 2, 4, &1_u32.to_le_bytes()).await;
    assert_eq!(receive_packet(&mut stream, key).await.0, 0x004e);
    (stream, key)
}

/// Builds a test-only generated catalog whose local Course record is course 1, hole 1, par 3.
fn m5_catalog() -> Catalog {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pangya-data/tests/fixtures/synthetic-catalog");
    let root = std::env::temp_dir().join(format!("pangya-m5-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).expect("unique M5 catalog directory");
    for filename in ["character.bin", "club_set.bin", "ball.bin"] {
        std::fs::copy(fixture.join(filename), root.join(filename)).expect("copy catalog family");
    }
    std::fs::write(
        root.join("Course.bin"),
        [1, 0, 4, 0, 1, 0, 0, 0, 1, 0, 0, 0, 3],
    )
    .expect("generated Course");
    std::fs::write(
        root.join("manifest.toml"),
        r#"manifest_version = 1

[[files]]
filename = "character.bin"
sha256 = "8e634d84dbf7ba1d9c8b8515d6ca1a4e0e87e270df97e28427d58dd53fd5b5c4"
kind = "character"
count = 1
binding = 1
version = 1
record_size = 8

[[files]]
filename = "club_set.bin"
sha256 = "2bc63711f5c8e4abbda812fe5a413b49250830c6b1861fc7c2be39ac2ffb574e"
kind = "club_set"
count = 1
binding = 2
version = 1
record_size = 8

[[files]]
filename = "ball.bin"
sha256 = "7f270c607407c9fecedefa12ae5c69408a41badfd82c989d4cbc67ab4765045e"
kind = "ball"
count = 1
binding = 3
version = 1
record_size = 8

[[files]]
filename = "Course.bin"
sha256 = "e1c73f87e1206253eef0097f67a259eb0721a7351564bd627df5c07c30e12611"
kind = "course"
count = 1
binding = 4
version = 1
record_size = 5
"#,
    )
    .expect("generated catalog manifest");
    let catalog = Catalog::load(&root, std::path::Path::new("manifest.toml")).expect("M5 catalog");
    std::fs::remove_dir_all(&root).expect("remove M5 catalog directory");
    catalog
}

fn solo_service(
    pool: PgPool,
    limits: GameRuntimeLimits,
    metrics: Arc<M2Metrics>,
    loading_timeout: Duration,
    shot_packets_per_window: u32,
) -> Arc<GameService<PgRepository>> {
    let catalog = m5_catalog();
    let course = catalog
        .one_hole_course(CourseId::new(1).expect("course ID"))
        .expect("one-hole course");
    Arc::new(
        GameService::new(
            Arc::new(PgRepository::new(pool)),
            catalog.clone(),
            GameRuntimeConfig {
                channel_id: 1,
                unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
                limits,
                solo_practice: Some(SoloRuntimeConfig {
                    course,
                    catalog_fingerprint: catalog.fingerprint(),
                    loading_timeout,
                    commit_timeout: Duration::from_secs(1),
                    max_strokes: 10,
                    startup_recovery_limit: IncompleteMatchAbortLimit::new(100)
                        .expect("recovery limit"),
                    shot_packets_per_window,
                }),
                stroke_two: None,
                economy: None,
                retail_bootstrap: false,
            },
            metrics,
        )
        .expect("solo game"),
    )
}

fn stroke_service(
    pool: PgPool,
    limits: GameRuntimeLimits,
    metrics: Arc<M2Metrics>,
) -> Arc<GameService<PgRepository>> {
    stroke_service_with_deadlines(
        pool,
        limits,
        metrics,
        Duration::from_secs(30),
        Duration::from_secs(30),
        Duration::from_secs(120),
    )
}

fn stroke_service_with_deadlines(
    pool: PgPool,
    limits: GameRuntimeLimits,
    metrics: Arc<M2Metrics>,
    loading_timeout: Duration,
    turn_timeout: Duration,
    game_timeout: Duration,
) -> Arc<GameService<PgRepository>> {
    let catalog = m5_catalog();
    let course = catalog
        .one_hole_course(CourseId::new(1).expect("course ID"))
        .expect("one-hole course");
    Arc::new(
        GameService::new(
            Arc::new(PgRepository::new(pool)),
            catalog.clone(),
            GameRuntimeConfig {
                channel_id: 1,
                unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
                limits,
                solo_practice: None,
                stroke_two: Some(StrokeRuntimeConfig {
                    course,
                    catalog_fingerprint: catalog.fingerprint(),
                    loading_timeout,
                    turn_timeout,
                    game_timeout,
                    commit_timeout: Duration::from_secs(2),
                    max_strokes: 10,
                    startup_recovery_limit: IncompleteMatchAbortLimit::new(100)
                        .expect("recovery limit"),
                    shot_packets_per_window: 120,
                }),
                economy: None,
                retail_bootstrap: false,
            },
            metrics,
        )
        .expect("stroke game"),
    )
}

fn game_service_with_policy(
    pool: PgPool,
    limits: GameRuntimeLimits,
    metrics: Arc<M2Metrics>,
    unknown_opcode_policy: UnknownOpcodePolicy,
) -> Arc<GameService<PgRepository>> {
    Arc::new(
        GameService::new(
            Arc::new(PgRepository::new(pool)),
            catalog(),
            GameRuntimeConfig {
                channel_id: 1,
                unknown_opcode_policy,
                limits,
                solo_practice: None,
                stroke_two: None,
                economy: None,
                retail_bootstrap: false,
            },
            metrics,
        )
        .expect("game"),
    )
}

fn game_service(
    pool: PgPool,
    limits: GameRuntimeLimits,
    metrics: Arc<M2Metrics>,
) -> Arc<GameService<PgRepository>> {
    game_service_with_policy(pool, limits, metrics, UnknownOpcodePolicy::Disconnect)
}

async fn start_service<R>(
    service: Arc<GameService<R>>,
) -> (
    std::net::SocketAddr,
    CancellationToken,
    tokio::task::JoinHandle<Result<(), pangya_game::GameRuntimeError>>,
)
where
    R: pangya_domain::HandoverRepository
        + pangya_domain::PlayerRepository
        + pangya_domain::MatchRepository
        + pangya_domain::EconomyRepository
        + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let shutdown = CancellationToken::new();
    let child = shutdown.clone();
    let task = tokio::spawn(async move { service.serve(listener, child).await });
    (address, shutdown, task)
}

async fn start_game(
    pool: PgPool,
    limits: GameRuntimeLimits,
) -> (
    std::net::SocketAddr,
    CancellationToken,
    tokio::task::JoinHandle<Result<(), pangya_game::GameRuntimeError>>,
    Arc<M2Metrics>,
) {
    let metrics = Arc::new(M2Metrics::default());
    let service = game_service(pool, limits, metrics.clone());
    let (address, shutdown, task) = start_service(service).await;
    (address, shutdown, task, metrics)
}

async fn connect_game(address: std::net::SocketAddr) -> (TcpStream, u8) {
    let mut stream = TcpStream::connect(address).await.expect("connect");
    let mut hello = [0_u8; 4];
    tokio::time::timeout(E2E_RECEIVE_TIMEOUT, stream.read_exact(&mut hello))
        .await
        .expect("bounded game hello")
        .expect("hello");
    assert!(hello[3] <= 0x0f);
    (stream, hello[3])
}

/// Reads the nine-byte retail GameService hello, pinning its constants over real TCP.
///
/// A real client that receives the shorter synthetic hello reads the following frame at the
/// wrong offset and disconnects, so the length difference is a compatibility surface and is
/// asserted here rather than only in a unit test.
async fn connect_game_retail(address: std::net::SocketAddr) -> (TcpStream, u8) {
    let mut stream = TcpStream::connect(address).await.expect("connect");
    let mut hello = [0_u8; 9];
    tokio::time::timeout(E2E_RECEIVE_TIMEOUT, stream.read_exact(&mut hello))
        .await
        .expect("bounded retail game hello")
        .expect("hello");
    assert_eq!(
        &hello[..8],
        &[0x00, 0x06, 0x00, 0x00, 0x3f, 0x00, 0x01, 0x01]
    );
    assert!(hello[8] <= 0x0f);
    (stream, hello[8])
}

async fn connect_login(address: std::net::SocketAddr) -> (TcpStream, u8) {
    let mut stream = TcpStream::connect(address).await.expect("connect");
    let mut hello = [0_u8; 14];
    tokio::time::timeout(E2E_RECEIVE_TIMEOUT, stream.read_exact(&mut hello))
        .await
        .expect("bounded login hello")
        .expect("hello");
    (stream, hello[6])
}

async fn send_packet(stream: &mut TcpStream, key: u8, salt: u8, opcode: u16, payload: &[u8]) {
    let mut plain = Vec::with_capacity(payload.len() + 2);
    plain.extend_from_slice(&opcode.to_le_bytes());
    plain.extend_from_slice(payload);
    let encrypted = pangya_crypto::client_encrypt(&plain, key, salt).expect("encrypt");
    stream.write_all(&encrypted).await.expect("send");
}

async fn receive_packet(stream: &mut TcpStream, key: u8) -> (u16, Vec<u8>) {
    tokio::time::timeout(E2E_RECEIVE_TIMEOUT, async {
        let mut header = [0_u8; 3];
        stream.read_exact(&mut header).await.expect("header");
        let total = usize::from(u16::from_le_bytes([header[1], header[2]])) + 3;
        let mut frame = vec![0_u8; total];
        frame[..3].copy_from_slice(&header);
        stream.read_exact(&mut frame[3..]).await.expect("frame");
        let plain =
            pangya_crypto::server_decrypt(&frame, key, 8 * 1024 * 1024, 128).expect("decrypt");
        (
            u16::from_le_bytes([plain[0], plain[1]]),
            plain[2..].to_vec(),
        )
    })
    .await
    .expect("bounded packet receive")
}

async fn send_typed<T: EncodePacket>(stream: &mut TcpStream, key: u8, salt: u8, packet: &T) {
    let payload = encode_packet_payload(packet, &CompatibilityProfile::US_852).expect("encode");
    send_packet(stream, key, salt, T::OPCODE, &payload).await;
}

async fn flood_ready(mut stream: TcpStream, key: u8, count: usize) {
    let mut plain = Vec::with_capacity(3);
    plain.extend_from_slice(&pangya_protocol::SYNTHETIC_M4_C2S_READY.to_le_bytes());
    plain.push(1);
    let encrypted = pangya_crypto::client_encrypt(&plain, key, 77).expect("encrypt flood");
    let (mut reader, mut writer) = stream.split();
    let writes = async {
        for _ in 0..count {
            if writer.write_all(&encrypted).await.is_err() {
                break;
            }
        }
    };
    tokio::pin!(writes);
    let mut discarded = [0_u8; 16 * 1024];
    loop {
        tokio::select! {
            () = &mut writes => break,
            read = reader.read(&mut discarded) => {
                if !matches!(read, Ok(bytes) if bytes > 0) {
                    break;
                }
            }
        }
    }
}

async fn receive_typed<T: DecodePacket>(stream: &mut TcpStream, key: u8) -> T {
    let (opcode, body) = receive_packet(stream, key).await;
    assert_eq!(opcode, T::OPCODE);
    decode_packet_payload::<T>(
        &body,
        &CompatibilityProfile::US_852,
        ProtocolServiceKind::Game,
    )
    .expect("typed response")
}

async fn receive_result(
    stream: &mut TcpStream,
    key: u8,
    command: RoomCommand,
    result: RoomCommandResult,
) {
    assert_eq!(
        receive_typed::<RoomCommandResultResponse>(stream, key).await,
        RoomCommandResultResponse { command, result }
    );
}

async fn maybe_receive_opcode(stream: &mut TcpStream, key: u8) -> Option<u16> {
    tokio::time::timeout(Duration::from_secs(1), async {
        let mut header = [0_u8; 3];
        stream.read_exact(&mut header).await.ok()?;
        let total = usize::from(u16::from_le_bytes([header[1], header[2]])) + 3;
        let mut frame = vec![0_u8; total];
        frame[..3].copy_from_slice(&header);
        stream.read_exact(&mut frame[3..]).await.ok()?;
        let plain = pangya_crypto::server_decrypt(&frame, key, 8 * 1024 * 1024, 128).ok()?;
        Some(u16::from_le_bytes([plain[0], plain[1]]))
    })
    .await
    .ok()
    .flatten()
}

async fn assert_closed(stream: &mut TcpStream) {
    assert_closed_within(stream, Duration::from_secs(1)).await;
}

async fn assert_closed_within(stream: &mut TcpStream, bound: Duration) {
    let mut eof = [0_u8; 1];
    let read = tokio::time::timeout(bound, stream.read(&mut eof))
        .await
        .expect("bounded close");
    assert!(
        matches!(read, Ok(0))
            || matches!(
                read,
                Err(ref error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
                    )
            ),
        "connection remained readable: {read:?}"
    );
}

async fn assert_closed_after_draining(stream: &mut TcpStream) {
    tokio::time::timeout(Duration::from_secs(1), async {
        let mut buffered = [0_u8; 1024];
        loop {
            match stream.read(&mut buffered).await {
                Ok(0) => break,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
                    ) =>
                {
                    break;
                }
                Err(error) => panic!("unexpected close error: {error}"),
            }
        }
    })
    .await
    .expect("bounded close after buffered events");
}

fn metric_sample(rendered: &str, key: &str) -> Option<f64> {
    rendered.lines().find_map(|line| {
        let (candidate, value) = line.rsplit_once(' ')?;
        (candidate == key)
            .then(|| value.parse::<f64>().ok())
            .flatten()
    })
}

fn parse_expected_metric(expected: &str) -> (String, f64) {
    let (key, value) = expected
        .rsplit_once(' ')
        .unwrap_or_else(|| panic!("metric sample lacks a value: {expected}"));
    let key = if key.starts_with("class=\"") {
        format!("pangya_game_rate_limit_total{{{key}")
    } else if key.starts_with("service=\"game\",reason=") {
        format!("pangya_connections_closed_total{{{key}")
    } else {
        key.to_owned()
    };
    let value = value
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("metric sample has a nonnumeric value: {expected}"));
    (key, value)
}

async fn assert_metric(metrics: &M2Metrics, expected: &str) {
    let (key, value) = parse_expected_metric(expected);
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if metric_sample(&metrics.render(), &key) == Some(value) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing exact metric {expected}: {}", metrics.render()));
}

async fn assert_counter_at_least(metrics: &M2Metrics, key: &str, minimum: u64) {
    let key = key.trim_end();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if metric_sample(&metrics.render(), key).is_some_and(|value| value >= minimum as f64) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("counter below {minimum} for {key}: {}", metrics.render()));
}

fn auth_payload(account_id: i64, token: &str) -> Vec<u8> {
    let mut writer = PacketWriter::new();
    writer.u64_le(u64::try_from(account_id).expect("account"));
    writer.pstring(token.as_bytes(), 128).expect("token");
    writer.into_inner()
}

fn login_payload(username: &str) -> Vec<u8> {
    let mut writer = PacketWriter::new();
    writer.pstring(username.as_bytes(), 64).expect("username");
    writer.pstring(SECRET.as_bytes(), 128).expect("secret");
    writer.bytes(&[0; 17]);
    writer.into_inner()
}

async fn issue_token(
    pool: &PgPool,
    account_id: pangya_domain::AccountId,
    now: SystemTime,
    target: ServiceKind,
) -> String {
    let generated = generate_handover(
        account_id,
        target,
        SourceAddressPrefix::from_ip("127.0.0.1".parse().expect("ip")),
        now,
    )
    .expect("handover");
    PgRepository::new(pool.clone())
        .issue(generated.record)
        .await
        .expect("issue");
    generated.token.expose_secret().to_owned()
}

async fn read_player_info(stream: &mut TcpStream, key: u8) -> (u64, Vec<u8>, u64, u64, u64) {
    let (opcode, body) = receive_packet(stream, key).await;
    assert_eq!(opcode, 0x0070);
    let account_id = u64::from_le_bytes(body[0..8].try_into().expect("account bytes"));
    let nickname_len = usize::from(u16::from_le_bytes(
        body[8..10].try_into().expect("nickname length"),
    ));
    let balances = 10 + nickname_len;
    assert_eq!(body.len(), balances + 24);
    (
        account_id,
        body[10..balances].to_vec(),
        u64::from_le_bytes(body[balances..balances + 8].try_into().expect("pang bytes")),
        u64::from_le_bytes(
            body[balances + 8..balances + 16]
                .try_into()
                .expect("points bytes"),
        ),
        u64::from_le_bytes(
            body[balances + 16..balances + 24]
                .try_into()
                .expect("experience bytes"),
        ),
    )
}

async fn read_bootstrap_after_player(stream: &mut TcpStream, key: u8, inventory_segments: usize) {
    assert_eq!(receive_packet(stream, key).await.0, 0x0072);
    for index in 0..inventory_segments {
        let (opcode, body) = receive_packet(stream, key).await;
        assert_eq!(opcode, 0x0073);
        assert_eq!(usize::from(u16::from_le_bytes([body[0], body[1]])), index);
        assert!(usize::from(u16::from_le_bytes([body[4], body[5]])) <= 50);
    }
    assert_eq!(receive_packet(stream, key).await.0, 0x004d);
}

async fn read_bootstrap(stream: &mut TcpStream, key: u8, inventory_segments: usize) {
    let _ = read_player_info(stream, key).await;
    assert_eq!(receive_packet(stream, key).await.0, 0x0072);
    for index in 0..inventory_segments {
        let (opcode, body) = receive_packet(stream, key).await;
        assert_eq!(opcode, 0x0073);
        assert_eq!(usize::from(u16::from_le_bytes([body[0], body[1]])), index);
        assert!(usize::from(u16::from_le_bytes([body[4], body[5]])) <= 50);
    }
    assert_eq!(receive_packet(stream, key).await.0, 0x004d);
}

struct M4Client {
    stream: TcpStream,
    key: u8,
    account_id: AccountId,
    nickname: String,
    token: String,
}

async fn connect_m4(pool: &PgPool, address: std::net::SocketAddr, username: &str) -> M4Client {
    let account = create_account(pool, username, 1, 0x1000_0000).await;
    let token = issue_token(
        pool,
        account.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut stream, key) = connect_game(address).await;
    send_packet(
        &mut stream,
        key,
        1,
        2,
        &auth_payload(account.account.id.get(), &token),
    )
    .await;
    read_bootstrap(&mut stream, key, 1).await;
    send_packet(&mut stream, key, 2, 4, &1_u32.to_le_bytes()).await;
    assert_eq!(receive_packet(&mut stream, key).await.0, 0x004e);
    M4Client {
        stream,
        key,
        account_id: account.account.id,
        nickname: format!("N{username}"),
        token,
    }
}

type PersistedBeginRow = (
    i64,
    i16,
    i16,
    Vec<u8>,
    Vec<u8>,
    String,
    i16,
    i16,
    String,
    String,
);

struct M5Client {
    stream: TcpStream,
    key: u8,
    account_id: AccountId,
    nickname: String,
    token: String,
    connection_id: u64,
}

async fn connect_m5_owner(
    pool: &PgPool,
    address: std::net::SocketAddr,
    username: &str,
    room_name: &str,
) -> M5Client {
    let mut client = connect_m4(pool, address, username).await;
    send_typed(
        &mut client.stream,
        client.key,
        3,
        &RoomCreateRequest {
            name: RoomName::parse(room_name).expect("room name"),
            password: None,
            settings: RoomSettings::new(2).expect("solo room settings"),
        },
    )
    .await;
    receive_result(
        &mut client.stream,
        client.key,
        RoomCommand::Create,
        RoomCommandResult::Success,
    )
    .await;
    let state = receive_typed::<RoomStateResponse>(&mut client.stream, client.key).await;
    assert_eq!(state.room.summary().name().as_str(), room_name);
    assert_eq!(state.room.members().len(), 1);
    let connection_id = state.room.members()[0].connection_id().get();
    assert!(state.room.members()[0].is_owner());
    M5Client {
        stream: client.stream,
        key: client.key,
        account_id: client.account_id,
        nickname: client.nickname,
        token: client.token,
        connection_id,
    }
}

async fn receive_solo_result(
    client: &mut M5Client,
    command: SoloCommand,
    result: SoloCommandOutcome,
) {
    assert_eq!(
        receive_typed::<SoloCommandResult>(&mut client.stream, client.key).await,
        SoloCommandResult::new(command, result)
    );
}

async fn start_solo(client: &mut M5Client) -> MatchStarted {
    send_typed(&mut client.stream, client.key, 4, &StartSolo::new()).await;
    receive_solo_result(client, SoloCommand::StartSolo, SoloCommandOutcome::Success).await;
    let started = receive_typed::<MatchStarted>(&mut client.stream, client.key).await;
    assert_eq!(
        receive_typed::<MatchPhase>(&mut client.stream, client.key).await,
        MatchPhase::new(started.match_id(), SoloPhase::Loading)
    );
    started
}

async fn enter_playing(client: &mut M5Client, started: &MatchStarted) {
    send_typed(
        &mut client.stream,
        client.key,
        5,
        &LoadingComplete::new(100).expect("loading complete"),
    )
    .await;
    // Documented loading sequence: durable transition success, then playing phase.
    receive_solo_result(
        client,
        SoloCommand::LoadingComplete,
        SoloCommandOutcome::Success,
    )
    .await;
    assert_eq!(
        receive_typed::<MatchPhase>(&mut client.stream, client.key).await,
        MatchPhase::new(started.match_id(), SoloPhase::Playing)
    );
}

async fn receive_action_success(client: &mut M5Client, action: ShotAction) {
    // Documented action sequence: command success, authoritative relay, playing phase.
    receive_solo_result(client, SoloCommand::ShotAction, SoloCommandOutcome::Success).await;
    assert_eq!(
        receive_typed::<ShotActionRelay>(&mut client.stream, client.key).await,
        ShotActionRelay::new(client.connection_id, action).expect("action relay")
    );
    assert_eq!(
        receive_typed::<MatchPhase>(&mut client.stream, client.key)
            .await
            .phase(),
        SoloPhase::Playing
    );
}

async fn receive_shot_result_success(client: &mut M5Client, result: ShotResult) {
    // Documented result sequence: command success, authoritative relay, resulting phase.
    receive_solo_result(client, SoloCommand::ShotResult, SoloCommandOutcome::Success).await;
    assert_eq!(
        receive_typed::<ShotResultRelay>(&mut client.stream, client.key).await,
        ShotResultRelay::new(client.connection_id, result).expect("result relay")
    );
    assert_eq!(
        receive_typed::<MatchPhase>(&mut client.stream, client.key)
            .await
            .phase(),
        if result.holed() {
            SoloPhase::HoleComplete
        } else {
            SoloPhase::Playing
        }
    );
}

async fn relay_shot(client: &mut M5Client, salt: u8, action: ShotAction, result: ShotResult) {
    send_typed(&mut client.stream, client.key, salt, &action).await;
    receive_action_success(client, action).await;
    send_typed(
        &mut client.stream,
        client.key,
        salt.wrapping_add(1),
        &result,
    )
    .await;
    receive_shot_result_success(client, result).await;
}

async fn finish_solo(client: &mut M5Client, started: &MatchStarted) -> (HoleResult, BalanceUpdate) {
    send_typed(&mut client.stream, client.key, 10, &FinishHole::new()).await;
    // Documented finish sequence: precommit phase, then committed result/balance/success/finished.
    assert_eq!(
        receive_typed::<MatchPhase>(&mut client.stream, client.key).await,
        MatchPhase::new(started.match_id(), SoloPhase::HoleComplete)
    );
    let hole = receive_typed::<HoleResult>(&mut client.stream, client.key).await;
    let balance = receive_typed::<BalanceUpdate>(&mut client.stream, client.key).await;
    receive_solo_result(client, SoloCommand::FinishHole, SoloCommandOutcome::Success).await;
    assert_eq!(
        receive_typed::<MatchPhase>(&mut client.stream, client.key).await,
        MatchPhase::new(started.match_id(), SoloPhase::Finished)
    );
    (hole, balance)
}

async fn wait_for_abort(pool: &PgPool, match_id: &str, reason: &str) {
    let completed = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let state: Option<(String, Option<String>, bool)> = sqlx::query_as(
                "SELECT m.status, m.abort_reason, mp.quit FROM matches m \
                 JOIN match_players mp ON mp.match_id = m.id WHERE m.id::text = $1",
            )
            .bind(match_id)
            .fetch_optional(pool)
            .await
            .expect("abort state");
            if state
                .as_ref()
                .is_some_and(|row| row.0 == "aborted" && row.1.as_deref() == Some(reason) && row.2)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    if completed.is_err() {
        let state: Option<(String, Option<String>, bool)> = sqlx::query_as(
            "SELECT m.status, m.abort_reason, mp.quit FROM matches m \
             JOIN match_players mp ON mp.match_id = m.id WHERE m.id::text = $1",
        )
        .bind(match_id)
        .fetch_optional(pool)
        .await
        .expect("timed-out abort state");
        panic!("abort {match_id} did not reach {reason}: {state:?}");
    }
}

async fn assert_no_match_reward(pool: &PgPool, match_id: &str, account_id: AccountId) {
    let state: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM currency_ledger WHERE match_id::text = $1), \
                (SELECT count(*) FROM progression_ledger WHERE match_id::text = $1), \
                (SELECT count(*) FROM match_audit_events WHERE match_id::text = $1), \
                (SELECT pang FROM profiles WHERE account_id = $2), \
                (SELECT experience FROM profiles WHERE account_id = $2)",
    )
    .bind(match_id)
    .bind(account_id.get())
    .fetch_one(pool)
    .await
    .expect("no-reward snapshot");
    assert_eq!(state, (0, 0, 2, 0, 0));
}

fn member(client: &M4Client, connection_id: u64, owner: bool, ready: bool) -> MemberSnapshot {
    MemberSnapshot::new(
        PlayerConnectionId::new(connection_id).expect("connection ID"),
        client.account_id,
        client.nickname.clone(),
        owner,
        ready,
    )
}

fn snapshot(
    room_id: RoomId,
    name: &str,
    owner_nickname: &str,
    capacity: u8,
    protected: bool,
    members: Vec<MemberSnapshot>,
) -> RoomSnapshot {
    RoomSnapshot::new(
        RoomSummary::new(
            room_id,
            RoomName::parse(name).expect("room name"),
            owner_nickname.to_owned(),
            u8::try_from(members.len()).expect("member count"),
            capacity,
            protected,
        ),
        members,
    )
}

async fn receive_state(stream: &mut TcpStream, key: u8, expected: &RoomSnapshot) {
    assert_eq!(
        receive_typed::<RoomStateResponse>(stream, key).await,
        RoomStateResponse {
            room: expected.clone()
        }
    );
}

async fn request_state(client: &mut M4Client, salt: u8, expected: &RoomSnapshot) {
    send_typed(&mut client.stream, client.key, salt, &RoomStateRequest).await;
    receive_state(&mut client.stream, client.key, expected).await;
}

async fn request_list(client: &mut M4Client, salt: u8, rooms: Vec<RoomSummary>) {
    send_typed(&mut client.stream, client.key, salt, &RoomListRequest).await;
    assert_eq!(
        receive_typed::<RoomListResponse>(&mut client.stream, client.key).await,
        RoomListResponse { rooms }
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn login_bearer_to_game_snapshot_catalog_segments_and_channel_is_real_db(pool: PgPool) {
    let traces = tracing_capture();
    let policy = CredentialPolicy::new().expect("policy");
    let secret = pangya_login::CanonicalTransportSecret::parse(SECRET).expect("secret");
    let hash = policy.hash(&secret).expect("hash");
    let aggregate = PgRepository::new(pool.clone())
        .create_account(NewAccount {
            username: Username::parse("VerticalGame").expect("username"),
            credential_hash: hash,
            nickname: Some(Nickname::parse("VerticalNick").expect("nickname")),
            starter: starter(101, 0x1000_0000),
        })
        .await
        .expect("account");
    let metrics = Arc::new(M2Metrics::default());
    let repository = Arc::new(PgRepository::new(pool.clone()));
    let login_listener = TcpListener::bind("127.0.0.1:0").await.expect("login bind");
    let login_address = login_listener.local_addr().expect("login address");
    let game_listener = TcpListener::bind("127.0.0.1:0").await.expect("game bind");
    let game_address = game_listener.local_addr().expect("game address");
    let credentials = BoundedCredentialExecutor::new(
        Arc::new(policy),
        2,
        Duration::from_secs(1),
        Duration::from_secs(5),
    )
    .expect("executor");
    let login = Arc::new(
        LoginService::new(
            Arc::clone(&repository),
            credentials,
            LoginRuntimeConfig {
                auto_create_accounts: false,
                starter: starter(1, 0x1000_0000),
                allowed_character_types: vec![0x0400_0000],
                game_server: AdvertisedGameServer {
                    id: 7,
                    name: "Synthetic M3".to_owned(),
                    ipv4: "127.0.0.1".to_owned(),
                    port: game_address.port(),
                    capacity: 200,
                },
                limits: LoginRuntimeLimits::default(),
            },
            metrics.clone(),
        )
        .expect("login"),
    );
    let game = Arc::new(
        GameService::new(
            repository,
            catalog(),
            GameRuntimeConfig {
                channel_id: 1,
                unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
                limits: GameRuntimeLimits::default(),
                solo_practice: None,
                stroke_two: None,
                economy: None,
                retail_bootstrap: false,
            },
            metrics.clone(),
        )
        .expect("game"),
    );
    let login_shutdown = CancellationToken::new();
    let game_shutdown = CancellationToken::new();
    let login_child = login_shutdown.clone();
    let game_child = game_shutdown.clone();
    let login_task = tokio::spawn(async move { login.serve(login_listener, login_child).await });
    let game_task = tokio::spawn(async move { game.serve(game_listener, game_child).await });

    let (mut login_stream, login_key) = connect_login(login_address).await;
    send_packet(
        &mut login_stream,
        login_key,
        1,
        1,
        &login_payload("VerticalGame"),
    )
    .await;
    // Take the bearer from `0x0010` the way a real client does, rather than from `0x0003`: the
    // client stores the login key on this packet and echoes that stored value to GameService.
    let mut login_key_body = None;
    for opcode in [1, 0x10, 6, 9, 2] {
        let (received, body) = receive_packet(&mut login_stream, login_key).await;
        assert_eq!(received, opcode);
        if received == 0x10 {
            login_key_body = Some(body);
        }
    }
    let login_key_body = login_key_body.expect("login key packet");
    let length = usize::from(u16::from_le_bytes([login_key_body[0], login_key_body[1]]));
    let token = std::str::from_utf8(&login_key_body[2..2 + length])
        .expect("token")
        .to_owned();
    send_packet(&mut login_stream, login_key, 2, 3, &[7, 0, 0, 0]).await;
    let (opcode, body) = receive_packet(&mut login_stream, login_key).await;
    assert_eq!(opcode, 3);
    assert_eq!(
        &body[4..],
        &login_key_body[..],
        "server selection repeats the same bearer"
    );

    let (mut game_stream, game_key) = connect_game(game_address).await;
    send_packet(
        &mut game_stream,
        game_key,
        3,
        2,
        &auth_payload(aggregate.account.id.get(), &token),
    )
    .await;
    read_bootstrap(&mut game_stream, game_key, 3).await;
    send_packet(&mut game_stream, game_key, 4, 4, &1_u32.to_le_bytes()).await;
    assert_eq!(receive_packet(&mut game_stream, game_key).await.0, 0x004e);
    let rendered = metrics.render();
    assert_eq!(
        metric_sample(&rendered, "pangya_game_auth_total{outcome=\"success\"}"),
        Some(1.0)
    );
    assert!(!rendered.contains(&token));
    assert!(!rendered.contains(SECRET));
    let trace_bytes = traces.lock().expect("traces").clone();
    let trace_text = String::from_utf8_lossy(&trace_bytes);
    assert!(!trace_text.contains(&token));
    assert!(!trace_text.contains(SECRET));

    login_shutdown.cancel();
    game_shutdown.cancel();
    login_task.await.expect("login join").expect("login serve");
    game_task.await.expect("game join").expect("game serve");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_rejects_expired_wrong_replayed_mismatched_banned_invalid_catalog_and_malformed(
    pool: PgPool,
) {
    let aggregate = create_account(&pool, "GameFailures", 1, 0x1000_0000).await;
    let (address, shutdown, task, _) = start_game(pool.clone(), GameRuntimeLimits::default()).await;

    let expired = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now() - Duration::from_secs(120),
        ServiceKind::Game,
    )
    .await;
    let wrong = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Message,
    )
    .await;
    for (salt, token) in [(1, expired), (2, wrong)] {
        let (mut stream, key) = connect_game(address).await;
        send_packet(
            &mut stream,
            key,
            salt,
            2,
            &auth_payload(aggregate.account.id.get(), &token),
        )
        .await;
        assert_eq!(maybe_receive_opcode(&mut stream, key).await, None);
    }

    let mismatch = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut stream, key) = connect_game(address).await;
    send_packet(
        &mut stream,
        key,
        3,
        2,
        &auth_payload(aggregate.account.id.get() + 1, &mismatch),
    )
    .await;
    assert_eq!(maybe_receive_opcode(&mut stream, key).await, None);

    let replay = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut first, first_key) = connect_game(address).await;
    send_packet(
        &mut first,
        first_key,
        4,
        2,
        &auth_payload(aggregate.account.id.get(), &replay),
    )
    .await;
    read_bootstrap(&mut first, first_key, 1).await;
    drop(first);
    let (mut second, second_key) = connect_game(address).await;
    send_packet(
        &mut second,
        second_key,
        5,
        2,
        &auth_payload(aggregate.account.id.get(), &replay),
    )
    .await;
    assert_eq!(maybe_receive_opcode(&mut second, second_key).await, None);

    let banned = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    PgRepository::new(pool.clone())
        .set_status(
            aggregate.account.id,
            AccountStatus::Banned,
            SystemTime::now(),
        )
        .await
        .expect("ban");
    let (mut banned_stream, banned_key) = connect_game(address).await;
    send_packet(
        &mut banned_stream,
        banned_key,
        6,
        2,
        &auth_payload(aggregate.account.id.get(), &banned),
    )
    .await;
    assert_eq!(
        maybe_receive_opcode(&mut banned_stream, banned_key).await,
        None
    );

    let (mut invalid, invalid_key) = connect_game(address).await;
    send_packet(
        &mut invalid,
        invalid_key,
        7,
        2,
        &auth_payload(aggregate.account.id.get(), "not-a-token"),
    )
    .await;
    assert_eq!(maybe_receive_opcode(&mut invalid, invalid_key).await, None);
    let (mut malformed, _) = connect_game(address).await;
    malformed.write_all(&[0, 0, 0, 0]).await.expect("malformed");
    let mut eof = [0_u8; 1];
    assert_eq!(malformed.read(&mut eof).await.expect("close"), 0);

    let invalid_catalog = create_account(&pool, "InvalidCatalog", 1, 0x1000_9999).await;
    let token = issue_token(
        &pool,
        invalid_catalog.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut stream, key) = connect_game(address).await;
    send_packet(
        &mut stream,
        key,
        8,
        2,
        &auth_payload(invalid_catalog.account.id.get(), &token),
    )
    .await;
    assert_eq!(maybe_receive_opcode(&mut stream, key).await, None);

    shutdown.cancel();
    task.await.expect("join").expect("serve");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_duplicate_presence_raii_replay_concurrency_rates_and_timeouts_are_bounded(
    pool: PgPool,
) {
    let aggregate = create_account(&pool, "GameBounds", 1, 0x1000_0000).await;
    // Both deadlines must clear a loaded runner's snapshot load and bootstrap write, or the
    // first connection loses its presence guard before the duplicate ever contends for it.
    // Every assertion below still bounds the close, so widening these only costs wall clock.
    let limits = GameRuntimeLimits {
        authentication_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(5),
        ..GameRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start_game(pool.clone(), limits.clone()).await;
    let first_token = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let second_token = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut first, first_key) = connect_game(address).await;
    send_packet(
        &mut first,
        first_key,
        1,
        2,
        &auth_payload(aggregate.account.id.get(), &first_token),
    )
    .await;
    read_bootstrap(&mut first, first_key, 1).await;
    let (mut duplicate, duplicate_key) = connect_game(address).await;
    send_packet(
        &mut duplicate,
        duplicate_key,
        2,
        2,
        &auth_payload(aggregate.account.id.get(), &second_token),
    )
    .await;
    read_bootstrap(&mut duplicate, duplicate_key, 1).await;
    assert_closed(&mut duplicate).await;
    assert_eq!(
        metric_sample(
            &metrics.render(),
            "pangya_game_auth_total{outcome=\"duplicate\"}",
        ),
        Some(1.0)
    );
    drop(first);
    shutdown.cancel();
    task.await
        .expect("presence service join")
        .expect("presence service serve");
    let (address, shutdown, task, _metrics) = start_game(pool.clone(), limits).await;
    let third_token = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut third, third_key) = connect_game(address).await;
    send_packet(
        &mut third,
        third_key,
        3,
        2,
        &auth_payload(aggregate.account.id.get(), &third_token),
    )
    .await;
    read_bootstrap(&mut third, third_key, 1).await;
    drop(third);

    let concurrent_account = create_account(&pool, "GameConcurrent", 1, 0x1000_0000).await;
    let concurrent = issue_token(
        &pool,
        concurrent_account.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut left, left_key) = connect_game(address).await;
    let (mut right, right_key) = connect_game(address).await;
    send_packet(
        &mut left,
        left_key,
        4,
        2,
        &auth_payload(concurrent_account.account.id.get(), &concurrent),
    )
    .await;
    send_packet(
        &mut right,
        right_key,
        5,
        2,
        &auth_payload(concurrent_account.account.id.get(), &concurrent),
    )
    .await;
    let (left_result, right_result) = tokio::join!(
        maybe_receive_opcode(&mut left, left_key),
        maybe_receive_opcode(&mut right, right_key)
    );
    assert_eq!(
        usize::from(left_result == Some(0x0070)) + usize::from(right_result == Some(0x0070)),
        1
    );
    drop(left);
    drop(right);

    let (mut idle, _) = connect_game(address).await;
    let mut eof = [0_u8; 1];
    assert_eq!(idle.read(&mut eof).await.expect("auth timeout"), 0);
    shutdown.cancel();
    task.await.expect("join").expect("serve");

    let rate_limits = GameRuntimeLimits {
        global_accepts_per_window: 1,
        accepts_per_window: 10,
        ..GameRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start_game(pool, rate_limits).await;
    let (_first, _) = connect_game(address).await;
    let mut second = TcpStream::connect(address).await.expect("second");
    assert_eq!(second.read(&mut eof).await.expect("rate close"), 0);
    assert_eq!(
        metric_sample(
            &metrics.render(),
            "pangya_game_rate_limit_total{class=\"accept_global\"}",
        ),
        Some(1.0)
    );
    shutdown.cancel();
    task.await.expect("join").expect("serve");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_source_admission_and_auth_rate_layers_are_independent(pool: PgPool) {
    let aggregate = create_account(&pool, "GameSourceRates", 1, 0x1000_0000).await;
    let source_accept_limits = GameRuntimeLimits {
        global_accepts_per_window: 10,
        accepts_per_window: 1,
        ..GameRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start_game(pool.clone(), source_accept_limits).await;
    let (_held, _) = connect_game(address).await;
    let mut rejected = TcpStream::connect(address)
        .await
        .expect("source accept connect");
    assert_closed(&mut rejected).await;
    assert_metric(&metrics, "class=\"accept_source\"} 1").await;
    shutdown.cancel();
    task.await.expect("join").expect("serve");

    let source_connection_limits = GameRuntimeLimits {
        global_connections: 2,
        connections_per_source: 1,
        global_accepts_per_window: 10,
        accepts_per_window: 10,
        ..GameRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) =
        start_game(pool.clone(), source_connection_limits).await;
    let (_held, _) = connect_game(address).await;
    let mut rejected = TcpStream::connect(address)
        .await
        .expect("source capacity connect");
    assert_closed(&mut rejected).await;
    assert_metric(&metrics, "class=\"connection_source\"} 1").await;
    shutdown.cancel();
    task.await.expect("join").expect("serve");

    for (limits, metric) in [
        (
            GameRuntimeLimits {
                global_auth_per_window: 1,
                auth_per_window: 10,
                ..GameRuntimeLimits::default()
            },
            "class=\"auth_global\"} 1",
        ),
        (
            GameRuntimeLimits {
                global_auth_per_window: 10,
                auth_per_window: 1,
                ..GameRuntimeLimits::default()
            },
            "class=\"auth_source\"} 1",
        ),
    ] {
        let (address, shutdown, task, metrics) = start_game(pool.clone(), limits).await;
        for salt in [1, 2] {
            let (mut stream, key) = connect_game(address).await;
            send_packet(
                &mut stream,
                key,
                salt,
                2,
                &auth_payload(aggregate.account.id.get(), "not-a-token"),
            )
            .await;
            assert_closed(&mut stream).await;
        }
        assert_metric(&metrics, metric).await;
        shutdown.cancel();
        task.await.expect("join").expect("serve");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_packet_and_byte_rate_layers_are_independent(pool: PgPool) {
    for (limits, metric, attempts) in [
        (
            GameRuntimeLimits {
                global_packets_per_window: 1,
                source_packets_per_window: 10,
                ..GameRuntimeLimits::default()
            },
            "class=\"packet_global\"} 1",
            2_u8,
        ),
        (
            GameRuntimeLimits {
                global_packets_per_window: 10,
                source_packets_per_window: 1,
                ..GameRuntimeLimits::default()
            },
            "class=\"packet_source\"} 1",
            2,
        ),
        (
            GameRuntimeLimits {
                global_bytes_per_window: 1,
                source_bytes_per_window: 1_024,
                ..GameRuntimeLimits::default()
            },
            "class=\"bytes_global\"} 1",
            1,
        ),
        (
            GameRuntimeLimits {
                global_bytes_per_window: 1_024,
                source_bytes_per_window: 1,
                ..GameRuntimeLimits::default()
            },
            "class=\"bytes_source\"} 1",
            1,
        ),
    ] {
        let (address, shutdown, task, metrics) = start_game(pool.clone(), limits).await;
        for salt in 0..attempts {
            let (mut stream, key) = connect_game(address).await;
            send_packet(&mut stream, key, salt, 0x7777, &[]).await;
            assert_closed(&mut stream).await;
        }
        assert_metric(&metrics, metric).await;
        shutdown.cancel();
        task.await.expect("join").expect("serve");
    }

    let aggregate = create_account(&pool, "GameConnRate", 1, 0x1000_0000).await;
    for byte_limited in [false, true] {
        let token = issue_token(
            &pool,
            aggregate.account.id,
            SystemTime::now(),
            ServiceKind::Game,
        )
        .await;
        let payload = auth_payload(aggregate.account.id.get(), &token);
        let limits = if byte_limited {
            GameRuntimeLimits {
                bytes_per_window: u64::try_from(payload.len() + 2).expect("auth bytes"),
                ..GameRuntimeLimits::default()
            }
        } else {
            GameRuntimeLimits {
                packets_per_window: 1,
                ..GameRuntimeLimits::default()
            }
        };
        let (address, shutdown, task, metrics) = start_game(pool.clone(), limits).await;
        let (mut stream, key) = connect_game(address).await;
        send_packet(&mut stream, key, 11, 2, &payload).await;
        read_bootstrap(&mut stream, key, 1).await;
        send_packet(&mut stream, key, 12, 4, &1_u32.to_le_bytes()).await;
        assert_closed(&mut stream).await;
        assert_metric(&metrics, "class=\"packet_or_bytes_connection\"} 1").await;
        shutdown.cancel();
        task.await.expect("join").expect("serve");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_protocol_idle_and_cancellation_cleanup_are_deterministic(pool: PgPool) {
    let aggregate = create_account(&pool, "GameProto", 1, 0x1000_0000).await;
    // The protocol, wrong-channel, and cancellation cases must never race the idle clock:
    // they assert why a connection closed, so an idle close would be a false pass. The idle
    // close gets its own service below, with the only deadline that test case observes.
    let limits = GameRuntimeLimits {
        authentication_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_millis(100),
        shutdown_grace: Duration::from_millis(200),
        ..GameRuntimeLimits::default()
    };
    let metrics = Arc::new(M2Metrics::default());
    let service = game_service(pool.clone(), limits.clone(), metrics.clone());
    let (address, shutdown, task) = start_service(Arc::clone(&service)).await;

    let (mut invalid_state, invalid_key) = connect_game(address).await;
    send_packet(&mut invalid_state, invalid_key, 1, 4, &1_u32.to_le_bytes()).await;
    assert_closed(&mut invalid_state).await;
    let (mut unknown, unknown_key) = connect_game(address).await;
    send_packet(&mut unknown, unknown_key, 2, 0x7777, &[]).await;
    assert_closed(&mut unknown).await;

    let wrong_token = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut wrong_channel, wrong_key) = connect_game(address).await;
    send_packet(
        &mut wrong_channel,
        wrong_key,
        3,
        2,
        &auth_payload(aggregate.account.id.get(), &wrong_token),
    )
    .await;
    read_bootstrap(&mut wrong_channel, wrong_key, 1).await;
    send_packet(&mut wrong_channel, wrong_key, 4, 4, &2_u32.to_le_bytes()).await;
    assert_closed(&mut wrong_channel).await;

    let idle_token = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    // A dedicated service so the idle deadline is short enough to observe without bounding
    // any exchange that precedes it. It shares the metrics, so the close is counted once.
    let idle_limits = GameRuntimeLimits {
        idle_timeout: Duration::from_secs(1),
        ..limits.clone()
    };
    let idle_service = game_service(pool.clone(), idle_limits, metrics.clone());
    let (idle_address, idle_shutdown, idle_task) = start_service(idle_service).await;
    let (mut idle, idle_key) = connect_game(idle_address).await;
    send_packet(
        &mut idle,
        idle_key,
        5,
        2,
        &auth_payload(aggregate.account.id.get(), &idle_token),
    )
    .await;
    read_bootstrap(&mut idle, idle_key, 1).await;
    send_packet(&mut idle, idle_key, 6, 4, &1_u32.to_le_bytes()).await;
    assert_eq!(receive_packet(&mut idle, idle_key).await.0, 0x004e);
    assert_closed_within(&mut idle, Duration::from_secs(5)).await;
    assert_counter_at_least(
        &metrics,
        "pangya_connections_closed_total{service=\"game\",reason=\"timeout\"} ",
        1,
    )
    .await;
    idle_shutdown.cancel();
    idle_task.await.expect("idle join").expect("idle serve");

    let cancellation_token = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut cancelled, cancelled_key) = connect_game(address).await;
    send_packet(
        &mut cancelled,
        cancelled_key,
        7,
        2,
        &auth_payload(aggregate.account.id.get(), &cancellation_token),
    )
    .await;
    read_bootstrap(&mut cancelled, cancelled_key, 1).await;
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .expect("shutdown bound")
        .expect("join")
        .expect("serve");
    assert_metric(&metrics, "service=\"game\",reason=\"cancelled\"} 1").await;
    assert_eq!(
        metric_sample(
            &metrics.render(),
            "pangya_connections_active{service=\"game\"}",
        ),
        Some(0.0)
    );

    let reconnect_token = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let service = game_service(pool, limits, metrics);
    let (address, shutdown, task) = start_service(service).await;
    let (mut reconnect, reconnect_key) = connect_game(address).await;
    send_packet(
        &mut reconnect,
        reconnect_key,
        8,
        2,
        &auth_payload(aggregate.account.id.get(), &reconnect_token),
    )
    .await;
    read_bootstrap(&mut reconnect, reconnect_key, 1).await;
    shutdown.cancel();
    task.await.expect("join").expect("serve");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m4_tcp_room_lifecycle_authority_password_capacity_and_cleanup(pool: PgPool) {
    let limits = GameRuntimeLimits {
        global_connections: 8,
        connections_per_source: 8,
        global_auth_per_window: 20,
        auth_per_window: 20,
        ..GameRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start_game(pool.clone(), limits).await;
    let mut owner = connect_m4(&pool, address, "M4Owner").await;
    let mut second = connect_m4(&pool, address, "M4Second").await;
    let mut third = connect_m4(&pool, address, "M4Third").await;
    let mut fourth = connect_m4(&pool, address, "M4Fourth").await;
    let sensitive_tokens = [
        owner.token.clone(),
        second.token.clone(),
        third.token.clone(),
        fourth.token.clone(),
    ];

    send_typed(
        &mut owner.stream,
        owner.key,
        3,
        &RoomCreateRequest {
            name: RoomName::parse("M4 Open Room").expect("room name"),
            password: None,
            settings: RoomSettings::new(4).expect("settings"),
        },
    )
    .await;
    receive_result(
        &mut owner.stream,
        owner.key,
        RoomCommand::Create,
        RoomCommandResult::Success,
    )
    .await;
    let open_id = RoomId::new(1).expect("room ID");
    let open_one = snapshot(
        open_id,
        "M4 Open Room",
        &owner.nickname,
        4,
        false,
        vec![member(&owner, 1, true, false)],
    );
    receive_state(&mut owner.stream, owner.key, &open_one).await;
    request_list(&mut second, 3, vec![open_one.summary().clone()]).await;

    send_typed(
        &mut second.stream,
        second.key,
        4,
        &RoomJoinRequest {
            room_id: open_id,
            password: None,
        },
    )
    .await;
    receive_result(
        &mut second.stream,
        second.key,
        RoomCommand::Join,
        RoomCommandResult::Success,
    )
    .await;
    let open_two = snapshot(
        open_id,
        "M4 Open Room",
        &owner.nickname,
        4,
        false,
        vec![
            member(&owner, 1, true, false),
            member(&second, 2, false, false),
        ],
    );
    receive_state(&mut second.stream, second.key, &open_two).await;
    receive_state(&mut owner.stream, owner.key, &open_two).await;
    receive_state(&mut second.stream, second.key, &open_two).await;

    send_typed(
        &mut second.stream,
        second.key,
        5,
        &RoomSettingsRequest {
            settings: RoomSettings::new(3).expect("settings"),
        },
    )
    .await;
    receive_result(
        &mut second.stream,
        second.key,
        RoomCommand::Settings,
        RoomCommandResult::NotOwner,
    )
    .await;
    request_state(&mut owner, 6, &open_two).await;
    send_typed(
        &mut second.stream,
        second.key,
        7,
        &RoomKickRequest {
            target: PlayerConnectionId::new(1).expect("owner connection"),
        },
    )
    .await;
    receive_result(
        &mut second.stream,
        second.key,
        RoomCommand::Kick,
        RoomCommandResult::NotOwner,
    )
    .await;
    request_state(&mut owner, 8, &open_two).await;

    send_typed(
        &mut owner.stream,
        owner.key,
        9,
        &RoomSettingsRequest {
            settings: RoomSettings::new(3).expect("settings"),
        },
    )
    .await;
    receive_result(
        &mut owner.stream,
        owner.key,
        RoomCommand::Settings,
        RoomCommandResult::Success,
    )
    .await;
    let open_three_capacity = snapshot(
        open_id,
        "M4 Open Room",
        &owner.nickname,
        3,
        false,
        vec![
            member(&owner, 1, true, false),
            member(&second, 2, false, false),
        ],
    );
    receive_state(&mut owner.stream, owner.key, &open_three_capacity).await;
    receive_state(&mut owner.stream, owner.key, &open_three_capacity).await;
    receive_state(&mut second.stream, second.key, &open_three_capacity).await;

    send_typed(
        &mut second.stream,
        second.key,
        10,
        &RoomReadyRequest { ready: true },
    )
    .await;
    receive_result(
        &mut second.stream,
        second.key,
        RoomCommand::Ready,
        RoomCommandResult::Success,
    )
    .await;
    let second_ready = snapshot(
        open_id,
        "M4 Open Room",
        &owner.nickname,
        3,
        false,
        vec![
            member(&owner, 1, true, false),
            member(&second, 2, false, true),
        ],
    );
    receive_state(&mut second.stream, second.key, &second_ready).await;
    receive_state(&mut owner.stream, owner.key, &second_ready).await;
    receive_state(&mut second.stream, second.key, &second_ready).await;
    send_typed(
        &mut owner.stream,
        owner.key,
        11,
        &RoomReadyRequest { ready: true },
    )
    .await;
    receive_result(
        &mut owner.stream,
        owner.key,
        RoomCommand::Ready,
        RoomCommandResult::Success,
    )
    .await;
    let both_ready = snapshot(
        open_id,
        "M4 Open Room",
        &owner.nickname,
        3,
        false,
        vec![
            member(&owner, 1, true, true),
            member(&second, 2, false, true),
        ],
    );
    receive_state(&mut owner.stream, owner.key, &both_ready).await;
    receive_state(&mut owner.stream, owner.key, &both_ready).await;
    receive_state(&mut second.stream, second.key, &both_ready).await;

    let chat = ChatText::parse("authoritative hello").expect("chat");
    send_typed(
        &mut second.stream,
        second.key,
        12,
        &RoomChatRequest { text: chat.clone() },
    )
    .await;
    receive_result(
        &mut second.stream,
        second.key,
        RoomCommand::Chat,
        RoomCommandResult::Success,
    )
    .await;
    let expected_chat = RoomChatEvent {
        room_id: open_id,
        sender: member(&second, 2, false, true),
        text: chat,
    };
    assert_eq!(
        receive_typed::<RoomChatEvent>(&mut owner.stream, owner.key).await,
        expected_chat
    );
    assert_eq!(
        receive_typed::<RoomChatEvent>(&mut second.stream, second.key).await,
        expected_chat
    );

    send_typed(
        &mut owner.stream,
        owner.key,
        13,
        &RoomKickRequest {
            target: PlayerConnectionId::new(2).expect("target connection"),
        },
    )
    .await;
    receive_result(
        &mut owner.stream,
        owner.key,
        RoomCommand::Kick,
        RoomCommandResult::Success,
    )
    .await;
    let after_kick = snapshot(
        open_id,
        "M4 Open Room",
        &owner.nickname,
        3,
        false,
        vec![member(&owner, 1, true, true)],
    );
    receive_state(&mut owner.stream, owner.key, &after_kick).await;
    receive_state(&mut owner.stream, owner.key, &after_kick).await;
    assert_eq!(
        receive_typed::<RoomMembershipEvent>(&mut second.stream, second.key).await,
        RoomMembershipEvent {
            room_id: open_id,
            kind: RoomMembershipKind::Kicked,
            member: member(&owner, 1, true, true),
        }
    );
    request_list(&mut second, 14, vec![after_kick.summary().clone()]).await;
    send_typed(&mut owner.stream, owner.key, 15, &RoomLeaveRequest).await;
    receive_result(
        &mut owner.stream,
        owner.key,
        RoomCommand::Leave,
        RoomCommandResult::Success,
    )
    .await;
    request_list(&mut second, 16, Vec::new()).await;

    send_typed(
        &mut second.stream,
        second.key,
        17,
        &RoomCreateRequest {
            name: RoomName::parse("M4 Secret Room").expect("room name"),
            password: Some(RoomPassword::parse("secret-pass").expect("password")),
            settings: RoomSettings::new(3).expect("settings"),
        },
    )
    .await;
    receive_result(
        &mut second.stream,
        second.key,
        RoomCommand::Create,
        RoomCommandResult::Success,
    )
    .await;
    let protected_id = RoomId::new(2).expect("room ID");
    let protected_one = snapshot(
        protected_id,
        "M4 Secret Room",
        &second.nickname,
        3,
        true,
        vec![member(&second, 2, true, false)],
    );
    receive_state(&mut second.stream, second.key, &protected_one).await;
    for (salt, password) in [
        (18, None),
        (
            19,
            Some(RoomPassword::parse("wrong-pass").expect("password")),
        ),
    ] {
        send_typed(
            &mut third.stream,
            third.key,
            salt,
            &RoomJoinRequest {
                room_id: protected_id,
                password,
            },
        )
        .await;
        receive_result(
            &mut third.stream,
            third.key,
            RoomCommand::Join,
            RoomCommandResult::InvalidPassword,
        )
        .await;
    }
    request_list(&mut third, 20, vec![protected_one.summary().clone()]).await;
    send_typed(
        &mut third.stream,
        third.key,
        21,
        &RoomJoinRequest {
            room_id: protected_id,
            password: Some(RoomPassword::parse("secret-pass").expect("password")),
        },
    )
    .await;
    receive_result(
        &mut third.stream,
        third.key,
        RoomCommand::Join,
        RoomCommandResult::Success,
    )
    .await;
    let protected_two = snapshot(
        protected_id,
        "M4 Secret Room",
        &second.nickname,
        3,
        true,
        vec![
            member(&second, 2, true, false),
            member(&third, 3, false, false),
        ],
    );
    receive_state(&mut third.stream, third.key, &protected_two).await;
    receive_state(&mut second.stream, second.key, &protected_two).await;
    receive_state(&mut third.stream, third.key, &protected_two).await;

    let owner_join = RoomJoinRequest {
        room_id: protected_id,
        password: Some(RoomPassword::parse("secret-pass").expect("password")),
    };
    let fourth_join = RoomJoinRequest {
        room_id: protected_id,
        password: Some(RoomPassword::parse("secret-pass").expect("password")),
    };
    tokio::join!(
        send_typed(&mut owner.stream, owner.key, 22, &owner_join),
        send_typed(&mut fourth.stream, fourth.key, 22, &fourth_join)
    );
    let owner_result =
        receive_typed::<RoomCommandResultResponse>(&mut owner.stream, owner.key).await;
    let fourth_result =
        receive_typed::<RoomCommandResultResponse>(&mut fourth.stream, fourth.key).await;
    assert_eq!(owner_result.command, RoomCommand::Join);
    assert_eq!(fourth_result.command, RoomCommand::Join);
    assert!(matches!(
        (owner_result.result, fourth_result.result),
        (RoomCommandResult::Success, RoomCommandResult::Full)
            | (RoomCommandResult::Full, RoomCommandResult::Success)
    ));
    let owner_won = owner_result.result == RoomCommandResult::Success;
    let admitted_member = if owner_won {
        member(&owner, 1, false, false)
    } else {
        member(&fourth, 4, false, false)
    };
    let protected_full = snapshot(
        protected_id,
        "M4 Secret Room",
        &second.nickname,
        3,
        true,
        vec![
            member(&second, 2, true, false),
            member(&third, 3, false, false),
            admitted_member.clone(),
        ],
    );
    if owner_won {
        receive_state(&mut owner.stream, owner.key, &protected_full).await;
        receive_state(&mut owner.stream, owner.key, &protected_full).await;
    } else {
        receive_state(&mut fourth.stream, fourth.key, &protected_full).await;
        receive_state(&mut fourth.stream, fourth.key, &protected_full).await;
    }
    receive_state(&mut second.stream, second.key, &protected_full).await;
    receive_state(&mut third.stream, third.key, &protected_full).await;

    assert_eq!(
        metric_sample(
            &metrics.render(),
            "pangya_game_rooms_active{service=\"game\"}",
        ),
        Some(1.0)
    );
    send_typed(&mut second.stream, second.key, 23, &RoomLeaveRequest).await;
    receive_result(
        &mut second.stream,
        second.key,
        RoomCommand::Leave,
        RoomCommandResult::Success,
    )
    .await;
    let transferred = snapshot(
        protected_id,
        "M4 Secret Room",
        &third.nickname,
        3,
        true,
        vec![member(&third, 3, true, false), admitted_member.clone()],
    );
    receive_state(&mut third.stream, third.key, &transferred).await;
    if owner_won {
        receive_state(&mut owner.stream, owner.key, &transferred).await;
    } else {
        receive_state(&mut fourth.stream, fourth.key, &transferred).await;
    }
    request_list(&mut second, 24, vec![transferred.summary().clone()]).await;

    drop(third);
    let admitted_owned = snapshot(
        protected_id,
        "M4 Secret Room",
        if owner_won {
            &owner.nickname
        } else {
            &fourth.nickname
        },
        3,
        true,
        vec![MemberSnapshot::new(
            admitted_member.connection_id(),
            admitted_member.account_id(),
            admitted_member.nickname().to_owned(),
            true,
            false,
        )],
    );
    if owner_won {
        receive_state(&mut owner.stream, owner.key, &admitted_owned).await;
        drop(owner);
    } else {
        receive_state(&mut fourth.stream, fourth.key, &admitted_owned).await;
        drop(fourth);
    }
    assert_metric(&metrics, "pangya_game_rooms_active{service=\"game\"} 0").await;
    request_list(&mut second, 25, Vec::new()).await;

    let rendered = metrics.render();
    for fixed_label in [
        "pangya_game_room_events_total{event=\"created\"}",
        "pangya_game_queue_events_total{event=\"outbound_dropped\"}",
        "pangya_game_chat_events_total{event=\"accepted\"}",
        "pangya_game_unknown_opcode_actions_total{action=\"captured\"}",
    ] {
        assert!(
            metric_sample(&rendered, fixed_label).is_some(),
            "missing {fixed_label}"
        );
    }
    for secret in [
        "M4 Open Room",
        "M4 Secret Room",
        "secret-pass",
        "authoritative hello",
    ] {
        assert!(!rendered.contains(secret));
    }
    for token in &sensitive_tokens {
        assert!(!rendered.contains(token));
    }
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .expect("bounded shutdown")
        .expect("join")
        .expect("serve");
    assert_eq!(
        metric_sample(
            &metrics.render(),
            "pangya_game_rooms_active{service=\"game\"}",
        ),
        Some(0.0)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m4_m5_m6_unknown_policies_continue_or_close_and_known_wrong_state_always_closes(
    pool: PgPool,
) {
    let limits = GameRuntimeLimits {
        global_connections: 8,
        connections_per_source: 8,
        global_auth_per_window: 20,
        auth_per_window: 20,
        unknown_opcode_strikes: 3,
        ..GameRuntimeLimits::default()
    };

    let metrics = Arc::new(M2Metrics::default());
    let disconnect_service = game_service_with_policy(
        pool.clone(),
        limits.clone(),
        metrics.clone(),
        UnknownOpcodePolicy::Disconnect,
    );
    let (address, shutdown, task) = start_service(disconnect_service).await;
    let mut disconnected = connect_m4(&pool, address, "M4UnkDisc").await;
    send_packet(
        &mut disconnected.stream,
        disconnected.key,
        3,
        0x7777,
        b"unknown-disconnect-body",
    )
    .await;
    assert_closed(&mut disconnected.stream).await;
    assert_metric(
        &metrics,
        "pangya_game_unknown_opcode_actions_total{action=\"disconnected\"} 1",
    )
    .await;
    let mut disconnect_wrong_state = connect_m4(&pool, address, "M4DiscState").await;
    send_typed(
        &mut disconnect_wrong_state.stream,
        disconnect_wrong_state.key,
        4,
        &RoomStateRequest,
    )
    .await;
    assert_closed(&mut disconnect_wrong_state.stream).await;
    let mut disconnect_wrong_m5 = connect_m4(&pool, address, "M5DiscState").await;
    send_packet(
        &mut disconnect_wrong_m5.stream,
        disconnect_wrong_m5.key,
        5,
        pangya_protocol::SYNTHETIC_M5_C2S_START_SOLO,
        &[],
    )
    .await;
    assert_closed(&mut disconnect_wrong_m5.stream).await;
    let mut disconnect_wrong_m6 = connect_m4(&pool, address, "M6DiscState").await;
    send_packet(
        &mut disconnect_wrong_m6.stream,
        disconnect_wrong_m6.key,
        6,
        pangya_protocol::SYNTHETIC_M6_C2S_START_STROKE_TWO,
        &[],
    )
    .await;
    assert_closed(&mut disconnect_wrong_m6.stream).await;
    shutdown.cancel();
    task.await.expect("join").expect("serve");

    let metrics = Arc::new(M2Metrics::default());
    let ignore_service = game_service_with_policy(
        pool.clone(),
        limits.clone(),
        metrics.clone(),
        UnknownOpcodePolicy::Ignore,
    );
    let (address, shutdown, task) = start_service(ignore_service).await;
    let mut ignored = connect_m4(&pool, address, "M4UnkIgnore").await;
    send_packet(
        &mut ignored.stream,
        ignored.key,
        3,
        0x7778,
        b"unknown-ignore-body",
    )
    .await;
    request_list(&mut ignored, 4, Vec::new()).await;
    assert_metric(
        &metrics,
        "pangya_game_unknown_opcode_actions_total{action=\"ignored\"} 1",
    )
    .await;
    send_typed(
        &mut ignored.stream,
        ignored.key,
        5,
        &RoomCreateRequest {
            name: RoomName::parse("Ignore First").expect("room name"),
            password: None,
            settings: RoomSettings::new(2).expect("settings"),
        },
    )
    .await;
    receive_result(
        &mut ignored.stream,
        ignored.key,
        RoomCommand::Create,
        RoomCommandResult::Success,
    )
    .await;
    let ignore_room = snapshot(
        RoomId::new(1).expect("room ID"),
        "Ignore First",
        &ignored.nickname,
        2,
        false,
        vec![member(&ignored, 1, true, false)],
    );
    receive_state(&mut ignored.stream, ignored.key, &ignore_room).await;
    let mut ignore_peer = connect_m4(&pool, address, "M4IgnPeer").await;
    send_typed(
        &mut ignore_peer.stream,
        ignore_peer.key,
        5,
        &RoomCreateRequest {
            name: RoomName::parse("Ignore Second").expect("room name"),
            password: None,
            settings: RoomSettings::new(2).expect("settings"),
        },
    )
    .await;
    receive_result(
        &mut ignore_peer.stream,
        ignore_peer.key,
        RoomCommand::Create,
        RoomCommandResult::Success,
    )
    .await;
    let ignore_second = snapshot(
        RoomId::new(2).expect("room ID"),
        "Ignore Second",
        &ignore_peer.nickname,
        2,
        false,
        vec![member(&ignore_peer, 2, true, false)],
    );
    receive_state(&mut ignore_peer.stream, ignore_peer.key, &ignore_second).await;
    send_typed(
        &mut ignored.stream,
        ignored.key,
        6,
        &RoomJoinRequest {
            room_id: RoomId::new(2).expect("room ID"),
            password: None,
        },
    )
    .await;
    assert_closed(&mut ignored.stream).await;
    send_typed(
        &mut ignore_peer.stream,
        ignore_peer.key,
        6,
        &RoomLeaveRequest,
    )
    .await;
    receive_result(
        &mut ignore_peer.stream,
        ignore_peer.key,
        RoomCommand::Leave,
        RoomCommandResult::Success,
    )
    .await;
    assert_metric(&metrics, "pangya_game_rooms_active{service=\"game\"} 0").await;
    let mut ignore_wrong_m5 = connect_m4(&pool, address, "M5IgnState").await;
    send_packet(
        &mut ignore_wrong_m5.stream,
        ignore_wrong_m5.key,
        30,
        pangya_protocol::SYNTHETIC_M5_C2S_START_SOLO,
        &[],
    )
    .await;
    assert_closed(&mut ignore_wrong_m5.stream).await;
    let mut ignore_wrong_m6 = connect_m4(&pool, address, "M6IgnState").await;
    send_packet(
        &mut ignore_wrong_m6.stream,
        ignore_wrong_m6.key,
        31,
        pangya_protocol::SYNTHETIC_M6_C2S_START_STROKE_TWO,
        &[],
    )
    .await;
    assert_closed(&mut ignore_wrong_m6.stream).await;
    shutdown.cancel();
    task.await.expect("join").expect("serve");

    let metrics = Arc::new(M2Metrics::default());
    let capture_service = game_service_with_policy(
        pool.clone(),
        limits.clone(),
        metrics.clone(),
        UnknownOpcodePolicy::Capture,
    );
    let inspectable_service = Arc::clone(&capture_service);
    let (address, shutdown, task) = start_service(capture_service).await;
    let mut captured = connect_m4(&pool, address, "M4UnkCapture").await;
    let capture_body = b"unknown-capture-private-body";
    send_packet(&mut captured.stream, captured.key, 3, 0x7779, capture_body).await;
    request_list(&mut captured, 4, Vec::new()).await;
    let captures = inspectable_service.unknown_opcode_captures();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].state, pangya_game::GameState::InChannel);
    assert_eq!(captures[0].opcode, 0x7779);
    assert_eq!(captures[0].payload_len, capture_body.len());
    let rendered = metrics.render();
    assert_eq!(
        metric_sample(
            &rendered,
            "pangya_game_unknown_opcode_actions_total{action=\"captured\"}",
        ),
        Some(1.0)
    );
    assert!(!rendered.contains("unknown-capture-private-body"));
    assert!(!rendered.contains(&captured.token));
    let mut capture_wrong_m5 = connect_m4(&pool, address, "M5CapState").await;
    send_packet(
        &mut capture_wrong_m5.stream,
        capture_wrong_m5.key,
        5,
        pangya_protocol::SYNTHETIC_M5_C2S_START_SOLO,
        &[],
    )
    .await;
    assert_closed(&mut capture_wrong_m5.stream).await;
    let mut capture_wrong_m6 = connect_m4(&pool, address, "M6CapState").await;
    send_packet(
        &mut capture_wrong_m6.stream,
        capture_wrong_m6.key,
        6,
        pangya_protocol::SYNTHETIC_M6_C2S_START_STROKE_TWO,
        &[],
    )
    .await;
    assert_closed(&mut capture_wrong_m6.stream).await;

    send_typed(
        &mut captured.stream,
        captured.key,
        5,
        &RoomCreateRequest {
            name: RoomName::parse("Single Membership").expect("room name"),
            password: None,
            settings: RoomSettings::new(2).expect("settings"),
        },
    )
    .await;
    receive_result(
        &mut captured.stream,
        captured.key,
        RoomCommand::Create,
        RoomCommandResult::Success,
    )
    .await;
    let only_room = snapshot(
        RoomId::new(1).expect("room ID"),
        "Single Membership",
        &captured.nickname,
        2,
        false,
        vec![member(&captured, 1, true, false)],
    );
    receive_state(&mut captured.stream, captured.key, &only_room).await;
    send_typed(
        &mut captured.stream,
        captured.key,
        6,
        &RoomCreateRequest {
            name: RoomName::parse("Forbidden Second").expect("room name"),
            password: None,
            settings: RoomSettings::new(2).expect("settings"),
        },
    )
    .await;
    assert_closed(&mut captured.stream).await;
    assert_metric(
        &metrics,
        "pangya_connections_closed_total{service=\"game\",reason=\"protocol\"} 3",
    )
    .await;
    assert_metric(&metrics, "pangya_game_rooms_active{service=\"game\"} 0").await;
    shutdown.cancel();
    task.await.expect("join").expect("serve");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m4_command_chat_and_outbound_queues_are_bounded(pool: PgPool) {
    let limits = GameRuntimeLimits {
        global_connections: 8,
        connections_per_source: 8,
        global_auth_per_window: 20,
        auth_per_window: 20,
        packets_per_window: 500,
        room_commands_per_window: 3,
        chat_messages_per_window: 1,
        ..GameRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start_game(pool.clone(), limits).await;
    let mut owner = connect_m4(&pool, address, "M4ChatOwner").await;
    let mut member_client = connect_m4(&pool, address, "M4ChatMember").await;
    let mut command_client = connect_m4(&pool, address, "M4CommandBound").await;
    send_typed(
        &mut owner.stream,
        owner.key,
        3,
        &RoomCreateRequest {
            name: RoomName::parse("Bounded Chat").expect("room name"),
            password: None,
            settings: RoomSettings::new(2).expect("settings"),
        },
    )
    .await;
    receive_result(
        &mut owner.stream,
        owner.key,
        RoomCommand::Create,
        RoomCommandResult::Success,
    )
    .await;
    let room_id = RoomId::new(1).expect("room ID");
    let one = snapshot(
        room_id,
        "Bounded Chat",
        &owner.nickname,
        2,
        false,
        vec![member(&owner, 1, true, false)],
    );
    receive_state(&mut owner.stream, owner.key, &one).await;
    send_typed(
        &mut member_client.stream,
        member_client.key,
        3,
        &RoomJoinRequest {
            room_id,
            password: None,
        },
    )
    .await;
    receive_result(
        &mut member_client.stream,
        member_client.key,
        RoomCommand::Join,
        RoomCommandResult::Success,
    )
    .await;
    let two = snapshot(
        room_id,
        "Bounded Chat",
        &owner.nickname,
        2,
        false,
        vec![
            member(&owner, 1, true, false),
            member(&member_client, 2, false, false),
        ],
    );
    receive_state(&mut member_client.stream, member_client.key, &two).await;
    receive_state(&mut owner.stream, owner.key, &two).await;
    receive_state(&mut member_client.stream, member_client.key, &two).await;
    let first_chat = ChatText::parse("one bounded chat").expect("chat");
    send_typed(
        &mut owner.stream,
        owner.key,
        4,
        &RoomChatRequest {
            text: first_chat.clone(),
        },
    )
    .await;
    receive_result(
        &mut owner.stream,
        owner.key,
        RoomCommand::Chat,
        RoomCommandResult::Success,
    )
    .await;
    let chat_event = RoomChatEvent {
        room_id,
        sender: member(&owner, 1, true, false),
        text: first_chat,
    };
    assert_eq!(
        receive_typed::<RoomChatEvent>(&mut owner.stream, owner.key).await,
        chat_event
    );
    assert_eq!(
        receive_typed::<RoomChatEvent>(&mut member_client.stream, member_client.key).await,
        chat_event
    );
    send_typed(
        &mut owner.stream,
        owner.key,
        5,
        &RoomChatRequest {
            text: ChatText::parse("rate limited chat").expect("chat"),
        },
    )
    .await;
    assert_closed(&mut owner.stream).await;
    assert_metric(&metrics, "class=\"chat_connection\"} 1").await;
    assert_metric(
        &metrics,
        "pangya_game_chat_events_total{event=\"rate_limited\"} 1",
    )
    .await;
    let member_owned = snapshot(
        room_id,
        "Bounded Chat",
        &member_client.nickname,
        2,
        false,
        vec![member(&member_client, 2, true, false)],
    );
    receive_state(&mut member_client.stream, member_client.key, &member_owned).await;

    for salt in [6, 7, 8] {
        request_list(
            &mut command_client,
            salt,
            vec![member_owned.summary().clone()],
        )
        .await;
    }
    send_typed(
        &mut command_client.stream,
        command_client.key,
        9,
        &RoomListRequest,
    )
    .await;
    assert_closed(&mut command_client.stream).await;
    assert_metric(&metrics, "class=\"room_commands_connection\"} 1").await;
    send_typed(
        &mut member_client.stream,
        member_client.key,
        10,
        &RoomLeaveRequest,
    )
    .await;
    receive_result(
        &mut member_client.stream,
        member_client.key,
        RoomCommand::Leave,
        RoomCommandResult::Success,
    )
    .await;
    assert_metric(&metrics, "pangya_game_rooms_active{service=\"game\"} 0").await;
    shutdown.cancel();
    task.await.expect("join").expect("serve");

    let queue_limits = GameRuntimeLimits {
        global_connections: 8,
        connections_per_source: 8,
        global_auth_per_window: 20,
        auth_per_window: 20,
        global_packets_per_window: 1_000_000,
        source_packets_per_window: 1_000_000,
        packets_per_window: 1_000_000,
        global_bytes_per_window: 1024 * 1024 * 1024,
        source_bytes_per_window: 1024 * 1024 * 1024,
        bytes_per_window: 1024 * 1024 * 1024,
        room_commands_per_window: 1_000_000,
        outbound_room_event_capacity: 1,
        ..GameRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start_game(pool.clone(), queue_limits).await;
    let mut queue_owner = connect_m4(&pool, address, "M4QueueOwner").await;
    let mut queue_two = connect_m4(&pool, address, "M4QueueTwo").await;
    let mut queue_three = connect_m4(&pool, address, "M4QueueThree").await;
    let mut queue_four = connect_m4(&pool, address, "M4QueueFour").await;
    send_typed(
        &mut queue_owner.stream,
        queue_owner.key,
        3,
        &RoomCreateRequest {
            name: RoomName::parse("Queue Bound").expect("room name"),
            password: None,
            settings: RoomSettings::new(4).expect("settings"),
        },
    )
    .await;
    receive_result(
        &mut queue_owner.stream,
        queue_owner.key,
        RoomCommand::Create,
        RoomCommandResult::Success,
    )
    .await;
    let queue_id = RoomId::new(1).expect("room ID");
    let queue_join = RoomJoinRequest {
        room_id: queue_id,
        password: None,
    };
    tokio::join!(
        send_typed(&mut queue_two.stream, queue_two.key, 4, &queue_join),
        send_typed(&mut queue_three.stream, queue_three.key, 4, &queue_join),
        send_typed(&mut queue_four.stream, queue_four.key, 4, &queue_join),
    );
    assert_counter_at_least(
        &metrics,
        "pangya_game_room_events_total{event=\"joined\"} ",
        3,
    )
    .await;
    let flood_two = tokio::spawn(flood_ready(queue_two.stream, queue_two.key, 20_000));
    let flood_three = tokio::spawn(flood_ready(queue_three.stream, queue_three.key, 20_000));
    let flood_four = tokio::spawn(flood_ready(queue_four.stream, queue_four.key, 20_000));
    assert_counter_at_least(
        &metrics,
        "pangya_game_queue_events_total{event=\"outbound_dropped\"} ",
        1,
    )
    .await;
    assert_counter_at_least(
        &metrics,
        "pangya_connections_closed_total{service=\"game\",reason=\"limited\"} ",
        1,
    )
    .await;
    flood_two.abort();
    flood_three.abort();
    flood_four.abort();
    drop(queue_owner);
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .expect("bounded shutdown")
        .expect("join")
        .expect("serve");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m5_encrypted_tcp_happy_path_persists_once_and_restarts_projection(pool: PgPool) {
    let traces = tracing_capture();
    let trace_start = traces.lock().expect("traces").len();
    let limits = GameRuntimeLimits {
        global_connections: 8,
        connections_per_source: 8,
        global_auth_per_window: 20,
        auth_per_window: 20,
        outbound_room_event_capacity: 2,
        ..GameRuntimeLimits::default()
    };
    let metrics = Arc::new(M2Metrics::default());
    let service = solo_service(
        pool.clone(),
        limits.clone(),
        metrics.clone(),
        Duration::from_secs(2),
        20,
    );
    let (address, shutdown, task) = start_service(service).await;
    let room_name = uuid::Uuid::new_v4().simple().to_string();
    let mut client = connect_m5_owner(&pool, address, "M5Happy", &room_name).await;
    let started = start_solo(&mut client).await;
    assert_eq!(started.course_id(), 1);
    assert_eq!(started.hole(), 1);
    assert_eq!(started.par(), 3);
    assert_ne!(started.match_id().as_u128(), 0);
    assert_eq!(started.seed().len(), 32);
    assert!(started.seed().iter().any(|byte| *byte != 0));
    assert_eq!(started.load_timeout_ms(), 2_000);
    let (expected_weather, expected_wind) =
        deterministic_conditions(MatchSeed::new(*started.seed())).expect("conditions");
    assert_eq!(
        started.weather(),
        match expected_weather {
            Weather::Clear => ProtocolWeather::Clear,
            Weather::Cloudy => ProtocolWeather::Cloudy,
            Weather::Rain => ProtocolWeather::Rain,
        }
    );
    assert_eq!(
        started.wind().speed(),
        f32::from(expected_wind.speed_tenths()) / 10.0
    );
    assert_eq!(
        started.wind().angle(),
        f32::from(expected_wind.angle_degrees())
    );

    let catalog_fingerprint = m5_catalog().fingerprint();
    let persisted_begin: PersistedBeginRow = sqlx::query_as(
        "SELECT course_id, hole, par, catalog_sha256, seed, weather, wind_speed_tenths, \
                    wind_angle_degrees, reward_formula, status FROM matches WHERE id::text = $1",
    )
    .bind(started.match_id().to_string())
    .fetch_one(&pool)
    .await
    .expect("persisted begin");
    assert_eq!(persisted_begin.0, 1);
    assert_eq!((persisted_begin.1, persisted_begin.2), (1, 3));
    assert_eq!(persisted_begin.3, catalog_fingerprint.as_bytes());
    assert_eq!(persisted_begin.4, started.seed());
    assert_eq!(
        persisted_begin.5,
        match expected_weather {
            Weather::Clear => "clear",
            Weather::Cloudy => "cloudy",
            Weather::Rain => "rain",
        }
    );
    assert_eq!(
        persisted_begin.6,
        i16::try_from(expected_wind.speed_tenths()).expect("speed")
    );
    assert_eq!(
        persisted_begin.7,
        i16::try_from(expected_wind.angle_degrees()).expect("angle")
    );
    assert_eq!(
        (&persisted_begin.8, &persisted_begin.9),
        (&"solo-v1".to_owned(), &"loading".to_owned())
    );

    enter_playing(&mut client, &started).await;
    let action_power_canary = f32::from_bits(0x42f6_abcd);
    let result_x_canary = f32::from_bits(0x42ca_dcba);
    let first_action =
        ShotAction::new(1, 0, action_power_canary, 10.25, 0.5, -0.25).expect("action one");
    let first_result =
        ShotResult::new(1, result_x_canary, 2.5, -3.75, Lie::Fairway, false).expect("result one");
    relay_shot(&mut client, 6, first_action, first_result).await;
    let second_action = ShotAction::new(2, 1, 77.0, -5.0, 0.0, 0.0).expect("action two");
    let second_result = ShotResult::new(2, 0.125, 0.25, 0.5, Lie::Green, true).expect("result two");
    relay_shot(&mut client, 8, second_action, second_result).await;

    let before_finish: (String, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT m.status, \
            (SELECT count(*) FROM match_players WHERE match_id = m.id), \
            (SELECT count(*) FROM currency_ledger WHERE match_id = m.id), \
            (SELECT count(*) FROM progression_ledger WHERE match_id = m.id), \
            (SELECT count(*) FROM match_audit_events WHERE match_id = m.id), \
            (SELECT pang + experience FROM profiles WHERE account_id = $2) \
         FROM matches m WHERE m.id::text = $1",
    )
    .bind(started.match_id().to_string())
    .bind(client.account_id.get())
    .fetch_one(&pool)
    .await
    .expect("pre-finish snapshot");
    assert_eq!(before_finish, ("in_game".to_owned(), 1, 0, 0, 1, 0));

    let (hole, balance) = finish_solo(&mut client, &started).await;
    assert_eq!(hole.match_id(), started.match_id());
    assert_eq!(hole.hole(), 1);
    assert_eq!(hole.strokes(), 2);
    assert_eq!(hole.score(), -1);
    assert_eq!(hole.pang(), 12);
    assert_eq!(hole.experience(), 5);
    assert_ne!(hole.result_id().as_u128(), 0);
    assert_eq!(balance, BalanceUpdate::new(12, 5));

    let committed: (String, String, i16, i16, bool, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT m.status, m.result_commit_key::text, mp.strokes, mp.score, mp.quit, \
                mp.pang_reward, mp.experience_reward, mp.pang_balance_after, \
                mp.experience_balance_after FROM matches m JOIN match_players mp \
                ON mp.match_id = m.id WHERE m.id::text = $1",
    )
    .bind(started.match_id().to_string())
    .fetch_one(&pool)
    .await
    .expect("committed match");
    assert_eq!(committed.0, "committed");
    assert_eq!(committed.1, hole.result_id().to_string());
    assert_eq!((committed.2, committed.3, committed.4), (2, -1, false));
    assert_eq!(
        (committed.5, committed.6, committed.7, committed.8),
        (12, 5, 12, 5)
    );
    let history: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM currency_ledger WHERE match_id::text = $1), \
                (SELECT count(*) FROM progression_ledger WHERE match_id::text = $1), \
                (SELECT count(*) FROM match_audit_events WHERE match_id::text = $1), \
                (SELECT pang FROM profiles WHERE account_id = $2), \
                (SELECT experience FROM profiles WHERE account_id = $2)",
    )
    .bind(started.match_id().to_string())
    .bind(client.account_id.get())
    .fetch_one(&pool)
    .await
    .expect("committed history");
    assert_eq!(history, (1, 1, 2, 12, 5));
    let audits: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT event, outcome, reason FROM match_audit_events WHERE match_id::text = $1 ORDER BY id",
    )
    .bind(started.match_id().to_string())
    .fetch_all(&pool)
    .await
    .expect("audits");
    assert_eq!(
        audits,
        vec![
            ("started".to_owned(), "success".to_owned(), None),
            ("committed".to_owned(), "success".to_owned(), None),
        ]
    );

    send_typed(&mut client.stream, client.key, 11, &FinishHole::new()).await;
    assert_closed(&mut client.stream).await;
    let history_after_duplicate: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM currency_ledger WHERE match_id::text = $1), \
                (SELECT count(*) FROM progression_ledger WHERE match_id::text = $1), \
                (SELECT count(*) FROM match_audit_events WHERE match_id::text = $1), \
                (SELECT pang FROM profiles WHERE account_id = $2), \
                (SELECT experience FROM profiles WHERE account_id = $2)",
    )
    .bind(started.match_id().to_string())
    .bind(client.account_id.get())
    .fetch_one(&pool)
    .await
    .expect("duplicate history");
    assert_eq!(history_after_duplicate, history);

    let rendered = metrics.render();
    for expected in [
        "pangya_game_match_events_total{mode=\"solo_practice\",event=\"started\"} 1",
        "pangya_game_match_events_total{mode=\"solo_practice\",event=\"loading_complete\"} 1",
        "pangya_game_match_events_total{mode=\"solo_practice\",event=\"finished\"} 1",
        "pangya_game_commit_outcomes_total{mode=\"solo_practice\",outcome=\"begun\"} 1",
        "pangya_game_commit_outcomes_total{mode=\"solo_practice\",outcome=\"committed\"} 1",
        "pangya_game_shot_outcomes_total{mode=\"solo_practice\",outcome=\"accepted\"} 4",
        "pangya_game_matches_active{mode=\"solo_practice\"} 0",
    ] {
        let (key, value) = parse_expected_metric(expected);
        assert_eq!(metric_sample(&rendered, &key), Some(value), "{expected}");
    }
    let seed_hex = started
        .seed()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let action_value_canary = format!("{action_power_canary:?}");
    let result_value_canary = format!("{result_x_canary:?}");
    let action_bits_canary = format!("{:08x}", action_power_canary.to_bits());
    let result_bits_canary = format!("{:08x}", result_x_canary.to_bits());
    for sensitive in [
        room_name.as_str(),
        client.nickname.as_str(),
        seed_hex.as_str(),
        committed.1.as_str(),
        action_value_canary.as_str(),
        result_value_canary.as_str(),
        action_bits_canary.as_str(),
        result_bits_canary.as_str(),
        client.token.as_str(),
        "pang=12",
        "experience=5",
    ] {
        assert!(!rendered.contains(sensitive));
    }
    let trace_bytes = traces.lock().expect("traces").clone();
    let trace_text = String::from_utf8_lossy(&trace_bytes[trace_start..]);
    for sensitive in [
        room_name.as_str(),
        client.nickname.as_str(),
        seed_hex.as_str(),
        committed.1.as_str(),
        action_value_canary.as_str(),
        result_value_canary.as_str(),
        action_bits_canary.as_str(),
        result_bits_canary.as_str(),
        client.token.as_str(),
        "pang=12",
        "experience=5",
    ] {
        assert!(!trace_text.contains(sensitive), "trace leaked {sensitive}");
    }

    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .expect("first service shutdown")
        .expect("first service join")
        .expect("first service serve");

    let restart_metrics = Arc::new(M2Metrics::default());
    let restarted = solo_service(
        pool.clone(),
        limits,
        restart_metrics,
        Duration::from_secs(2),
        20,
    );
    let (address, shutdown, task) = start_service(restarted).await;
    let fresh_token = issue_token(
        &pool,
        client.account_id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut fresh, key) = connect_game(address).await;
    send_packet(
        &mut fresh,
        key,
        12,
        2,
        &auth_payload(client.account_id.get(), &fresh_token),
    )
    .await;
    let player = read_player_info(&mut fresh, key).await;
    assert_eq!(
        player.0,
        u64::try_from(client.account_id.get()).expect("account")
    );
    assert_eq!(player.1, client.nickname.as_bytes());
    assert_eq!((player.2, player.3, player.4), (12, 0, 5));
    read_bootstrap_after_player(&mut fresh, key, 1).await;
    let unchanged: (String, i64, i64, i64) = sqlx::query_as(
        "SELECT status, \
                (SELECT count(*) FROM currency_ledger WHERE match_id::text = $1), \
                (SELECT count(*) FROM progression_ledger WHERE match_id::text = $1), \
                (SELECT count(*) FROM match_audit_events WHERE match_id::text = $1) \
         FROM matches WHERE id::text = $1",
    )
    .bind(started.match_id().to_string())
    .fetch_one(&pool)
    .await
    .expect("restart persistence");
    assert_eq!(unchanged, ("committed".to_owned(), 1, 1, 2));
    shutdown.cancel();
    task.await.expect("restart join").expect("restart serve");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m5_unclean_in_game_restart_recovers_before_fresh_auth(pool: PgPool) {
    // The loading deadline passed to each service below is only "long enough to finish
    // loading"; this test asserts startup recovery, never a loading timeout. Recovery
    // itself stays bounded by its own one-second budget.
    let limits = GameRuntimeLimits {
        global_connections: 4,
        connections_per_source: 4,
        global_auth_per_window: 10,
        auth_per_window: 10,
        ..GameRuntimeLimits::default()
    };
    let service = solo_service(
        pool.clone(),
        limits.clone(),
        Arc::new(M2Metrics::default()),
        Duration::from_secs(10),
        20,
    );
    let (address, _shutdown, task) = start_service(service).await;
    let mut client = connect_m5_owner(&pool, address, "M5Unclean", "Unclean Recovery").await;
    let started = start_solo(&mut client).await;
    enter_playing(&mut client, &started).await;
    let match_id = started.match_id().to_string();
    let account_id = client.account_id;
    let status: String = sqlx::query_scalar("SELECT status FROM matches WHERE id::text = $1")
        .bind(&match_id)
        .fetch_one(&pool)
        .await
        .expect("durable in-game status");
    assert_eq!(status, "in_game");

    // Simulate process loss: abort supervision without allowing connection or lobby cleanup.
    task.abort();
    assert!(task.await.expect_err("unclean task abort").is_cancelled());
    drop(client);
    let status: String = sqlx::query_scalar("SELECT status FROM matches WHERE id::text = $1")
        .bind(&match_id)
        .fetch_one(&pool)
        .await
        .expect("stale in-game status");
    assert_eq!(status, "in_game");

    // This is the same bounded repository call made by production before listener binding.
    let repository = PgRepository::new(pool.clone());
    let recovered = tokio::time::timeout(
        Duration::from_secs(1),
        pangya_domain::MatchRepository::abort_incomplete_matches(
            &repository,
            IncompleteMatchAbortLimit::new(100).expect("recovery limit"),
        ),
    )
    .await
    .expect("bounded recovery")
    .expect("startup recovery");
    assert_eq!(recovered, 1);
    wait_for_abort(&pool, &match_id, "startup_recovery").await;
    assert_no_match_reward(&pool, &match_id, account_id).await;

    // Only after recovery completes is a fresh GameService listener started and authenticated.
    let service = solo_service(
        pool.clone(),
        limits,
        Arc::new(M2Metrics::default()),
        Duration::from_secs(10),
        20,
    );
    let (address, shutdown, task) = start_service(service).await;
    let token = issue_token(&pool, account_id, SystemTime::now(), ServiceKind::Game).await;
    let (mut fresh, key) = connect_game(address).await;
    send_packet(
        &mut fresh,
        key,
        30,
        2,
        &auth_payload(account_id.get(), &token),
    )
    .await;
    read_bootstrap(&mut fresh, key, 1).await;
    shutdown.cancel();
    task.await.expect("fresh join").expect("fresh serve");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m5_shot_sequence_and_fixed_window_limits_are_independent(pool: PgPool) {
    let limits = GameRuntimeLimits {
        global_connections: 4,
        connections_per_source: 4,
        global_auth_per_window: 20,
        auth_per_window: 20,
        rate_window: Duration::from_secs(2),
        ..GameRuntimeLimits::default()
    };
    let metrics = Arc::new(M2Metrics::default());
    let service = solo_service(
        pool.clone(),
        limits,
        metrics.clone(),
        Duration::from_secs(1),
        4,
    );
    let (address, shutdown, task) = start_service(service).await;
    let mut client = connect_m5_owner(&pool, address, "M5ShotBound", "Shot Bounds").await;
    let started = start_solo(&mut client).await;
    enter_playing(&mut client, &started).await;
    let action = ShotAction::new(1, 1, 50.0, 0.0, 0.0, 0.0).expect("action");
    send_typed(&mut client.stream, client.key, 6, &action).await;
    receive_action_success(&mut client, action).await;

    // Exact duplicate coalesces without another relay, while changed content conflicts.
    send_typed(&mut client.stream, client.key, 7, &action).await;
    receive_solo_result(
        &mut client,
        SoloCommand::ShotAction,
        SoloCommandOutcome::Success,
    )
    .await;
    let conflict = ShotAction::new(1, 1, 51.0, 0.0, 0.0, 0.0).expect("conflict");
    send_typed(&mut client.stream, client.key, 8, &conflict).await;
    receive_solo_result(
        &mut client,
        SoloCommand::ShotAction,
        SoloCommandOutcome::InvalidSequence,
    )
    .await;
    let result = ShotResult::new(1, 1.0, 2.0, 3.0, Lie::Fairway, false).expect("result");
    send_typed(&mut client.stream, client.key, 9, &result).await;
    receive_shot_result_success(&mut client, result).await;

    // The fifth action/result packet hits only the M5 shot budget, not general packet limits.
    let next = ShotAction::new(2, 1, 50.0, 0.0, 0.0, 0.0).expect("next action");
    send_typed(&mut client.stream, client.key, 10, &next).await;
    receive_solo_result(
        &mut client,
        SoloCommand::ShotAction,
        SoloCommandOutcome::Timeout,
    )
    .await;
    send_typed(&mut client.stream, client.key, 11, &next).await;
    receive_solo_result(
        &mut client,
        SoloCommand::ShotAction,
        SoloCommandOutcome::Timeout,
    )
    .await;
    assert_metric(&metrics, "class=\"shot_packets_connection\"} 2").await;
    assert_metric(
        &metrics,
        "pangya_game_shot_outcomes_total{mode=\"solo_practice\",outcome=\"duplicate\"} 1",
    )
    .await;
    shutdown.cancel();
    task.await.expect("shot join").expect("shot serve");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m5_encrypted_tcp_abort_timeout_malformed_and_shutdown_paths_do_not_reward(
    pool: PgPool,
) {
    // No assertion below observes these two deadlines; they only have to outlast a loaded
    // runner's actor round trip. The deadlines this test does assert on are the 150ms
    // loading timeout and the bounded receives, which keep their original values.
    let limits = GameRuntimeLimits {
        global_connections: 8,
        connections_per_source: 8,
        global_auth_per_window: 50,
        auth_per_window: 50,
        idle_timeout: Duration::from_secs(10),
        command_timeout: Duration::from_secs(1),
        shutdown_grace: Duration::from_secs(1),
        ..GameRuntimeLimits::default()
    };

    // Disconnect while loading.
    let metrics = Arc::new(M2Metrics::default());
    let service = solo_service(
        pool.clone(),
        limits.clone(),
        metrics.clone(),
        Duration::from_secs(10),
        20,
    );
    let (address, shutdown, task) = start_service(service).await;
    let mut loading = connect_m5_owner(&pool, address, "M5AbortLoad", "Abort Loading").await;
    let loading_started = start_solo(&mut loading).await;
    let loading_id = loading_started.match_id().to_string();
    let loading_account = loading.account_id;
    loading.stream.shutdown().await.expect("loading disconnect");
    drop(loading);
    wait_for_abort(&pool, &loading_id, "disconnect").await;
    assert_no_match_reward(&pool, &loading_id, loading_account).await;
    assert_metric(
        &metrics,
        "pangya_game_match_events_total{mode=\"solo_practice\",event=\"aborted\"} 1",
    )
    .await;
    shutdown.cancel();
    task.await.expect("loading join").expect("loading serve");

    // A fresh service proves presence cleanup without scheduler-yield timing assumptions.
    let service = solo_service(
        pool.clone(),
        limits.clone(),
        Arc::new(M2Metrics::default()),
        Duration::from_secs(10),
        20,
    );
    let (address, shutdown, task) = start_service(service).await;
    let reconnect_token =
        issue_token(&pool, loading_account, SystemTime::now(), ServiceKind::Game).await;
    let (mut reconnect, reconnect_key) = connect_game(address).await;
    send_packet(
        &mut reconnect,
        reconnect_key,
        20,
        2,
        &auth_payload(loading_account.get(), &reconnect_token),
    )
    .await;
    read_bootstrap(&mut reconnect, reconnect_key, 1).await;
    send_packet(&mut reconnect, reconnect_key, 21, 4, &1_u32.to_le_bytes()).await;
    assert_eq!(
        receive_packet(&mut reconnect, reconnect_key).await.0,
        0x004e
    );
    send_typed(
        &mut reconnect,
        reconnect_key,
        22,
        &LoadingComplete::new(100).expect("loading"),
    )
    .await;
    assert_closed(&mut reconnect).await;
    assert_no_match_reward(&pool, &loading_id, loading_account).await;
    shutdown.cancel();
    task.await
        .expect("loading reconnect join")
        .expect("loading reconnect serve");

    // Disconnect after an accepted action in game.
    let metrics = Arc::new(M2Metrics::default());
    let service = solo_service(
        pool.clone(),
        limits.clone(),
        metrics.clone(),
        Duration::from_secs(10),
        20,
    );
    let (address, shutdown, task) = start_service(service).await;
    let mut playing = connect_m5_owner(&pool, address, "M5AbortPlay", "Abort Playing").await;
    let playing_started = start_solo(&mut playing).await;
    enter_playing(&mut playing, &playing_started).await;
    let action = ShotAction::new(1, 2, 88.0, 0.0, 0.0, 0.0).expect("abort action");
    send_typed(&mut playing.stream, playing.key, 6, &action).await;
    receive_action_success(&mut playing, action).await;
    let playing_id = playing_started.match_id().to_string();
    let playing_account = playing.account_id;
    playing.stream.shutdown().await.expect("playing disconnect");
    drop(playing);
    wait_for_abort(&pool, &playing_id, "disconnect").await;
    assert_no_match_reward(&pool, &playing_id, playing_account).await;
    shutdown.cancel();
    task.await.expect("playing join").expect("playing serve");

    // Actor loading timeout is visible on the encrypted wire and persisted once.
    let metrics = Arc::new(M2Metrics::default());
    let service = solo_service(
        pool.clone(),
        limits.clone(),
        metrics.clone(),
        Duration::from_millis(150),
        20,
    );
    let (address, shutdown, task) = start_service(service).await;
    let mut timed = connect_m5_owner(&pool, address, "M5LoadTimeout", "Loading Timeout").await;
    let timed_started = start_solo(&mut timed).await;
    // Still bounded, and the assertion is on the abort reason rather than on arrival speed,
    // so the wider window costs nothing and cannot be tripped by a busy runner.
    let aborted = tokio::time::timeout(
        Duration::from_secs(5),
        receive_typed::<MatchAborted>(&mut timed.stream, timed.key),
    )
    .await
    .expect("bounded timeout event");
    assert_eq!(
        aborted,
        MatchAborted::new(timed_started.match_id(), MatchAbortReason::LoadingTimeout)
    );
    let timed_id = timed_started.match_id().to_string();
    wait_for_abort(&pool, &timed_id, "loading_timeout").await;
    assert_no_match_reward(&pool, &timed_id, timed.account_id).await;
    for needle in [
        "pangya_game_match_events_total{mode=\"solo_practice\",event=\"started\"} 1",
        "pangya_game_match_events_total{mode=\"solo_practice\",event=\"loading_timeout\"} 1",
        "pangya_game_commit_outcomes_total{mode=\"solo_practice\",outcome=\"begun\"} 1",
        "pangya_game_matches_active{mode=\"solo_practice\"} 0",
    ] {
        assert_metric(&metrics, needle).await;
    }
    shutdown.cancel();
    task.await.expect("timeout join").expect("timeout serve");

    // A non-finite raw action is rejected by protocol decoding; cleanup aborts without reward.
    let metrics = Arc::new(M2Metrics::default());
    let service = solo_service(
        pool.clone(),
        limits.clone(),
        metrics,
        Duration::from_secs(10),
        20,
    );
    let (address, shutdown, task) = start_service(service).await;
    let mut malformed = connect_m5_owner(&pool, address, "M5Malformed", "Malformed Shot").await;
    let malformed_started = start_solo(&mut malformed).await;
    enter_playing(&mut malformed, &malformed_started).await;
    let mut body = Vec::new();
    body.extend_from_slice(&1_u32.to_le_bytes());
    body.push(0);
    body.extend_from_slice(&f32::NAN.to_le_bytes());
    body.extend_from_slice(&0_f32.to_le_bytes());
    body.extend_from_slice(&0_f32.to_le_bytes());
    body.extend_from_slice(&0_f32.to_le_bytes());
    send_packet(
        &mut malformed.stream,
        malformed.key,
        7,
        pangya_protocol::SYNTHETIC_M5_C2S_SHOT_ACTION,
        &body,
    )
    .await;
    assert_closed(&mut malformed.stream).await;
    let malformed_id = malformed_started.match_id().to_string();
    wait_for_abort(&pool, &malformed_id, "disconnect").await;
    assert_no_match_reward(&pool, &malformed_id, malformed.account_id).await;
    shutdown.cancel();
    task.await
        .expect("malformed join")
        .expect("malformed serve");

    // Service shutdown drains the active room and persists its terminal reason.
    let metrics = Arc::new(M2Metrics::default());
    let service = solo_service(pool.clone(), limits, metrics, Duration::from_secs(10), 20);
    let (address, shutdown, task) = start_service(service).await;
    let mut stopping = connect_m5_owner(&pool, address, "M5Shutdown", "Shutdown Match").await;
    send_typed(&mut stopping.stream, stopping.key, 4, &StartSolo::new()).await;
    receive_solo_result(
        &mut stopping,
        SoloCommand::StartSolo,
        SoloCommandOutcome::Success,
    )
    .await;
    // Cancel before draining SoloStarted/Loading so cleanup cannot depend on lagging local state.
    let stopping_account = stopping.account_id;
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("shutdown bound")
        .expect("shutdown join")
        .expect("shutdown serve");
    let stopping_id: String = sqlx::query_scalar(
        "SELECT id::text FROM matches WHERE id IN \
         (SELECT match_id FROM match_players WHERE account_id = $1)",
    )
    .bind(stopping_account.get())
    .fetch_one(&pool)
    .await
    .expect("shutdown match ID");
    wait_for_abort(&pool, &stopping_id, "shutdown").await;
    assert_no_match_reward(&pool, &stopping_id, stopping_account).await;
    assert_closed_after_draining(&mut stopping.stream).await;
}

async fn start_stroke_loading_pair(
    pool: &PgPool,
    address: std::net::SocketAddr,
    suffix: &str,
) -> (M4Client, M4Client, StrokeMatchStarted, u64, u64) {
    let mut owner = connect_m4(pool, address, &format!("M6Own{suffix}")).await;
    let mut peer = connect_m4(pool, address, &format!("M6Peer{suffix}")).await;
    send_typed(
        &mut owner.stream,
        owner.key,
        40,
        &RoomCreateRequest {
            name: RoomName::parse(&format!("Stroke {suffix}")).expect("room"),
            password: None,
            settings: RoomSettings::new(2).expect("settings"),
        },
    )
    .await;
    receive_result(
        &mut owner.stream,
        owner.key,
        RoomCommand::Create,
        RoomCommandResult::Success,
    )
    .await;
    let owner_state = receive_typed::<RoomStateResponse>(&mut owner.stream, owner.key).await;
    let room_id = owner_state.room.summary().id();
    let owner_connection = owner_state.room.members()[0].connection_id().get();
    send_typed(
        &mut peer.stream,
        peer.key,
        41,
        &RoomJoinRequest {
            room_id,
            password: None,
        },
    )
    .await;
    receive_result(
        &mut peer.stream,
        peer.key,
        RoomCommand::Join,
        RoomCommandResult::Success,
    )
    .await;
    let joined = receive_typed::<RoomStateResponse>(&mut peer.stream, peer.key).await;
    let peer_connection = joined.room.members()[1].connection_id().get();
    let _: RoomStateResponse = receive_typed(&mut owner.stream, owner.key).await;
    let _: RoomStateResponse = receive_typed(&mut peer.stream, peer.key).await;
    send_typed(
        &mut owner.stream,
        owner.key,
        42,
        &RoomReadyRequest { ready: true },
    )
    .await;
    receive_result(
        &mut owner.stream,
        owner.key,
        RoomCommand::Ready,
        RoomCommandResult::Success,
    )
    .await;
    let _: RoomStateResponse = receive_typed(&mut owner.stream, owner.key).await;
    let _: RoomStateResponse = receive_typed(&mut owner.stream, owner.key).await;
    let _: RoomStateResponse = receive_typed(&mut peer.stream, peer.key).await;
    send_typed(
        &mut peer.stream,
        peer.key,
        42,
        &RoomReadyRequest { ready: true },
    )
    .await;
    receive_result(
        &mut peer.stream,
        peer.key,
        RoomCommand::Ready,
        RoomCommandResult::Success,
    )
    .await;
    let _: RoomStateResponse = receive_typed(&mut peer.stream, peer.key).await;
    let _: RoomStateResponse = receive_typed(&mut owner.stream, owner.key).await;
    let _: RoomStateResponse = receive_typed(&mut peer.stream, peer.key).await;
    send_typed(&mut owner.stream, owner.key, 43, &StartStrokeTwo).await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut owner.stream, owner.key).await,
        StrokeCommandResult::new(StrokeCommand::Start, StrokeCommandOutcome::Success)
    );
    let started = receive_typed::<StrokeMatchStarted>(&mut owner.stream, owner.key).await;
    assert_eq!(
        receive_typed::<StrokeMatchStarted>(&mut peer.stream, peer.key).await,
        started
    );
    for client in [&mut owner, &mut peer] {
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(started.match_id(), StrokePhaseKind::Loading)
        );
    }
    (owner, peer, started, owner_connection, peer_connection)
}

async fn enter_stroke_playing(
    owner: &mut M4Client,
    peer: &mut M4Client,
    started: &StrokeMatchStarted,
) -> u64 {
    send_typed(
        &mut peer.stream,
        peer.key,
        45,
        &StrokeLoadingComplete::new(100).expect("load"),
    )
    .await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut peer.stream, peer.key).await,
        StrokeCommandResult::new(StrokeCommand::Load, StrokeCommandOutcome::Success)
    );
    send_typed(
        &mut owner.stream,
        owner.key,
        46,
        &StrokeLoadingComplete::new(100).expect("load"),
    )
    .await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut owner.stream, owner.key).await,
        StrokeCommandResult::new(StrokeCommand::Load, StrokeCommandOutcome::Success)
    );
    let mut active = None;
    for client in [owner, peer] {
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(started.match_id(), StrokePhaseKind::Playing)
        );
        let turn = receive_typed::<StrokeTurnStarted>(&mut client.stream, client.key).await;
        if let Some(expected) = active {
            assert_eq!(turn.active_connection_id(), expected);
        } else {
            active = Some(turn.active_connection_id());
        }
    }
    active.expect("active stroke participant")
}

async fn assert_stroke_match_status(
    pool: &PgPool,
    match_id: uuid::Uuid,
    status: &str,
    expected_audit_terminal: &str,
) {
    let row: (String, i64) = sqlx::query_as(
        "SELECT status, (SELECT count(*) FROM match_audit_events \
         WHERE match_id = matches.id AND event = $2) FROM matches WHERE id = $1",
    )
    .bind(match_id)
    .bind(expected_audit_terminal)
    .fetch_one(pool)
    .await
    .expect("terminal match status");
    assert_eq!(row, (status.to_owned(), 1));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m6_encrypted_tcp_deadlines_persist_normative_terminal_policy(pool: PgPool) {
    let limits = GameRuntimeLimits {
        global_connections: 6,
        connections_per_source: 6,
        auth_per_window: 40,
        packets_per_window: 300,
        room_commands_per_window: 150,
        outbound_room_event_capacity: 16,
        ..GameRuntimeLimits::default()
    };

    // Loading deadline is an aggregate no-reward abort.
    let metrics = Arc::new(M2Metrics::default());
    let service = stroke_service_with_deadlines(
        pool.clone(),
        limits.clone(),
        metrics.clone(),
        Duration::from_millis(750),
        Duration::from_secs(2),
        Duration::from_secs(3),
    );
    let (address, shutdown, task) = start_service(service).await;
    let (mut loading_owner, mut loading_peer, loading_started, _, _) =
        start_stroke_loading_pair(&pool, address, "DLoad").await;
    for client in [&mut loading_owner, &mut loading_peer] {
        assert_eq!(
            tokio::time::timeout(
                Duration::from_secs(3),
                receive_typed::<StrokeMatchAborted>(&mut client.stream, client.key),
            )
            .await
            .expect("bounded loading deadline"),
            StrokeMatchAborted::new(
                loading_started.match_id(),
                StrokeAbortReason::LoadingTimeout,
            )
        );
    }
    assert_stroke_match_status(&pool, loading_started.match_id(), "aborted", "aborted").await;
    let loading_ledgers: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM currency_ledger WHERE match_id = $1) + \
         (SELECT count(*) FROM progression_ledger WHERE match_id = $1)",
    )
    .bind(loading_started.match_id())
    .fetch_one(&pool)
    .await
    .expect("loading deadline ledgers");
    assert_eq!(loading_ledgers, 0);
    assert_metric(
        &metrics,
        "pangya_game_matches_active{mode=\"stroke_two\"} 0",
    )
    .await;
    shutdown.cancel();
    task.await
        .expect("loading deadline join")
        .expect("loading deadline service");

    // The active participant loses on a turn deadline and the other receives one reward pair.
    let metrics = Arc::new(M2Metrics::default());
    let service = stroke_service_with_deadlines(
        pool.clone(),
        limits.clone(),
        metrics.clone(),
        Duration::from_secs(2),
        Duration::from_millis(750),
        Duration::from_secs(3),
    );
    let (address, shutdown, task) = start_service(service).await;
    let (mut turn_owner, mut turn_peer, turn_started, owner_connection, peer_connection) =
        start_stroke_loading_pair(&pool, address, "DTurn").await;
    assert_eq!(
        enter_stroke_playing(&mut turn_owner, &mut turn_peer, &turn_started).await,
        owner_connection
    );
    let mut turn_standings = None;
    for client in [&mut turn_owner, &mut turn_peer] {
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(turn_started.match_id(), StrokePhaseKind::ResultsPending)
        );
        let standings = receive_typed::<StrokeStandings>(&mut client.stream, client.key).await;
        if let Some(expected) = &turn_standings {
            assert_eq!(&standings, expected);
        } else {
            turn_standings = Some(standings);
        }
        let _: StrokeBalanceUpdate = receive_typed(&mut client.stream, client.key).await;
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(turn_started.match_id(), StrokePhaseKind::Finished)
        );
    }
    let entries = turn_standings.expect("turn standings");
    assert_eq!(entries.entries()[0].connection_id(), peer_connection);
    assert_eq!(
        entries.entries()[0].completion(),
        StrokeCompletion::WinnerByForfeit
    );
    assert_eq!(
        (
            entries.entries()[0].pang(),
            entries.entries()[0].experience()
        ),
        (10, 5)
    );
    assert_eq!(entries.entries()[1].connection_id(), owner_connection);
    assert_eq!(
        entries.entries()[1].completion(),
        StrokeCompletion::TurnTimeout
    );
    assert_eq!(
        (
            entries.entries()[1].pang(),
            entries.entries()[1].experience()
        ),
        (0, 0)
    );
    assert_stroke_match_status(&pool, turn_started.match_id(), "committed", "committed").await;
    let turn_ledgers: Vec<(String, i64)> = sqlx::query_as(
        "SELECT 'currency', count(*) FROM currency_ledger WHERE match_id = $1 \
         UNION ALL SELECT 'progression', count(*) FROM progression_ledger WHERE match_id = $1 \
         ORDER BY 1",
    )
    .bind(turn_started.match_id())
    .fetch_all(&pool)
    .await
    .expect("turn deadline ledgers");
    assert_eq!(
        turn_ledgers,
        vec![("currency".to_owned(), 1), ("progression".to_owned(), 1)]
    );
    assert_metric(
        &metrics,
        "pangya_game_matches_active{mode=\"stroke_two\"} 0",
    )
    .await;
    shutdown.cancel();
    task.await
        .expect("turn deadline join")
        .expect("turn deadline service");

    // An exact turn/game tie is actor-defined to choose the whole-game cap; both unfinished
    // participants complete as GameTimeout and the aggregate is atomically committed once.
    let metrics = Arc::new(M2Metrics::default());
    let service = stroke_service_with_deadlines(
        pool.clone(),
        limits,
        metrics.clone(),
        Duration::from_secs(2),
        Duration::from_millis(750),
        Duration::from_millis(750),
    );
    let (address, shutdown, task) = start_service(service).await;
    let (mut game_owner, mut game_peer, game_started, _, _) =
        start_stroke_loading_pair(&pool, address, "DGame").await;
    let _active = enter_stroke_playing(&mut game_owner, &mut game_peer, &game_started).await;
    let mut game_standings = None;
    for client in [&mut game_owner, &mut game_peer] {
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(game_started.match_id(), StrokePhaseKind::ResultsPending)
        );
        let standings = receive_typed::<StrokeStandings>(&mut client.stream, client.key).await;
        if let Some(expected) = &game_standings {
            assert_eq!(&standings, expected);
        } else {
            game_standings = Some(standings);
        }
        let _: StrokeBalanceUpdate = receive_typed(&mut client.stream, client.key).await;
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(game_started.match_id(), StrokePhaseKind::Finished)
        );
    }
    assert!(
        game_standings
            .expect("game standings")
            .entries()
            .iter()
            .all(|entry| entry.completion() == StrokeCompletion::GameTimeout)
    );
    assert_stroke_match_status(&pool, game_started.match_id(), "committed", "committed").await;
    let game_ledgers: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM currency_ledger WHERE match_id = $1) + \
         (SELECT count(*) FROM progression_ledger WHERE match_id = $1)",
    )
    .bind(game_started.match_id())
    .fetch_one(&pool)
    .await
    .expect("game deadline ledgers");
    assert_eq!(game_ledgers, 0);
    assert_metric(
        &metrics,
        "pangya_game_matches_active{mode=\"stroke_two\"} 0",
    )
    .await;
    shutdown.cancel();
    task.await
        .expect("game deadline join")
        .expect("game deadline service");
}

async fn run_m6_shutdown_close_race(pool: &PgPool, suffix: &str, owner_closes_first: bool) {
    let metrics = Arc::new(M2Metrics::default());
    let service = stroke_service(
        pool.clone(),
        GameRuntimeLimits {
            global_connections: 4,
            connections_per_source: 4,
            auth_per_window: 20,
            packets_per_window: 200,
            room_commands_per_window: 100,
            outbound_room_event_capacity: 16,
            ..GameRuntimeLimits::default()
        },
        metrics.clone(),
    );
    let (address, shutdown, task) = start_service(service).await;
    let (mut owner, mut peer, started, _, _) =
        start_stroke_loading_pair(pool, address, suffix).await;
    let _active = enter_stroke_playing(&mut owner, &mut peer, &started).await;
    if owner_closes_first {
        drop(owner);
        shutdown.cancel();
        drop(peer);
    } else {
        drop(peer);
        shutdown.cancel();
        drop(owner);
    }
    let service_result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .expect("bounded M6 shutdown race")
        .expect("M6 shutdown race join");

    let terminal: (String, i64, i64, Option<String>) = sqlx::query_as(
        "SELECT m.status, \
         (SELECT count(*) FROM match_audit_events WHERE match_id = m.id AND event = 'aborted'), \
         (SELECT count(*) FROM match_audit_events WHERE match_id = m.id AND event = 'committed'), \
         (SELECT reason FROM match_audit_events WHERE match_id = m.id AND event = 'aborted') \
         FROM matches m WHERE m.id = $1",
    )
    .bind(started.match_id())
    .fetch_one(pool)
    .await
    .expect("shutdown terminal authority");
    assert_eq!(service_result, Ok(()), "terminal at failure: {terminal:?}");
    assert_eq!(
        terminal,
        ("aborted".to_owned(), 1, 0, Some("shutdown".to_owned()))
    );
    let rewards: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM currency_ledger WHERE match_id = $1) + \
         (SELECT count(*) FROM progression_ledger WHERE match_id = $1)",
    )
    .bind(started.match_id())
    .fetch_one(pool)
    .await
    .expect("shutdown has no ledgers");
    assert_eq!(rewards, 0);
    type AbortedPlayerRow = (Option<String>, Option<i16>, Option<i64>, Option<i64>);
    let terminal_players: Vec<AbortedPlayerRow> = sqlx::query_as(
        "SELECT completion, place, pang_reward, experience_reward FROM match_players \
         WHERE match_id = $1 ORDER BY participant_order",
    )
    .bind(started.match_id())
    .fetch_all(pool)
    .await
    .expect("shutdown player rows");
    assert_eq!(
        terminal_players,
        vec![(None, None, None, None), (None, None, None, None)]
    );
    let incomplete: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM matches WHERE id = $1 AND status IN ('begun', 'loading', 'in_game')",
    )
    .bind(started.match_id())
    .fetch_one(pool)
    .await
    .expect("startup recovery not needed");
    assert_eq!(incomplete, 0);
    assert_eq!(
        metric_sample(
            &metrics.render(),
            "pangya_game_matches_active{mode=\"stroke_two\"}",
        ),
        Some(0.0)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m6_shutdown_cancellation_beats_double_socket_close_in_either_order(pool: PgPool) {
    run_m6_shutdown_close_race(&pool, "SOwn", true).await;
    run_m6_shutdown_close_race(&pool, "SPeer", false).await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m6_shutdown_replacement_retains_the_only_cleanup_claim(pool: PgPool) {
    let metrics = Arc::new(M2Metrics::default());
    let repository = Arc::new(BlockingStrokeCommitRepository::new(pool.clone()));
    let catalog = m5_catalog();
    let course = catalog
        .one_hole_course(CourseId::new(1).expect("course ID"))
        .expect("one-hole course");
    let service = Arc::new(
        GameService::new(
            Arc::clone(&repository),
            catalog.clone(),
            GameRuntimeConfig {
                channel_id: 1,
                unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
                limits: GameRuntimeLimits {
                    global_connections: 4,
                    connections_per_source: 4,
                    auth_per_window: 20,
                    packets_per_window: 200,
                    room_commands_per_window: 100,
                    outbound_room_event_capacity: 16,
                    ..GameRuntimeLimits::default()
                },
                solo_practice: None,
                stroke_two: Some(StrokeRuntimeConfig {
                    course,
                    catalog_fingerprint: catalog.fingerprint(),
                    loading_timeout: Duration::from_secs(30),
                    turn_timeout: Duration::from_secs(30),
                    game_timeout: Duration::from_secs(120),
                    commit_timeout: Duration::from_secs(2),
                    max_strokes: 10,
                    startup_recovery_limit: IncompleteMatchAbortLimit::new(100)
                        .expect("recovery limit"),
                    shot_packets_per_window: 120,
                }),
                economy: None,
                retail_bootstrap: false,
            },
            metrics.clone(),
        )
        .expect("stroke game"),
    );
    let (address, shutdown, task) = start_service(service).await;
    let (owner, peer, started, _, _) = start_stroke_loading_pair(&pool, address, "SClaim").await;
    let (mut owner, mut peer) = (owner, peer);
    let _active = enter_stroke_playing(&mut owner, &mut peer, &started).await;

    drop(owner);
    tokio::time::timeout(Duration::from_secs(2), repository.commit_started.notified())
        .await
        .expect("first disconnect claimed settlement and entered persistence");
    shutdown.cancel();
    drop(peer);

    let service_result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .expect("bounded deterministic shutdown claim race")
        .expect("deterministic shutdown join");
    assert_eq!(service_result, Ok(()));
    assert_eq!(repository.commit_calls.load(Ordering::Relaxed), 1);
    assert_eq!(repository.abort_calls.load(Ordering::Relaxed), 1);

    let terminal: (String, i64, i64, Option<String>) = sqlx::query_as(
        "SELECT m.status, \
         (SELECT count(*) FROM match_audit_events WHERE match_id = m.id AND event = 'aborted'), \
         (SELECT count(*) FROM match_audit_events WHERE match_id = m.id AND event = 'committed'), \
         (SELECT reason FROM match_audit_events WHERE match_id = m.id AND event = 'aborted') \
         FROM matches m WHERE m.id = $1",
    )
    .bind(started.match_id())
    .fetch_one(&pool)
    .await
    .expect("single abort and acknowledgement");
    assert_eq!(
        terminal,
        ("aborted".to_owned(), 1, 0, Some("shutdown".to_owned()))
    );
    let ledgers: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM currency_ledger WHERE match_id = $1) + \
         (SELECT count(*) FROM progression_ledger WHERE match_id = $1)",
    )
    .bind(started.match_id())
    .fetch_one(&pool)
    .await
    .expect("shutdown ledgers");
    assert_eq!(ledgers, 0);
    assert_eq!(
        metric_sample(
            &metrics.render(),
            "pangya_game_matches_active{mode=\"stroke_two\"}",
        ),
        Some(0.0)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m6_disconnect_and_nonactive_giveup_persist_once_and_return_to_room(pool: PgPool) {
    let metrics = Arc::new(M2Metrics::default());
    let service = stroke_service(
        pool.clone(),
        GameRuntimeLimits {
            global_connections: 8,
            connections_per_source: 8,
            auth_per_window: 40,
            packets_per_window: 300,
            room_commands_per_window: 150,
            outbound_room_event_capacity: 16,
            ..GameRuntimeLimits::default()
        },
        metrics.clone(),
    );
    let (address, shutdown, task) = start_service(service).await;

    let (loading_owner, mut loading_peer, loading_started, _, _) =
        start_stroke_loading_pair(&pool, address, "Load").await;
    drop(loading_owner);
    let loading_room =
        receive_typed::<RoomStateResponse>(&mut loading_peer.stream, loading_peer.key).await;
    assert_eq!(loading_room.room.members().len(), 1);
    assert_eq!(
        loading_room.room.members()[0].account_id(),
        loading_peer.account_id
    );
    assert_eq!(
        receive_typed::<StrokeMatchAborted>(&mut loading_peer.stream, loading_peer.key).await,
        StrokeMatchAborted::new(
            loading_started.match_id(),
            StrokeAbortReason::LoadingDisconnect,
        )
    );
    send_typed(
        &mut loading_peer.stream,
        loading_peer.key,
        44,
        &RoomStateRequest,
    )
    .await;
    assert_eq!(
        receive_typed::<RoomStateResponse>(&mut loading_peer.stream, loading_peer.key).await,
        loading_room
    );
    let loading_audit: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT event, reason FROM match_audit_events WHERE match_id = $1 ORDER BY id",
    )
    .bind(loading_started.match_id())
    .fetch_all(&pool)
    .await
    .expect("loading disconnect audit");
    assert_eq!(
        loading_audit,
        vec![
            ("started".to_owned(), None),
            ("aborted".to_owned(), Some("disconnect".to_owned())),
        ]
    );
    let loading_rewards: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM currency_ledger WHERE match_id = $1) + \
         (SELECT count(*) FROM progression_ledger WHERE match_id = $1)",
    )
    .bind(loading_started.match_id())
    .fetch_one(&pool)
    .await
    .expect("loading disconnect no rewards");
    assert_eq!(loading_rewards, 0);

    let (playing_owner, mut playing_peer, playing_started, owner_connection, peer_connection) =
        start_stroke_loading_pair(&pool, address, "Play").await;
    for client in [&mut playing_peer] {
        send_typed(
            &mut client.stream,
            client.key,
            45,
            &StrokeLoadingComplete::new(100).expect("load"),
        )
        .await;
        assert_eq!(
            receive_typed::<StrokeCommandResult>(&mut client.stream, client.key).await,
            StrokeCommandResult::new(StrokeCommand::Load, StrokeCommandOutcome::Success)
        );
    }
    let mut playing_owner = playing_owner;
    send_typed(
        &mut playing_owner.stream,
        playing_owner.key,
        46,
        &StrokeLoadingComplete::new(100).expect("load"),
    )
    .await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut playing_owner.stream, playing_owner.key).await,
        StrokeCommandResult::new(StrokeCommand::Load, StrokeCommandOutcome::Success)
    );
    for client in [&mut playing_owner, &mut playing_peer] {
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(playing_started.match_id(), StrokePhaseKind::Playing)
        );
        assert_eq!(
            receive_typed::<StrokeTurnStarted>(&mut client.stream, client.key)
                .await
                .active_connection_id(),
            owner_connection
        );
    }
    let playing_owner_account = playing_owner.account_id;
    drop(playing_owner);
    assert_eq!(
        receive_typed::<StrokePhase>(&mut playing_peer.stream, playing_peer.key).await,
        StrokePhase::new(playing_started.match_id(), StrokePhaseKind::ResultsPending)
    );
    let playing_room =
        receive_typed::<RoomStateResponse>(&mut playing_peer.stream, playing_peer.key).await;
    assert_eq!(playing_room.room.members().len(), 1);
    let standings =
        receive_typed::<StrokeStandings>(&mut playing_peer.stream, playing_peer.key).await;
    assert_eq!(standings.match_id(), playing_started.match_id());
    let entries = standings.entries();
    assert_eq!(entries[0].connection_id(), peer_connection);
    assert_eq!(entries[0].completion(), StrokeCompletion::WinnerByForfeit);
    assert_eq!((entries[0].pang(), entries[0].experience()), (10, 5));
    assert_eq!(entries[1].connection_id(), owner_connection);
    assert_eq!(entries[1].completion(), StrokeCompletion::Disconnect);
    assert_eq!((entries[1].pang(), entries[1].experience()), (0, 0));
    assert_eq!(
        receive_typed::<StrokeBalanceUpdate>(&mut playing_peer.stream, playing_peer.key).await,
        StrokeBalanceUpdate::new(10, 5)
    );
    assert_eq!(
        receive_typed::<StrokePhase>(&mut playing_peer.stream, playing_peer.key).await,
        StrokePhase::new(playing_started.match_id(), StrokePhaseKind::Finished)
    );
    send_typed(
        &mut playing_peer.stream,
        playing_peer.key,
        47,
        &RoomStateRequest,
    )
    .await;
    assert_eq!(
        receive_typed::<RoomStateResponse>(&mut playing_peer.stream, playing_peer.key).await,
        playing_room
    );
    let persisted: Vec<(i64, String, i64, i64)> = sqlx::query_as(
        "SELECT account_id, completion, pang_reward, experience_reward FROM match_players \
         WHERE match_id = $1 ORDER BY participant_order",
    )
    .bind(playing_started.match_id())
    .fetch_all(&pool)
    .await
    .expect("disconnect settlement");
    assert_eq!(
        persisted,
        vec![
            (playing_owner_account.get(), "disconnect".to_owned(), 0, 0),
            (
                playing_peer.account_id.get(),
                "winner_by_forfeit".to_owned(),
                10,
                5,
            ),
        ],
        "participant account authority is persisted exactly once"
    );
    let loser_ledgers: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM currency_ledger WHERE match_id = $1 AND account_id = $2) + \
         (SELECT count(*) FROM progression_ledger WHERE match_id = $1 AND account_id = $2)",
    )
    .bind(playing_started.match_id())
    .bind(playing_owner_account.get())
    .fetch_one(&pool)
    .await
    .expect("disconnect loser no ledgers");
    assert_eq!(loser_ledgers, 0);

    let (mut give_owner, mut give_peer, give_started, give_owner_connection, give_peer_connection) =
        start_stroke_loading_pair(&pool, address, "Give").await;
    send_typed(
        &mut give_peer.stream,
        give_peer.key,
        48,
        &StrokeLoadingComplete::new(100).expect("load"),
    )
    .await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut give_peer.stream, give_peer.key).await,
        StrokeCommandResult::new(StrokeCommand::Load, StrokeCommandOutcome::Success)
    );
    send_typed(
        &mut give_owner.stream,
        give_owner.key,
        49,
        &StrokeLoadingComplete::new(100).expect("load"),
    )
    .await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut give_owner.stream, give_owner.key).await,
        StrokeCommandResult::new(StrokeCommand::Load, StrokeCommandOutcome::Success)
    );
    for client in [&mut give_owner, &mut give_peer] {
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(give_started.match_id(), StrokePhaseKind::Playing)
        );
        assert_eq!(
            receive_typed::<StrokeTurnStarted>(&mut client.stream, client.key)
                .await
                .active_connection_id(),
            give_owner_connection
        );
    }
    send_typed(&mut give_peer.stream, give_peer.key, 50, &StrokeGiveUp).await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut give_peer.stream, give_peer.key).await,
        StrokeCommandResult::new(StrokeCommand::GiveUp, StrokeCommandOutcome::Success)
    );
    for (client, own_balance) in [
        (&mut give_owner, StrokeBalanceUpdate::new(10, 5)),
        (&mut give_peer, StrokeBalanceUpdate::new(0, 0)),
    ] {
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(give_started.match_id(), StrokePhaseKind::ResultsPending)
        );
        let standings = receive_typed::<StrokeStandings>(&mut client.stream, client.key).await;
        assert_eq!(
            standings.entries()[0].connection_id(),
            give_owner_connection
        );
        assert_eq!(
            standings.entries()[0].completion(),
            StrokeCompletion::WinnerByForfeit
        );
        assert_eq!(standings.entries()[1].connection_id(), give_peer_connection);
        assert_eq!(
            standings.entries()[1].completion(),
            StrokeCompletion::GiveUp
        );
        assert_eq!(
            receive_typed::<StrokeBalanceUpdate>(&mut client.stream, client.key).await,
            own_balance
        );
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(give_started.match_id(), StrokePhaseKind::Finished)
        );
    }
    let give_players: Vec<(i64, i16, String, i64, i64)> = sqlx::query_as(
        "SELECT account_id, place, completion, pang_reward, experience_reward \
         FROM match_players WHERE match_id = $1 ORDER BY participant_order",
    )
    .bind(give_started.match_id())
    .fetch_all(&pool)
    .await
    .expect("give-up exact players");
    assert_eq!(
        give_players,
        vec![
            (
                give_owner.account_id.get(),
                1,
                "winner_by_forfeit".to_owned(),
                10,
                5,
            ),
            (give_peer.account_id.get(), 2, "give_up".to_owned(), 0, 0),
        ]
    );
    let give_currency: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT account_id, delta FROM currency_ledger WHERE match_id = $1 ORDER BY account_id",
    )
    .bind(give_started.match_id())
    .fetch_all(&pool)
    .await
    .expect("give-up currency ledger");
    let give_progression: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT account_id, delta FROM progression_ledger WHERE match_id = $1 ORDER BY account_id",
    )
    .bind(give_started.match_id())
    .fetch_all(&pool)
    .await
    .expect("give-up progression ledger");
    assert_eq!(give_currency, vec![(give_owner.account_id.get(), 10)]);
    assert_eq!(give_progression, vec![(give_owner.account_id.get(), 5)]);

    let rendered = metrics.render();
    assert!(rendered.contains(
        "pangya_game_commit_outcomes_total{mode=\"stroke_two\",outcome=\"committed\"} 2"
    ));
    assert!(rendered.contains(
        "pangya_game_commit_outcomes_total{mode=\"stroke_two\",outcome=\"idempotent\"} 0"
    ));
    assert!(rendered.contains("pangya_game_matches_active{mode=\"stroke_two\"} 0"));
    let _live = connect_m4(&pool, address, "M6StillLive").await;
    shutdown.cancel();
    assert!(task.await.expect("disconnect join").is_ok());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m6_encrypted_tcp_two_player_turns_and_atomic_settlement(pool: PgPool) {
    let metrics = Arc::new(M2Metrics::default());
    let service = stroke_service(
        pool.clone(),
        GameRuntimeLimits {
            global_connections: 4,
            connections_per_source: 4,
            auth_per_window: 20,
            packets_per_window: 200,
            room_commands_per_window: 100,
            outbound_room_event_capacity: 16,
            ..GameRuntimeLimits::default()
        },
        metrics.clone(),
    );
    let (address, shutdown, task) = start_service(service).await;
    let mut owner = connect_m4(&pool, address, "M6Owner").await;
    let mut peer = connect_m4(&pool, address, "M6Peer").await;
    sqlx::query("UPDATE profiles SET pang = 100, experience = 50 WHERE account_id = $1")
        .bind(peer.account_id.get())
        .execute(&pool)
        .await
        .expect("distinct peer starting projection");

    send_typed(
        &mut owner.stream,
        owner.key,
        3,
        &RoomCreateRequest {
            name: RoomName::parse("M6 Stroke").expect("room"),
            password: None,
            settings: RoomSettings::new(2).expect("settings"),
        },
    )
    .await;
    receive_result(
        &mut owner.stream,
        owner.key,
        RoomCommand::Create,
        RoomCommandResult::Success,
    )
    .await;
    let owner_state = receive_typed::<RoomStateResponse>(&mut owner.stream, owner.key).await;
    let room_id = owner_state.room.summary().id();
    let owner_connection = owner_state.room.members()[0].connection_id().get();

    send_typed(
        &mut peer.stream,
        peer.key,
        4,
        &RoomJoinRequest {
            room_id,
            password: None,
        },
    )
    .await;
    receive_result(
        &mut peer.stream,
        peer.key,
        RoomCommand::Join,
        RoomCommandResult::Success,
    )
    .await;
    let joined = receive_typed::<RoomStateResponse>(&mut peer.stream, peer.key).await;
    let peer_connection = joined.room.members()[1].connection_id().get();
    let _: RoomStateResponse = receive_typed(&mut owner.stream, owner.key).await;
    let _: RoomStateResponse = receive_typed(&mut peer.stream, peer.key).await;

    send_typed(
        &mut owner.stream,
        owner.key,
        5,
        &RoomReadyRequest { ready: true },
    )
    .await;
    receive_result(
        &mut owner.stream,
        owner.key,
        RoomCommand::Ready,
        RoomCommandResult::Success,
    )
    .await;
    let _: RoomStateResponse = receive_typed(&mut owner.stream, owner.key).await;
    let _: RoomStateResponse = receive_typed(&mut owner.stream, owner.key).await;
    let _: RoomStateResponse = receive_typed(&mut peer.stream, peer.key).await;
    send_typed(
        &mut peer.stream,
        peer.key,
        6,
        &RoomReadyRequest { ready: true },
    )
    .await;
    receive_result(
        &mut peer.stream,
        peer.key,
        RoomCommand::Ready,
        RoomCommandResult::Success,
    )
    .await;
    let _: RoomStateResponse = receive_typed(&mut peer.stream, peer.key).await;
    let _: RoomStateResponse = receive_typed(&mut owner.stream, owner.key).await;
    let _: RoomStateResponse = receive_typed(&mut peer.stream, peer.key).await;

    send_typed(&mut owner.stream, owner.key, 7, &StartStrokeTwo).await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut owner.stream, owner.key).await,
        StrokeCommandResult::new(StrokeCommand::Start, StrokeCommandOutcome::Success)
    );
    let owner_started = receive_typed::<StrokeMatchStarted>(&mut owner.stream, owner.key).await;
    let peer_started = receive_typed::<StrokeMatchStarted>(&mut peer.stream, peer.key).await;
    assert_eq!(owner_started, peer_started);
    assert_eq!(owner_started.course_id(), 1);
    assert_eq!(owner_started.hole(), 1);
    assert_eq!(owner_started.par(), 3);
    assert_eq!(owner_started.load_timeout_ms(), 30_000);
    assert_eq!(owner_started.turn_timeout_ms(), 30_000);
    assert_eq!(owner_started.game_timeout_ms(), 120_000);
    assert_eq!(
        owner_started.participant_connection_ids(),
        [owner_connection, peer_connection]
    );
    assert!(owner_started.seed().iter().any(|byte| *byte != 0));
    let (expected_weather, expected_wind) =
        deterministic_conditions(MatchSeed::new(*owner_started.seed())).expect("conditions");
    assert_eq!(
        owner_started.weather(),
        match expected_weather {
            Weather::Clear => ProtocolWeather::Clear,
            Weather::Cloudy => ProtocolWeather::Cloudy,
            Weather::Rain => ProtocolWeather::Rain,
        }
    );
    assert_eq!(
        owner_started.wind().speed(),
        f32::from(expected_wind.speed_tenths()) / 10.0
    );
    assert_eq!(
        owner_started.wind().angle(),
        f32::from(expected_wind.angle_degrees())
    );
    for client in [&mut owner, &mut peer] {
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(owner_started.match_id(), StrokePhaseKind::Loading)
        );
    }

    send_typed(
        &mut peer.stream,
        peer.key,
        8,
        &StrokeLoadingComplete::new(100).expect("load"),
    )
    .await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut peer.stream, peer.key).await,
        StrokeCommandResult::new(StrokeCommand::Load, StrokeCommandOutcome::Success)
    );
    send_typed(
        &mut owner.stream,
        owner.key,
        9,
        &StrokeLoadingComplete::new(100).expect("load"),
    )
    .await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut owner.stream, owner.key).await,
        StrokeCommandResult::new(StrokeCommand::Load, StrokeCommandOutcome::Success)
    );
    for client in [&mut owner, &mut peer] {
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(owner_started.match_id(), StrokePhaseKind::Playing)
        );
        let turn = receive_typed::<StrokeTurnStarted>(&mut client.stream, client.key).await;
        assert_eq!(
            turn,
            StrokeTurnStarted::new(owner_started.match_id(), 1, owner_connection, 1, 30_000)
                .expect("first turn")
        );
    }

    type PersistenceSnapshot = (String, Vec<(i16, Option<i16>, Option<i16>, Option<String>)>);
    let in_game_snapshot: PersistenceSnapshot = (
        sqlx::query_scalar("SELECT status FROM matches WHERE id = $1")
            .bind(owner_started.match_id())
            .fetch_one(&pool)
            .await
            .expect("in-game status"),
        sqlx::query_as(
            "SELECT participant_order, strokes, place, completion FROM match_players \
             WHERE match_id = $1 ORDER BY participant_order",
        )
        .bind(owner_started.match_id())
        .fetch_all(&pool)
        .await
        .expect("in-game players"),
    );
    assert_eq!(in_game_snapshot.0, "in_game");
    assert_eq!(
        in_game_snapshot.1,
        vec![(0, None, None, None), (1, None, None, None)]
    );

    let peer_action = StrokeShotAction::new(1, 1, 50.0, 0.0, 0.0, 0.0).expect("action");
    send_typed(&mut peer.stream, peer.key, 10, &peer_action).await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut peer.stream, peer.key).await,
        StrokeCommandResult::new(StrokeCommand::Action, StrokeCommandOutcome::InvalidTurn)
    );

    let owner_action = StrokeShotAction::new(1, 1, 60.0, 0.0, 0.0, 0.0).expect("action");
    send_typed(&mut owner.stream, owner.key, 11, &owner_action).await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut owner.stream, owner.key).await,
        StrokeCommandResult::new(StrokeCommand::Action, StrokeCommandOutcome::Success)
    );
    for client in [&mut owner, &mut peer] {
        let relay = receive_typed::<StrokeActionRelay>(&mut client.stream, client.key).await;
        assert_eq!(relay.connection_id(), owner_connection);
        assert_eq!(relay.action(), owner_action);
    }
    send_typed(&mut owner.stream, owner.key, 12, &owner_action).await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut owner.stream, owner.key).await,
        StrokeCommandResult::new(StrokeCommand::Action, StrokeCommandOutcome::Success)
    );
    let owner_result = StrokeShotResult::new(1, 1.0, 0.0, 0.0, Lie::Green, true).expect("result");
    send_typed(&mut owner.stream, owner.key, 13, &owner_result).await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut owner.stream, owner.key).await,
        StrokeCommandResult::new(StrokeCommand::Result, StrokeCommandOutcome::Success)
    );
    for client in [&mut owner, &mut peer] {
        let relay = receive_typed::<StrokeResultRelay>(&mut client.stream, client.key).await;
        assert_eq!(relay.connection_id(), owner_connection);
        assert_eq!(relay.result(), owner_result);
        let turn = receive_typed::<StrokeTurnStarted>(&mut client.stream, client.key).await;
        assert_eq!(
            turn,
            StrokeTurnStarted::new(owner_started.match_id(), 2, peer_connection, 1, 30_000)
                .expect("second turn")
        );
    }
    let after_shot_snapshot: PersistenceSnapshot = (
        sqlx::query_scalar("SELECT status FROM matches WHERE id = $1")
            .bind(owner_started.match_id())
            .fetch_one(&pool)
            .await
            .expect("post-shot status"),
        sqlx::query_as(
            "SELECT participant_order, strokes, place, completion FROM match_players \
             WHERE match_id = $1 ORDER BY participant_order",
        )
        .bind(owner_started.match_id())
        .fetch_all(&pool)
        .await
        .expect("post-shot players"),
    );
    assert_eq!(after_shot_snapshot, in_game_snapshot);

    send_typed(&mut peer.stream, peer.key, 14, &peer_action).await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut peer.stream, peer.key).await,
        StrokeCommandResult::new(StrokeCommand::Action, StrokeCommandOutcome::Success)
    );
    for client in [&mut owner, &mut peer] {
        let relay = receive_typed::<StrokeActionRelay>(&mut client.stream, client.key).await;
        assert_eq!(relay.connection_id(), peer_connection);
        assert_eq!(relay.action(), peer_action);
    }
    let peer_result = StrokeShotResult::new(1, 2.0, 0.0, 0.0, Lie::Green, true).expect("result");
    send_typed(&mut peer.stream, peer.key, 15, &peer_result).await;
    assert_eq!(
        receive_typed::<StrokeCommandResult>(&mut peer.stream, peer.key).await,
        StrokeCommandResult::new(StrokeCommand::Result, StrokeCommandOutcome::Success)
    );
    let mut standings = Vec::new();
    let mut balances = Vec::new();
    for client in [&mut owner, &mut peer] {
        let relay = receive_typed::<StrokeResultRelay>(&mut client.stream, client.key).await;
        assert_eq!(relay.connection_id(), peer_connection);
        assert_eq!(relay.result(), peer_result);
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(owner_started.match_id(), StrokePhaseKind::ResultsPending)
        );
        standings.push(receive_typed::<StrokeStandings>(&mut client.stream, client.key).await);
        balances.push(receive_typed::<StrokeBalanceUpdate>(&mut client.stream, client.key).await);
        assert_eq!(
            receive_typed::<StrokePhase>(&mut client.stream, client.key).await,
            StrokePhase::new(owner_started.match_id(), StrokePhaseKind::Finished)
        );
    }
    assert_eq!(standings[0], standings[1]);
    assert_eq!(standings[0].match_id(), owner_started.match_id());
    let entries = standings[0].entries();
    assert_eq!(entries[0].connection_id(), owner_connection);
    assert_eq!(entries[0].place(), 1);
    assert_eq!(entries[0].completion(), StrokeCompletion::Holed);
    assert_eq!(entries[0].strokes(), 1);
    assert_eq!(entries[0].score(), Some(-2));
    assert_eq!((entries[0].pang(), entries[0].experience()), (14, 5));
    assert_eq!(entries[1].connection_id(), peer_connection);
    assert_eq!(entries[1].place(), 2);
    assert_eq!(entries[1].completion(), StrokeCompletion::Holed);
    assert_eq!(entries[1].strokes(), 1);
    assert_eq!(entries[1].score(), Some(-2));
    assert_eq!((entries[1].pang(), entries[1].experience()), (14, 5));
    assert_ne!(entries[0].player_result_id(), entries[1].player_result_id());
    assert_ne!(entries[0].player_result_id(), owner_started.match_id());
    assert_ne!(entries[1].player_result_id(), owner_started.match_id());
    assert_eq!(balances[0], StrokeBalanceUpdate::new(14, 5));
    assert_eq!(balances[1], StrokeBalanceUpdate::new(114, 55));
    assert_ne!(balances[0], balances[1]);
    assert_eq!(
        maybe_receive_opcode(&mut owner.stream, owner.key).await,
        None
    );
    assert_eq!(maybe_receive_opcode(&mut peer.stream, peer.key).await, None);

    let (matches, players, ledgers, records): (i64, i64, i64, i64) = tokio::try_join!(
        sqlx::query_scalar("SELECT count(*) FROM matches WHERE mode = 'stroke_two' AND status = 'committed'").fetch_one(&pool),
        sqlx::query_scalar("SELECT count(*) FROM match_players mp JOIN matches m ON m.id = mp.match_id WHERE m.mode = 'stroke_two' AND m.status = 'committed'").fetch_one(&pool),
        sqlx::query_scalar("SELECT (SELECT count(*) FROM currency_ledger WHERE reason = 'stroke-two-v1') + (SELECT count(*) FROM progression_ledger WHERE reason = 'stroke-two-v1')").fetch_one(&pool),
        sqlx::query_scalar("SELECT count(*) FROM course_records WHERE mode = 'stroke_two'").fetch_one(&pool),
    )
    .expect("M6 persisted counts");
    assert_eq!((matches, players, ledgers, records), (1, 2, 4, 2));
    type PlayerRow = (
        i16,
        i64,
        uuid::Uuid,
        i16,
        String,
        i16,
        i16,
        bool,
        i64,
        i64,
        i64,
        i64,
    );
    let persisted_players: Vec<PlayerRow> = sqlx::query_as(
        "SELECT participant_order, account_id, player_result_key, place, completion, strokes, \
         score, quit, pang_reward, experience_reward, pang_balance_after, \
         experience_balance_after FROM match_players WHERE match_id = $1 \
         ORDER BY participant_order",
    )
    .bind(owner_started.match_id())
    .fetch_all(&pool)
    .await
    .expect("exact match players");
    assert_eq!(
        persisted_players,
        vec![
            (
                0,
                owner.account_id.get(),
                entries[0].player_result_id(),
                1,
                "holed".to_owned(),
                1,
                -2,
                false,
                14,
                5,
                14,
                5,
            ),
            (
                1,
                peer.account_id.get(),
                entries[1].player_result_id(),
                2,
                "holed".to_owned(),
                1,
                -2,
                false,
                14,
                5,
                114,
                55,
            ),
        ]
    );
    let currency: Vec<(i64, uuid::Uuid, i64, i64)> = sqlx::query_as(
        "SELECT account_id, idempotency_key, delta, balance_after FROM currency_ledger \
         WHERE match_id = $1 ORDER BY account_id",
    )
    .bind(owner_started.match_id())
    .fetch_all(&pool)
    .await
    .expect("currency authority");
    assert_eq!(
        currency,
        vec![
            (
                owner.account_id.get(),
                entries[0].player_result_id(),
                14,
                14
            ),
            (
                peer.account_id.get(),
                entries[1].player_result_id(),
                14,
                114
            ),
        ]
    );
    let progression: Vec<(i64, uuid::Uuid, i64, i64)> = sqlx::query_as(
        "SELECT account_id, idempotency_key, delta, balance_after FROM progression_ledger \
         WHERE match_id = $1 ORDER BY account_id",
    )
    .bind(owner_started.match_id())
    .fetch_all(&pool)
    .await
    .expect("progression authority");
    assert_eq!(
        progression,
        vec![
            (owner.account_id.get(), entries[0].player_result_id(), 5, 5),
            (peer.account_id.get(), entries[1].player_result_id(), 5, 55),
        ]
    );
    let audit: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT event, outcome, reason FROM match_audit_events WHERE match_id = $1 \
         ORDER BY id",
    )
    .bind(owner_started.match_id())
    .fetch_all(&pool)
    .await
    .expect("audit rows");
    assert_eq!(
        audit,
        vec![
            ("started".to_owned(), "success".to_owned(), None),
            ("committed".to_owned(), "success".to_owned(), None),
        ]
    );
    let records: Vec<(i64, i16, i16, i64, uuid::Uuid, uuid::Uuid)> = sqlx::query_as(
        "SELECT account_id, best_score, best_strokes, rounds_completed, best_match_id, \
         best_player_result_key FROM course_records WHERE mode = 'stroke_two' \
         ORDER BY account_id",
    )
    .fetch_all(&pool)
    .await
    .expect("record authority");
    assert_eq!(
        records,
        vec![
            (
                owner.account_id.get(),
                -2,
                1,
                1,
                owner_started.match_id(),
                entries[0].player_result_id(),
            ),
            (
                peer.account_id.get(),
                -2,
                1,
                1,
                owner_started.match_id(),
                entries[1].player_result_id(),
            ),
        ]
    );
    let rendered = metrics.render();
    assert!(rendered.contains("pangya_game_matches_active{mode=\"stroke_two\"} 0"));
    assert!(rendered.contains("mode=\"stroke_two\",outcome=\"out_of_turn\"} 1"));
    assert!(rendered.contains("mode=\"stroke_two\",outcome=\"duplicate\"} 1"));
    for forbidden in ["match_id=", "result_key=", "seed=", "balance=", "x="] {
        assert!(!rendered.contains(forbidden));
    }

    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());

    let persisted_owner: (i64, i64) =
        sqlx::query_as("SELECT pang, experience FROM profiles WHERE account_id = $1")
            .bind(owner.account_id.get())
            .fetch_one(&pool)
            .await
            .expect("owner balances");
    assert_eq!(
        (
            u64::try_from(persisted_owner.0).expect("pang"),
            u64::try_from(persisted_owner.1).expect("experience")
        ),
        (balances[0].pang(), balances[0].experience())
    );

    let restart = stroke_service(
        pool.clone(),
        GameRuntimeLimits::default(),
        Arc::new(M2Metrics::default()),
    );
    let (restart_address, restart_shutdown, restart_task) = start_service(restart).await;
    let token = issue_token(
        &pool,
        owner.account_id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut restarted, restart_key) = connect_game(restart_address).await;
    send_packet(
        &mut restarted,
        restart_key,
        16,
        2,
        &auth_payload(owner.account_id.get(), &token),
    )
    .await;
    let projection = read_player_info(&mut restarted, restart_key).await;
    assert_eq!(
        (projection.2, projection.4),
        (balances[0].pang(), balances[0].experience())
    );
    let peer_token =
        issue_token(&pool, peer.account_id, SystemTime::now(), ServiceKind::Game).await;
    let (mut restarted_peer, restarted_peer_key) = connect_game(restart_address).await;
    send_packet(
        &mut restarted_peer,
        restarted_peer_key,
        17,
        2,
        &auth_payload(peer.account_id.get(), &peer_token),
    )
    .await;
    let peer_projection = read_player_info(&mut restarted_peer, restarted_peer_key).await;
    assert_eq!(
        (peer_projection.2, peer_projection.4),
        (balances[1].pang(), balances[1].experience())
    );
    restart_shutdown.cancel();
    assert!(restart_task.await.expect("restart join").is_ok());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_connection_task_bound_and_shutdown_grace_are_enforced(pool: PgPool) {
    let limits = GameRuntimeLimits {
        global_connections: 2,
        connections_per_source: 2,
        global_accepts_per_window: 10,
        accepts_per_window: 10,
        command_timeout: Duration::from_millis(50),
        shutdown_grace: Duration::from_millis(100),
        ..GameRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start_game(pool, limits).await;
    let (_first, _) = connect_game(address).await;
    let (_second, _) = connect_game(address).await;
    let mut excess = TcpStream::connect(address).await.expect("excess connect");
    assert_closed(&mut excess).await;
    assert_metric(&metrics, "class=\"connection_global\"} 1").await;
    assert_eq!(
        metric_sample(
            &metrics.render(),
            "pangya_connections_active{service=\"game\"}",
        ),
        Some(2.0)
    );
    shutdown.cancel();
    tokio::time::timeout(Duration::from_millis(200), task)
        .await
        .expect("shutdown grace bound")
        .expect("join")
        .expect("serve");
    assert_eq!(
        metric_sample(
            &metrics.render(),
            "pangya_connections_active{service=\"game\"}",
        ),
        Some(0.0)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m7_encrypted_economy_is_catalog_priced_idempotent_and_restart_safe(pool: PgPool) {
    let account = create_account(&pool, "EconomyFlow", 1, 0x1000_0000).await;
    sqlx::query("UPDATE profiles SET pang = 5000 WHERE account_id = $1")
        .bind(account.account.id.get())
        .execute(&pool)
        .await
        .expect("fund profile");
    let metrics = Arc::new(M2Metrics::default());
    let (address, shutdown, task) = start_service(economy_service(pool.clone(), metrics)).await;
    let token = issue_token(
        &pool,
        account.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut stream, key) = connect_game(address).await;
    send_packet(
        &mut stream,
        key,
        1,
        2,
        &auth_payload(account.account.id.get(), &token),
    )
    .await;
    read_bootstrap(&mut stream, key, 1).await;
    send_packet(&mut stream, key, 2, 4, &1_u32.to_le_bytes()).await;
    assert_eq!(receive_packet(&mut stream, key).await.0, 0x004e);

    send_typed(&mut stream, key, 3, &ShopPageRequest::new(0)).await;
    let page = receive_typed::<ShopPage>(&mut stream, key).await;
    assert!(
        page.entries()
            .iter()
            .any(|offer| offer.type_id() == 0x1a00_0001 && offer.pang_price() == 25)
    );
    assert!(
        page.entries()
            .iter()
            .any(|offer| offer.type_id() == 0x1000_0001 && offer.pang_price() == 500)
    );

    let purchase_op = uuid::Uuid::new_v4();
    let purchase = PurchaseRequestPacket::new(purchase_op, 0x1a00_0001, 2).expect("purchase");
    send_typed(&mut stream, key, 4, &purchase).await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key).await,
        EconomyCommandResult::new(EconomyCommand::Purchase, EconomyOutcome::Success)
    );
    let bought = receive_typed::<PurchaseCommitted>(&mut stream, key).await;
    assert_eq!((bought.quantity_after(), bought.pang_balance()), (2, 4950));
    send_typed(&mut stream, key, 5, &purchase).await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::Success
    );
    assert_eq!(
        receive_typed::<PurchaseCommitted>(&mut stream, key).await,
        bought
    );

    for (salt, expected) in [(6, 1), (7, 0)] {
        let consume =
            ConsumeOneRequest::new(uuid::Uuid::new_v4(), bought.inventory_id()).expect("consume");
        send_typed(&mut stream, key, salt, &consume).await;
        assert_eq!(
            receive_typed::<EconomyCommandResult>(&mut stream, key)
                .await
                .outcome(),
            EconomyOutcome::Success
        );
        assert_eq!(
            receive_typed::<InventoryChanged>(&mut stream, key)
                .await
                .quantity_after(),
            expected
        );
    }

    let club_op = uuid::Uuid::new_v4();
    send_typed(
        &mut stream,
        key,
        8,
        &PurchaseRequestPacket::new(club_op, 0x1000_0001, 1).expect("club"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::Success
    );
    let club = receive_typed::<PurchaseCommitted>(&mut stream, key).await;
    assert_eq!((club.durability(), club.pang_balance()), (Some(100), 4450));
    sqlx::query("UPDATE inventory_items SET durability = 90 WHERE id = $1")
        .bind(i64::try_from(club.inventory_id()).expect("id"))
        .execute(&pool)
        .await
        .expect("synthetic wear");
    send_typed(
        &mut stream,
        key,
        9,
        &RepairRequest::new(uuid::Uuid::new_v4(), club.inventory_id()).expect("repair"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::Success
    );
    let repaired = receive_typed::<RepairCommitted>(&mut stream, key).await;
    assert_eq!(
        (repaired.durability(), repaired.pang_balance()),
        (100, 4420)
    );

    let character_id: i64 = sqlx::query_scalar("SELECT id FROM characters WHERE account_id=$1")
        .bind(account.account.id.get())
        .fetch_one(&pool)
        .await
        .expect("character");
    send_typed(
        &mut stream,
        key,
        10,
        &EquipRequest::new(
            uuid::Uuid::new_v4(),
            0,
            u64::try_from(character_id).expect("character"),
            Some(club.inventory_id()),
            None,
        )
        .expect("equip"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::Success
    );
    let equipped = receive_typed::<EquipmentChanged>(&mut stream, key).await;
    assert_eq!(
        (equipped.club_id(), equipped.version()),
        (Some(club.inventory_id()), 1)
    );

    let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM economy_operations),(SELECT count(*) FROM shop_currency_ledger),(SELECT count(*) FROM item_ledger),(SELECT count(*) FROM equipment_ledger)").fetch_one(&pool).await.expect("counts");
    assert_eq!(counts, (6, 3, 5, 1));
    let pang: i64 = sqlx::query_scalar("SELECT pang FROM profiles WHERE account_id=$1")
        .bind(account.account.id.get())
        .fetch_one(&pool)
        .await
        .expect("pang");
    assert_eq!(pang, 4420);
    drop(stream);
    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());

    let (address, shutdown, task) = start_service(economy_service(
        pool.clone(),
        Arc::new(M2Metrics::default()),
    ))
    .await;
    let token = issue_token(
        &pool,
        account.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut restarted, key) = connect_game(address).await;
    send_packet(
        &mut restarted,
        key,
        1,
        2,
        &auth_payload(account.account.id.get(), &token),
    )
    .await;
    let (_, _, projected, _, _) = read_player_info(&mut restarted, key).await;
    assert_eq!(projected, 4420);
    assert_eq!(receive_packet(&mut restarted, key).await.0, 0x0072);
    for _ in 0..1 {
        assert_eq!(receive_packet(&mut restarted, key).await.0, 0x0073);
    }
    assert_eq!(receive_packet(&mut restarted, key).await.0, 0x004d);
    drop(restarted);
    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m7_disabled_economy_refuses_each_command_without_closing(pool: PgPool) {
    let account = create_account(&pool, "EconomyOff", 1, 0x1000_0000).await;
    let (address, shutdown, task) = start_service(economy_service_with(
        pool.clone(),
        Arc::new(M2Metrics::default()),
        None,
    ))
    .await;
    let (mut stream, key) = connect_economy_client(&pool, address, account.account.id).await;

    // Every command decodes and is refused explicitly; none of them closes the connection.
    send_typed(&mut stream, key, 3, &ShopPageRequest::new(0)).await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key).await,
        EconomyCommandResult::new(EconomyCommand::ShopPage, EconomyOutcome::Disabled)
    );
    send_typed(
        &mut stream,
        key,
        4,
        &PurchaseRequestPacket::new(uuid::Uuid::new_v4(), 0x1a00_0001, 1).expect("purchase"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key).await,
        EconomyCommandResult::new(EconomyCommand::Purchase, EconomyOutcome::Disabled)
    );
    send_typed(
        &mut stream,
        key,
        5,
        &EquipRequest::new(uuid::Uuid::new_v4(), 0, 1, None, None).expect("equip"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key).await,
        EconomyCommandResult::new(EconomyCommand::Equip, EconomyOutcome::Disabled)
    );
    send_typed(
        &mut stream,
        key,
        6,
        &ConsumeOneRequest::new(uuid::Uuid::new_v4(), 1).expect("consume"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key).await,
        EconomyCommandResult::new(EconomyCommand::Consume, EconomyOutcome::Disabled)
    );
    send_typed(
        &mut stream,
        key,
        7,
        &RepairRequest::new(uuid::Uuid::new_v4(), 1).expect("repair"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key).await,
        EconomyCommandResult::new(EconomyCommand::Repair, EconomyOutcome::Disabled)
    );

    // Nothing was persisted by a disabled economy.
    let operations: i64 = sqlx::query_scalar("SELECT count(*) FROM economy_operations")
        .fetch_one(&pool)
        .await
        .expect("operations");
    assert_eq!(operations, 0);
    drop(stream);
    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m7_economy_reports_each_rejection_outcome_without_persisting(pool: PgPool) {
    let account = create_account(&pool, "EconomyReject", 1, 0x1000_0000).await;
    sqlx::query("UPDATE profiles SET pang = 5000 WHERE account_id = $1")
        .bind(account.account.id.get())
        .execute(&pool)
        .await
        .expect("fund profile");
    let metrics = Arc::new(M2Metrics::default());
    // Configure the purchase cap below the protocol's own hard cap of 99 so the runtime
    // policy check is reachable; `PurchaseRequestPacket::new` already refuses to build
    // anything above 99, so the wire type can never carry an over-protocol quantity.
    let (address, shutdown, task) = start_service(economy_service_with(
        pool.clone(),
        metrics.clone(),
        Some(EconomyRuntimeConfig {
            max_purchase_quantity: 50,
            ..default_economy()
        }),
    ))
    .await;
    let (mut stream, key) = connect_economy_client(&pool, address, account.account.id).await;
    let mut salt = 3_u8;

    // The v2 catalog sells exactly four offers, so page 1 is past the only page.
    send_typed(&mut stream, key, salt, &ShopPageRequest::new(1)).await;
    salt += 1;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key).await,
        EconomyCommandResult::new(EconomyCommand::ShopPage, EconomyOutcome::Invalid)
    );

    // Quantity above the configured cap is refused before any repository work.
    send_typed(
        &mut stream,
        key,
        salt,
        &PurchaseRequestPacket::new(uuid::Uuid::new_v4(), 0x1a00_0001, 51).expect("purchase"),
    )
    .await;
    salt += 1;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::Invalid
    );

    // 0x1a00_0002 exists in the catalog but is not sold, so it is not a shop offer.
    send_typed(
        &mut stream,
        key,
        salt,
        &PurchaseRequestPacket::new(uuid::Uuid::new_v4(), 0x1a00_0002, 1).expect("purchase"),
    )
    .await;
    salt += 1;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::Invalid
    );

    // An inventory row the account does not hold.
    send_typed(
        &mut stream,
        key,
        salt,
        &ConsumeOneRequest::new(uuid::Uuid::new_v4(), 999_999).expect("consume"),
    )
    .await;
    salt += 1;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::NotOwned
    );

    // Buy a consumable so a real inventory row exists for the incompatible-slot check.
    send_typed(
        &mut stream,
        key,
        salt,
        &PurchaseRequestPacket::new(uuid::Uuid::new_v4(), 0x1a00_0001, 1).expect("purchase"),
    )
    .await;
    salt += 1;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::Success
    );
    let consumable = receive_typed::<PurchaseCommitted>(&mut stream, key).await;

    let character_id: i64 = sqlx::query_scalar("SELECT id FROM characters WHERE account_id=$1")
        .bind(account.account.id.get())
        .fetch_one(&pool)
        .await
        .expect("character");
    let character = u64::try_from(character_id).expect("character id");

    // A consumable is not a club set, so the club slot rejects it on kind.
    send_typed(
        &mut stream,
        key,
        salt,
        &EquipRequest::new(
            uuid::Uuid::new_v4(),
            0,
            character,
            Some(consumable.inventory_id()),
            None,
        )
        .expect("equip"),
    )
    .await;
    salt += 1;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::Incompatible
    );

    // A stale optimistic version cannot commit an equipment change.
    send_typed(
        &mut stream,
        key,
        salt,
        &EquipRequest::new(uuid::Uuid::new_v4(), 7, character, None, None).expect("equip"),
    )
    .await;
    salt += 1;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::VersionConflict
    );

    // Replaying one operation id with different parameters is drift, not a replay.
    let reused = uuid::Uuid::new_v4();
    send_typed(
        &mut stream,
        key,
        salt,
        &PurchaseRequestPacket::new(reused, 0x1a00_0001, 2).expect("purchase"),
    )
    .await;
    salt += 1;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::Success
    );
    let _ = receive_typed::<PurchaseCommitted>(&mut stream, key).await;
    send_typed(
        &mut stream,
        key,
        salt,
        &PurchaseRequestPacket::new(reused, 0x1a00_0001, 3).expect("purchase"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::IdempotencyDrift
    );

    // Only the two successful purchases moved money; every rejection was inert.
    let pang: i64 = sqlx::query_scalar("SELECT pang FROM profiles WHERE account_id=$1")
        .bind(account.account.id.get())
        .fetch_one(&pool)
        .await
        .expect("pang");
    assert_eq!(pang, 5000 - 25 - 50);

    let rendered = metrics.render();
    for expected in [
        "pangya_game_economy_outcomes_total{outcome=\"invalid\"} 3",
        "pangya_game_economy_outcomes_total{outcome=\"not_owned\"} 1",
        "pangya_game_economy_outcomes_total{outcome=\"incompatible\"} 1",
        "pangya_game_economy_outcomes_total{outcome=\"version_conflict\"} 1",
        "pangya_game_economy_outcomes_total{outcome=\"idempotency_drift\"} 1",
    ] {
        assert!(rendered.contains(expected), "missing {expected}");
    }
    drop(stream);
    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m7_economy_reports_insufficient_pang_and_stack_limits(pool: PgPool) {
    let account = create_account(&pool, "EconomyLimits", 1, 0x1000_0000).await;
    sqlx::query("UPDATE profiles SET pang = 2500 WHERE account_id = $1")
        .bind(account.account.id.get())
        .execute(&pool)
        .await
        .expect("fund profile");
    let (address, shutdown, task) = start_service(economy_service(
        pool.clone(),
        Arc::new(M2Metrics::default()),
    ))
    .await;
    let (mut stream, key) = connect_economy_client(&pool, address, account.account.id).await;

    // 2500 Pang cannot cover a 500-Pang club after 99 consumables cost 2475.
    send_typed(
        &mut stream,
        key,
        3,
        &PurchaseRequestPacket::new(uuid::Uuid::new_v4(), 0x1a00_0001, 99).expect("purchase"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::Success
    );
    let stacked = receive_typed::<PurchaseCommitted>(&mut stream, key).await;
    assert_eq!((stacked.quantity_after(), stacked.pang_balance()), (99, 25));

    // The stack is at its catalog maximum of 99.
    send_typed(
        &mut stream,
        key,
        4,
        &PurchaseRequestPacket::new(uuid::Uuid::new_v4(), 0x1a00_0001, 1).expect("purchase"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::StackFull
    );

    // 25 Pang cannot cover the 500-Pang club set.
    send_typed(
        &mut stream,
        key,
        5,
        &PurchaseRequestPacket::new(uuid::Uuid::new_v4(), 0x1000_0001, 1).expect("purchase"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key)
            .await
            .outcome(),
        EconomyOutcome::InsufficientPang
    );

    let pang: i64 = sqlx::query_scalar("SELECT pang FROM profiles WHERE account_id=$1")
        .bind(account.account.id.get())
        .fetch_one(&pool)
        .await
        .expect("pang");
    assert_eq!(pang, 25);
    drop(stream);
    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m7_economy_commands_are_bounded_per_connection(pool: PgPool) {
    let account = create_account(&pool, "EconomyRate", 1, 0x1000_0000).await;
    let (address, shutdown, task) = start_service(economy_service_with(
        pool.clone(),
        Arc::new(M2Metrics::default()),
        Some(EconomyRuntimeConfig {
            commands_per_window: 2,
            ..default_economy()
        }),
    ))
    .await;
    let (mut stream, key) = connect_economy_client(&pool, address, account.account.id).await;

    for salt in 3..5 {
        send_typed(&mut stream, key, salt, &ShopPageRequest::new(0)).await;
        let _ = receive_typed::<ShopPage>(&mut stream, key).await;
    }
    // The third command exhausts the per-connection budget and closes the connection.
    send_typed(&mut stream, key, 5, &ShopPageRequest::new(0)).await;
    assert_closed(&mut stream).await;

    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m7_economy_opcodes_require_authentication_and_channel_entry(pool: PgPool) {
    let account = create_account(&pool, "EconomyGate", 1, 0x1000_0000).await;
    let (address, shutdown, task) = start_service(economy_service(
        pool.clone(),
        Arc::new(M2Metrics::default()),
    ))
    .await;

    // Before authentication the connection has no identity to charge.
    let (mut unauthenticated, key) = connect_game(address).await;
    send_typed(&mut unauthenticated, key, 1, &ShopPageRequest::new(0)).await;
    assert_closed(&mut unauthenticated).await;

    // Authenticated but still outside a channel is equally refused.
    let token = issue_token(
        &pool,
        account.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut lobbyless, key) = connect_game(address).await;
    send_packet(
        &mut lobbyless,
        key,
        1,
        2,
        &auth_payload(account.account.id.get(), &token),
    )
    .await;
    read_bootstrap(&mut lobbyless, key, 1).await;
    send_typed(&mut lobbyless, key, 2, &ShopPageRequest::new(0)).await;
    assert_closed(&mut lobbyless).await;

    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m7_economy_command_deadline_reports_timeout_without_persisting(pool: PgPool) {
    let account = create_account(&pool, "EconomySlow", 1, 0x1000_0000).await;
    sqlx::query("UPDATE profiles SET pang = 5000 WHERE account_id = $1")
        .bind(account.account.id.get())
        .execute(&pool)
        .await
        .expect("fund profile");
    let repository = Arc::new(BlockingStrokeCommitRepository::stalling_economy(
        pool.clone(),
    ));
    let service = Arc::new(
        GameService::new(
            repository,
            economy_catalog(),
            GameRuntimeConfig {
                channel_id: 1,
                unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
                limits: GameRuntimeLimits {
                    packets_per_window: 200,
                    ..GameRuntimeLimits::default()
                },
                solo_practice: None,
                stroke_two: None,
                economy: Some(EconomyRuntimeConfig {
                    command_timeout: Duration::from_millis(100),
                    ..default_economy()
                }),
                retail_bootstrap: false,
            },
            Arc::new(M2Metrics::default()),
        )
        .expect("stalling economy service"),
    );
    let (address, shutdown, task) = start_service(service).await;
    let (mut stream, key) = connect_economy_client(&pool, address, account.account.id).await;

    // The repository never answers, so the deadline decides the outcome.
    send_typed(
        &mut stream,
        key,
        3,
        &PurchaseRequestPacket::new(uuid::Uuid::new_v4(), 0x1a00_0001, 1).expect("purchase"),
    )
    .await;
    assert_eq!(
        receive_typed::<EconomyCommandResult>(&mut stream, key).await,
        EconomyCommandResult::new(EconomyCommand::Purchase, EconomyOutcome::Timeout)
    );

    // A timed-out command leaves the connection usable and the balance untouched.
    send_typed(&mut stream, key, 4, &ShopPageRequest::new(0)).await;
    let _ = receive_typed::<ShopPage>(&mut stream, key).await;
    let pang: i64 = sqlx::query_scalar("SELECT pang FROM profiles WHERE account_id=$1")
        .bind(account.account.id.get())
        .fetch_one(&pool)
        .await
        .expect("pang");
    assert_eq!(pang, 5000);

    drop(stream);
    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_retail_bootstrap_emits_the_reference_derived_sequence(pool: PgPool) {
    let account = create_account(&pool, "RetailBoot", 1, 0x1000_0000).await;
    let service = Arc::new(
        GameService::new(
            Arc::new(PgRepository::new(pool.clone())),
            economy_catalog(),
            GameRuntimeConfig {
                channel_id: 1,
                unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
                limits: GameRuntimeLimits {
                    packets_per_window: 200,
                    ..GameRuntimeLimits::default()
                },
                solo_practice: None,
                stroke_two: None,
                economy: None,
                retail_bootstrap: true,
            },
            Arc::new(M2Metrics::default()),
        )
        .expect("retail service"),
    );
    let (address, shutdown, task) = start_service(service).await;
    let token = issue_token(
        &pool,
        account.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut stream, key) = connect_game_retail(address).await;
    // The retail auth packet a real client sends, not the synthetic one.
    send_typed(
        &mut stream,
        key,
        1,
        &pangya_protocol::RetailGameAuth {
            username: b"RetailBoot".to_vec(),
            user_id: u32::try_from(account.account.id.get()).expect("user id"),
            login_key: zeroize::Zeroizing::new(token.clone().into_bytes()),
            client_version: pangya_protocol::US852_SERVER_VERSION.to_vec(),
            session_key: zeroize::Zeroizing::new(Vec::new()),
        },
    )
    .await;

    // Three progress ticks, the full reply, then roster/caddie/equipment/inventory and
    // the channel list. Order is what keeps the client off its loading screen.
    let mut seen = Vec::new();
    for _ in 0..9 {
        let (opcode, body) = receive_packet(&mut stream, key).await;
        seen.push((opcode, body));
    }
    let opcodes: Vec<u16> = seen.iter().map(|(opcode, _)| *opcode).collect();
    assert_eq!(
        opcodes,
        vec![
            0x0044, 0x0044, 0x0044, 0x0044, 0x0070, 0x0071, 0x0072, 0x0073, 0x004d
        ]
    );

    // The three control frames are progress ticks 0..2.
    for (index, expected) in [0_u8, 1, 2].into_iter().enumerate() {
        assert_eq!(seen[index].1, vec![0xd2, expected]);
    }

    // The full reply must announce 852.00 or the client raises a version mismatch.
    let reply = &seen[3].1;
    assert_eq!(reply[0], 0x00);
    assert_eq!(u16::from_le_bytes([reply[1], reply[2]]), 6);
    assert_eq!(&reply[3..9], b"852.00");

    // Character roster carries the account's real starter character type id.
    let roster = &seen[4].1;
    assert_eq!(u16::from_le_bytes([roster[0], roster[1]]), 1);
    assert_eq!(u16::from_le_bytes([roster[2], roster[3]]), 1);
    assert_eq!(
        u32::from_le_bytes([roster[4], roster[5], roster[6], roster[7]]),
        0x0400_0000
    );

    // The caddie container is empty but must still arrive.
    assert_eq!(seen[5].1, vec![0, 0, 0, 0]);

    // The channel list advertises exactly one channel with a zero-padded name.
    let channels = &seen[8].1;
    assert_eq!(channels[0], 1);
    assert_eq!(&channels[1..10], b"pangya-rs");
    assert_eq!(channels[10], 0);

    drop(stream);
    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_retail_rooms_create_join_and_leave_over_tcp(pool: PgPool) {
    let owner = create_account(&pool, "RetailOwner", 1, 0x1000_0000).await;
    let guest = create_account(&pool, "RetailGuest", 1, 0x1000_0000).await;
    let service = Arc::new(
        GameService::new(
            Arc::new(PgRepository::new(pool.clone())),
            economy_catalog(),
            GameRuntimeConfig {
                channel_id: 1,
                unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
                limits: GameRuntimeLimits {
                    packets_per_window: 200,
                    ..GameRuntimeLimits::default()
                },
                solo_practice: None,
                stroke_two: None,
                economy: None,
                retail_bootstrap: true,
            },
            Arc::new(M2Metrics::default()),
        )
        .expect("retail service"),
    );
    let (address, shutdown, task) = start_service(service).await;

    async fn connect_retail(
        pool: &PgPool,
        address: std::net::SocketAddr,
        account_id: pangya_domain::AccountId,
        username: &str,
    ) -> (TcpStream, u8) {
        let token = issue_token(pool, account_id, SystemTime::now(), ServiceKind::Game).await;
        let (mut stream, key) = connect_game_retail(address).await;
        send_typed(
            &mut stream,
            key,
            1,
            &pangya_protocol::RetailGameAuth {
                username: username.as_bytes().to_vec(),
                user_id: u32::try_from(account_id.get()).expect("user id"),
                login_key: zeroize::Zeroizing::new(token.into_bytes()),
                client_version: pangya_protocol::US852_SERVER_VERSION.to_vec(),
                session_key: zeroize::Zeroizing::new(Vec::new()),
            },
        )
        .await;
        // Drain the nine bootstrap frames.
        for _ in 0..9 {
            let _ = receive_packet(&mut stream, key).await;
        }
        // Enter the channel so room commands are in-state. Retail sends the one-byte sub-server
        // ID, not the synthetic `u32` channel ID.
        send_packet(&mut stream, key, 2, 4, &[1]).await;
        let (opcode, body) = receive_packet(&mut stream, key).await;
        assert_eq!(opcode, 0x004e);
        assert_eq!(body, [0x01]);
        (stream, key)
    }

    let (mut host, host_key) =
        connect_retail(&pool, address, owner.account.id, "RetailOwner").await;

    // Create a room with the retail packet the client actually sends.
    let mut writer = pangya_protocol::PacketWriter::default();
    writer.u8(0);
    writer.u32_le(30_000);
    writer.u32_le(600_000);
    writer.u8(4);
    writer.u8(0);
    writer.u8(3);
    writer.u8(1);
    writer.bytes(&[0; 5]);
    writer.pstring(b"Retail Room", 64).expect("name");
    writer.pstring(b"", 64).expect("password");
    send_packet(&mut host, host_key, 3, 0x0008, &writer.into_inner()).await;

    let (opcode, body) = receive_packet(&mut host, host_key).await;
    assert_eq!(opcode, 0x0049);
    // Accepted carries a u16 status then the 210-byte room record.
    assert_eq!(u16::from_le_bytes([body[0], body[1]]), 0);
    // Creating a room immediately yields a census listing the creator as master.
    let (census_opcode, census) = receive_packet(&mut host, host_key).await;
    assert_eq!(census_opcode, 0x0048);
    assert_eq!(census[0], 0, "census kind = list");
    assert_eq!(census[3], 1, "one player in the room");
    // Owner flag is bit 3, written after the fixed identity block.
    let flags_at = 4 + 4 + 22 + 17 + 1 + 4 + 4 + 4 + 16 + 4 + 4;
    assert_eq!(
        u16::from_le_bytes([census[flags_at], census[flags_at + 1]]) & (1 << 3),
        1 << 3,
        "creator is room master"
    );
    assert_eq!(body.len(), 2 + pangya_protocol::ROOM_RECORD_BYTES);
    assert_eq!(&body[2..13], b"Retail Room");
    let room_id = u16::from_le_bytes([body[2 + 64 + 5 + 17 + 3], body[2 + 64 + 5 + 17 + 4]]);
    assert_eq!(body[2 + 64 + 3], 4, "capacity");
    assert_eq!(body[2 + 64 + 4], 1, "occupancy");

    // A second client joins the same room by number.
    let (mut visitor, visitor_key) =
        connect_retail(&pool, address, guest.account.id, "RetailGuest").await;
    let mut join = pangya_protocol::PacketWriter::default();
    join.u16_le(room_id);
    join.pstring(b"", 64).expect("password");
    send_packet(&mut visitor, visitor_key, 3, 0x0009, &join.into_inner()).await;
    let (opcode, body) = receive_packet(&mut visitor, visitor_key).await;
    assert_eq!(opcode, 0x0049);
    assert_eq!(u16::from_le_bytes([body[0], body[1]]), 0);
    assert_eq!(body[2 + 64 + 4], 2, "room now holds both players");
    let (census_opcode, census) = receive_packet(&mut visitor, visitor_key).await;
    assert_eq!(census_opcode, 0x0048);
    assert_eq!(census[3], 2, "census lists both occupants");
    assert_eq!(
        census.len(),
        4 + 2 * pangya_protocol::ROOM_PLAYER_RECORD_BYTES + 1
    );

    // Leaving returns the client to the lobby and re-lists rooms.
    send_packet(&mut visitor, visitor_key, 4, 0x000f, &[]).await;
    let (opcode, body) = receive_packet(&mut visitor, visitor_key).await;
    assert_eq!(opcode, 0x004c);
    assert_eq!(body, vec![0xff, 0xff]);
    let (opcode, body) = receive_packet(&mut visitor, visitor_key).await;
    assert_eq!(opcode, 0x0047);
    assert_eq!(body[0], 1, "the host's room is still listed");

    // Joining a room number that does not exist is refused, not fatal.
    let mut bad = pangya_protocol::PacketWriter::default();
    bad.u16_le(4242);
    bad.pstring(b"", 64).expect("password");
    send_packet(&mut visitor, visitor_key, 5, 0x0009, &bad.into_inner()).await;
    let (opcode, body) = receive_packet(&mut visitor, visitor_key).await;
    assert_eq!(opcode, 0x0049);
    assert_eq!(body, vec![18]);

    drop(visitor);
    drop(host);
    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_retail_match_plays_and_settles_one_hole(pool: PgPool) {
    let catalog = economy_catalog();
    let course = catalog
        .one_hole_course(CourseId::new(7).expect("course ID"))
        .expect("one-hole course");
    let account = create_account(&pool, "RetailGolfer", 1, 0x1000_0000).await;
    let service = Arc::new(
        GameService::new(
            Arc::new(PgRepository::new(pool.clone())),
            catalog.clone(),
            GameRuntimeConfig {
                channel_id: 1,
                unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
                limits: GameRuntimeLimits {
                    packets_per_window: 200,
                    ..GameRuntimeLimits::default()
                },
                solo_practice: Some(SoloRuntimeConfig {
                    course,
                    catalog_fingerprint: catalog.fingerprint(),
                    loading_timeout: Duration::from_secs(30),
                    commit_timeout: Duration::from_secs(2),
                    max_strokes: 10,
                    startup_recovery_limit: IncompleteMatchAbortLimit::new(100)
                        .expect("recovery limit"),
                    shot_packets_per_window: 80,
                }),
                stroke_two: None,
                economy: None,
                retail_bootstrap: true,
            },
            Arc::new(M2Metrics::default()),
        )
        .expect("retail match service"),
    );
    let (address, shutdown, task) = start_service(service).await;
    let token = issue_token(
        &pool,
        account.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    let (mut stream, key) = connect_game_retail(address).await;
    send_typed(
        &mut stream,
        key,
        1,
        &pangya_protocol::RetailGameAuth {
            username: b"RetailGolfer".to_vec(),
            user_id: u32::try_from(account.account.id.get()).expect("user id"),
            login_key: zeroize::Zeroizing::new(token.into_bytes()),
            client_version: pangya_protocol::US852_SERVER_VERSION.to_vec(),
            session_key: zeroize::Zeroizing::new(Vec::new()),
        },
    )
    .await;
    for _ in 0..9 {
        let _ = receive_packet(&mut stream, key).await;
    }
    // Retail sends the one-byte sub-server ID for channel entry.
    send_packet(&mut stream, key, 2, 4, &[1]).await;
    assert_eq!(receive_packet(&mut stream, key).await.0, 0x004e);

    // Create a room, then start a match from inside it.
    let mut writer = pangya_protocol::PacketWriter::default();
    writer.u8(0);
    writer.u32_le(30_000);
    writer.u32_le(600_000);
    writer.u8(4);
    writer.u8(0);
    writer.u8(1);
    writer.u8(1);
    writer.bytes(&[0; 5]);
    writer.pstring(b"Hole One", 64).expect("name");
    writer.pstring(b"", 64).expect("password");
    send_packet(&mut stream, key, 3, 0x0008, &writer.into_inner()).await;
    eprintln!(
        "PROBE create -> {:#06x}",
        receive_packet(&mut stream, key).await.0
    );
    eprintln!(
        "PROBE census -> {:#06x}",
        receive_packet(&mut stream, key).await.0
    );

    // Start match: the client receives the plan, weather, and wind.
    send_packet(&mut stream, key, 4, 0x000e, &[]).await;
    eprintln!(
        "PROBE start -> {:#06x}",
        receive_packet(&mut stream, key).await.0
    );
    let (opcode, info) = receive_packet(&mut stream, key).await;
    assert_eq!(opcode, 0x0052, "match plan");
    // Four header bytes, three u32 fields, one hole, the seed, then eighteen collectible
    // counts the client reads regardless of how many holes the match has.
    assert_eq!(info.len(), 4 + 12 + 7 + 4 + 18);
    assert_eq!(receive_packet(&mut stream, key).await.0, 0x009e, "weather");
    assert_eq!(receive_packet(&mut stream, key).await.0, 0x005b, "wind");

    // Loading finished moves the hole into play and hands the player the turn.
    send_packet(&mut stream, key, 5, 0x0011, &[]).await;
    assert_eq!(
        receive_packet(&mut stream, key).await.0,
        0x0053,
        "hole intro"
    );
    assert_eq!(
        receive_packet(&mut stream, key).await.0,
        0x0063,
        "turn start"
    );

    // A shot is one stroke; the turn is handed back so the next can be played.
    send_packet(&mut stream, key, 6, 0x0012, &[0xaa; 8]).await;
    assert_eq!(receive_packet(&mut stream, key).await.0, 0x00cc, "turn end");
    assert_eq!(
        receive_packet(&mut stream, key).await.0,
        0x0063,
        "turn start"
    );

    // Finishing holes out and settles durably.
    send_packet(&mut stream, key, 7, 0x0031, &[]).await;
    assert_eq!(
        receive_packet(&mut stream, key).await.0,
        0x0065,
        "hole finished"
    );

    let (pang, currency_rows, progression_rows): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT pang FROM profiles WHERE account_id=$1), \
         (SELECT count(*) FROM currency_ledger), \
         (SELECT count(*) FROM progression_ledger)",
    )
    .bind(account.account.id.get())
    .fetch_one(&pool)
    .await
    .expect("settlement");
    // Two strokes against a par-3 hole: the server scored it, not the client.
    assert!(pang > 0, "the hole paid a Pang reward");
    assert_eq!(currency_rows, 1, "exactly one immutable Pang ledger row");
    assert_eq!(progression_rows, 1, "exactly one immutable EXP ledger row");

    drop(stream);
    shutdown.cancel();
    assert!(task.await.expect("join").is_ok());
}
