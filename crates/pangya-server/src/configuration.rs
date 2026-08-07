//! Layered, validated, and redacted server configuration.

use std::{
    collections::HashSet,
    env, fmt, fs,
    io::Read as _,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    time::Duration,
};

use config::{Config, Environment, File};
use pangya_domain::{
    CourseId, IncompleteMatchAbortLimit, ItemTypeId, MAX_STARTER_ITEMS, StarterCharacter,
    StarterGrant, StarterItem, StarterKey,
};
use pangya_game::UnknownOpcodePolicy;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Explicit CLI overrides applied after defaults, file, and environment.
#[derive(Clone, Debug, Default)]
pub struct CliOverrides {
    /// Acknowledges non-loopback binds.
    pub acknowledge_public_bind: bool,
    /// Optional profile override.
    pub profile: Option<String>,
    /// Optional LoginService bind override.
    pub login_bind: Option<String>,
    /// Optional admin HTTP bind override.
    pub http_bind: Option<String>,
}

/// One nonsecret configuration validation finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigIssue {
    /// Stable field path.
    pub field: &'static str,
    /// Actionable nonsecret description.
    pub message: &'static str,
}

/// Aggregated configuration validation error.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("configuration has {count} validation error(s)", count = .issues.len())]
pub struct ValidationError {
    /// Every independently detected validation failure.
    pub issues: Vec<ConfigIssue>,
}

/// Layering, secret resolution, or validation failure.
#[derive(Debug, Error)]
pub enum ConfigLoadError {
    /// Config provider/deserialization failed.
    #[error("configuration could not be loaded")]
    Load,
    /// Secret file could not be read.
    #[error("database secret file could not be read")]
    SecretFile,
    /// One or more validated invariants failed.
    #[error(transparent)]
    Validation(#[from] ValidationError),
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct RawConfig {
    server: ServerSection,
    login: LoginSection,
    game: GameSection,
    http: HttpSection,
    database: DatabaseSection,
    protocol: ProtocolSection,
    logging: LoggingSection,
    security: SecuritySection,
    starter: StarterSection,
    data: DataSection,
}

macro_rules! section_default {
    ($name:ident { $($field:ident : $ty:ty = $value:expr),+ $(,)? }) => {
        #[derive(Clone, Debug, Deserialize, Serialize)]
        #[serde(default)]
        struct $name { $(pub $field: $ty),+ }
        impl Default for $name {
            fn default() -> Self { Self { $($field: $value),+ } }
        }
    };
}

section_default!(ServerSection {
    profile: String = "local".to_owned(),
    shutdown_grace: String = "10s".to_owned()
});
section_default!(LoginSection {
    bind: String = "127.0.0.1:10103".to_owned(),
    advertise: String = "127.0.0.1:10103".to_owned(),
    client_profile: String = "us_852".to_owned(),
    auto_create_accounts: bool = false
});
section_default!(SoloPracticeSection {
    enabled: bool = false,
    course_id: u32 = 7,
    loading_timeout: String = "30s".to_owned(),
    commit_timeout: String = "3s".to_owned(),
    max_strokes: u8 = 30,
    startup_recovery_limit: u32 = 1_000,
    shot_packets_per_window: u32 = 120
});
section_default!(StrokeTwoSection {
    enabled: bool = false,
    course_id: u32 = 7,
    loading_timeout: String = "30s".to_owned(),
    turn_timeout: String = "30s".to_owned(),
    game_timeout: String = "10m".to_owned(),
    commit_timeout: String = "3s".to_owned(),
    max_strokes: u8 = 30,
    startup_recovery_limit: u32 = 1_000,
    shot_packets_per_window: u32 = 120
});
section_default!(EconomySection {
    enabled: bool = false,
    command_timeout: String = "3s".to_owned(),
    commands_per_window: u32 = 30,
    page_size: usize = 50,
    max_purchase_quantity: u32 = 99
});
section_default!(GameSection {
    enabled: bool = false,
    bind: String = "127.0.0.1:20201".to_owned(),
    advertise: String = "127.0.0.1:20201".to_owned(),
    id: u16 = 1,
    name: String = "PangYa-RS Local".to_owned(),
    capacity: u32 = 200,
    channel_id: u32 = 1,
    max_rooms: usize = 1_024,
    lobby_command_capacity: usize = 256,
    lobby_event_capacity: usize = 256,
    room_normal_capacity: usize = 64,
    room_control_capacity: usize = 16,
    outbound_room_event_capacity: usize = 64,
    room_commands_per_window: u32 = 30,
    chat_messages_per_window: u32 = 10,
    unknown_opcode_strikes: u32 = 3,
    unknown_capture_capacity: usize = 256,
    command_timeout: String = "3s".to_owned(),
    solo_practice: SoloPracticeSection = SoloPracticeSection::default(),
    stroke_two: StrokeTwoSection = StrokeTwoSection::default(),
    economy: EconomySection = EconomySection::default()
});
section_default!(HttpSection {
    bind: String = "127.0.0.1:8080".to_owned(),
    metrics: bool = true,
    heartbeat_stale_after: String = "5s".to_owned()
});
section_default!(DatabaseSection {
    url_env: String = "DATABASE_URL".to_owned(),
    secret_file: Option<PathBuf> = None,
    max_connections: u32 = 10,
    min_connections: u32 = 1,
    acquire_timeout: String = "5s".to_owned(),
    connect_attempts: u32 = 5,
    retry_initial: String = "100ms".to_owned(),
    retry_max: String = "2s".to_owned(),
    readiness_probe_interval: String = "1s".to_owned(),
    readiness_probe_timeout: String = "2s".to_owned()
});
section_default!(ProtocolSection {
    max_client_frame_bytes: usize = 65_535,
    max_plaintext_bytes: usize = 8 * 1024 * 1024,
    max_expansion_ratio: usize = 128,
    unknown_opcode_policy: String = "disconnect".to_owned()
});
section_default!(LoggingSection {
    filter: String = "info".to_owned(),
    format: String = "pretty".to_owned(),
    packet_bodies: bool = false
});
section_default!(SecuritySection {
    credential_concurrency: usize = 2,
    credential_queue_timeout: String = "250ms".to_owned(),
    credential_operation_timeout: String = "5s".to_owned(),
    global_connections: usize = 256,
    connections_per_source: usize = 8,
    source_capacity: usize = 1_024,
    global_accepts_per_window: u32 = 1_000,
    accepts_per_window: u32 = 30,
    global_logins_per_window: u32 = 1_000,
    logins_per_window: u32 = 10,
    username_logins_per_window: u32 = 10,
    rate_window: String = "60s".to_owned(),
    global_packets_per_window: u32 = 10_000,
    global_bytes_per_window: u64 = 16_777_216,
    source_packets_per_window: u32 = 600,
    source_bytes_per_window: u64 = 2_097_152,
    packets_per_window: u32 = 60,
    bytes_per_window: u64 = 262_144,
    malformed_strike_cap: u8 = 1,
    max_retries: u8 = 3,
    login_timeout: String = "15s".to_owned(),
    idle_timeout: String = "120s".to_owned()
});

const MAX_ALLOWED_CHARACTER_TYPES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
struct StarterSection {
    character_key: String,
    character_type_id: u32,
    allowed_character_type_ids: Vec<u32>,
    items: Vec<StarterItemSection>,
    equipped_club_key: Option<String>,
    equipped_ball_key: Option<String>,
}
impl Default for StarterSection {
    fn default() -> Self {
        Self {
            character_key: "starter_character".to_owned(),
            character_type_id: 0x0400_0000,
            allowed_character_type_ids: vec![0x0400_0000],
            items: vec![StarterItemSection {
                key: "starter_club".to_owned(),
                item_type_id: 0x1000_0000,
                quantity: 1,
            }],
            equipped_club_key: Some("starter_club".to_owned()),
            equipped_ball_key: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StarterItemSection {
    key: String,
    item_type_id: u32,
    quantity: u32,
}

section_default!(DataSection {
    iff_directory: Option<PathBuf> = None,
    manifest: Option<PathBuf> = None,
    catalog_required_m3: bool = false,
    load_timeout: String = "5s".to_owned()
});

/// Secret database URL whose formatting is always redacted.
#[derive(Clone)]
pub struct DatabaseUrl(String);
impl DatabaseUrl {
    /// Exposes the URL only to the PostgreSQL connection boundary.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for DatabaseUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseUrl([REDACTED])")
    }
}

/// Validated application configuration.
#[derive(Clone)]
pub struct AppConfig {
    /// Runtime profile (`local` is the only profile permitting auto-create).
    pub profile: String,
    /// Process shutdown bound.
    pub shutdown_grace: Duration,
    /// Login listener.
    pub login_bind: SocketAddr,
    /// Login advertised endpoint (modeled for SPEC completeness).
    pub login_advertise: SocketAddr,
    /// Whether local missing accounts may be atomically created.
    pub auto_create_accounts: bool,
    /// Whether an explicitly acknowledged public bind requires durable audit.
    pub public_bind_enabled: bool,
    /// Enables the local synthetic M3 GameService slice.
    pub game_enabled: bool,
    /// GameService listener, validated only when GameService is enabled.
    pub game_bind: Option<SocketAddr>,
    /// IPv4 GameService endpoint advertised by LoginService.
    pub game_advertise: SocketAddr,
    /// Game server ID.
    pub game_id: u16,
    /// Game server display name.
    pub game_name: String,
    /// Game server capacity.
    pub game_capacity: u32,
    /// Sole synthetic channel ID.
    pub game_channel_id: u32,
    /// Maximum concurrently registered rooms.
    pub game_max_rooms: usize,
    /// Lobby command queue capacity.
    pub game_lobby_command_capacity: usize,
    /// Lobby room-event queue capacity.
    pub game_lobby_event_capacity: usize,
    /// Normal room command queue capacity.
    pub game_room_normal_capacity: usize,
    /// Room control queue capacity.
    pub game_room_control_capacity: usize,
    /// Per-connection outbound room-event queue capacity.
    pub game_outbound_room_event_capacity: usize,
    /// Per-connection room command budget.
    pub game_room_commands_per_window: u32,
    /// Per-connection chat message budget.
    pub game_chat_messages_per_window: u32,
    /// Unknown opcode strike limit.
    pub game_unknown_opcode_strikes: u32,
    /// Process-local metadata-and-digest capture capacity.
    pub game_unknown_capture_capacity: usize,
    /// Individual lobby command and cleanup deadline.
    pub game_command_timeout: Duration,
    /// Post-channel policy for truly unknown opcodes.
    pub unknown_opcode_policy: UnknownOpcodePolicy,
    /// Optional validated local-only solo-practice policy.
    pub solo_practice: Option<ValidatedSoloPractice>,
    /// Optional validated local-only exactly-two stroke policy.
    pub stroke_two: Option<ValidatedStrokeTwo>,
    /// Optional validated local-only synthetic economy policy.
    pub economy: Option<ValidatedEconomy>,
    /// Admin HTTP listener.
    pub http_bind: SocketAddr,
    /// Enables read-only metrics exposition.
    pub metrics_enabled: bool,
    /// Event-loop heartbeat stale threshold.
    pub heartbeat_stale_after: Duration,
    /// Secret DB URL.
    pub database_url: DatabaseUrl,
    /// Pool maximum.
    pub database_max_connections: u32,
    /// Pool minimum.
    pub database_min_connections: u32,
    /// Pool acquisition bound.
    pub database_acquire_timeout: Duration,
    /// Connect/migration attempts.
    pub database_connect_attempts: u32,
    /// Initial exponential retry delay.
    pub database_retry_initial: Duration,
    /// Maximum exponential retry delay.
    pub database_retry_max: Duration,
    /// Continuous database readiness probe interval.
    pub database_readiness_probe_interval: Duration,
    /// Per-probe timeout, including pool acquisition and query.
    pub database_readiness_probe_timeout: Duration,
    /// Client encrypted-frame bound.
    pub max_client_frame_bytes: usize,
    /// Server plaintext bound.
    pub max_plaintext_bytes: usize,
    /// Expansion ratio bound.
    pub max_expansion_ratio: usize,
    /// Logging filter.
    pub logging_filter: String,
    /// Logging format.
    pub logging_format: String,
    /// Raw packet bodies remain disabled for M2.
    pub packet_bodies: bool,
    /// Credential blocking concurrency.
    pub credential_concurrency: usize,
    /// Credential worker queue bound.
    pub credential_queue_timeout: Duration,
    /// Credential operation timeout.
    pub credential_operation_timeout: Duration,
    /// Runtime security limits.
    pub security: ValidatedSecurity,
    /// Starter aggregate.
    pub starter: StarterGrant,
    /// Provisional allowlisted character IDs.
    pub allowed_character_type_ids: Vec<u32>,
    /// Operator-mounted IFF directory required only when GameService is enabled.
    pub iff_directory: Option<PathBuf>,
    /// Versioned catalog manifest path relative to the IFF directory.
    pub data_manifest: Option<PathBuf>,
    /// Bounded blocking catalog load duration.
    pub data_load_timeout: Duration,
}

/// Validated local-only synthetic solo-practice policy.
#[derive(Clone, Copy, Debug)]
pub struct ValidatedSoloPractice {
    /// Catalog course to resolve after loading.
    pub course_id: CourseId,
    /// Actor loading deadline.
    pub loading_timeout: Duration,
    /// Repository deadline.
    pub commit_timeout: Duration,
    /// Authoritative stroke cap.
    pub max_strokes: u8,
    /// Startup recovery cap.
    pub startup_recovery_limit: IncompleteMatchAbortLimit,
    /// Per-connection shot packet budget.
    pub shot_packets_per_window: u32,
}

/// Validated local-only synthetic economy policy.
#[derive(Clone, Copy, Debug)]
pub struct ValidatedEconomy {
    /// Repository command deadline.
    pub command_timeout: Duration,
    /// Per-connection command budget.
    pub commands_per_window: u32,
    /// Offers per page.
    pub page_size: usize,
    /// Maximum purchase quantity.
    pub max_purchase_quantity: u32,
}

/// Validated local-only synthetic exactly-two stroke policy.
#[derive(Clone, Copy, Debug)]
pub struct ValidatedStrokeTwo {
    /// Catalog course to resolve after loading.
    pub course_id: CourseId,
    /// Actor loading barrier deadline.
    pub loading_timeout: Duration,
    /// Actor active-turn deadline.
    pub turn_timeout: Duration,
    /// Actor whole-game deadline.
    pub game_timeout: Duration,
    /// Repository deadline.
    pub commit_timeout: Duration,
    /// Authoritative per-player stroke cap.
    pub max_strokes: u8,
    /// Startup recovery cap.
    pub startup_recovery_limit: IncompleteMatchAbortLimit,
    /// Per-connection shot packet budget.
    pub shot_packets_per_window: u32,
}

/// Validated security limits.
#[derive(Clone, Debug)]
pub struct ValidatedSecurity {
    /// Total concurrent connections.
    pub global_connections: usize,
    /// Concurrent connections per masked source.
    pub connections_per_source: usize,
    /// Bounded source/username key capacity.
    pub source_capacity: usize,
    /// Global accept rate.
    pub global_accepts_per_window: u32,
    /// Per-source accept rate.
    pub accepts_per_window: u32,
    /// Global login rate.
    pub global_logins_per_window: u32,
    /// Per-source login rate.
    pub logins_per_window: u32,
    /// Per-normalized-username login rate.
    pub username_logins_per_window: u32,
    /// Rate interval.
    pub rate_window: Duration,
    /// Global packet rate.
    pub global_packets_per_window: u32,
    /// Global plaintext byte rate.
    pub global_bytes_per_window: u64,
    /// Per-source packet rate.
    pub source_packets_per_window: u32,
    /// Per-source plaintext byte rate.
    pub source_bytes_per_window: u64,
    /// Per-connection packet rate.
    pub packets_per_window: u32,
    /// Byte rate.
    pub bytes_per_window: u64,
    /// Malformed strike cap (M2 closes on first strike).
    pub malformed_strike_cap: u8,
    /// Friendly retry bound.
    pub max_retries: u8,
    /// Login deadline.
    pub login_timeout: Duration,
    /// Idle deadline.
    pub idle_timeout: Duration,
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppConfig")
            .field("profile", &self.profile)
            .field("login_bind", &self.login_bind)
            .field("game_advertise", &self.game_advertise)
            .field("http_bind", &self.http_bind)
            .field("database_url", &self.database_url)
            .field("secret_file", &"[REDACTED]")
            .field("packet_bodies", &self.packet_bodies)
            .finish_non_exhaustive()
    }
}

/// Loads defaults, optional TOML, `PANGYA__...`, then explicit CLI overrides.
///
/// # Errors
/// Returns a redacted provider error or all validation findings together.
pub fn load(path: Option<&Path>, overrides: &CliOverrides) -> Result<AppConfig, ConfigLoadError> {
    load_with_secret_resolver(path, overrides, |name| env::var(name).ok())
}

fn load_with_secret_resolver<F>(
    path: Option<&Path>,
    overrides: &CliOverrides,
    resolve_secret: F,
) -> Result<AppConfig, ConfigLoadError>
where
    F: FnOnce(&str) -> Option<String>,
{
    let mut builder = Config::builder()
        .add_source(Config::try_from(&RawConfig::default()).map_err(|_| ConfigLoadError::Load)?);
    if let Some(path) = path {
        builder = builder.add_source(File::from(path).required(true));
    }
    builder = builder.add_source(
        Environment::with_prefix("PANGYA")
            .separator("__")
            .try_parsing(true),
    );
    let mut raw: RawConfig = builder
        .build()
        .and_then(Config::try_deserialize)
        .map_err(|_| ConfigLoadError::Load)?;
    if let Some(profile) = &overrides.profile {
        raw.server.profile.clone_from(profile);
    }
    if let Some(bind) = &overrides.login_bind {
        raw.login.bind.clone_from(bind);
    }
    if let Some(bind) = &overrides.http_bind {
        raw.http.bind.clone_from(bind);
    }
    let database_secret = resolve_secret(&raw.database.url_env);
    validate(raw, overrides.acknowledge_public_bind, database_secret)
}

fn validate(
    raw: RawConfig,
    acknowledge_public_bind: bool,
    database_secret: Option<String>,
) -> Result<AppConfig, ConfigLoadError> {
    let mut issues = Vec::new();
    let login_bind = socket(&raw.login.bind, "login.bind", &mut issues);
    let login_advertise = socket(&raw.login.advertise, "login.advertise", &mut issues);
    let game_bind = raw
        .game
        .enabled
        .then(|| socket(&raw.game.bind, "game.bind", &mut issues))
        .flatten();
    let game_advertise = socket(&raw.game.advertise, "game.advertise", &mut issues);
    let http_bind = socket(&raw.http.bind, "http.bind", &mut issues);
    if let Some(address) = game_advertise {
        if !matches!(address.ip(), IpAddr::V4(_)) {
            issue(
                &mut issues,
                "game.advertise",
                "must use representable IPv4 text",
            );
        }
        if address.port() == 0 {
            issue(&mut issues, "game.advertise", "port must be nonzero");
        }
    }
    if let Some(address) = login_advertise {
        if !matches!(address.ip(), IpAddr::V4(_)) {
            issue(
                &mut issues,
                "login.advertise",
                "reserved endpoint must use representable IPv4 text",
            );
        }
        if address.port() == 0 {
            issue(
                &mut issues,
                "login.advertise",
                "reserved advertised port must be nonzero",
            );
        }
    }
    if raw.game.name.is_empty()
        || raw.game.name.len() > 39
        || !raw.game.name.is_ascii()
        || raw.game.name.as_bytes().contains(&0)
    {
        issue(
            &mut issues,
            "game.name",
            "must be nonempty ASCII without NUL and at most 39 bytes",
        );
    }
    for (field, address) in [
        ("login.bind", login_bind),
        ("game.bind", game_bind),
        ("http.bind", http_bind),
    ] {
        if address.is_some_and(|value| value.port() == 0) {
            issue(&mut issues, field, "listener port must be nonzero");
        }
    }
    let binds = [login_bind, game_bind, http_bind]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let unique = binds.iter().collect::<HashSet<_>>();
    if unique.len() != binds.len() {
        issue(&mut issues, "binds", "listener binds must be unique");
    }
    let public_bind_enabled = [login_bind, game_bind, http_bind]
        .into_iter()
        .flatten()
        .any(|address| !address.ip().is_loopback());
    if public_bind_enabled && !acknowledge_public_bind {
        issue(
            &mut issues,
            "binds",
            "every public bind requires explicit acknowledgement",
        );
    }
    if raw.login.auto_create_accounts && raw.server.profile != "local" {
        issue(
            &mut issues,
            "login.auto_create_accounts",
            "is permitted only in local profile",
        );
    }
    if raw.login.client_profile != "us_852" {
        issue(
            &mut issues,
            "login.client_profile",
            "unknown client profile",
        );
    }
    let unknown_opcode_policy = match raw.protocol.unknown_opcode_policy.as_str() {
        "disconnect" => Some(UnknownOpcodePolicy::Disconnect),
        "ignore" => Some(UnknownOpcodePolicy::Ignore),
        "capture" => Some(UnknownOpcodePolicy::Capture),
        _ => {
            issue(
                &mut issues,
                "protocol.unknown_opcode_policy",
                "must be disconnect, ignore, or capture",
            );
            None
        }
    };
    if raw.logging.packet_bodies {
        issue(
            &mut issues,
            "logging.packet_bodies",
            "raw packet bodies are unsupported in M2",
        );
    }
    if !matches!(raw.logging.format.as_str(), "pretty" | "json") {
        issue(&mut issues, "logging.format", "must be pretty or json");
    }

    let shutdown_grace = duration(
        &raw.server.shutdown_grace,
        "server.shutdown_grace",
        &mut issues,
    );
    let heartbeat = duration(
        &raw.http.heartbeat_stale_after,
        "http.heartbeat_stale_after",
        &mut issues,
    );
    let acquire = duration(
        &raw.database.acquire_timeout,
        "database.acquire_timeout",
        &mut issues,
    );
    let retry_initial = duration(
        &raw.database.retry_initial,
        "database.retry_initial",
        &mut issues,
    );
    let retry_max = duration(&raw.database.retry_max, "database.retry_max", &mut issues);
    let readiness_probe_interval = duration(
        &raw.database.readiness_probe_interval,
        "database.readiness_probe_interval",
        &mut issues,
    );
    let readiness_probe_timeout = duration(
        &raw.database.readiness_probe_timeout,
        "database.readiness_probe_timeout",
        &mut issues,
    );
    let data_load_timeout = duration(&raw.data.load_timeout, "data.load_timeout", &mut issues);
    let credential_queue = duration(
        &raw.security.credential_queue_timeout,
        "security.credential_queue_timeout",
        &mut issues,
    );
    let credential_operation = duration(
        &raw.security.credential_operation_timeout,
        "security.credential_operation_timeout",
        &mut issues,
    );
    let rate_window = duration(
        &raw.security.rate_window,
        "security.rate_window",
        &mut issues,
    );
    let login_timeout = duration(
        &raw.security.login_timeout,
        "security.login_timeout",
        &mut issues,
    );
    let idle_timeout = duration(
        &raw.security.idle_timeout,
        "security.idle_timeout",
        &mut issues,
    );
    let game_command_timeout = duration(
        &raw.game.command_timeout,
        "game.command_timeout",
        &mut issues,
    );
    let solo_loading_timeout = duration(
        &raw.game.solo_practice.loading_timeout,
        "game.solo_practice.loading_timeout",
        &mut issues,
    );
    let solo_commit_timeout = duration(
        &raw.game.solo_practice.commit_timeout,
        "game.solo_practice.commit_timeout",
        &mut issues,
    );
    let stroke_loading_timeout = duration(
        &raw.game.stroke_two.loading_timeout,
        "game.stroke_two.loading_timeout",
        &mut issues,
    );
    let stroke_turn_timeout = duration(
        &raw.game.stroke_two.turn_timeout,
        "game.stroke_two.turn_timeout",
        &mut issues,
    );
    let stroke_game_timeout = duration(
        &raw.game.stroke_two.game_timeout,
        "game.stroke_two.game_timeout",
        &mut issues,
    );
    let stroke_commit_timeout = duration(
        &raw.game.stroke_two.commit_timeout,
        "game.stroke_two.commit_timeout",
        &mut issues,
    );
    let economy_command_timeout = duration(
        &raw.game.economy.command_timeout,
        "game.economy.command_timeout",
        &mut issues,
    );

    for (field, zero) in [
        ("game.id", raw.game.id == 0),
        ("game.capacity", raw.game.capacity == 0),
        (
            "game.channel_id",
            raw.game.enabled && raw.game.channel_id == 0,
        ),
        (
            "database.max_connections",
            raw.database.max_connections == 0,
        ),
        (
            "database.connect_attempts",
            raw.database.connect_attempts == 0,
        ),
        (
            "protocol.max_client_frame_bytes",
            raw.protocol.max_client_frame_bytes < 5,
        ),
        (
            "protocol.max_plaintext_bytes",
            raw.protocol.max_plaintext_bytes < 2,
        ),
        (
            "protocol.max_expansion_ratio",
            raw.protocol.max_expansion_ratio == 0,
        ),
        (
            "security.credential_concurrency",
            raw.security.credential_concurrency == 0,
        ),
        (
            "security.global_connections",
            raw.security.global_connections == 0,
        ),
        (
            "security.connections_per_source",
            raw.security.connections_per_source == 0,
        ),
        (
            "security.source_capacity",
            raw.security.source_capacity == 0,
        ),
        (
            "security.global_accepts_per_window",
            raw.security.global_accepts_per_window == 0,
        ),
        (
            "security.accepts_per_window",
            raw.security.accepts_per_window == 0,
        ),
        (
            "security.global_logins_per_window",
            raw.security.global_logins_per_window == 0,
        ),
        (
            "security.logins_per_window",
            raw.security.logins_per_window == 0,
        ),
        (
            "security.username_logins_per_window",
            raw.security.username_logins_per_window == 0,
        ),
        (
            "security.global_packets_per_window",
            raw.security.global_packets_per_window == 0,
        ),
        (
            "security.global_bytes_per_window",
            raw.security.global_bytes_per_window == 0,
        ),
        (
            "security.source_packets_per_window",
            raw.security.source_packets_per_window == 0,
        ),
        (
            "security.source_bytes_per_window",
            raw.security.source_bytes_per_window == 0,
        ),
        (
            "security.packets_per_window",
            raw.security.packets_per_window == 0,
        ),
        (
            "security.bytes_per_window",
            raw.security.bytes_per_window == 0,
        ),
        ("security.max_retries", raw.security.max_retries == 0),
        ("game.max_rooms", raw.game.max_rooms == 0),
        (
            "game.lobby_command_capacity",
            raw.game.lobby_command_capacity == 0,
        ),
        (
            "game.lobby_event_capacity",
            raw.game.lobby_event_capacity == 0,
        ),
        (
            "game.room_normal_capacity",
            raw.game.room_normal_capacity == 0,
        ),
        (
            "game.room_control_capacity",
            raw.game.room_control_capacity == 0,
        ),
        (
            "game.outbound_room_event_capacity",
            raw.game.outbound_room_event_capacity == 0,
        ),
        (
            "game.room_commands_per_window",
            raw.game.room_commands_per_window == 0,
        ),
        (
            "game.chat_messages_per_window",
            raw.game.chat_messages_per_window == 0,
        ),
        (
            "game.unknown_opcode_strikes",
            raw.game.unknown_opcode_strikes == 0,
        ),
        (
            "game.unknown_capture_capacity",
            raw.game.unknown_capture_capacity == 0,
        ),
        (
            "game.solo_practice.course_id",
            raw.game.solo_practice.course_id == 0,
        ),
        (
            "game.solo_practice.startup_recovery_limit",
            raw.game.solo_practice.startup_recovery_limit == 0,
        ),
        (
            "game.solo_practice.shot_packets_per_window",
            raw.game.solo_practice.shot_packets_per_window == 0,
        ),
        (
            "game.stroke_two.course_id",
            raw.game.stroke_two.course_id == 0,
        ),
        (
            "game.stroke_two.startup_recovery_limit",
            raw.game.stroke_two.startup_recovery_limit == 0,
        ),
        (
            "game.stroke_two.shot_packets_per_window",
            raw.game.stroke_two.shot_packets_per_window == 0,
        ),
    ] {
        if zero {
            issue(
                &mut issues,
                field,
                "must be nonzero and within its hard cap",
            );
        }
    }
    for (field, exceeded) in [
        (
            "database.max_connections",
            raw.database.max_connections > 256,
        ),
        (
            "database.connect_attempts",
            raw.database.connect_attempts > 32,
        ),
        (
            "protocol.max_client_frame_bytes",
            raw.protocol.max_client_frame_bytes > 65_535,
        ),
        (
            "protocol.max_plaintext_bytes",
            raw.protocol.max_plaintext_bytes > 64 * 1024 * 1024,
        ),
        (
            "protocol.max_expansion_ratio",
            raw.protocol.max_expansion_ratio > 1_024,
        ),
        (
            "security.credential_concurrency",
            raw.security.credential_concurrency > 64,
        ),
        (
            "security.global_connections",
            raw.security.global_connections > 10_000,
        ),
        (
            "security.connections_per_source",
            raw.security.connections_per_source > 10_000,
        ),
        (
            "security.source_capacity",
            raw.security.source_capacity > 65_536,
        ),
        (
            "security.global_accepts_per_window",
            raw.security.global_accepts_per_window > 1_000_000,
        ),
        (
            "security.accepts_per_window",
            raw.security.accepts_per_window > 1_000_000,
        ),
        (
            "security.global_logins_per_window",
            raw.security.global_logins_per_window > 1_000_000,
        ),
        (
            "security.logins_per_window",
            raw.security.logins_per_window > 1_000_000,
        ),
        (
            "security.username_logins_per_window",
            raw.security.username_logins_per_window > 1_000_000,
        ),
        (
            "security.global_packets_per_window",
            raw.security.global_packets_per_window > 1_000_000,
        ),
        (
            "security.source_packets_per_window",
            raw.security.source_packets_per_window > 1_000_000,
        ),
        (
            "security.packets_per_window",
            raw.security.packets_per_window > 1_000_000,
        ),
        (
            "security.global_bytes_per_window",
            raw.security.global_bytes_per_window > 1024 * 1024 * 1024,
        ),
        (
            "security.source_bytes_per_window",
            raw.security.source_bytes_per_window > 1024 * 1024 * 1024,
        ),
        (
            "security.bytes_per_window",
            raw.security.bytes_per_window > 1024 * 1024 * 1024,
        ),
        ("security.max_retries", raw.security.max_retries > 10),
        ("game.max_rooms", raw.game.max_rooms > 4_096),
        (
            "game.lobby_command_capacity",
            raw.game.lobby_command_capacity > 8_192,
        ),
        (
            "game.lobby_event_capacity",
            raw.game.lobby_event_capacity > 8_192,
        ),
        (
            "game.room_normal_capacity",
            raw.game.room_normal_capacity > 4_096,
        ),
        (
            "game.room_control_capacity",
            raw.game.room_control_capacity > 64,
        ),
        (
            "game.outbound_room_event_capacity",
            raw.game.outbound_room_event_capacity > 4_096,
        ),
        (
            "game.room_commands_per_window",
            raw.game.room_commands_per_window > 10_000,
        ),
        (
            "game.chat_messages_per_window",
            raw.game.chat_messages_per_window > 1_000,
        ),
        (
            "game.unknown_opcode_strikes",
            raw.game.unknown_opcode_strikes > 32,
        ),
        (
            "game.unknown_capture_capacity",
            raw.game.unknown_capture_capacity > 4_096,
        ),
        (
            "game.solo_practice.startup_recovery_limit",
            raw.game.solo_practice.startup_recovery_limit > IncompleteMatchAbortLimit::MAX,
        ),
        (
            "game.solo_practice.shot_packets_per_window",
            raw.game.solo_practice.shot_packets_per_window > 1_000_000,
        ),
        (
            "game.stroke_two.startup_recovery_limit",
            raw.game.stroke_two.startup_recovery_limit > IncompleteMatchAbortLimit::MAX,
        ),
        (
            "game.stroke_two.shot_packets_per_window",
            raw.game.stroke_two.shot_packets_per_window > 1_000_000,
        ),
    ] {
        if exceeded {
            issue(&mut issues, field, "exceeds the supported hard upper bound");
        }
    }
    if raw.security.connections_per_source > raw.security.global_connections {
        issue(
            &mut issues,
            "security.connections_per_source",
            "must not exceed global_connections",
        );
    }
    if raw.game.solo_practice.enabled && !raw.game.enabled {
        issue(
            &mut issues,
            "game.solo_practice.enabled",
            "requires game.enabled",
        );
    }
    if raw.game.stroke_two.enabled && !raw.game.enabled {
        issue(
            &mut issues,
            "game.stroke_two.enabled",
            "requires game.enabled",
        );
    }
    if raw.game.solo_practice.enabled && raw.game.outbound_room_event_capacity < 2 {
        issue(
            &mut issues,
            "game.outbound_room_event_capacity",
            "must be at least 2 when solo practice is enabled",
        );
    }
    if raw.game.stroke_two.enabled && raw.game.outbound_room_event_capacity < 3 {
        issue(
            &mut issues,
            "game.outbound_room_event_capacity",
            "must be at least 3 for the stroke standings/balance/finished burst",
        );
    }
    if !(1..=30).contains(&raw.game.solo_practice.max_strokes) {
        issue(
            &mut issues,
            "game.solo_practice.max_strokes",
            "must be within 1..=30",
        );
    }
    if !(1..=30).contains(&raw.game.stroke_two.max_strokes) {
        issue(
            &mut issues,
            "game.stroke_two.max_strokes",
            "must be within 1..=30",
        );
    }
    if raw.game.enabled {
        for (field, below_partition_minimum) in [
            (
                "security.global_connections",
                raw.security.global_connections < 2,
            ),
            (
                "security.global_accepts_per_window",
                raw.security.global_accepts_per_window < 2,
            ),
            (
                "security.global_logins_per_window",
                raw.security.global_logins_per_window < 2,
            ),
            (
                "security.global_packets_per_window",
                raw.security.global_packets_per_window < 2,
            ),
            (
                "security.global_bytes_per_window",
                raw.security.global_bytes_per_window < 2,
            ),
        ] {
            if below_partition_minimum {
                issue(
                    &mut issues,
                    field,
                    "must be at least 2 when GameService is enabled",
                );
            }
        }
    }
    if shutdown_grace
        .zip(credential_operation)
        .is_some_and(|(grace, operation)| operation > grace)
    {
        issue(
            &mut issues,
            "security.credential_operation_timeout",
            "must not exceed server.shutdown_grace",
        );
    }
    if shutdown_grace
        .zip(game_command_timeout)
        .is_some_and(|(grace, command)| command > grace)
    {
        issue(
            &mut issues,
            "game.command_timeout",
            "must not exceed server.shutdown_grace",
        );
    }
    if shutdown_grace
        .zip(solo_commit_timeout)
        .is_some_and(|(grace, commit)| commit > grace)
    {
        issue(
            &mut issues,
            "game.solo_practice.commit_timeout",
            "must not exceed server.shutdown_grace",
        );
    }
    if solo_commit_timeout.is_some_and(|commit| commit > Duration::from_secs(60)) {
        issue(
            &mut issues,
            "game.solo_practice.commit_timeout",
            "exceeds the 60 second hard cap",
        );
    }
    if solo_loading_timeout.is_some_and(|loading| {
        loading > pangya_game::LOADING_TIMEOUT_HARD_CAP
            || loading.as_millis() == 0
            || loading.as_millis() > u128::from(u32::MAX)
    }) {
        issue(
            &mut issues,
            "game.solo_practice.loading_timeout",
            "exceeds the actor or u32 millisecond hard cap",
        );
    }
    if shutdown_grace
        .zip(stroke_commit_timeout)
        .is_some_and(|(grace, commit)| commit > grace)
        || stroke_commit_timeout.is_some_and(|commit| commit > Duration::from_secs(60))
    {
        issue(
            &mut issues,
            "game.stroke_two.commit_timeout",
            "must not exceed shutdown grace or the 60 second hard cap",
        );
    }
    for (field, value, maximum) in [
        (
            "game.stroke_two.loading_timeout",
            stroke_loading_timeout,
            pangya_game::LOADING_TIMEOUT_HARD_CAP,
        ),
        (
            "game.stroke_two.turn_timeout",
            stroke_turn_timeout,
            pangya_game::STROKE_GAME_TIMEOUT_HARD_CAP,
        ),
        (
            "game.stroke_two.game_timeout",
            stroke_game_timeout,
            pangya_game::STROKE_GAME_TIMEOUT_HARD_CAP,
        ),
    ] {
        if value.is_some_and(|duration| {
            duration > maximum
                || duration.as_millis() == 0
                || duration.as_millis() > u128::from(u32::MAX)
        }) {
            issue(
                &mut issues,
                field,
                "exceeds the actor or u32 millisecond hard cap",
            );
        }
    }
    if stroke_turn_timeout
        .zip(stroke_game_timeout)
        .is_some_and(|(turn, game)| turn > game)
    {
        issue(
            &mut issues,
            "game.stroke_two.turn_timeout",
            "must not exceed game_timeout",
        );
    }
    for (field, value, maximum) in [
        (
            "server.shutdown_grace",
            shutdown_grace,
            Duration::from_secs(300),
        ),
        (
            "http.heartbeat_stale_after",
            heartbeat,
            Duration::from_secs(60),
        ),
        ("database.acquire_timeout", acquire, Duration::from_secs(60)),
        (
            "database.retry_initial",
            retry_initial,
            Duration::from_secs(60),
        ),
        ("database.retry_max", retry_max, Duration::from_secs(60)),
        (
            "database.readiness_probe_interval",
            readiness_probe_interval,
            Duration::from_secs(60),
        ),
        (
            "database.readiness_probe_timeout",
            readiness_probe_timeout,
            Duration::from_secs(60),
        ),
        (
            "data.load_timeout",
            data_load_timeout,
            Duration::from_secs(60),
        ),
        (
            "security.credential_queue_timeout",
            credential_queue,
            Duration::from_secs(10),
        ),
        (
            "security.credential_operation_timeout",
            credential_operation,
            Duration::from_secs(60),
        ),
        (
            "security.rate_window",
            rate_window,
            Duration::from_secs(3_600),
        ),
        (
            "security.login_timeout",
            login_timeout,
            Duration::from_secs(3_600),
        ),
        (
            "security.idle_timeout",
            idle_timeout,
            Duration::from_secs(3_600),
        ),
        (
            "game.command_timeout",
            game_command_timeout,
            Duration::from_secs(300),
        ),
    ] {
        if value.is_some_and(|duration| duration > maximum) {
            issue(
                &mut issues,
                field,
                "exceeds the supported duration upper bound",
            );
        }
    }
    if raw.security.malformed_strike_cap != 1 {
        issue(
            &mut issues,
            "security.malformed_strike_cap",
            "M2 transport resynchronization is unsafe, so the supported cap is exactly 1",
        );
    }
    if raw.database.min_connections > raw.database.max_connections {
        issue(
            &mut issues,
            "database.min_connections",
            "must not exceed maximum",
        );
    }
    if retry_initial
        .zip(retry_max)
        .is_some_and(|(initial, maximum)| initial > maximum)
    {
        issue(
            &mut issues,
            "database.retry_initial",
            "must not exceed retry maximum",
        );
    }
    if raw.game.enabled && !raw.data.catalog_required_m3 {
        issue(
            &mut issues,
            "data.catalog_required_m3",
            "must be true when GameService is enabled",
        );
    }
    if raw.game.enabled && raw.data.iff_directory.is_none() {
        issue(
            &mut issues,
            "data.iff_directory",
            "is required when GameService is enabled",
        );
    }
    if raw.game.enabled && raw.data.manifest.is_none() {
        issue(
            &mut issues,
            "data.manifest",
            "is required when GameService is enabled",
        );
    }
    if !raw.game.enabled && raw.data.catalog_required_m3 {
        issue(
            &mut issues,
            "data.catalog_required_m3",
            "requires game.enabled",
        );
    }

    if raw.game.economy.enabled && !raw.game.enabled {
        issue(&mut issues, "game.economy.enabled", "requires game.enabled");
    }
    if raw.game.economy.commands_per_window == 0 || raw.game.economy.commands_per_window > 1_000_000
    {
        issue(
            &mut issues,
            "game.economy.commands_per_window",
            "must be within 1..=1000000",
        );
    }
    if raw.game.economy.page_size == 0 || raw.game.economy.page_size > 50 {
        issue(
            &mut issues,
            "game.economy.page_size",
            "must be within 1..=50",
        );
    }
    if raw.game.economy.max_purchase_quantity == 0 || raw.game.economy.max_purchase_quantity > 99 {
        issue(
            &mut issues,
            "game.economy.max_purchase_quantity",
            "must be within 1..=99",
        );
    }
    if shutdown_grace
        .zip(economy_command_timeout)
        .is_some_and(|(grace, command)| command > grace)
        || economy_command_timeout.is_some_and(|command| command > Duration::from_secs(60))
    {
        issue(
            &mut issues,
            "game.economy.command_timeout",
            "must not exceed shutdown grace or 60 seconds",
        );
    }
    let economy = if raw.game.economy.enabled {
        economy_command_timeout.map(|command_timeout| ValidatedEconomy {
            command_timeout,
            commands_per_window: raw.game.economy.commands_per_window,
            page_size: raw.game.economy.page_size,
            max_purchase_quantity: raw.game.economy.max_purchase_quantity,
        })
    } else {
        None
    };

    let solo_course_id = match CourseId::new(raw.game.solo_practice.course_id) {
        Ok(value) => Some(value),
        Err(_) => {
            issue(
                &mut issues,
                "game.solo_practice.course_id",
                "must be a nonzero course identifier",
            );
            None
        }
    };
    let solo_recovery =
        match IncompleteMatchAbortLimit::new(raw.game.solo_practice.startup_recovery_limit) {
            Ok(value) => Some(value),
            Err(_) => {
                issue(
                    &mut issues,
                    "game.solo_practice.startup_recovery_limit",
                    "must be within 1..=10000",
                );
                None
            }
        };
    let solo_practice = if raw.game.solo_practice.enabled {
        solo_course_id
            .zip(solo_loading_timeout)
            .zip(solo_commit_timeout)
            .zip(solo_recovery)
            .map(
                |(((course_id, loading_timeout), commit_timeout), startup_recovery_limit)| {
                    ValidatedSoloPractice {
                        course_id,
                        loading_timeout,
                        commit_timeout,
                        max_strokes: raw.game.solo_practice.max_strokes,
                        startup_recovery_limit,
                        shot_packets_per_window: raw.game.solo_practice.shot_packets_per_window,
                    }
                },
            )
    } else {
        None
    };

    let stroke_course_id = match CourseId::new(raw.game.stroke_two.course_id) {
        Ok(value) => Some(value),
        Err(_) => {
            issue(
                &mut issues,
                "game.stroke_two.course_id",
                "must be a nonzero course identifier",
            );
            None
        }
    };
    let stroke_recovery =
        match IncompleteMatchAbortLimit::new(raw.game.stroke_two.startup_recovery_limit) {
            Ok(value) => Some(value),
            Err(_) => {
                issue(
                    &mut issues,
                    "game.stroke_two.startup_recovery_limit",
                    "must be within 1..=10000",
                );
                None
            }
        };
    let stroke_two = if raw.game.stroke_two.enabled {
        stroke_course_id
            .zip(stroke_loading_timeout)
            .zip(stroke_turn_timeout)
            .zip(stroke_game_timeout)
            .zip(stroke_commit_timeout)
            .zip(stroke_recovery)
            .map(
                |(
                    ((((course_id, loading_timeout), turn_timeout), game_timeout), commit_timeout),
                    startup_recovery_limit,
                )| {
                    ValidatedStrokeTwo {
                        course_id,
                        loading_timeout,
                        turn_timeout,
                        game_timeout,
                        commit_timeout,
                        max_strokes: raw.game.stroke_two.max_strokes,
                        startup_recovery_limit,
                        shot_packets_per_window: raw.game.stroke_two.shot_packets_per_window,
                    }
                },
            )
    } else {
        None
    };

    let starter = starter(&raw.starter, &mut issues);
    let database_url = resolve_database_url(&raw.database, database_secret, &mut issues);

    if !issues.is_empty() {
        return Err(ValidationError { issues }.into());
    }
    Ok(AppConfig {
        profile: raw.server.profile,
        shutdown_grace: required(shutdown_grace)?,
        login_bind: required(login_bind)?,
        login_advertise: required(login_advertise)?,
        auto_create_accounts: raw.login.auto_create_accounts,
        public_bind_enabled,
        game_enabled: raw.game.enabled,
        game_bind,
        game_advertise: required(game_advertise)?,
        game_id: raw.game.id,
        game_name: raw.game.name,
        game_capacity: raw.game.capacity,
        game_channel_id: raw.game.channel_id,
        game_max_rooms: raw.game.max_rooms,
        game_lobby_command_capacity: raw.game.lobby_command_capacity,
        game_lobby_event_capacity: raw.game.lobby_event_capacity,
        game_room_normal_capacity: raw.game.room_normal_capacity,
        game_room_control_capacity: raw.game.room_control_capacity,
        game_outbound_room_event_capacity: raw.game.outbound_room_event_capacity,
        game_room_commands_per_window: raw.game.room_commands_per_window,
        game_chat_messages_per_window: raw.game.chat_messages_per_window,
        game_unknown_opcode_strikes: raw.game.unknown_opcode_strikes,
        game_unknown_capture_capacity: raw.game.unknown_capture_capacity,
        game_command_timeout: required(game_command_timeout)?,
        unknown_opcode_policy: required(unknown_opcode_policy)?,
        solo_practice,
        stroke_two,
        economy,
        http_bind: required(http_bind)?,
        metrics_enabled: raw.http.metrics,
        heartbeat_stale_after: required(heartbeat)?,
        database_url: required(database_url)?,
        database_max_connections: raw.database.max_connections,
        database_min_connections: raw.database.min_connections,
        database_acquire_timeout: required(acquire)?,
        database_connect_attempts: raw.database.connect_attempts,
        database_retry_initial: required(retry_initial)?,
        database_retry_max: required(retry_max)?,
        database_readiness_probe_interval: required(readiness_probe_interval)?,
        database_readiness_probe_timeout: required(readiness_probe_timeout)?,
        max_client_frame_bytes: raw.protocol.max_client_frame_bytes,
        max_plaintext_bytes: raw.protocol.max_plaintext_bytes,
        max_expansion_ratio: raw.protocol.max_expansion_ratio,
        logging_filter: raw.logging.filter,
        logging_format: raw.logging.format,
        packet_bodies: raw.logging.packet_bodies,
        credential_concurrency: raw.security.credential_concurrency,
        credential_queue_timeout: required(credential_queue)?,
        credential_operation_timeout: required(credential_operation)?,
        security: ValidatedSecurity {
            global_connections: raw.security.global_connections,
            connections_per_source: raw.security.connections_per_source,
            source_capacity: raw.security.source_capacity,
            global_accepts_per_window: raw.security.global_accepts_per_window,
            accepts_per_window: raw.security.accepts_per_window,
            global_logins_per_window: raw.security.global_logins_per_window,
            logins_per_window: raw.security.logins_per_window,
            username_logins_per_window: raw.security.username_logins_per_window,
            rate_window: required(rate_window)?,
            global_packets_per_window: raw.security.global_packets_per_window,
            global_bytes_per_window: raw.security.global_bytes_per_window,
            source_packets_per_window: raw.security.source_packets_per_window,
            source_bytes_per_window: raw.security.source_bytes_per_window,
            packets_per_window: raw.security.packets_per_window,
            bytes_per_window: raw.security.bytes_per_window,
            malformed_strike_cap: raw.security.malformed_strike_cap,
            max_retries: raw.security.max_retries,
            login_timeout: required(login_timeout)?,
            idle_timeout: required(idle_timeout)?,
        },
        starter: required(starter)?,
        allowed_character_type_ids: raw.starter.allowed_character_type_ids,
        iff_directory: raw.data.iff_directory,
        data_manifest: raw.data.manifest,
        data_load_timeout: required(data_load_timeout)?,
    })
}

fn required<T>(value: Option<T>) -> Result<T, ConfigLoadError> {
    value.ok_or_else(|| {
        ConfigLoadError::Validation(ValidationError {
            issues: vec![ConfigIssue {
                field: "internal",
                message: "validated value missing",
            }],
        })
    })
}

fn socket(value: &str, field: &'static str, issues: &mut Vec<ConfigIssue>) -> Option<SocketAddr> {
    match value.parse() {
        Ok(address) => Some(address),
        Err(_) => {
            issue(
                issues,
                field,
                "must be a numeric socket address with a u16 port",
            );
            None
        }
    }
}

fn duration(value: &str, field: &'static str, issues: &mut Vec<ConfigIssue>) -> Option<Duration> {
    match humantime::parse_duration(value) {
        Ok(duration) if !duration.is_zero() => Some(duration),
        _ => {
            issue(issues, field, "must be a nonzero duration");
            None
        }
    }
}

fn starter(raw: &StarterSection, issues: &mut Vec<ConfigIssue>) -> Option<StarterGrant> {
    if raw.allowed_character_type_ids.len() > MAX_ALLOWED_CHARACTER_TYPES {
        issue(
            issues,
            "starter.allowed_character_type_ids",
            "exceeds the supported hard upper bound",
        );
    }
    if raw.items.len() > MAX_STARTER_ITEMS {
        issue(
            issues,
            "starter.items",
            "exceeds the supported hard upper bound",
        );
    }
    let character_key = match StarterKey::parse(&raw.character_key) {
        Ok(key) => Some(key),
        Err(_) => {
            issue(issues, "starter.character_key", "is invalid");
            None
        }
    };
    let mut seen = HashSet::new();
    let mut items = Vec::with_capacity(raw.items.len().min(MAX_STARTER_ITEMS));
    for item in raw.items.iter().take(MAX_STARTER_ITEMS) {
        let Ok(key) = StarterKey::parse(&item.key) else {
            issue(issues, "starter.items", "contains an invalid stable key");
            continue;
        };
        if item.quantity == 0 || !seen.insert(item.key.as_str()) {
            issue(
                issues,
                "starter.items",
                "contains zero quantity or duplicate key",
            );
            continue;
        }
        items.push(StarterItem {
            key,
            item_type_id: ItemTypeId::new(item.item_type_id),
            quantity: item.quantity,
        });
    }
    if raw.allowed_character_type_ids.is_empty()
        || !raw
            .allowed_character_type_ids
            .contains(&raw.character_type_id)
        || (raw.allowed_character_type_ids.len() <= MAX_ALLOWED_CHARACTER_TYPES
            && raw
                .allowed_character_type_ids
                .iter()
                .collect::<HashSet<_>>()
                .len()
                != raw.allowed_character_type_ids.len())
    {
        issue(
            issues,
            "starter.allowed_character_type_ids",
            "must be unique and include starter character",
        );
    }
    let club = optional_key(
        raw.equipped_club_key.as_deref(),
        "starter.equipped_club_key",
        issues,
    );
    let ball = optional_key(
        raw.equipped_ball_key.as_deref(),
        "starter.equipped_ball_key",
        issues,
    );
    for key in [club.as_ref(), ball.as_ref()].into_iter().flatten() {
        if !seen.contains(key.as_str()) {
            issue(
                issues,
                "starter.equipment",
                "must reference a configured item key",
            );
        }
    }
    character_key.map(|key| StarterGrant {
        character: StarterCharacter {
            key,
            item_type_id: ItemTypeId::new(raw.character_type_id),
        },
        items,
        equipped_club_key: club,
        equipped_ball_key: ball,
    })
}

fn optional_key(
    value: Option<&str>,
    field: &'static str,
    issues: &mut Vec<ConfigIssue>,
) -> Option<StarterKey> {
    value.and_then(|value| match StarterKey::parse(value) {
        Ok(key) => Some(key),
        Err(_) => {
            issue(issues, field, "is invalid");
            None
        }
    })
}

fn resolve_database_url(
    raw: &DatabaseSection,
    environment_value: Option<String>,
    issues: &mut Vec<ConfigIssue>,
) -> Option<DatabaseUrl> {
    if let Some(value) = environment_value
        && !value.trim().is_empty()
    {
        return Some(DatabaseUrl(value));
    }
    if let Some(path) = &raw.secret_file {
        match read_bounded_utf8_file(path, 128) {
            Ok(value) if !value.trim().is_empty() => {
                return Some(DatabaseUrl(value.trim().to_owned()));
            }
            Ok(_) | Err(_) => {
                issue(
                    issues,
                    "database.secret_file",
                    "must be readable UTF-8, nonempty, and at most 128 bytes",
                );
            }
        }
    }
    issue(
        issues,
        "database.url_env",
        "named environment variable or secret file is absent",
    );
    None
}

fn read_bounded_utf8_file(path: &Path, maximum: usize) -> Result<String, ()> {
    let file = fs::File::open(path).map_err(|_| ())?;
    let limit = u64::try_from(maximum.saturating_add(1)).map_err(|_| ())?;
    let mut bytes = Vec::with_capacity(maximum.saturating_add(1));
    file.take(limit).read_to_end(&mut bytes).map_err(|_| ())?;
    if bytes.len() > maximum {
        return Err(());
    }
    String::from_utf8(bytes).map_err(|_| ())
}

fn issue(issues: &mut Vec<ConfigIssue>, field: &'static str, message: &'static str) {
    issues.push(ConfigIssue { field, message });
}

#[cfg(test)]
mod tests {
    use std::{
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    fn test_load(
        path: Option<&Path>,
        overrides: &CliOverrides,
    ) -> Result<AppConfig, ConfigLoadError> {
        load_with_secret_resolver(path, overrides, |name| {
            if name == "PANGYA_INTENTIONALLY_ABSENT_DATABASE_URL" {
                None
            } else {
                Some("postgres://synthetic.invalid/test".to_owned())
            }
        })
    }

    static NEXT_FILE: AtomicU64 = AtomicU64::new(1);

    fn file(contents: &str) -> PathBuf {
        let id = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("pangya-config-{}-{id}.toml", std::process::id()));
        fs::write(&path, contents).expect("write config");
        path
    }

    #[test]
    fn file_overrides_defaults_and_cli_overrides_file() {
        let path = file("[game]\nname = 'From File'\n[login]\nbind = '127.0.0.1:11111'\n");
        let config = test_load(
            Some(&path),
            &CliOverrides {
                login_bind: Some("127.0.0.1:12222".to_owned()),
                ..CliOverrides::default()
            },
        )
        .expect("config");
        assert_eq!(config.game_name, "From File");
        assert_eq!(config.login_bind.port(), 12_222);
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn explicit_missing_config_is_a_load_error() {
        let missing = env::temp_dir().join("pangya-intentionally-missing-config.toml");
        assert!(matches!(
            test_load(Some(&missing), &CliOverrides::default()),
            Err(ConfigLoadError::Load)
        ));
    }

    #[test]
    fn advertised_fixed_width_fields_and_ports_are_validated() {
        let path = file(
            "[login]\nadvertise='127.0.0.1:0'\n\
             [game]\nadvertise='127.0.0.1:0'\nname='this server name is intentionally much longer than thirty nine bytes'\n",
        );
        let error =
            test_load(Some(&path), &CliOverrides::default()).expect_err("invalid advertise");
        let ConfigLoadError::Validation(error) = error else {
            panic!("expected validation");
        };
        let fields = error
            .issues
            .iter()
            .map(|issue| issue.field)
            .collect::<HashSet<_>>();
        assert!(fields.contains("login.advertise"));
        assert!(fields.contains("game.advertise"));
        assert!(fields.contains("game.name"));
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn extreme_allocation_concurrency_and_retry_values_are_rejected() {
        let path = file(
            "[database]\nmax_connections=4294967295\nconnect_attempts=4294967295\nretry_initial='1s'\nretry_max='2h'\n\
             [protocol]\nmax_client_frame_bytes=999999999\nmax_plaintext_bytes=999999999\nmax_expansion_ratio=999999\n\
             [security]\ncredential_concurrency=999999\nglobal_connections=999999\nconnections_per_source=999999\nsource_capacity=999999999\n\
             global_accepts_per_window=4294967295\naccepts_per_window=4294967295\nglobal_logins_per_window=4294967295\n\
             logins_per_window=4294967295\nusername_logins_per_window=4294967295\nglobal_packets_per_window=4294967295\n\
             source_packets_per_window=4294967295\npackets_per_window=4294967295\nglobal_bytes_per_window=2000000000\n\
             source_bytes_per_window=2000000000\nbytes_per_window=2000000000\nmax_retries=255\n",
        );
        let error = test_load(Some(&path), &CliOverrides::default()).expect_err("extreme values");
        let ConfigLoadError::Validation(error) = error else {
            panic!("expected validation");
        };
        for expected in [
            "database.max_connections",
            "database.connect_attempts",
            "database.retry_max",
            "protocol.max_client_frame_bytes",
            "protocol.max_plaintext_bytes",
            "protocol.max_expansion_ratio",
            "security.credential_concurrency",
            "security.global_connections",
            "security.connections_per_source",
            "security.source_capacity",
            "security.global_accepts_per_window",
            "security.global_bytes_per_window",
            "security.max_retries",
        ] {
            assert!(
                error.issues.iter().any(|issue| issue.field == expected),
                "{expected}"
            );
        }
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn game_m4_defaults_file_values_and_unknown_policy_are_typed() {
        let defaults = test_load(None, &CliOverrides::default()).expect("default config");
        assert_eq!(defaults.game_max_rooms, 1_024);
        assert_eq!(defaults.game_lobby_command_capacity, 256);
        assert_eq!(defaults.game_lobby_event_capacity, 256);
        assert_eq!(defaults.game_room_normal_capacity, 64);
        assert_eq!(defaults.game_room_control_capacity, 16);
        assert_eq!(defaults.game_outbound_room_event_capacity, 64);
        assert_eq!(defaults.game_room_commands_per_window, 30);
        assert_eq!(defaults.game_chat_messages_per_window, 10);
        assert_eq!(defaults.game_unknown_opcode_strikes, 3);
        assert_eq!(defaults.game_unknown_capture_capacity, 256);
        assert_eq!(defaults.game_command_timeout, Duration::from_secs(3));
        assert_eq!(
            defaults.unknown_opcode_policy,
            UnknownOpcodePolicy::Disconnect
        );

        let path = file(
            "[game]\nmax_rooms=4096\nlobby_command_capacity=8192\nlobby_event_capacity=8192\n\
             room_normal_capacity=4096\nroom_control_capacity=64\noutbound_room_event_capacity=4096\n\
             room_commands_per_window=10000\nchat_messages_per_window=1000\nunknown_opcode_strikes=32\n\
             unknown_capture_capacity=4096\ncommand_timeout='10s'\n\
             [protocol]\nunknown_opcode_policy='capture'\n",
        );
        let config = test_load(Some(&path), &CliOverrides::default()).expect("maximum config");
        assert_eq!(config.game_max_rooms, 4_096);
        assert_eq!(config.game_room_control_capacity, 64);
        assert_eq!(config.game_room_commands_per_window, 10_000);
        assert_eq!(config.game_command_timeout, config.shutdown_grace);
        assert_eq!(config.unknown_opcode_policy, UnknownOpcodePolicy::Capture);
        fs::remove_file(path).expect("remove");

        let path = file("[protocol]\nunknown_opcode_policy='ignore'\n");
        let config = test_load(Some(&path), &CliOverrides::default()).expect("ignore policy");
        assert_eq!(config.unknown_opcode_policy, UnknownOpcodePolicy::Ignore);
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn game_m4_zero_limits_and_unknown_policy_errors_aggregate() {
        let path = file(
            "[game]\nmax_rooms=0\nlobby_command_capacity=0\nlobby_event_capacity=0\n\
             room_normal_capacity=0\nroom_control_capacity=0\noutbound_room_event_capacity=0\n\
             room_commands_per_window=0\nchat_messages_per_window=0\nunknown_opcode_strikes=0\n\
             unknown_capture_capacity=0\ncommand_timeout='0s'\n\
             [protocol]\nunknown_opcode_policy='retain-raw'\n",
        );
        let ConfigLoadError::Validation(error) =
            test_load(Some(&path), &CliOverrides::default()).expect_err("zero limits")
        else {
            panic!("expected validation");
        };
        let fields = error
            .issues
            .iter()
            .map(|issue| issue.field)
            .collect::<HashSet<_>>();
        for expected in [
            "game.max_rooms",
            "game.lobby_command_capacity",
            "game.lobby_event_capacity",
            "game.room_normal_capacity",
            "game.room_control_capacity",
            "game.outbound_room_event_capacity",
            "game.room_commands_per_window",
            "game.chat_messages_per_window",
            "game.unknown_opcode_strikes",
            "game.unknown_capture_capacity",
            "game.command_timeout",
            "protocol.unknown_opcode_policy",
        ] {
            assert!(fields.contains(expected), "missing {expected}: {fields:?}");
        }
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn game_m4_hard_caps_and_command_shutdown_relation_aggregate() {
        let path = file(
            "[server]\nshutdown_grace='1s'\n[game]\nmax_rooms=4097\nlobby_command_capacity=8193\n\
             lobby_event_capacity=8193\nroom_normal_capacity=4097\nroom_control_capacity=65\n\
             outbound_room_event_capacity=4097\nroom_commands_per_window=10001\nchat_messages_per_window=1001\n\
             unknown_opcode_strikes=33\nunknown_capture_capacity=4097\ncommand_timeout='2s'\n",
        );
        let ConfigLoadError::Validation(error) =
            test_load(Some(&path), &CliOverrides::default()).expect_err("hard caps")
        else {
            panic!("expected validation");
        };
        let fields = error
            .issues
            .iter()
            .map(|issue| issue.field)
            .collect::<HashSet<_>>();
        for expected in [
            "game.max_rooms",
            "game.lobby_command_capacity",
            "game.lobby_event_capacity",
            "game.room_normal_capacity",
            "game.room_control_capacity",
            "game.outbound_room_event_capacity",
            "game.room_commands_per_window",
            "game.chat_messages_per_window",
            "game.unknown_opcode_strikes",
            "game.unknown_capture_capacity",
            "game.command_timeout",
        ] {
            assert!(fields.contains(expected), "missing {expected}: {fields:?}");
        }
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn starter_collection_caps_and_credential_shutdown_relation_aggregate() {
        let allowed = (0_u32..=64)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let items = (0_u32..=256)
            .map(|value| {
                format!(
                    "{{key='item_{value}',item_type_id={},quantity=1}}",
                    value + 1
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let path = file(&format!(
            "[server]\nshutdown_grace='1s'\n[security]\ncredential_operation_timeout='2s'\n\
             [starter]\ncharacter_key='character'\ncharacter_type_id=0\nallowed_character_type_ids=[{allowed}]\nitems=[{items}]\n"
        ));
        let error = test_load(Some(&path), &CliOverrides::default()).expect_err("hard caps");
        let ConfigLoadError::Validation(error) = error else {
            panic!("expected validation");
        };
        let fields = error
            .issues
            .iter()
            .map(|issue| issue.field)
            .collect::<HashSet<_>>();
        for expected in [
            "starter.allowed_character_type_ids",
            "starter.items",
            "security.credential_operation_timeout",
        ] {
            assert!(fields.contains(expected), "missing {expected}: {fields:?}");
        }
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn database_secret_file_is_race_safe_and_bounded_to_128_bytes() {
        let secret = env::temp_dir().join("pangya-oversized-db-secret");
        fs::write(&secret, vec![b'x'; 129]).expect("secret");
        let path = file(&format!(
            "[database]\nurl_env='ABSENT'\nsecret_file='{}'\n",
            secret.display()
        ));
        let result = load_with_secret_resolver(Some(&path), &CliOverrides::default(), |_| None);
        let ConfigLoadError::Validation(error) = result.expect_err("oversized secret") else {
            panic!("expected validation");
        };
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.field == "database.secret_file")
        );
        fs::remove_file(path).expect("remove config");
        fs::remove_file(secret).expect("remove secret");
    }

    #[test]
    fn validation_aggregates_every_required_class() {
        let path = file(
            "[server]\nprofile='production'\nshutdown_grace='0s'\n\
             [login]\nbind='not-an-address'\nadvertise='127.0.0.1:1'\nclient_profile='other'\nauto_create_accounts=true\n\
             [game]\nbind='0.0.0.0:8080'\nadvertise='[::1]:20201'\nid=0\ncapacity=0\n\
             [http]\nbind='0.0.0.0:8080'\n\
             [database]\nurl_env='PANGYA_INTENTIONALLY_ABSENT_DATABASE_URL'\nmax_connections=0\nmin_connections=9\n\
             [protocol]\nmax_expansion_ratio=0\n\
             [logging]\npacket_bodies=true\nformat='binary'\n\
             [security]\ncredential_concurrency=0\nmalformed_strike_cap=0\n\
             [starter]\ncharacter_key='BAD KEY'\ncharacter_type_id=1\nallowed_character_type_ids=[]\nitems=[]\n",
        );
        let error = test_load(Some(&path), &CliOverrides::default()).expect_err("invalid");
        let ConfigLoadError::Validation(error) = error else {
            panic!("expected validation");
        };
        let fields = error
            .issues
            .iter()
            .map(|issue| issue.field)
            .collect::<HashSet<_>>();
        for expected in [
            "login.bind",
            "game.advertise",
            "binds",
            "login.auto_create_accounts",
            "login.client_profile",
            "server.shutdown_grace",
            "database.max_connections",
            "database.min_connections",
            "database.url_env",
            "protocol.max_expansion_ratio",
            "logging.packet_bodies",
            "security.credential_concurrency",
            "starter.character_key",
            "starter.allowed_character_type_ids",
        ] {
            assert!(fields.contains(expected), "missing {expected}: {fields:?}");
        }
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn game_enablement_requires_explicit_catalog_gate_and_paths() {
        let path = file("[game]\nenabled=true\n");
        let error = test_load(Some(&path), &CliOverrides::default()).expect_err("catalog gates");
        let ConfigLoadError::Validation(error) = error else {
            panic!("expected validation");
        };
        for field in [
            "data.catalog_required_m3",
            "data.iff_directory",
            "data.manifest",
        ] {
            assert!(
                error.issues.iter().any(|issue| issue.field == field),
                "{field}"
            );
        }
        fs::remove_file(path).expect("remove");

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../pangya-data/tests/fixtures/synthetic-catalog");
        let path = file(&format!(
            "[game]\nenabled=true\n[data]\ncatalog_required_m3=true\niff_directory='{}'\nmanifest='manifest.toml'\n",
            root.display()
        ));
        let config = test_load(Some(&path), &CliOverrides::default()).expect("enabled config");
        assert!(config.game_enabled);
        assert_eq!(config.game_channel_id, 1);
        let login_limits = crate::runtime_limits(&config);
        let game_limits = crate::game_runtime_limits(&config).expect("game limits");
        assert_eq!(
            login_limits.global_connections + game_limits.global_connections,
            config.security.global_connections
        );
        assert_eq!(
            login_limits.global_accepts_per_window + game_limits.global_accepts_per_window,
            config.security.global_accepts_per_window
        );
        assert_eq!(
            login_limits.global_logins_per_window + game_limits.global_auth_per_window,
            config.security.global_logins_per_window
        );
        assert_eq!(
            login_limits.global_packets_per_window + game_limits.global_packets_per_window,
            config.security.global_packets_per_window
        );
        assert_eq!(
            login_limits.global_bytes_per_window + game_limits.global_bytes_per_window,
            config.security.global_bytes_per_window
        );
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn solo_practice_defaults_disabled_and_validates_enablement_and_hard_caps() {
        let defaults = test_load(None, &CliOverrides::default()).expect("defaults");
        assert!(defaults.solo_practice.is_none());
        assert!(defaults.stroke_two.is_none());

        let path = file(
            "[game.solo_practice]\nenabled=true\ncourse_id=0\nloading_timeout='301s'\n\
             commit_timeout='61s'\nmax_strokes=31\nstartup_recovery_limit=10001\n\
             shot_packets_per_window=1000001\n",
        );
        let ConfigLoadError::Validation(error) =
            test_load(Some(&path), &CliOverrides::default()).expect_err("invalid solo")
        else {
            panic!("expected validation");
        };
        let fields = error
            .issues
            .iter()
            .map(|issue| issue.field)
            .collect::<HashSet<_>>();
        for expected in [
            "game.solo_practice.enabled",
            "game.solo_practice.course_id",
            "game.solo_practice.loading_timeout",
            "game.solo_practice.commit_timeout",
            "game.solo_practice.max_strokes",
            "game.solo_practice.startup_recovery_limit",
            "game.solo_practice.shot_packets_per_window",
        ] {
            assert!(fields.contains(expected), "missing {expected}: {fields:?}");
        }
        fs::remove_file(path).expect("remove");

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../pangya-data/tests/fixtures/synthetic-catalog");
        let solo_config = |capacity| {
            format!(
                "[game]\nenabled=true\noutbound_room_event_capacity={capacity}\n\
                 [game.solo_practice]\nenabled=true\ncourse_id=7\nloading_timeout='5s'\n\
                 commit_timeout='2s'\nmax_strokes=9\nstartup_recovery_limit=50\n\
                 shot_packets_per_window=80\n[data]\ncatalog_required_m3=true\n\
                 iff_directory='{}'\nmanifest='manifest.toml'\n",
                root.display()
            )
        };
        let path = file(&solo_config(1));
        let ConfigLoadError::Validation(error) =
            test_load(Some(&path), &CliOverrides::default()).expect_err("solo queue too small")
        else {
            panic!("expected validation");
        };
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.field == "game.outbound_room_event_capacity")
        );
        fs::remove_file(path).expect("remove");

        let path = file(&solo_config(2));
        let config = test_load(Some(&path), &CliOverrides::default()).expect("valid solo");
        let solo = config.solo_practice.expect("enabled solo");
        assert_eq!(solo.course_id.get(), 7);
        assert_eq!(solo.loading_timeout, Duration::from_secs(5));
        assert_eq!(solo.commit_timeout, Duration::from_secs(2));
        assert_eq!(solo.max_strokes, 9);
        assert_eq!(solo.startup_recovery_limit.get(), 50);
        assert_eq!(solo.shot_packets_per_window, 80);
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn stroke_two_validates_cross_relations_caps_and_terminal_burst_capacity() {
        let path = file(
            "[game.stroke_two]\nenabled=true\ncourse_id=0\nloading_timeout='301s'\n\
             turn_timeout='20s'\ngame_timeout='10s'\ncommit_timeout='61s'\nmax_strokes=31\n\
             startup_recovery_limit=10001\nshot_packets_per_window=1000001\n",
        );
        let ConfigLoadError::Validation(error) =
            test_load(Some(&path), &CliOverrides::default()).expect_err("invalid stroke")
        else {
            panic!("expected validation");
        };
        let fields = error
            .issues
            .iter()
            .map(|issue| issue.field)
            .collect::<HashSet<_>>();
        for expected in [
            "game.stroke_two.enabled",
            "game.stroke_two.course_id",
            "game.stroke_two.loading_timeout",
            "game.stroke_two.turn_timeout",
            "game.stroke_two.commit_timeout",
            "game.stroke_two.max_strokes",
            "game.stroke_two.startup_recovery_limit",
            "game.stroke_two.shot_packets_per_window",
        ] {
            assert!(fields.contains(expected), "missing {expected}: {fields:?}");
        }
        fs::remove_file(path).expect("remove");

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../pangya-data/tests/fixtures/synthetic-catalog");
        let config_text = |capacity| {
            format!(
                "[server]\nshutdown_grace='5s'\n[game]\nenabled=true\noutbound_room_event_capacity={capacity}\n\
                 [game.stroke_two]\nenabled=true\ncourse_id=7\nloading_timeout='2s'\n\
                 turn_timeout='3s'\ngame_timeout='30s'\ncommit_timeout='2s'\nmax_strokes=9\n\
                 startup_recovery_limit=50\nshot_packets_per_window=80\n[data]\n\
                 catalog_required_m3=true\niff_directory='{}'\nmanifest='manifest.toml'\n",
                root.display()
            )
        };
        let path = file(&config_text(2));
        assert!(test_load(Some(&path), &CliOverrides::default()).is_err());
        fs::remove_file(path).expect("remove");
        let path = file(&config_text(3));
        let config = test_load(Some(&path), &CliOverrides::default()).expect("valid stroke");
        let stroke = config.stroke_two.expect("enabled stroke");
        assert_eq!(stroke.course_id.get(), 7);
        assert_eq!(stroke.loading_timeout, Duration::from_secs(2));
        assert_eq!(stroke.turn_timeout, Duration::from_secs(3));
        assert_eq!(stroke.game_timeout, Duration::from_secs(30));
        assert_eq!(stroke.commit_timeout, Duration::from_secs(2));
        assert_eq!(stroke.max_strokes, 9);
        assert_eq!(stroke.startup_recovery_limit.get(), 50);
        assert_eq!(stroke.shot_packets_per_window, 80);
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn production_profile_ignores_public_disabled_game_bind() {
        let path = file(
            "[server]\nprofile='production'\n[game]\nenabled=false\nbind='0.0.0.0:20201'\nchannel_id=0\n",
        );
        let config = test_load(Some(&path), &CliOverrides::default())
            .expect("disabled GameService bind is not a listener");
        assert!(!config.public_bind_enabled);
        assert_eq!(config.game_bind, None);
        let login_limits = crate::runtime_limits(&config);
        assert_eq!(
            login_limits.global_connections,
            config.security.global_connections
        );
        assert_eq!(
            login_limits.global_accepts_per_window,
            config.security.global_accepts_per_window
        );
        assert_eq!(
            login_limits.global_logins_per_window,
            config.security.global_logins_per_window
        );
        assert_eq!(
            login_limits.global_packets_per_window,
            config.security.global_packets_per_window
        );
        assert_eq!(
            login_limits.global_bytes_per_window,
            config.security.global_bytes_per_window
        );
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn disabled_game_bind_may_duplicate_an_enabled_listener() {
        let path = file("[game]\nenabled=false\nbind='127.0.0.1:10103'\n");
        let config = test_load(Some(&path), &CliOverrides::default())
            .expect("disabled GameService bind does not participate in uniqueness");
        assert_eq!(config.game_bind, None);
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn enabled_game_requires_partitionable_process_totals() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../pangya-data/tests/fixtures/synthetic-catalog");
        let path = file(&format!(
            "[game]\nenabled=true\n[data]\ncatalog_required_m3=true\niff_directory='{}'\nmanifest='manifest.toml'\n\
             [security]\nglobal_connections=1\nconnections_per_source=1\nglobal_accepts_per_window=1\n\
             global_logins_per_window=1\nglobal_packets_per_window=1\nglobal_bytes_per_window=1\n",
            root.display()
        ));
        let error = test_load(Some(&path), &CliOverrides::default()).expect_err("small totals");
        let ConfigLoadError::Validation(error) = error else {
            panic!("expected validation");
        };
        for field in [
            "security.global_connections",
            "security.global_accepts_per_window",
            "security.global_logins_per_window",
            "security.global_packets_per_window",
            "security.global_bytes_per_window",
        ] {
            assert!(
                error.issues.iter().any(|issue| issue.field == field),
                "{field}"
            );
        }
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn public_local_bind_requires_explicit_acknowledgement() {
        let path = file("[login]\nbind='0.0.0.0:10103'\n");
        let error = test_load(Some(&path), &CliOverrides::default()).expect_err("public rejected");
        assert!(matches!(error, ConfigLoadError::Validation(_)));
        assert!(
            test_load(
                Some(&path),
                &CliOverrides {
                    acknowledge_public_bind: true,
                    ..CliOverrides::default()
                }
            )
            .is_ok()
        );
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn nonlocal_public_bind_also_requires_acknowledgement() {
        let path = file("[server]\nprofile='production'\n[http]\nbind='0.0.0.0:8080'\n");
        assert!(matches!(
            test_load(Some(&path), &CliOverrides::default()),
            Err(ConfigLoadError::Validation(_))
        ));
        let config = test_load(
            Some(&path),
            &CliOverrides {
                acknowledge_public_bind: true,
                ..CliOverrides::default()
            },
        )
        .expect("acknowledged public config");
        assert!(config.public_bind_enabled);
        fs::remove_file(path).expect("remove");
    }

    #[test]
    fn debug_redacts_database_url_and_secret_path() {
        let config = test_load(None, &CliOverrides::default()).expect("config");
        let debug = format!("{config:?}");
        assert!(!debug.contains(config.database_url.expose()));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn environment_precedence_runs_in_an_isolated_child() {
        let output = Command::new(std::env::current_exe().expect("test exe"))
            .args([
                "--ignored",
                "--exact",
                "configuration::tests::environment_child",
            ])
            .env("PANGYA__GAME__NAME", "From Environment")
            .env("PANGYA__GAME__MAX_ROOMS", "2048")
            .env("PANGYA__PROTOCOL__UNKNOWN_OPCODE_POLICY", "ignore")
            .env("PANGYA__LOGIN__BIND", "127.0.0.1:13333")
            .output()
            .expect("child");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    #[ignore]
    fn environment_child() {
        let path = file("[game]\nname='From File'\n[login]\nbind='127.0.0.1:11111'\n");
        let config = test_load(Some(&path), &CliOverrides::default()).expect("config");
        assert_eq!(config.game_name, "From Environment");
        assert_eq!(config.game_max_rooms, 2_048);
        assert_eq!(config.unknown_opcode_policy, UnknownOpcodePolicy::Ignore);
        assert_eq!(config.login_bind.port(), 13_333);
        fs::remove_file(path).expect("remove");
    }
}
