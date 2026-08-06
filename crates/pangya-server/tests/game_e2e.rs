//! Local synthetic M3 LoginService-to-GameService real-PostgreSQL acceptance.

use std::{
    io,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime},
};

use pangya_data::Catalog;
use pangya_domain::{
    AccountAggregate, AccountId, AccountRepository as _, AccountStatus, ChatText, CredentialHash,
    HandoverRepository as _, ItemTypeId, MemberSnapshot, NewAccount, Nickname, PlayerConnectionId,
    RoomId, RoomName, RoomPassword, RoomSettings, RoomSnapshot, RoomSummary, ServiceKind,
    SourceAddressPrefix, StarterCharacter, StarterGrant, StarterItem, StarterKey, Username,
};
use pangya_game::{GameRuntimeConfig, GameRuntimeLimits, GameService, UnknownOpcodePolicy};
use pangya_login::{
    AdvertisedGameServer, BoundedCredentialExecutor, CredentialPolicy, LoginRuntimeConfig,
    LoginRuntimeLimits, LoginService, generate_handover,
};
use pangya_observability::M2Metrics;
use pangya_protocol::{
    CompatibilityProfile, DecodePacket, EncodePacket, PacketWriter, RoomChatEvent, RoomChatRequest,
    RoomCommand, RoomCommandResult, RoomCommandResultResponse, RoomCreateRequest, RoomJoinRequest,
    RoomKickRequest, RoomLeaveRequest, RoomListRequest, RoomListResponse, RoomMembershipEvent,
    RoomMembershipKind, RoomReadyRequest, RoomSettingsRequest, RoomStateRequest, RoomStateResponse,
    ServiceKind as ProtocolServiceKind, decode_packet_payload, encode_packet_payload,
};
use pangya_storage::{MIGRATOR, PgRepository};
use sqlx::PgPool;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::fmt::MakeWriter;

const SECRET: &str = "0123456789abcdef0123456789abcdef";

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

async fn start_service(
    service: Arc<GameService<PgRepository>>,
) -> (
    std::net::SocketAddr,
    CancellationToken,
    tokio::task::JoinHandle<Result<(), pangya_game::GameRuntimeError>>,
) {
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
    stream.read_exact(&mut hello).await.expect("hello");
    assert!(hello[3] <= 0x0f);
    (stream, hello[3])
}

async fn connect_login(address: std::net::SocketAddr) -> (TcpStream, u8) {
    let mut stream = TcpStream::connect(address).await.expect("connect");
    let mut hello = [0_u8; 14];
    stream.read_exact(&mut hello).await.expect("hello");
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
    let mut header = [0_u8; 3];
    stream.read_exact(&mut header).await.expect("header");
    let total = usize::from(u16::from_le_bytes([header[1], header[2]])) + 3;
    let mut frame = vec![0_u8; total];
    frame[..3].copy_from_slice(&header);
    stream.read_exact(&mut frame[3..]).await.expect("frame");
    let plain = pangya_crypto::server_decrypt(&frame, key, 8 * 1024 * 1024, 128).expect("decrypt");
    (
        u16::from_le_bytes([plain[0], plain[1]]),
        plain[2..].to_vec(),
    )
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
    let mut eof = [0_u8; 1];
    let read = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut eof))
        .await
        .expect("bounded close")
        .expect("close read");
    assert_eq!(read, 0);
}

async fn assert_metric(metrics: &M2Metrics, needle: &str) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while !metrics.render().contains(needle) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing metric {needle}: {}", metrics.render()));
}

async fn assert_counter_at_least(metrics: &M2Metrics, prefix: &str, minimum: u64) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let rendered = metrics.render();
            let reached = rendered.lines().any(|line| {
                line.strip_prefix(prefix)
                    .and_then(|value| value.parse::<u64>().ok())
                    .is_some_and(|value| value >= minimum)
            });
            if reached {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("counter below {minimum} for {prefix}: {}", metrics.render()));
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

async fn read_bootstrap(stream: &mut TcpStream, key: u8, inventory_segments: usize) {
    assert_eq!(receive_packet(stream, key).await.0, 0x0070);
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
    for opcode in [1, 6, 9, 2] {
        assert_eq!(receive_packet(&mut login_stream, login_key).await.0, opcode);
    }
    send_packet(&mut login_stream, login_key, 2, 3, &[7, 0, 0, 0]).await;
    let (opcode, body) = receive_packet(&mut login_stream, login_key).await;
    assert_eq!(opcode, 3);
    let length = usize::from(u16::from_le_bytes([body[4], body[5]]));
    let token = std::str::from_utf8(&body[6..6 + length])
        .expect("token")
        .to_owned();

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
    assert!(rendered.contains("pangya_game_auth_total{outcome=\"success\"} 1"));
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
    let limits = GameRuntimeLimits {
        authentication_timeout: Duration::from_millis(100),
        idle_timeout: Duration::from_secs(2),
        ..GameRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start_game(pool.clone(), limits).await;
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
    assert!(metrics.render().contains("outcome=\"duplicate\"} 1"));
    drop(first);
    tokio::time::sleep(Duration::from_millis(20)).await;
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

    let concurrent = issue_token(
        &pool,
        aggregate.account.id,
        SystemTime::now(),
        ServiceKind::Game,
    )
    .await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    let (mut left, left_key) = connect_game(address).await;
    let (mut right, right_key) = connect_game(address).await;
    send_packet(
        &mut left,
        left_key,
        4,
        2,
        &auth_payload(aggregate.account.id.get(), &concurrent),
    )
    .await;
    send_packet(
        &mut right,
        right_key,
        5,
        2,
        &auth_payload(aggregate.account.id.get(), &concurrent),
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
    assert!(metrics.render().contains("class=\"accept_global\"} 1"));
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
    let limits = GameRuntimeLimits {
        authentication_timeout: Duration::from_secs(1),
        idle_timeout: Duration::from_millis(50),
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
    let (mut idle, idle_key) = connect_game(address).await;
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
    assert_closed(&mut idle).await;
    assert_counter_at_least(
        &metrics,
        "pangya_connections_closed_total{service=\"game\",reason=\"timeout\"} ",
        1,
    )
    .await;

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
    tokio::time::timeout(Duration::from_millis(300), task)
        .await
        .expect("shutdown bound")
        .expect("join")
        .expect("serve");
    assert_metric(&metrics, "service=\"game\",reason=\"cancelled\"} 1").await;
    assert!(
        metrics
            .render()
            .contains("pangya_connections_active{service=\"game\"} 0")
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

    assert!(
        metrics
            .render()
            .contains("pangya_game_rooms_active{service=\"game\"} 1")
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
        assert!(rendered.contains(fixed_label), "missing {fixed_label}");
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
    assert!(
        metrics
            .render()
            .contains("pangya_game_rooms_active{service=\"game\"} 0")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn game_m4_m5_unknown_policies_continue_or_close_and_known_wrong_state_always_closes(
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
    assert!(rendered.contains("pangya_game_unknown_opcode_actions_total{action=\"captured\"} 1"));
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
        "pangya_connections_closed_total{service=\"game\",reason=\"protocol\"} 2",
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
    assert!(
        metrics
            .render()
            .contains("pangya_connections_active{service=\"game\"} 2")
    );
    shutdown.cancel();
    tokio::time::timeout(Duration::from_millis(200), task)
        .await
        .expect("shutdown grace bound")
        .expect("join")
        .expect("serve");
    assert!(
        metrics
            .render()
            .contains("pangya_connections_active{service=\"game\"} 0")
    );
}
