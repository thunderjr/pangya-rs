//! Synthetic provisional LoginService acceptance against isolated real PostgreSQL.

use std::{
    io,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use pangya_domain::{
    AccountRepository as _, ConsumeHandover, CredentialHash, HandoverError,
    HandoverRepository as _, ItemTypeId, NewAccount, Nickname, ServiceKind, StarterCharacter,
    StarterGrant, StarterItem, StarterKey, Username,
};
use pangya_login::{
    AdvertisedGameServer, BoundedCredentialExecutor, CredentialEngine, CredentialError,
    CredentialPolicy, LoginRuntimeConfig, LoginRuntimeError, LoginRuntimeLimits, LoginService,
    parse_handover,
};
use pangya_observability::M2Metrics;
use pangya_protocol::{
    CodecLimits, LOGIN_ERROR_ALREADY_LOGGED_IN, LOGIN_ERROR_INVALID_CREDENTIALS, PacketWriter,
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
const TRACE_CAPACITY: usize = 128 * 1024;

#[derive(Clone)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

struct CaptureGuard(Arc<Mutex<Vec<u8>>>);

impl io::Write for CaptureGuard {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut output = self
            .0
            .lock()
            .map_err(|_| io::Error::other("capture lock"))?;
        let remaining = TRACE_CAPACITY.saturating_sub(output.len());
        let written = remaining.min(bytes.len());
        output.extend_from_slice(&bytes[..written]);
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
        let output = Arc::new(Mutex::new(Vec::with_capacity(TRACE_CAPACITY)));
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(CaptureWriter(Arc::clone(&output)))
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("capture subscriber");
        output
    }))
}

struct SlowCredentialEngine {
    active: Arc<AtomicUsize>,
    delay: Duration,
}

impl CredentialEngine for SlowCredentialEngine {
    fn hash(
        &self,
        _secret: &pangya_login::CanonicalTransportSecret,
    ) -> Result<CredentialHash, CredentialError> {
        self.active.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(self.delay);
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(CredentialHash::new("synthetic-cancelled".to_owned()))
    }

    fn verify(
        &self,
        secret: &pangya_login::CanonicalTransportSecret,
        _stored: &CredentialHash,
    ) -> Result<(), CredentialError> {
        self.hash(secret).map(drop)
    }
}

fn key(value: &str) -> StarterKey {
    StarterKey::parse(value).expect("starter key")
}

fn starter() -> StarterGrant {
    StarterGrant {
        character: StarterCharacter {
            key: key("starter.character"),
            item_type_id: ItemTypeId::new(0x0400_0000),
        },
        items: vec![StarterItem {
            key: key("starter.club"),
            item_type_id: ItemTypeId::new(0x1000_0000),
            quantity: 1,
        }],
        equipped_club_key: Some(key("starter.club")),
        equipped_ball_key: None,
    }
}

async fn start(
    pool: PgPool,
    limits: LoginRuntimeLimits,
) -> (
    std::net::SocketAddr,
    CancellationToken,
    tokio::task::JoinHandle<()>,
    Arc<M2Metrics>,
) {
    start_named(pool, limits, "Synthetic").await
}

async fn start_named(
    pool: PgPool,
    limits: LoginRuntimeLimits,
    game_name: &str,
) -> (
    std::net::SocketAddr,
    CancellationToken,
    tokio::task::JoinHandle<()>,
    Arc<M2Metrics>,
) {
    let policy = Arc::new(CredentialPolicy::new().expect("policy"));
    let executor =
        BoundedCredentialExecutor::new(policy, 2, Duration::from_secs(2), Duration::from_secs(5))
            .expect("executor");
    start_with_executor(pool, limits, game_name, executor).await
}

async fn start_with_executor(
    pool: PgPool,
    limits: LoginRuntimeLimits,
    game_name: &str,
    executor: BoundedCredentialExecutor,
) -> (
    std::net::SocketAddr,
    CancellationToken,
    tokio::task::JoinHandle<()>,
    Arc<M2Metrics>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let metrics = Arc::new(M2Metrics::default());
    let service = Arc::new(
        LoginService::new(
            Arc::new(PgRepository::new(pool)),
            executor,
            LoginRuntimeConfig {
                auto_create_accounts: true,
                starter: starter(),
                allowed_character_types: vec![0x0400_0000],
                game_server: AdvertisedGameServer {
                    id: 7,
                    name: game_name.to_owned(),
                    ipv4: "127.0.0.1".to_owned(),
                    port: 20_201,
                    capacity: 20,
                },
                limits,
            },
            metrics.clone(),
        )
        .expect("runtime config"),
    );
    let shutdown = CancellationToken::new();
    let task_shutdown = shutdown.clone();
    let task = tokio::spawn(async move {
        service.serve(listener, task_shutdown).await.expect("serve");
    });
    (address, shutdown, task, metrics)
}

async fn connect(address: std::net::SocketAddr) -> (TcpStream, u8) {
    let mut stream = TcpStream::connect(address).await.expect("connect");
    let mut hello = [0_u8; 14];
    stream.read_exact(&mut hello).await.expect("hello");
    assert_eq!(&hello[..6], &[0, 0x0b, 0, 0, 0, 0]);
    assert!(hello[6] <= 0x0f);
    (stream, hello[6])
}

async fn send_packet(stream: &mut TcpStream, key: u8, salt: u8, opcode: u16, payload: &[u8]) {
    let mut plain = Vec::with_capacity(payload.len() + 2);
    plain.extend_from_slice(&opcode.to_le_bytes());
    plain.extend_from_slice(payload);
    let encrypted = pangya_crypto::client_encrypt(&plain, key, salt).expect("client frame");
    stream.write_all(&encrypted).await.expect("write frame");
}

async fn receive_packet(stream: &mut TcpStream, key: u8) -> (u16, Vec<u8>) {
    let mut header = [0_u8; 3];
    stream.read_exact(&mut header).await.expect("server header");
    let total = usize::from(u16::from_le_bytes([header[1], header[2]])) + 3;
    let mut frame = vec![0_u8; total];
    frame[..3].copy_from_slice(&header);
    stream
        .read_exact(&mut frame[3..])
        .await
        .expect("server frame");
    let plain =
        pangya_crypto::server_decrypt(&frame, key, 8 * 1024 * 1024, 128).expect("decrypt server");
    let opcode = u16::from_le_bytes([plain[0], plain[1]]);
    (opcode, plain[2..].to_vec())
}

fn pstring(value: &[u8]) -> Vec<u8> {
    let mut writer = PacketWriter::new();
    writer.pstring(value, 128).expect("pstring");
    writer.into_inner()
}

fn login_payload(username: &str, secret: &str) -> Vec<u8> {
    let mut writer = PacketWriter::new();
    writer.pstring(username.as_bytes(), 64).expect("username");
    writer.pstring(secret.as_bytes(), 128).expect("secret");
    writer.bytes(&[0; 17]);
    writer.into_inner()
}

fn reconnect_payload(username: &str, user_id: u32, token: &[u8]) -> Vec<u8> {
    let mut writer = PacketWriter::new();
    writer.pstring(username.as_bytes(), 64).expect("username");
    writer.u32_le(user_id);
    writer.pstring(token, 128).expect("token");
    writer.into_inner()
}

async fn create_ready_account(pool: &PgPool, username: &str) {
    let policy = CredentialPolicy::new().expect("policy");
    let secret = pangya_login::CanonicalTransportSecret::parse(SECRET).expect("secret");
    let hash = policy.hash(&secret).expect("hash");
    PgRepository::new(pool.clone())
        .create_account(NewAccount {
            username: Username::parse(username).expect("username"),
            credential_hash: hash,
            nickname: Some(Nickname::parse(&format!("N{username}")).expect("nickname")),
            starter: starter(),
        })
        .await
        .expect("ready account");
}

async fn create_needs_starter_account(pool: &PgPool, username: &str) {
    let policy = CredentialPolicy::new().expect("policy");
    let secret = pangya_login::CanonicalTransportSecret::parse(SECRET).expect("secret");
    let hash = policy.hash(&secret).expect("hash");
    let account_id: i64 = sqlx::query_scalar(
        "INSERT INTO accounts (username_normalized, username_display) VALUES ($1, $2) RETURNING id",
    )
    .bind(username.to_ascii_lowercase())
    .bind(username)
    .fetch_one(pool)
    .await
    .expect("account");
    sqlx::query(
        "INSERT INTO credentials (account_id, scheme, password_hash) VALUES ($1, 'argon2id-client-md5-v1', $2)",
    )
    .bind(account_id)
    .bind(hash.expose_phc())
    .execute(pool)
    .await
    .expect("credential");
    sqlx::query(
        "INSERT INTO profiles (account_id, nickname_display, nickname_normalized, setup_state) VALUES ($1, $2, $3, 'needs_starter')",
    )
    .bind(account_id)
    .bind("starter_nick")
    .bind("starter_nick")
    .execute(pool)
    .await
    .expect("profile");
}

async fn assert_second_packet_limited(
    pool: PgPool,
    limits: LoginRuntimeLimits,
    username: &str,
    expected_metric: &str,
) {
    let (address, shutdown, task, metrics) = start(pool, limits).await;
    let (mut stream, key) = connect(address).await;
    send_packet(&mut stream, key, 1, 1, &login_payload(username, SECRET)).await;
    assert_eq!(receive_packet(&mut stream, key).await.0, 1);
    send_packet(&mut stream, key, 2, 7, &pstring(b"RateNick")).await;
    let mut byte = [0_u8; 1];
    assert_eq!(stream.read(&mut byte).await.expect("rate close"), 0);
    assert!(
        metrics.render().contains(expected_metric),
        "{}",
        metrics.render()
    );
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn local_login_server_selection_and_single_use_handover_are_real_db_proven(pool: PgPool) {
    let trace_capture = tracing_capture();
    let limits = LoginRuntimeLimits {
        login_timeout: Duration::from_secs(10),
        idle_timeout: Duration::from_secs(10),
        codec: CodecLimits::default(),
        ..LoginRuntimeLimits::default()
    };
    let repository = PgRepository::new(pool.clone());
    let (address, shutdown, task, metrics) = start(pool.clone(), limits).await;
    let (mut stream, transport_key) = connect(address).await;

    send_packet(
        &mut stream,
        transport_key,
        1,
        1,
        &login_payload("Synthetic_One", SECRET),
    )
    .await;
    let (opcode, body) = receive_packet(&mut stream, transport_key).await;
    assert_eq!(opcode, 1);
    // U.S. 852 numbers both setup statuses one lower than the TH captures do.
    assert_eq!(body, [0xd8, 0xff, 0xff, 0xff, 0xff]);

    send_packet(
        &mut stream,
        transport_key,
        2,
        7,
        &pstring(b"Synthetic-Nick"),
    )
    .await;
    let (opcode, body) = receive_packet(&mut stream, transport_key).await;
    assert_eq!(opcode, 0x000e);
    assert_eq!(&body[..4], &[0, 0, 0, 0]);

    send_packet(
        &mut stream,
        transport_key,
        3,
        6,
        &pstring(b"Synthetic-Nick"),
    )
    .await;
    // A successful set is answered with the login result, not another `0x000e` check response.
    let (opcode, body) = receive_packet(&mut stream, transport_key).await;
    assert_eq!(opcode, 1);
    assert_eq!(body[0], 0, "status byte is success");
    assert_eq!(receive_packet(&mut stream, transport_key).await.0, 0x10);
    assert_eq!(receive_packet(&mut stream, transport_key).await.0, 6);
    assert_eq!(receive_packet(&mut stream, transport_key).await.0, 9);
    let (opcode, server_list) = receive_packet(&mut stream, transport_key).await;
    assert_eq!(opcode, 2);
    assert_eq!(server_list[0], 1);
    assert!(server_list.windows(9).any(|bytes| bytes == b"127.0.0.1"));

    send_packet(&mut stream, transport_key, 4, 3, &[7, 0, 0, 0]).await;
    let (opcode, session) = receive_packet(&mut stream, transport_key).await;
    assert_eq!(opcode, 3);
    let length = usize::from(u16::from_le_bytes([session[4], session[5]]));
    let token = std::str::from_utf8(&session[6..6 + length]).expect("token");
    assert!(!format!("{metrics:?}").contains(token));
    assert!(!metrics.render().contains(SECRET));
    assert!(!metrics.render().contains(token));
    let traces = trace_capture.lock().expect("trace capture").clone();
    let traces = String::from_utf8_lossy(&traces);
    for prohibited in [
        SECRET,
        token,
        "Synthetic_One",
        "Synthetic-Nick",
        "127.0.0.1",
    ] {
        assert!(!traces.contains(prohibited), "trace leaked {prohibited}");
    }
    assert!(traces.contains("127.0.0.0/24"));
    assert!(traces.contains("opcode"));
    tokio::time::timeout(Duration::from_secs(1), async {
        while !metrics.render().contains("reason=\"complete\"} 1") {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("completed connection metric");

    let parsed = parse_handover(token).expect("parse token");
    let consumed = repository
        .consume(ConsumeHandover {
            id: parsed.id,
            digest: parsed.digest.clone(),
            target: ServiceKind::Game,
            now: SystemTime::now(),
        })
        .await
        .expect("consume once");
    assert_eq!(
        repository
            .consume(ConsumeHandover {
                id: parsed.id,
                digest: parsed.digest,
                target: ServiceKind::Game,
                now: SystemTime::now(),
            })
            .await,
        Err(HandoverError::AlreadyConsumed)
    );
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM accounts), (SELECT count(*) FROM characters), \
         (SELECT count(*) FROM inventory_items), (SELECT count(*) FROM equipment_sets)",
    )
    .fetch_one(&pool)
    .await
    .expect("aggregate counts");
    assert_eq!(counts, (1, 1, 1, 1));
    assert!(consumed.account_id.get() > 0);

    // Duplicate authenticated LoginService handshake arms the reference ghost recovery flow
    // while the first waits on select.
    let (mut first_active, first_key) = connect(address).await;
    send_packet(
        &mut first_active,
        first_key,
        5,
        1,
        &login_payload("Synthetic_One", SECRET),
    )
    .await;
    assert_eq!(receive_packet(&mut first_active, first_key).await.0, 1);
    assert_eq!(receive_packet(&mut first_active, first_key).await.0, 0x10);
    assert_eq!(receive_packet(&mut first_active, first_key).await.0, 6);
    assert_eq!(receive_packet(&mut first_active, first_key).await.0, 9);
    assert_eq!(receive_packet(&mut first_active, first_key).await.0, 2);
    let (mut duplicate, duplicate_key) = connect(address).await;
    send_packet(
        &mut duplicate,
        duplicate_key,
        6,
        1,
        &login_payload("Synthetic_One", SECRET),
    )
    .await;
    let (opcode, duplicate_body) = receive_packet(&mut duplicate, duplicate_key).await;
    assert_eq!(opcode, 1);
    assert_eq!(duplicate_body[0], 0xe3);
    assert_eq!(
        u32::from_le_bytes(duplicate_body[1..5].try_into().expect("duplicate code")),
        LOGIN_ERROR_ALREADY_LOGGED_IN
    );
    // 0x0004 has an empty body and no response; the retry is accepted on this same connection.
    send_packet(&mut duplicate, duplicate_key, 7, 4, &[]).await;
    send_packet(
        &mut duplicate,
        duplicate_key,
        8,
        1,
        &login_payload("Synthetic_One", SECRET),
    )
    .await;
    assert_eq!(receive_packet(&mut duplicate, duplicate_key).await.0, 1);
    for expected in [0x10, 6, 9, 2] {
        assert_eq!(
            receive_packet(&mut duplicate, duplicate_key).await.0,
            expected
        );
    }
    send_packet(&mut duplicate, duplicate_key, 9, 3, &[7, 0, 0, 0]).await;
    assert_eq!(receive_packet(&mut duplicate, duplicate_key).await.0, 3);
    // The old guard may unwind after replacement; its generation cannot remove the new lease.
    send_packet(&mut first_active, first_key, 10, 3, &[7, 0, 0, 0]).await;
    assert_eq!(receive_packet(&mut first_active, first_key).await.0, 3);

    // Invalid-state packet closes without mutating another connection's state.
    let (mut invalid, invalid_key) = connect(address).await;
    send_packet(&mut invalid, invalid_key, 8, 3, &[7, 0, 0, 0]).await;
    let mut eof = [0_u8; 1];
    assert_eq!(invalid.read(&mut eof).await.expect("invalid close"), 0);

    // A true unknown opcode is separate from known-opcode invalid-state metrics.
    let (mut unknown, unknown_key) = connect(address).await;
    send_packet(&mut unknown, unknown_key, 9, 0x7777, &[]).await;
    let mut unknown_eof = [0_u8; 1];
    assert_eq!(
        unknown.read(&mut unknown_eof).await.expect("unknown close"),
        0
    );

    // An authenticated protocol error drops the RAII presence guard.
    let (mut errored, errored_key) = connect(address).await;
    send_packet(
        &mut errored,
        errored_key,
        10,
        1,
        &login_payload("Synthetic_One", SECRET),
    )
    .await;
    for expected in [1, 0x10, 6, 9, 2] {
        assert_eq!(receive_packet(&mut errored, errored_key).await.0, expected);
    }
    send_packet(&mut errored, errored_key, 11, 7, &pstring(b"WrongState")).await;
    let mut error_eof = [0_u8; 1];
    assert_eq!(errored.read(&mut error_eof).await.expect("error close"), 0);
    let (mut after_error, after_error_key) = connect(address).await;
    send_packet(
        &mut after_error,
        after_error_key,
        12,
        1,
        &login_payload("Synthetic_One", SECRET),
    )
    .await;
    assert_eq!(receive_packet(&mut after_error, after_error_key).await.0, 1);
    for expected in [0x10, 6, 9, 2] {
        assert_eq!(
            receive_packet(&mut after_error, after_error_key).await.0,
            expected
        );
    }
    send_packet(&mut after_error, after_error_key, 13, 3, &[7, 0, 0, 0]).await;
    assert_eq!(receive_packet(&mut after_error, after_error_key).await.0, 3);

    // Bad credentials receive only the redacted friendly failure.
    let (mut bad, bad_key) = connect(address).await;
    send_packet(
        &mut bad,
        bad_key,
        9,
        1,
        &login_payload("Synthetic_One", "1123456789abcdef0123456789abcdef"),
    )
    .await;
    let (opcode, body) = receive_packet(&mut bad, bad_key).await;
    assert_eq!(opcode, 1);
    assert_eq!(body[0], 0xe3);
    assert_eq!(
        u32::from_le_bytes(body[1..5].try_into().expect("credential code")),
        LOGIN_ERROR_INVALID_CREDENTIALS
    );
    let (mut nickname_conflict, conflict_key) = connect(address).await;
    send_packet(
        &mut nickname_conflict,
        conflict_key,
        14,
        1,
        &login_payload("OtherUser", SECRET),
    )
    .await;
    assert_eq!(
        receive_packet(&mut nickname_conflict, conflict_key).await.0,
        1
    );
    send_packet(
        &mut nickname_conflict,
        conflict_key,
        15,
        6,
        &pstring(b"Synthetic-Nick"),
    )
    .await;
    let (opcode, conflict_body) = receive_packet(&mut nickname_conflict, conflict_key).await;
    assert_eq!(opcode, 0x000e);
    assert_eq!(&conflict_body[..4], &[2, 0, 0, 0]);

    create_ready_account(&pool, "PolicyUser").await;
    sqlx::query(
        "UPDATE credentials SET password_hash = 'unsupported-phc' \
         WHERE account_id = (SELECT id FROM accounts WHERE username_normalized = 'policyuser')",
    )
    .execute(&pool)
    .await
    .expect("corrupt stored policy");
    let (mut policy, policy_key) = connect(address).await;
    send_packet(
        &mut policy,
        policy_key,
        16,
        1,
        &login_payload("PolicyUser", SECRET),
    )
    .await;
    let mut policy_eof = [0_u8; 1];
    assert_eq!(policy.read(&mut policy_eof).await.expect("policy close"), 0);

    let rendered = metrics.render();
    assert!(rendered.contains("class=\"fast\"} "));
    assert!(!rendered.contains("class=\"fast\"} 0"));
    assert!(rendered.contains("class=\"error\"} 1"));
    assert!(rendered.contains("outcome=\"operational_error\"} 1"));
    assert!(rendered.contains("class=\"invalid_state\"} 2"));
    assert!(rendered.contains("class=\"unknown_opcode\"} 1"));
    assert!(rendered.contains("range=\"other\"} 1"));

    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn disallowed_character_refusal_is_client_visible(pool: PgPool) {
    create_needs_starter_account(&pool, "WrongCharacterUser").await;
    let (address, shutdown, task, _) = start(pool, LoginRuntimeLimits::default()).await;
    let (mut stream, key) = connect(address).await;
    send_packet(
        &mut stream,
        key,
        1,
        1,
        &login_payload("WrongCharacterUser", SECRET),
    )
    .await;
    assert_eq!(receive_packet(&mut stream, key).await.0, 1);
    send_packet(&mut stream, key, 2, 8, &[0x09, 0, 0, 4, 0, 0]).await;
    let (opcode, body) = receive_packet(&mut stream, key).await;
    assert_eq!(opcode, 1);
    assert_eq!(body[0], 0xe3);
    assert_eq!(
        u32::from_le_bytes(body[1..5].try_into().expect("character refusal code")),
        LOGIN_ERROR_INVALID_CREDENTIALS
    );
    let mut eof = [0_u8; 1];
    assert_eq!(
        stream
            .read(&mut eof)
            .await
            .expect("character refusal close"),
        0
    );
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn configured_server_refusal_is_client_visible(pool: PgPool) {
    create_ready_account(&pool, "WrongServerUser").await;
    let (address, shutdown, task, _) = start(pool, LoginRuntimeLimits::default()).await;
    let (mut stream, key) = connect(address).await;
    send_packet(
        &mut stream,
        key,
        1,
        1,
        &login_payload("WrongServerUser", SECRET),
    )
    .await;
    assert_eq!(receive_packet(&mut stream, key).await.0, 1);
    for expected in [0x10, 6, 9, 2] {
        assert_eq!(receive_packet(&mut stream, key).await.0, expected);
    }
    send_packet(&mut stream, key, 2, 3, &[99, 0, 0, 0]).await;
    let (opcode, body) = receive_packet(&mut stream, key).await;
    assert_eq!(opcode, 1);
    assert_eq!(body[0], 0xe3);
    assert_eq!(
        u32::from_le_bytes(body[1..5].try_into().expect("server refusal code")),
        LOGIN_ERROR_INVALID_CREDENTIALS
    );
    let mut eof = [0_u8; 1];
    assert_eq!(
        stream.read(&mut eof).await.expect("server refusal close"),
        0
    );
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn reconnect_is_refused_with_a_client_visible_token_error(pool: PgPool) {
    let (address, shutdown, task, _) = start(pool, LoginRuntimeLimits::default()).await;
    let (mut stream, key) = connect(address).await;
    send_packet(
        &mut stream,
        key,
        1,
        0x000b,
        &reconnect_payload("ReconnectUser", 42, b"stale-session-token"),
    )
    .await;
    let (opcode, body) = receive_packet(&mut stream, key).await;
    assert_eq!(opcode, 1);
    assert_eq!(body[0], 0xe3);
    assert_eq!(
        u32::from_le_bytes(body[1..5].try_into().expect("reconnect code")),
        pangya_protocol::LOGIN_ERROR_INVALID_RECONNECT_TOKEN
    );
    let mut eof = [0_u8; 1];
    assert_eq!(stream.read(&mut eof).await.expect("reconnect close"), 0);
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn needs_starter_selects_only_allowlisted_character_and_persists(pool: PgPool) {
    let policy = CredentialPolicy::new().expect("policy");
    let secret = pangya_login::CanonicalTransportSecret::parse(SECRET).expect("secret");
    let hash = policy.hash(&secret).expect("hash");
    let account_id: i64 = sqlx::query_scalar(
        "INSERT INTO accounts (username_normalized, username_display) \
         VALUES ('needsstarter', 'NeedsStarter') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("account");
    sqlx::query(
        "INSERT INTO credentials (account_id, scheme, password_hash) \
         VALUES ($1, 'argon2id-client-md5-v1', $2)",
    )
    .bind(account_id)
    .bind(hash.expose_phc())
    .execute(&pool)
    .await
    .expect("credential");
    sqlx::query(
        "INSERT INTO profiles \
         (account_id, nickname_display, nickname_normalized, setup_state) \
         VALUES ($1, 'StarterNick', 'starternick', 'needs_starter')",
    )
    .bind(account_id)
    .execute(&pool)
    .await
    .expect("profile");

    let (address, shutdown, task, _) = start(pool.clone(), LoginRuntimeLimits::default()).await;
    let (mut stream, key) = connect(address).await;
    send_packet(
        &mut stream,
        key,
        1,
        1,
        &login_payload("NeedsStarter", SECRET),
    )
    .await;
    let (opcode, body) = receive_packet(&mut stream, key).await;
    assert_eq!(opcode, 1);
    assert_eq!(body, [0xd9]);
    let mut character = Vec::new();
    character.extend_from_slice(&0x0400_0000_u32.to_le_bytes());
    character.extend_from_slice(&0_u16.to_le_bytes());
    send_packet(&mut stream, key, 2, 8, &character).await;
    // Upstream documents the login packet being resent with `success` once the character is
    // selected; a real client blocks until it arrives.
    let mut login_key_body = None;
    for expected in [1, 0x10, 6, 9, 2] {
        let (opcode, body) = receive_packet(&mut stream, key).await;
        assert_eq!(opcode, expected);
        if opcode == 0x10 {
            login_key_body = Some(body);
        }
    }
    send_packet(&mut stream, key, 3, 3, &[7, 0, 0, 0]).await;
    let (opcode, game_key_body) = receive_packet(&mut stream, key).await;
    assert_eq!(opcode, 3);
    // A real client stores the `0x0010` key and echoes it to GameService; `0x0003` repeats the
    // same value after server selection. Both must carry the identical non-empty bearer, or the
    // client authenticates with an empty login key. Only the opcodes were asserted before, which
    // is why omitting `0x0010` entirely went unnoticed here.
    let login_key_body = login_key_body.expect("login key packet");
    let bearer_length = usize::from(u16::from_le_bytes([login_key_body[0], login_key_body[1]]));
    assert!(bearer_length > 0, "login key must not be empty");
    assert_eq!(login_key_body.len(), bearer_length + 2);
    assert_eq!(
        game_key_body[4..],
        login_key_body[..],
        "0x0003 repeats the 0x0010 bearer after four unknown bytes"
    );
    let state: String =
        sqlx::query_scalar("SELECT setup_state FROM profiles WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&pool)
            .await
            .expect("setup state");
    assert_eq!(state, "complete");
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn total_login_deadline_cancels_blocked_setup_database_work(pool: PgPool) {
    let policy = CredentialPolicy::new().expect("policy");
    let secret = pangya_login::CanonicalTransportSecret::parse(SECRET).expect("secret");
    let hash = policy.hash(&secret).expect("hash");
    let aggregate = PgRepository::new(pool.clone())
        .create_account(NewAccount {
            username: Username::parse("DeadlineUser").expect("user"),
            credential_hash: hash,
            nickname: None,
            starter: starter(),
        })
        .await
        .expect("account");
    let limits = LoginRuntimeLimits {
        login_timeout: Duration::from_secs(1),
        idle_timeout: Duration::from_secs(2),
        ..LoginRuntimeLimits::default()
    };
    let (address, shutdown, task, _) = start(pool.clone(), limits).await;
    let (mut stream, key) = connect(address).await;
    send_packet(
        &mut stream,
        key,
        1,
        1,
        &login_payload("DeadlineUser", SECRET),
    )
    .await;
    assert_eq!(receive_packet(&mut stream, key).await.0, 1);

    let mut lock = pool.begin().await.expect("lock transaction");
    sqlx::query("SELECT id FROM accounts WHERE id = $1 FOR UPDATE")
        .bind(aggregate.account.id.get())
        .fetch_one(&mut *lock)
        .await
        .expect("account lock");
    send_packet(&mut stream, key, 2, 6, &pstring(b"DeadlineNick")).await;
    let mut eof = [0_u8; 1];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut eof))
        .await
        .expect("total deadline")
        .expect("read");
    assert_eq!(read, 0);
    lock.rollback().await.expect("unlock");
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn nickname_unavailable_and_duplicate_retries_are_bounded(pool: PgPool) {
    create_ready_account(&pool, "TakenUser").await;
    let repository = PgRepository::new(pool.clone());
    let taken = repository
        .load_authentication(&pangya_domain::NormalizedUsername::parse("TakenUser").expect("name"))
        .await
        .expect("load")
        .expect("record");
    repository
        .set_nickname(
            taken.account.id,
            Nickname::parse("TakenNick").expect("nick"),
        )
        .await
        .expect("set taken nickname");

    let (address, shutdown, task, _) = start(pool, LoginRuntimeLimits::default()).await;
    let (mut check, check_key) = connect(address).await;
    send_packet(
        &mut check,
        check_key,
        1,
        1,
        &login_payload("CheckRetry", SECRET),
    )
    .await;
    assert_eq!(receive_packet(&mut check, check_key).await.0, 1);
    for salt in 2..=4 {
        send_packet(&mut check, check_key, salt, 7, &pstring(b"TakenNick")).await;
        let (opcode, body) = receive_packet(&mut check, check_key).await;
        assert_eq!(opcode, 0x000e);
        assert_eq!(&body[..4], &[2, 0, 0, 0]);
    }
    let mut eof = [0_u8; 1];
    assert_eq!(check.read(&mut eof).await.expect("check retries close"), 0);

    let (mut alternating, alternating_key) = connect(address).await;
    send_packet(
        &mut alternating,
        alternating_key,
        5,
        1,
        &login_payload("AlternatingRetry", SECRET),
    )
    .await;
    assert_eq!(receive_packet(&mut alternating, alternating_key).await.0, 1);
    for (salt, nickname, expected) in [
        (6, b"TakenNick".as_slice(), 2_u32),
        (7, b"FreshOne".as_slice(), 0_u32),
        (8, b"TakenNick".as_slice(), 2_u32),
        (9, b"FreshTwo".as_slice(), 0_u32),
        (10, b"TakenNick".as_slice(), 2_u32),
    ] {
        send_packet(
            &mut alternating,
            alternating_key,
            salt,
            7,
            &pstring(nickname),
        )
        .await;
        let (opcode, body) = receive_packet(&mut alternating, alternating_key).await;
        assert_eq!(opcode, 0x000e);
        assert_eq!(
            u32::from_le_bytes(body[..4].try_into().expect("result")),
            expected
        );
    }
    assert_eq!(
        alternating
            .read(&mut eof)
            .await
            .expect("alternating retries close"),
        0
    );

    let (mut set, set_key) = connect(address).await;
    send_packet(&mut set, set_key, 5, 1, &login_payload("SetRetry", SECRET)).await;
    assert_eq!(receive_packet(&mut set, set_key).await.0, 1);
    for salt in 6..=8 {
        send_packet(&mut set, set_key, salt, 6, &pstring(b"TakenNick")).await;
        let (opcode, body) = receive_packet(&mut set, set_key).await;
        assert_eq!(opcode, 0x000e);
        assert_eq!(&body[..4], &[2, 0, 0, 0]);
    }
    assert_eq!(set.read(&mut eof).await.expect("set retries close"), 0);
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn incomplete_peer_eof_and_service_cancellation_are_not_completion(pool: PgPool) {
    let (address, shutdown, task, metrics) = start(pool, LoginRuntimeLimits::default()).await;

    let (peer_closed, _) = connect(address).await;
    drop(peer_closed);
    tokio::time::timeout(Duration::from_secs(1), async {
        while !metrics.render().contains("reason=\"peer_closed\"} 1") {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("peer EOF metric");

    let (_cancelled, _) = connect(address).await;
    shutdown.cancel();
    task.await.expect("join");
    let rendered = metrics.render();
    assert!(rendered.contains("reason=\"cancelled\"} 1"), "{rendered}");
    assert!(rendered.contains("reason=\"complete\"} 0"), "{rendered}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn global_and_source_accept_rate_limits_close_before_hello(pool: PgPool) {
    let limits = LoginRuntimeLimits {
        accepts_per_window: 10,
        global_accepts_per_window: 1,
        ..LoginRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start(pool.clone(), limits).await;
    let (_first, _) = connect(address).await;
    let mut second = TcpStream::connect(address).await.expect("second connect");
    let mut byte = [0_u8; 1];
    assert_eq!(second.read(&mut byte).await.expect("limited close"), 0);
    assert!(metrics.render().contains("class=\"accept_global\"} 1"));
    shutdown.cancel();
    task.await.expect("join");

    let limits = LoginRuntimeLimits {
        accepts_per_window: 1,
        global_accepts_per_window: 2,
        ..LoginRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start(pool, limits).await;
    let (_first, _) = connect(address).await;
    let mut second = TcpStream::connect(address).await.expect("second connect");
    let mut byte = [0_u8; 1];
    assert_eq!(second.read(&mut byte).await.expect("limited close"), 0);
    assert!(metrics.render().contains("class=\"accept_source\"} 1"));
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn pre_spawn_admission_hard_bounds_hello_tasks_and_sockets(pool: PgPool) {
    let limits = LoginRuntimeLimits {
        global_connections: 2,
        connections_per_source: 2,
        global_accepts_per_window: 100,
        accepts_per_window: 100,
        ..LoginRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start(pool.clone(), limits).await;
    let mut admitted = Vec::new();
    for _ in 0..2 {
        admitted.push(connect(address).await.0);
    }
    for _ in 0..18 {
        let mut excess = TcpStream::connect(address).await.expect("excess connect");
        let mut byte = [0_u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(1), excess.read(&mut byte))
            .await
            .expect("bounded excess close")
            .expect("excess read");
        assert_eq!(read, 0);
    }
    assert!(
        metrics
            .render()
            .contains("pangya_connections_accepted_total{service=\"login\"} 2")
    );
    assert!(metrics.render().contains("class=\"connection_global\"} 18"));
    drop(admitted);
    shutdown.cancel();
    task.await.expect("join");

    let limits = LoginRuntimeLimits {
        global_connections: 4,
        connections_per_source: 1,
        global_accepts_per_window: 100,
        accepts_per_window: 100,
        ..LoginRuntimeLimits::default()
    };
    let (address, shutdown, task, metrics) = start(pool, limits).await;
    let (_held, _) = connect(address).await;
    let mut excess = TcpStream::connect(address).await.expect("source excess");
    let mut byte = [0_u8; 1];
    assert_eq!(excess.read(&mut byte).await.expect("source close"), 0);
    assert!(metrics.render().contains("class=\"connection_source\"} 1"));
    shutdown.cancel();
    task.await.expect("join");
}

async fn assert_second_login_limited(
    pool: PgPool,
    limits: LoginRuntimeLimits,
    username: &str,
    expected_metric: &str,
) {
    create_ready_account(&pool, username).await;
    let (address, shutdown, task, metrics) = start(pool, limits).await;
    let (mut stream, key) = connect(address).await;
    let wrong = "1123456789abcdef0123456789abcdef";
    send_packet(&mut stream, key, 1, 1, &login_payload(username, wrong)).await;
    assert_eq!(receive_packet(&mut stream, key).await.0, 1);
    send_packet(&mut stream, key, 2, 1, &login_payload(username, wrong)).await;
    let mut byte = [0_u8; 1];
    assert_eq!(stream.read(&mut byte).await.expect("login limit close"), 0);
    assert!(
        metrics.render().contains(expected_metric),
        "{}",
        metrics.render()
    );
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn global_source_and_username_login_budgets_and_retry_exhaustion_fire_on_tcp(pool: PgPool) {
    let high = LoginRuntimeLimits::default();
    let mut limits = high.clone();
    limits.global_logins_per_window = 1;
    assert_second_login_limited(
        pool.clone(),
        limits,
        "GlobRate",
        "class=\"login_global\"} 1",
    )
    .await;

    let mut limits = high.clone();
    limits.logins_per_window = 1;
    assert_second_login_limited(
        pool.clone(),
        limits,
        "SourceRate",
        "class=\"login_source\"} 1",
    )
    .await;

    let mut limits = high.clone();
    limits.username_logins_per_window = 1;
    assert_second_login_limited(
        pool.clone(),
        limits,
        "UserRate",
        "class=\"login_username\"} 1",
    )
    .await;

    create_ready_account(&pool, "RetryUser").await;
    let (address, shutdown, task, metrics) = start(pool.clone(), high).await;
    let (mut stream, key) = connect(address).await;
    let wrong = "1123456789abcdef0123456789abcdef";
    for salt in 1..=3 {
        send_packet(
            &mut stream,
            key,
            salt,
            1,
            &login_payload("RetryUser", wrong),
        )
        .await;
        let (opcode, body) = receive_packet(&mut stream, key).await;
        assert_eq!(opcode, 1);
        assert_eq!(body[0], 0xe3);
    }
    let mut byte = [0_u8; 1];
    assert_eq!(stream.read(&mut byte).await.expect("retry close"), 0);
    assert!(metrics.render().contains("outcome=\"rejected\"} 3"));
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn global_source_and_connection_packet_and_weighted_byte_limits_fire_on_tcp(pool: PgPool) {
    let high = LoginRuntimeLimits::default();
    let mut limits = high.clone();
    limits.global_packets_per_window = 1;
    assert_second_packet_limited(
        pool.clone(),
        limits,
        "RateGlobalPacket",
        "class=\"packet_global\"} 1",
    )
    .await;

    let mut limits = high.clone();
    limits.source_packets_per_window = 1;
    assert_second_packet_limited(
        pool.clone(),
        limits,
        "RateSourcePacket",
        "class=\"packet_source\"} 1",
    )
    .await;

    let mut limits = high.clone();
    limits.packets_per_window = 1;
    assert_second_packet_limited(
        pool.clone(),
        limits,
        "RateConnPacket",
        "class=\"packet_or_bytes_connection\"} 1",
    )
    .await;

    let login_bytes =
        u64::try_from(login_payload("RateGlobalBytes", SECRET).len() + 2).expect("login bytes");
    let mut limits = high.clone();
    limits.global_bytes_per_window = login_bytes;
    assert_second_packet_limited(
        pool.clone(),
        limits,
        "RateGlobalBytes",
        "class=\"bytes_global\"} 1",
    )
    .await;

    let login_bytes =
        u64::try_from(login_payload("RateSourceBytes", SECRET).len() + 2).expect("login bytes");
    let mut limits = high.clone();
    limits.source_bytes_per_window = login_bytes;
    assert_second_packet_limited(
        pool.clone(),
        limits,
        "RateSourceBytes",
        "class=\"bytes_source\"} 1",
    )
    .await;

    let login_bytes =
        u64::try_from(login_payload("RateConnBytes", SECRET).len() + 2).expect("login bytes");
    let mut limits = high;
    limits.bytes_per_window = login_bytes;
    assert_second_packet_limited(
        pool,
        limits,
        "RateConnBytes",
        "class=\"packet_or_bytes_connection\"} 1",
    )
    .await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn runtime_encode_failure_records_nonzero_protocol_metric(pool: PgPool) {
    create_ready_account(&pool, "EncodeUser").await;
    let (address, shutdown, task, metrics) = start_named(
        pool,
        LoginRuntimeLimits::default(),
        "this-game-server-name-is-deliberately-over-forty-bytes",
    )
    .await;
    let (mut stream, key) = connect(address).await;
    send_packet(&mut stream, key, 1, 1, &login_payload("EncodeUser", SECRET)).await;
    for expected in [1, 0x10, 6, 9] {
        assert_eq!(receive_packet(&mut stream, key).await.0, expected);
    }
    let mut byte = [0_u8; 1];
    assert_eq!(stream.read(&mut byte).await.expect("encode close"), 0);
    assert!(metrics.render().contains("class=\"encode_or_compress\"} 1"));
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn malformed_frame_records_typed_decode_error(pool: PgPool) {
    let (address, shutdown, task, metrics) = start(pool, LoginRuntimeLimits::default()).await;
    let (mut stream, _) = connect(address).await;
    stream.write_all(&[0, 0, 0, 0]).await.expect("malformed");
    let mut byte = [0_u8; 1];
    assert_eq!(stream.read(&mut byte).await.expect("malformed close"), 0);
    assert!(metrics.render().contains("class=\"decode_invalid\"} 1"));
    shutdown.cancel();
    task.await.expect("join");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn credential_timeout_and_queue_overload_emit_distinct_runtime_metrics(pool: PgPool) {
    let active = Arc::new(AtomicUsize::new(0));
    let timeout_executor = BoundedCredentialExecutor::new(
        Arc::new(SlowCredentialEngine {
            active: Arc::clone(&active),
            delay: Duration::from_millis(200),
        }),
        1,
        Duration::from_millis(2),
        Duration::from_millis(5),
    )
    .expect("timeout executor");
    let (address, shutdown, task, metrics) = start_with_executor(
        pool.clone(),
        LoginRuntimeLimits::default(),
        "Synthetic",
        timeout_executor,
    )
    .await;
    let (mut stream, key) = connect(address).await;
    send_packet(
        &mut stream,
        key,
        1,
        1,
        &login_payload("TimeoutUser", SECRET),
    )
    .await;
    let mut byte = [0_u8; 1];
    assert_eq!(stream.read(&mut byte).await.expect("timeout close"), 0);
    assert!(metrics.render().contains("outcome=\"timeout\"} 1"));
    shutdown.cancel();
    task.await.expect("join");
    tokio::time::timeout(Duration::from_secs(1), async {
        while active.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("timeout worker finished");

    let active = Arc::new(AtomicUsize::new(0));
    let overload_executor = BoundedCredentialExecutor::new(
        Arc::new(SlowCredentialEngine {
            active: Arc::clone(&active),
            delay: Duration::from_millis(200),
        }),
        1,
        Duration::from_millis(2),
        Duration::from_secs(1),
    )
    .expect("overload executor");
    let (address, shutdown, task, metrics) = start_with_executor(
        pool,
        LoginRuntimeLimits::default(),
        "Synthetic",
        overload_executor,
    )
    .await;
    let (mut first, first_key) = connect(address).await;
    send_packet(
        &mut first,
        first_key,
        1,
        1,
        &login_payload("OverloadOne", SECRET),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while active.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first worker entered");
    let (mut second, second_key) = connect(address).await;
    send_packet(
        &mut second,
        second_key,
        2,
        1,
        &login_payload("OverloadTwo", SECRET),
    )
    .await;
    assert_eq!(second.read(&mut byte).await.expect("overload close"), 0);
    assert!(metrics.render().contains("outcome=\"overload\"} 1"));
    shutdown.cancel();
    task.await.expect("join");
    tokio::time::timeout(Duration::from_secs(1), async {
        while active.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("overload worker finished");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn shutdown_cancels_connection_while_bounded_credential_worker_retains_permit(pool: PgPool) {
    let active = Arc::new(AtomicUsize::new(0));
    let executor = BoundedCredentialExecutor::new(
        Arc::new(SlowCredentialEngine {
            active: Arc::clone(&active),
            delay: Duration::from_millis(200),
        }),
        1,
        Duration::from_millis(10),
        Duration::from_secs(1),
    )
    .expect("executor");
    let (address, shutdown, task, _) = start_with_executor(
        pool,
        LoginRuntimeLimits {
            shutdown_grace: Duration::from_millis(500),
            ..LoginRuntimeLimits::default()
        },
        "Synthetic",
        executor,
    )
    .await;
    let (mut stream, key) = connect(address).await;
    send_packet(&mut stream, key, 1, 1, &login_payload("CancelUser", SECRET)).await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while active.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("credential worker entered");
    shutdown.cancel();
    tokio::time::timeout(Duration::from_millis(550), task)
        .await
        .expect("connection cancellation bounded")
        .expect("serve join");
    assert_eq!(
        active.load(Ordering::SeqCst),
        0,
        "service shutdown waits for credential workers within grace"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn over_grace_noncancellable_worker_returns_bounded_shutdown_error(pool: PgPool) {
    let active = Arc::new(AtomicUsize::new(0));
    let executor = BoundedCredentialExecutor::new(
        Arc::new(SlowCredentialEngine {
            active: Arc::clone(&active),
            delay: Duration::from_millis(300),
        }),
        1,
        Duration::from_millis(10),
        Duration::from_secs(1),
    )
    .expect("executor");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let service = Arc::new(
        LoginService::new(
            Arc::new(PgRepository::new(pool)),
            executor,
            LoginRuntimeConfig {
                auto_create_accounts: true,
                starter: starter(),
                allowed_character_types: vec![0x0400_0000],
                game_server: AdvertisedGameServer {
                    id: 7,
                    name: "Synthetic".to_owned(),
                    ipv4: "127.0.0.1".to_owned(),
                    port: 20_201,
                    capacity: 20,
                },
                limits: LoginRuntimeLimits {
                    shutdown_grace: Duration::from_millis(50),
                    ..LoginRuntimeLimits::default()
                },
            },
            Arc::new(M2Metrics::default()),
        )
        .expect("runtime config"),
    );
    let shutdown = CancellationToken::new();
    let task_shutdown = shutdown.clone();
    let task = tokio::spawn(async move { service.serve(listener, task_shutdown).await });
    let (mut stream, key) = connect(address).await;
    send_packet(
        &mut stream,
        key,
        1,
        1,
        &login_payload("OverGraceUser", SECRET),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while active.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("credential worker entered");
    shutdown.cancel();
    let result = tokio::time::timeout(Duration::from_millis(150), task)
        .await
        .expect("service shutdown remained bounded")
        .expect("serve join");
    assert!(matches!(result, Err(LoginRuntimeError::ShutdownTimeout)));
    assert_eq!(
        active.load(Ordering::SeqCst),
        1,
        "spawn_blocking work is not cancellable"
    );
    tokio::time::timeout(Duration::from_secs(1), async {
        while active.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("detached blocking worker eventually finished");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn idle_login_times_out_and_shutdown_drains(pool: PgPool) {
    let limits = LoginRuntimeLimits {
        login_timeout: Duration::from_millis(100),
        idle_timeout: Duration::from_millis(100),
        shutdown_grace: Duration::from_secs(1),
        ..LoginRuntimeLimits::default()
    };
    let (address, shutdown, task, _) = start(pool, limits).await;
    let (mut stream, _) = connect(address).await;
    let mut eof = [0_u8; 1];
    assert_eq!(stream.read(&mut eof).await.expect("timeout close"), 0);
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("bounded shutdown")
        .expect("join");
}
