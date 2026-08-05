//! Local synthetic M3 LoginService-to-GameService real-PostgreSQL acceptance.

use std::{
    io,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime},
};

use pangya_data::Catalog;
use pangya_domain::{
    AccountAggregate, AccountRepository as _, AccountStatus, CredentialHash,
    HandoverRepository as _, ItemTypeId, NewAccount, Nickname, ServiceKind, SourceAddressPrefix,
    StarterCharacter, StarterGrant, StarterItem, StarterKey, Username,
};
use pangya_game::{GameRuntimeConfig, GameRuntimeLimits, GameService, UnknownOpcodePolicy};
use pangya_login::{
    AdvertisedGameServer, BoundedCredentialExecutor, CredentialPolicy, LoginRuntimeConfig,
    LoginRuntimeLimits, LoginService, generate_handover,
};
use pangya_observability::M2Metrics;
use pangya_protocol::PacketWriter;
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

fn game_service(
    pool: PgPool,
    limits: GameRuntimeLimits,
    metrics: Arc<M2Metrics>,
) -> Arc<GameService<PgRepository>> {
    Arc::new(
        GameService::new(
            Arc::new(PgRepository::new(pool)),
            catalog(),
            GameRuntimeConfig {
                channel_id: 1,
                unknown_opcode_policy: UnknownOpcodePolicy::Disconnect,
                limits,
            },
            metrics,
        )
        .expect("game"),
    )
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
