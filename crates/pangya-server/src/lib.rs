#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Testable configuration and M2/M3 modular-monolith composition.

pub mod configuration;

use std::{
    env, fs,
    io::{Read as _, Write as _},
    net::SocketAddr,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use clap::{ArgGroup, Args, Parser, Subcommand};
use configuration::{AppConfig, CliOverrides, ConfigLoadError};
use pangya_data::Catalog;
use pangya_domain::{
    HandoverRepository, MatchRepository, NewAccount, Nickname, PlayerRepository, RepositoryError,
    Username,
};
use pangya_game::{
    GameObserver, GameRuntimeConfig, GameRuntimeLimits, GameService, LobbyLimits, RoomActorLimits,
    SoloRuntimeConfig,
};
use pangya_login::{
    AdvertisedGameServer, BoundedCredentialExecutor, CanonicalTransportSecret, CredentialPolicy,
    LoginRuntimeConfig, LoginRuntimeLimits, LoginService,
};
use pangya_observability::{
    HealthState, LogFormat, M2Metrics, TracingError, install_tracing, serve_admin,
};
use pangya_protocol::CodecLimits;
use pangya_storage::{PgRepository, PgStorageConfig, StorageBootstrapError, migrate};
use thiserror::Error;
use tokio::{
    io::AsyncReadExt as _,
    net::TcpListener,
    task::JoinSet,
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

/// PangYa-RS command line. Secret content is deliberately absent.
#[derive(Debug, Parser)]
#[command(name = "pangya-server", version, about)]
pub struct Cli {
    /// Optional TOML configuration path.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    /// Explicitly acknowledges any configured non-loopback bind.
    #[arg(long, global = true)]
    pub acknowledge_public_bind: bool,
    /// Explicit profile override.
    #[arg(long, global = true)]
    pub profile: Option<String>,
    /// Explicit LoginService bind override.
    #[arg(long, global = true)]
    pub login_bind: Option<String>,
    /// Explicit admin HTTP bind override.
    #[arg(long, global = true)]
    pub http_bind: Option<String>,
    /// Action to execute.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level server commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Runs LoginService, optional synthetic M3 GameService, and read-only admin HTTP.
    Serve,
    /// Operator account management.
    Account {
        /// Account action.
        #[command(subcommand)]
        command: AccountCommand,
    },
}

/// Account operator commands.
#[derive(Debug, Subcommand)]
pub enum AccountCommand {
    /// Atomically creates an account aggregate.
    Create(AccountCreateArgs),
}

/// Nonsecret account-create arguments and one secret source selector.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("secret_source")
        .required(true)
        .multiple(false)
        .args(["secret_stdin", "secret_env", "secret_file"])
))]
pub struct AccountCreateArgs {
    /// Display username.
    #[arg(long)]
    pub username: String,
    /// Display nickname.
    #[arg(long)]
    pub nickname: String,
    /// Reads the canonical 32-hex client secret from standard input.
    #[arg(long)]
    pub secret_stdin: bool,
    /// Reads the secret from this environment variable name (not its value).
    #[arg(long, value_name = "ENV_NAME")]
    pub secret_env: Option<String>,
    /// Reads the secret from this mounted file.
    #[arg(long, value_name = "PATH")]
    pub secret_file: Option<PathBuf>,
}

/// Redacted typed process failure.
#[derive(Debug, Error)]
pub enum ServerError {
    /// Configuration failed.
    #[error(transparent)]
    Config(#[from] ConfigLoadError),
    /// Tracing failed.
    #[error(transparent)]
    Tracing(#[from] TracingError),
    /// Database bootstrap exhausted bounded retry.
    #[error("database connection or migration failed after bounded retries")]
    Database,
    /// Listener bind failed.
    #[error("required listener could not bind")]
    Bind,
    /// Credential executor policy failed.
    #[error("credential executor configuration is invalid")]
    Credential,
    /// Signal handler failed.
    #[error("shutdown signal handler failed")]
    Signal,
    /// Retry schedule exceeded checked hard bounds.
    #[error("database retry schedule is invalid")]
    InvalidRetry,
    /// A required runtime task exited unexpectedly.
    #[error("required runtime task exited")]
    Runtime,
    /// Secret source was unreadable/invalid; content is never echoed.
    #[error("account secret source is invalid")]
    Secret,
    /// Account input violated policy.
    #[error("account input is invalid")]
    AccountInput,
    /// Account repository operation failed.
    #[error("account creation failed")]
    AccountCreate,
    /// Audit persistence failed.
    #[error("operator audit persistence failed")]
    Audit,
    /// Bounded catalog load or starter cross-check failed.
    #[error("catalog loading or validation failed")]
    Data,
}

/// Executes a parsed command.
///
/// # Errors
/// Returns typed redacted startup, runtime, or operator failures.
pub async fn run(cli: Cli) -> Result<(), ServerError> {
    let overrides = CliOverrides {
        acknowledge_public_bind: cli.acknowledge_public_bind,
        profile: cli.profile,
        login_bind: cli.login_bind,
        http_bind: cli.http_bind,
    };
    let config = configuration::load(cli.config.as_deref(), &overrides)?;
    let format = match config.logging_format.as_str() {
        "json" => LogFormat::Json,
        _ => LogFormat::Pretty,
    };
    install_tracing(&config.logging_filter, format)?;
    match cli.command {
        Command::Serve => serve(config).await,
        Command::Account {
            command: AccountCommand::Create(args),
        } => account_create(config, args).await,
    }
}

async fn bind_after_startup_recovery<R: MatchRepository>(
    repository: &R,
    solo: Option<configuration::ValidatedSoloPractice>,
    login_bind: SocketAddr,
    http_bind: SocketAddr,
    game_bind: Option<SocketAddr>,
) -> Result<(TcpListener, TcpListener, Option<TcpListener>), ServerError> {
    if let Some(solo) = solo {
        timeout(
            solo.commit_timeout,
            repository.abort_incomplete_matches(solo.startup_recovery_limit),
        )
        .await
        .map_err(|_| ServerError::Runtime)?
        .map_err(|_| ServerError::Runtime)?;
    }
    let login = TcpListener::bind(login_bind)
        .await
        .map_err(|_| ServerError::Bind)?;
    let http = TcpListener::bind(http_bind)
        .await
        .map_err(|_| ServerError::Bind)?;
    let game = match game_bind {
        Some(address) => Some(
            TcpListener::bind(address)
                .await
                .map_err(|_| ServerError::Bind)?,
        ),
        None => None,
    };
    Ok((login, http, game))
}

fn resolve_solo_runtime_config(
    catalog: &Catalog,
    solo: Option<configuration::ValidatedSoloPractice>,
) -> Result<Option<SoloRuntimeConfig>, ServerError> {
    solo.map(|solo| {
        let course = catalog
            .one_hole_course(solo.course_id)
            .map_err(|_| ServerError::Data)?;
        Ok(SoloRuntimeConfig {
            course,
            catalog_fingerprint: catalog.fingerprint(),
            loading_timeout: solo.loading_timeout,
            commit_timeout: solo.commit_timeout,
            max_strokes: solo.max_strokes,
            startup_recovery_limit: solo.startup_recovery_limit,
            shot_packets_per_window: solo.shot_packets_per_window,
        })
    })
    .transpose()
}

fn compose_game_service<R>(
    repository: Arc<R>,
    catalog: Catalog,
    config: GameRuntimeConfig,
    observer: Arc<dyn GameObserver>,
) -> Result<Arc<GameService<R>>, ServerError>
where
    R: HandoverRepository + PlayerRepository + MatchRepository + 'static,
{
    GameService::new(repository, catalog, config, observer)
        .map(Arc::new)
        .map_err(|_| ServerError::Runtime)
}

async fn serve(config: AppConfig) -> Result<(), ServerError> {
    let pool = connect_and_migrate(&config).await?;
    let repository = Arc::new(PgRepository::new(pool.clone()));
    prepare_public_bind(&repository, config.public_bind_enabled).await?;
    let catalog = if config.game_enabled {
        let directory = config.iff_directory.clone().ok_or(ServerError::Data)?;
        let manifest = config.data_manifest.clone().ok_or(ServerError::Data)?;
        let loaded = run_detached_with_timeout(config.data_load_timeout, move || {
            Catalog::load(&directory, &manifest)
        })
        .await?
        .map_err(|_| ServerError::Data)?;
        loaded
            .validate_starter(&config.starter)
            .map_err(|_| ServerError::Data)?;
        Some(loaded)
    } else {
        None
    };
    let solo_practice = catalog
        .as_ref()
        .map(|catalog| resolve_solo_runtime_config(catalog, config.solo_practice))
        .transpose()?
        .flatten();
    let metrics = Arc::new(M2Metrics::default());
    let game = match catalog {
        Some(catalog) => Some(compose_game_service(
            Arc::clone(&repository),
            catalog,
            GameRuntimeConfig {
                channel_id: config.game_channel_id,
                unknown_opcode_policy: config.unknown_opcode_policy,
                limits: game_runtime_limits(&config)?,
                solo_practice,
            },
            metrics.clone(),
        )?),
        None => None,
    };
    let game_bind = if config.game_enabled {
        Some(config.game_bind.ok_or(ServerError::Bind)?)
    } else {
        None
    };
    let (login_listener, http_listener, game_listener) = bind_after_startup_recovery(
        repository.as_ref(),
        config.solo_practice,
        config.login_bind,
        config.http_bind,
        game_bind,
    )
    .await?;

    let health = Arc::new(HealthState::new(
        Arc::clone(&metrics),
        config.heartbeat_stale_after,
        config.metrics_enabled,
    ));
    health.set_game_required(config.game_enabled);
    health.set_config_valid(true);
    health.set_database_migrated(true);
    health.set_login_bound(true);
    health.set_catalog_loaded(game.is_some());
    health.set_game_bound(game_listener.is_some());

    let policy = Arc::new(CredentialPolicy::new().map_err(|_| ServerError::Credential)?);
    let credentials = BoundedCredentialExecutor::new(
        policy,
        config.credential_concurrency,
        config.credential_queue_timeout,
        config.credential_operation_timeout,
    )
    .map_err(|_| ServerError::Credential)?;
    let source = config.game_advertise;
    let game_ipv4 = match source.ip() {
        std::net::IpAddr::V4(address) => address.to_string(),
        std::net::IpAddr::V6(_) => {
            return Err(ServerError::Config(
                configuration::ValidationError {
                    issues: vec![configuration::ConfigIssue {
                        field: "game.advertise",
                        message: "must use IPv4",
                    }],
                }
                .into(),
            ));
        }
    };
    let login = Arc::new(
        LoginService::new(
            Arc::clone(&repository),
            credentials,
            LoginRuntimeConfig {
                auto_create_accounts: config.auto_create_accounts,
                starter: config.starter.clone(),
                allowed_character_types: config.allowed_character_type_ids.clone(),
                game_server: AdvertisedGameServer {
                    id: config.game_id,
                    name: config.game_name.clone(),
                    ipv4: game_ipv4,
                    port: source.port(),
                    capacity: config.game_capacity,
                },
                limits: runtime_limits(&config),
            },
            metrics.clone(),
        )
        .map_err(|_| ServerError::Runtime)?,
    );
    let shutdown = CancellationToken::new();
    let mut tasks = JoinSet::new();
    let login_shutdown = shutdown.child_token();
    tasks.spawn(async move {
        login
            .serve(login_listener, login_shutdown)
            .await
            .map_err(|_| ServerError::Runtime)
    });
    if let (Some(game), Some(listener)) = (game, game_listener) {
        let game_shutdown = shutdown.child_token();
        tasks.spawn(async move {
            game.serve(listener, game_shutdown)
                .await
                .map_err(|_| ServerError::Runtime)
        });
    }
    let http_shutdown = shutdown.child_token();
    let http_health = Arc::clone(&health);
    tasks.spawn(async move {
        serve_admin(http_listener, http_health, http_shutdown)
            .await
            .map_err(|_| ServerError::Runtime)
    });
    let heartbeat_shutdown = shutdown.child_token();
    let heartbeat_health = Arc::clone(&health);
    tasks.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                () = heartbeat_shutdown.cancelled() => return Ok(()),
                _ = interval.tick() => heartbeat_health.heartbeat(),
            }
        }
    });
    let probe_shutdown = shutdown.child_token();
    let probe_health = Arc::clone(&health);
    let probe_pool = pool.clone();
    let probe_interval = config.database_readiness_probe_interval;
    let probe_timeout = config.database_readiness_probe_timeout;
    tasks.spawn(async move {
        run_database_probe(
            probe_pool,
            probe_health,
            probe_shutdown,
            probe_interval,
            probe_timeout,
        )
        .await
    });

    let status = supervise_tasks(
        tasks,
        shutdown_signal(),
        Arc::clone(&health),
        shutdown,
        config.shutdown_grace,
    )
    .await;
    pool.close().await;
    status
}

async fn run_detached_with_timeout<T, F>(duration: Duration, operation: F) -> Result<T, ServerError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("pangya-catalog-load".to_owned())
        .spawn(move || {
            let _ = result_tx.send(operation());
        })
        .map_err(|_| ServerError::Data)?;
    timeout(duration, result_rx)
        .await
        .map_err(|_| ServerError::Data)?
        .map_err(|_| ServerError::Data)
}

/// Persists acknowledged public-bind audit before any public listener binds.
///
/// # Errors
/// Returns [`ServerError::Audit`] and prevents binding if durable audit fails.
pub async fn prepare_public_bind(
    repository: &PgRepository,
    enabled: bool,
) -> Result<(), ServerError> {
    if enabled {
        repository
            .record_operator_audit("public_bind_enabled", None, "success")
            .await
            .map_err(|_| ServerError::Audit)?;
        tracing::warn!(
            action = "public_bind_enabled",
            "acknowledged public listener mode enabled"
        );
    }
    Ok(())
}

const SUPERVISOR_CLEANUP_ALLOWANCE: Duration = Duration::from_millis(250);

/// Supervises required tasks and guarantees one readiness-first cleanup path.
///
/// # Errors
/// Returns the original signal error or required-task failure after bounded cleanup.
pub async fn supervise_tasks<F>(
    mut tasks: JoinSet<Result<(), ServerError>>,
    signal: F,
    health: Arc<HealthState>,
    shutdown: CancellationToken,
    grace: Duration,
) -> Result<(), ServerError>
where
    F: std::future::Future<Output = Result<(), ServerError>>,
{
    let cleanup_grace = grace
        .checked_add(SUPERVISOR_CLEANUP_ALLOWANCE)
        .map_or(grace, |duration| duration);
    tokio::pin!(signal);
    let mut status = tokio::select! {
        signal_status = &mut signal => signal_status,
        joined = tasks.join_next() => match joined {
            Some(Ok(Err(error))) => Err(error),
            Some(Ok(Ok(()))) | Some(Err(_)) | None => Err(ServerError::Runtime),
        },
    };
    health.begin_shutdown();
    health.set_login_bound(false);
    health.set_game_bound(false);
    health.set_catalog_loaded(false);
    health.set_database_migrated(false);
    shutdown.cancel();
    let drain = async {
        let mut first_failure = None;
        while let Some(joined) = tasks.join_next().await {
            let failure = required_task_failure(joined);
            if first_failure.is_none() {
                first_failure = failure;
            }
        }
        first_failure
    };
    match timeout(cleanup_grace, drain).await {
        Ok(cleanup_failure) => {
            if status.is_ok()
                && let Some(error) = cleanup_failure
            {
                status = Err(error);
            }
        }
        Err(_) => {
            tasks.abort_all();
            while let Some(joined) = tasks.join_next().await {
                let _ = required_task_failure(joined);
            }
            if status.is_ok() {
                status = Err(ServerError::Runtime);
            }
        }
    }
    status
}

fn required_task_failure(
    joined: Result<Result<(), ServerError>, tokio::task::JoinError>,
) -> Option<ServerError> {
    match joined {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(_) => Some(ServerError::Runtime),
    }
}

/// Continuously probes the migrated primary database until cancellation.
///
/// # Errors
/// This supervised task returns only if its timer machinery is cancelled normally.
pub async fn run_database_probe(
    pool: sqlx::PgPool,
    health: Arc<HealthState>,
    shutdown: CancellationToken,
    interval: Duration,
    probe_timeout: Duration,
) -> Result<(), ServerError> {
    run_readiness_probe(health, shutdown, interval, probe_timeout, move || {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar!(r#"SELECT 1::int4 AS "one!""#)
                .fetch_one(&pool)
                .await
                .is_ok()
        }
    })
    .await
}

/// Runs a cancellable readiness probe and reflects both failure and recovery.
///
/// # Errors
/// Returns only if the supervised timer task is cancelled normally.
pub async fn run_readiness_probe<F, Fut>(
    health: Arc<HealthState>,
    shutdown: CancellationToken,
    interval: Duration,
    probe_timeout: Duration,
    mut probe: F,
) -> Result<(), ServerError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let mut ticker = tokio::time::interval(interval);
    loop {
        tokio::select! {
            () = shutdown.cancelled() => return Ok(()),
            _ = ticker.tick() => {
                let ready = matches!(timeout(probe_timeout, probe()).await, Ok(true));
                health.set_database_migrated(ready);
            }
        }
    }
}

fn runtime_limits(config: &AppConfig) -> LoginRuntimeLimits {
    let (global_connections, _) =
        partition_usize(config.security.global_connections, config.game_enabled);
    let (global_accepts_per_window, _) = partition_u32(
        config.security.global_accepts_per_window,
        config.game_enabled,
    );
    let (global_logins_per_window, _) = partition_u32(
        config.security.global_logins_per_window,
        config.game_enabled,
    );
    let (global_packets_per_window, _) = partition_u32(
        config.security.global_packets_per_window,
        config.game_enabled,
    );
    let (global_bytes_per_window, _) =
        partition_u64(config.security.global_bytes_per_window, config.game_enabled);
    LoginRuntimeLimits {
        global_connections,
        connections_per_source: config
            .security
            .connections_per_source
            .min(global_connections),
        source_capacity: config.security.source_capacity,
        global_accepts_per_window,
        accepts_per_window: config.security.accepts_per_window,
        global_logins_per_window,
        logins_per_window: config.security.logins_per_window,
        username_logins_per_window: config.security.username_logins_per_window,
        rate_window: config.security.rate_window,
        global_packets_per_window,
        global_bytes_per_window,
        source_packets_per_window: config.security.source_packets_per_window,
        source_bytes_per_window: config.security.source_bytes_per_window,
        packets_per_window: config.security.packets_per_window,
        bytes_per_window: config.security.bytes_per_window,
        malformed_strike_cap: config.security.malformed_strike_cap,
        max_retries: config.security.max_retries,
        login_timeout: config.security.login_timeout,
        idle_timeout: config.security.idle_timeout,
        shutdown_grace: config.shutdown_grace,
        codec: CodecLimits {
            max_client_frame_bytes: config.max_client_frame_bytes,
            max_server_plaintext_bytes: config.max_plaintext_bytes,
            max_expansion_ratio: config.max_expansion_ratio,
        },
    }
}

fn game_runtime_limits(config: &AppConfig) -> Result<GameRuntimeLimits, ServerError> {
    let (_, global_connections) =
        partition_usize(config.security.global_connections, config.game_enabled);
    let (_, global_accepts_per_window) = partition_u32(
        config.security.global_accepts_per_window,
        config.game_enabled,
    );
    let (_, global_auth_per_window) = partition_u32(
        config.security.global_logins_per_window,
        config.game_enabled,
    );
    let (_, global_packets_per_window) = partition_u32(
        config.security.global_packets_per_window,
        config.game_enabled,
    );
    let (_, global_bytes_per_window) =
        partition_u64(config.security.global_bytes_per_window, config.game_enabled);
    let nonzero = |value| NonZeroUsize::new(value).ok_or(ServerError::Runtime);
    let room = RoomActorLimits::new(
        nonzero(config.game_room_normal_capacity)?,
        nonzero(config.game_room_control_capacity)?,
        config.game_command_timeout,
    );
    let lobby = LobbyLimits::new(
        nonzero(config.game_max_rooms)?,
        nonzero(config.game_lobby_command_capacity)?,
        nonzero(global_connections.saturating_add(1))?,
        nonzero(config.game_lobby_event_capacity)?,
        config.game_command_timeout,
        config.shutdown_grace,
        room,
    );
    Ok(GameRuntimeLimits {
        global_connections,
        connections_per_source: config
            .security
            .connections_per_source
            .min(global_connections),
        source_capacity: config.security.source_capacity,
        global_accepts_per_window,
        accepts_per_window: config.security.accepts_per_window,
        global_auth_per_window,
        auth_per_window: config.security.logins_per_window,
        global_packets_per_window,
        global_bytes_per_window,
        source_packets_per_window: config.security.source_packets_per_window,
        source_bytes_per_window: config.security.source_bytes_per_window,
        packets_per_window: config.security.packets_per_window,
        bytes_per_window: config.security.bytes_per_window,
        room_commands_per_window: config.game_room_commands_per_window,
        chat_messages_per_window: config.game_chat_messages_per_window,
        unknown_opcode_strikes: config.game_unknown_opcode_strikes,
        unknown_capture_capacity: config.game_unknown_capture_capacity,
        outbound_room_event_capacity: config.game_outbound_room_event_capacity,
        lobby,
        rate_window: config.security.rate_window,
        authentication_timeout: config.security.login_timeout,
        idle_timeout: config.security.idle_timeout,
        command_timeout: config.game_command_timeout,
        shutdown_grace: config.shutdown_grace,
        codec: CodecLimits {
            max_client_frame_bytes: config.max_client_frame_bytes,
            max_server_plaintext_bytes: config.max_plaintext_bytes,
            max_expansion_ratio: config.max_expansion_ratio,
        },
    })
}

fn partition_usize(total: usize, game_enabled: bool) -> (usize, usize) {
    if game_enabled {
        let game = total / 2;
        (total - game, game)
    } else {
        (total, 0)
    }
}

fn partition_u32(total: u32, game_enabled: bool) -> (u32, u32) {
    if game_enabled {
        let game = total / 2;
        (total - game, game)
    } else {
        (total, 0)
    }
}

fn partition_u64(total: u64, game_enabled: bool) -> (u64, u64) {
    if game_enabled {
        let game = total / 2;
        (total - game, game)
    } else {
        (total, 0)
    }
}

/// Returns checked bounded exponential delays between connection attempts.
///
/// # Errors
/// Rejects zero or more than 32 attempts, zero durations, or an inverted cap.
pub fn retry_schedule(
    attempts: u32,
    initial: Duration,
    maximum: Duration,
) -> Result<Vec<Duration>, ServerError> {
    if attempts == 0 || attempts > 32 || initial.is_zero() || initial > maximum {
        return Err(ServerError::InvalidRetry);
    }
    let capacity = usize::try_from(attempts - 1).map_err(|_| ServerError::InvalidRetry)?;
    let mut delays = Vec::with_capacity(capacity);
    let mut delay = initial;
    for _ in 1..attempts {
        delays.push(delay);
        delay = match delay.checked_mul(2) {
            Some(next) => next.min(maximum),
            None => maximum,
        };
    }
    Ok(delays)
}

async fn connect_and_migrate(config: &AppConfig) -> Result<sqlx::PgPool, ServerError> {
    let schedule = retry_schedule(
        config.database_connect_attempts,
        config.database_retry_initial,
        config.database_retry_max,
    )?;
    for attempt in 0..config.database_connect_attempts {
        let mut storage = PgStorageConfig::new(config.database_url.expose());
        storage.max_connections = config.database_max_connections;
        storage.min_connections = config.database_min_connections;
        storage.acquire_timeout = config.database_acquire_timeout;
        match storage.connect().await {
            Ok(pool) => match migrate(&pool).await {
                Ok(()) => return Ok(pool),
                Err(_) => pool.close().await,
            },
            Err(StorageBootstrapError::InvalidConfig) => return Err(ServerError::Database),
            Err(_) => {}
        }
        if let Some(delay) = schedule.get(attempt as usize) {
            sleep(*delay).await;
        }
    }
    Err(ServerError::Database)
}

async fn account_create(config: AppConfig, args: AccountCreateArgs) -> Result<(), ServerError> {
    let secret_text = read_secret(&args).await?;
    let secret =
        CanonicalTransportSecret::parse(secret_text.trim()).map_err(|_| ServerError::Secret)?;
    let username = Username::parse(&args.username).map_err(|_| ServerError::AccountInput)?;
    let nickname = Nickname::parse(&args.nickname).map_err(|_| ServerError::AccountInput)?;
    let policy = Arc::new(CredentialPolicy::new().map_err(|_| ServerError::Credential)?);
    let executor = BoundedCredentialExecutor::new(
        policy,
        config.credential_concurrency,
        config.credential_queue_timeout,
        config.credential_operation_timeout,
    )
    .map_err(|_| ServerError::Credential)?;
    let hash = executor
        .hash(secret)
        .await
        .map_err(|_| ServerError::Credential)?;
    let pool = connect_and_migrate(&config).await?;
    let repository = PgRepository::new(pool.clone());
    let created = repository
        .create_operator_account(NewAccount {
            username,
            credential_hash: hash,
            nickname: Some(nickname),
            starter: config.starter,
        })
        .await;
    match created {
        Ok(aggregate) => {
            tracing::info!(
                action = "account_create",
                account_id = aggregate.account.id.get(),
                outcome = "success",
                "operator audit"
            );
            let mut stdout = std::io::stdout().lock();
            writeln!(
                stdout,
                "account created: id={} status=active",
                aggregate.account.id.get()
            )
            .map_err(|_| ServerError::AccountCreate)?;
        }
        Err(error) => {
            let outcome = if error == RepositoryError::DuplicateUsername {
                "duplicate"
            } else {
                "failed"
            };
            repository
                .record_operator_audit("account_create", None, outcome)
                .await
                .map_err(|_| ServerError::Audit)?;
            pool.close().await;
            return Err(ServerError::AccountCreate);
        }
    }
    pool.close().await;
    Ok(())
}

async fn read_secret(args: &AccountCreateArgs) -> Result<Zeroizing<String>, ServerError> {
    if args.secret_stdin {
        let mut bytes = Zeroizing::new(Vec::with_capacity(129));
        tokio::io::stdin()
            .take(129)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| ServerError::Secret)?;
        return validated_secret_text(&bytes);
    }
    if let Some(name) = &args.secret_env {
        return env::var(name)
            .map(Zeroizing::new)
            .map_err(|_| ServerError::Secret);
    }
    if let Some(path) = &args.secret_file {
        return read_bounded_secret_file(path);
    }
    Err(ServerError::Secret)
}

fn read_bounded_secret_file(path: &Path) -> Result<Zeroizing<String>, ServerError> {
    let file = fs::File::open(path).map_err(|_| ServerError::Secret)?;
    let mut bytes = Zeroizing::new(Vec::with_capacity(129));
    file.take(129)
        .read_to_end(&mut bytes)
        .map_err(|_| ServerError::Secret)?;
    validated_secret_text(&bytes)
}

fn validated_secret_text(bytes: &[u8]) -> Result<Zeroizing<String>, ServerError> {
    if bytes.len() > 128 {
        return Err(ServerError::Secret);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ServerError::Secret)?;
    Ok(Zeroizing::new(text.to_owned()))
}

#[cfg(unix)]
async fn shutdown_signal() -> Result<(), ServerError> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| ServerError::Signal)?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result.map_err(|_| ServerError::Signal),
        _ = terminate.recv() => Ok(()),
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() -> Result<(), ServerError> {
    tokio::signal::ctrl_c()
        .await
        .map_err(|_| ServerError::Signal)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use pangya_domain::{
        AbortMatch, AbortMatchOutcome, AccountId, AuthenticatedSession, BeginSoloMatch,
        BeginSoloMatchOutcome, CatalogFingerprint, CommitSoloHole, ConsumeHandover, CourseId,
        HandoverError, HandoverRepository, IncompleteMatchAbortLimit, MarkSoloInGame,
        MarkSoloInGameOutcome, MatchRepositoryError, NewHandover, PlayerRepository, PlayerSnapshot,
        RepositoryError, RepositoryFuture, SoloMatchResult,
    };
    use tokio::{net::TcpStream, sync::Notify};

    use super::*;

    struct RecoveryGate {
        calls: AtomicUsize,
        started: Notify,
        release: Notify,
    }

    impl RecoveryGate {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                started: Notify::new(),
                release: Notify::new(),
            }
        }
    }

    impl HandoverRepository for RecoveryGate {
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

    impl PlayerRepository for RecoveryGate {
        fn load_player_snapshot(
            &self,
            _account_id: AccountId,
        ) -> RepositoryFuture<'_, Result<PlayerSnapshot, RepositoryError>> {
            Box::pin(async { Err(RepositoryError::Storage) })
        }
    }

    impl MatchRepository for RecoveryGate {
        fn begin_solo(
            &self,
            _request: BeginSoloMatch,
        ) -> RepositoryFuture<'_, Result<BeginSoloMatchOutcome, MatchRepositoryError>> {
            Box::pin(async { Err(MatchRepositoryError::Storage) })
        }

        fn mark_solo_in_game(
            &self,
            _request: MarkSoloInGame,
        ) -> RepositoryFuture<'_, Result<MarkSoloInGameOutcome, MatchRepositoryError>> {
            Box::pin(async { Err(MatchRepositoryError::Storage) })
        }

        fn abort(
            &self,
            _request: AbortMatch,
        ) -> RepositoryFuture<'_, Result<AbortMatchOutcome, MatchRepositoryError>> {
            Box::pin(async { Err(MatchRepositoryError::Storage) })
        }

        fn commit_solo_hole(
            &self,
            _request: CommitSoloHole,
        ) -> RepositoryFuture<'_, Result<SoloMatchResult, MatchRepositoryError>> {
            Box::pin(async { Err(MatchRepositoryError::Storage) })
        }

        fn abort_incomplete_matches(
            &self,
            _limit: IncompleteMatchAbortLimit,
        ) -> RepositoryFuture<'_, Result<u32, MatchRepositoryError>> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.started.notify_one();
            Box::pin(async {
                self.release.notified().await;
                Ok(0)
            })
        }
    }

    #[test]
    fn invalid_solo_course_and_fingerprint_fail_before_listener_bind() {
        let login_reservation = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve login");
        let http_reservation = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve http");
        let game_reservation = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve game");
        let addresses = [
            login_reservation.local_addr().expect("login address"),
            http_reservation.local_addr().expect("http address"),
            game_reservation.local_addr().expect("game address"),
        ];
        drop((login_reservation, http_reservation, game_reservation));

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../pangya-data/tests/fixtures/synthetic-catalog");
        let catalog = Catalog::load(&root, Path::new("manifest.toml")).expect("catalog");
        let invalid_course = configuration::ValidatedSoloPractice {
            course_id: CourseId::new(u32::MAX).expect("course"),
            loading_timeout: Duration::from_secs(5),
            commit_timeout: Duration::from_secs(1),
            max_strokes: 9,
            startup_recovery_limit: IncompleteMatchAbortLimit::new(10).expect("limit"),
            shot_packets_per_window: 80,
        };
        assert!(matches!(
            resolve_solo_runtime_config(&catalog, Some(invalid_course)),
            Err(ServerError::Data)
        ));

        let valid = resolve_solo_runtime_config(
            &catalog,
            Some(configuration::ValidatedSoloPractice {
                course_id: CourseId::new(7).expect("course"),
                ..invalid_course
            }),
        )
        .expect("resolve")
        .expect("solo");
        let drifted = SoloRuntimeConfig {
            catalog_fingerprint: CatalogFingerprint::new([0; 32]),
            ..valid
        };
        assert!(matches!(
            compose_game_service(
                Arc::new(RecoveryGate::new()),
                catalog,
                GameRuntimeConfig {
                    solo_practice: Some(drifted),
                    ..GameRuntimeConfig::default()
                },
                Arc::new(M2Metrics::default()),
            ),
            Err(ServerError::Runtime)
        ));
        let rebound = addresses.map(|address| {
            std::net::TcpListener::bind(address).expect("startup failure left port bindable")
        });
        drop(rebound);
    }

    #[tokio::test]
    async fn startup_recovery_completes_before_any_listener_binds() {
        let login_reservation = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve login");
        let http_reservation = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve http");
        let game_reservation = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve game");
        let login = login_reservation.local_addr().expect("login address");
        let http = http_reservation.local_addr().expect("http address");
        let game = game_reservation.local_addr().expect("game address");
        drop((login_reservation, http_reservation, game_reservation));

        let repository = Arc::new(RecoveryGate::new());
        let task_repository = Arc::clone(&repository);
        let startup = tokio::spawn(async move {
            bind_after_startup_recovery(
                task_repository.as_ref(),
                Some(configuration::ValidatedSoloPractice {
                    course_id: CourseId::new(7).expect("course"),
                    loading_timeout: Duration::from_secs(5),
                    commit_timeout: Duration::from_secs(1),
                    max_strokes: 9,
                    startup_recovery_limit: IncompleteMatchAbortLimit::new(10).expect("limit"),
                    shot_packets_per_window: 80,
                }),
                login,
                http,
                Some(game),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(1), repository.started.notified())
            .await
            .expect("recovery started");
        assert_eq!(repository.calls.load(Ordering::Relaxed), 1);
        for address in [login, http, game] {
            assert!(TcpStream::connect(address).await.is_err());
        }

        repository.release.notify_one();
        let (login_listener, http_listener, game_listener) = startup
            .await
            .expect("startup join")
            .expect("startup listeners");
        assert!(TcpStream::connect(login).await.is_ok());
        assert!(TcpStream::connect(http).await.is_ok());
        assert!(TcpStream::connect(game).await.is_ok());
        drop((login_listener, http_listener, game_listener));
    }

    #[test]
    fn enabled_game_partitions_every_process_total_without_inflation() {
        let connections = partition_usize(5, true);
        let accepts = partition_u32(7, true);
        let auth = partition_u32(9, true);
        let packets = partition_u32(11, true);
        let bytes = partition_u64(13, true);
        assert_eq!(connections, (3, 2));
        assert_eq!(accepts.0 + accepts.1, 7);
        assert_eq!(auth.0 + auth.1, 9);
        assert_eq!(packets.0 + packets.1, 11);
        assert_eq!(bytes.0 + bytes.1, 13);
    }

    #[test]
    fn disabled_game_leaves_every_process_total_with_login() {
        assert_eq!(partition_usize(5, false), (5, 0));
        assert_eq!(partition_u32(7, false), (7, 0));
        assert_eq!(partition_u64(9, false), (9, 0));
    }

    #[test]
    fn timed_out_detached_catalog_work_does_not_delay_runtime_drop() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let started = std::time::Instant::now();
        let result = runtime.block_on(run_detached_with_timeout(Duration::from_millis(10), || {
            std::thread::sleep(Duration::from_millis(250));
            true
        }));
        assert!(matches!(result, Err(ServerError::Data)));
        drop(runtime);
        assert!(started.elapsed() < Duration::from_millis(150));
    }
}
