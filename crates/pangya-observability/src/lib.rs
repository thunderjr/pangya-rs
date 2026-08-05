#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Redacted tracing, fixed-cardinality M2 metrics, and admin health endpoints.

use std::{
    fmt::Write as _,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use pangya_domain::{AccountId, SourceAddressPrefix};
use pangya_game::{
    GameChatObservation, GameConnectionId, GameObserver, GameQueueObservation, GameRateClass,
    GameRoomObservation, GameTermination, GameUnknownObservation,
};
use pangya_login::{
    ConnectionId, ConnectionTermination, CredentialWorkerOutcome, DbQueryClass, LoginObserver,
    ProtocolMetricClass, RateLimitClass, UnknownOpcodeBucket,
};
use serde::Serialize;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

/// Supported tracing output format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogFormat {
    /// Human-oriented local output.
    Pretty,
    /// Machine-oriented newline-delimited JSON.
    Json,
}

/// Tracing initialization failure without config secrets.
#[derive(Debug, Error)]
pub enum TracingError {
    /// Filter syntax was invalid.
    #[error("logging filter is invalid")]
    InvalidFilter,
    /// A global subscriber was already installed.
    #[error("tracing subscriber could not be installed")]
    Install,
}

/// Installs the process-wide redacted tracing subscriber.
///
/// # Errors
/// Returns an invalid-filter or global-install failure.
pub fn install_tracing(filter: &str, format: LogFormat) -> Result<(), TracingError> {
    let filter = EnvFilter::try_new(filter).map_err(|_| TracingError::InvalidFilter)?;
    match format {
        LogFormat::Pretty => tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().pretty())
            .try_init()
            .map_err(|_| TracingError::Install),
        LogFormat::Json => tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().json())
            .try_init()
            .map_err(|_| TracingError::Install),
    }
}

/// Fixed-cardinality counters for the implemented M2 and optional synthetic M3 services.
#[derive(Debug, Default)]
pub struct M2Metrics {
    accepted_login: AtomicU64,
    active_login: AtomicU64,
    closed_complete: AtomicU64,
    closed_rejected: AtomicU64,
    closed_cancelled: AtomicU64,
    closed_peer_closed: AtomicU64,
    closed_timeout: AtomicU64,
    closed_limited: AtomicU64,
    closed_protocol: AtomicU64,
    closed_error: AtomicU64,
    packets_in: AtomicU64,
    packets_out: AtomicU64,
    plaintext_bytes_in: AtomicU64,
    plaintext_bytes_out: AtomicU64,
    login_success: AtomicU64,
    login_rejected: AtomicU64,
    login_duplicate: AtomicU64,
    credential_overload: AtomicU64,
    credential_timeout: AtomicU64,
    credential_operational: AtomicU64,
    protocol_decode: AtomicU64,
    protocol_decode_truncated: AtomicU64,
    protocol_decode_limit: AtomicU64,
    protocol_decode_overflow: AtomicU64,
    protocol_decode_missing_terminator: AtomicU64,
    protocol_decode_invalid: AtomicU64,
    protocol_io: AtomicU64,
    protocol_crypto: AtomicU64,
    protocol_encode: AtomicU64,
    protocol_invalid_state: AtomicU64,
    protocol_unknown_opcode: AtomicU64,
    unknown_low: AtomicU64,
    unknown_other: AtomicU64,
    rate_accept_global: AtomicU64,
    rate_accept_source: AtomicU64,
    rate_connection_global: AtomicU64,
    rate_connection_source: AtomicU64,
    rate_login_global: AtomicU64,
    rate_login_source: AtomicU64,
    rate_login_username: AtomicU64,
    rate_packet_global: AtomicU64,
    rate_packet_source: AtomicU64,
    rate_packet_connection: AtomicU64,
    rate_bytes_global: AtomicU64,
    rate_bytes_source: AtomicU64,
    db_query_fast: AtomicU64,
    db_query_slow: AtomicU64,
    db_query_error: AtomicU64,
    db_ready: AtomicU64,
    game_accepted: AtomicU64,
    game_active: AtomicU64,
    game_closed_peer: AtomicU64,
    game_closed_cancelled: AtomicU64,
    game_closed_rejected: AtomicU64,
    game_closed_timeout: AtomicU64,
    game_closed_limited: AtomicU64,
    game_closed_protocol: AtomicU64,
    game_closed_error: AtomicU64,
    game_packets_in: AtomicU64,
    game_packets_out: AtomicU64,
    game_bytes_in: AtomicU64,
    game_bytes_out: AtomicU64,
    game_auth_success: AtomicU64,
    game_auth_rejected: AtomicU64,
    game_auth_mismatch: AtomicU64,
    game_auth_duplicate: AtomicU64,
    game_rate: [AtomicU64; 13],
    game_room: [AtomicU64; 9],
    game_active_rooms: AtomicU64,
    game_queue: [AtomicU64; 2],
    game_chat: [AtomicU64; 3],
    game_unknown: [AtomicU64; 4],
}

impl M2Metrics {
    /// Sets the DB readiness gauge to zero or one.
    pub fn set_db_ready(&self, ready: bool) {
        self.db_ready.store(u64::from(ready), Ordering::Relaxed);
    }

    /// Renders stable Prometheus text without attacker-controlled labels.
    /// Packet byte counters are plaintext `opcode + payload` bytes in both directions.
    #[must_use]
    pub fn render(&self) -> String {
        let mut output = String::with_capacity(4_096);
        let values = [
            (
                "pangya_connections_accepted_total",
                "service=\"login\"",
                self.accepted_login.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_active",
                "service=\"login\"",
                self.active_login.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"login\",reason=\"complete\"",
                self.closed_complete.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"login\",reason=\"rejected\"",
                self.closed_rejected.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"login\",reason=\"cancelled\"",
                self.closed_cancelled.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"login\",reason=\"peer_closed\"",
                self.closed_peer_closed.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"login\",reason=\"timeout\"",
                self.closed_timeout.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"login\",reason=\"limited\"",
                self.closed_limited.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"login\",reason=\"protocol\"",
                self.closed_protocol.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"login\",reason=\"error\"",
                self.closed_error.load(Ordering::Relaxed),
            ),
            (
                "pangya_packets_total",
                "service=\"login\",direction=\"in\"",
                self.packets_in.load(Ordering::Relaxed),
            ),
            (
                "pangya_packets_total",
                "service=\"login\",direction=\"out\"",
                self.packets_out.load(Ordering::Relaxed),
            ),
            (
                "pangya_packet_plaintext_bytes_total",
                "service=\"login\",direction=\"in\"",
                self.plaintext_bytes_in.load(Ordering::Relaxed),
            ),
            (
                "pangya_packet_plaintext_bytes_total",
                "service=\"login\",direction=\"out\"",
                self.plaintext_bytes_out.load(Ordering::Relaxed),
            ),
            (
                "pangya_login_attempts_total",
                "outcome=\"success\"",
                self.login_success.load(Ordering::Relaxed),
            ),
            (
                "pangya_login_attempts_total",
                "outcome=\"rejected\"",
                self.login_rejected.load(Ordering::Relaxed),
            ),
            (
                "pangya_login_attempts_total",
                "outcome=\"duplicate\"",
                self.login_duplicate.load(Ordering::Relaxed),
            ),
            (
                "pangya_credential_worker_total",
                "outcome=\"overload\"",
                self.credential_overload.load(Ordering::Relaxed),
            ),
            (
                "pangya_credential_worker_total",
                "outcome=\"timeout\"",
                self.credential_timeout.load(Ordering::Relaxed),
            ),
            (
                "pangya_credential_worker_total",
                "outcome=\"operational_error\"",
                self.credential_operational.load(Ordering::Relaxed),
            ),
            (
                "pangya_protocol_errors_total",
                "service=\"login\",class=\"decode\"",
                self.protocol_decode.load(Ordering::Relaxed),
            ),
            (
                "pangya_protocol_errors_total",
                "service=\"login\",class=\"decode_truncated\"",
                self.protocol_decode_truncated.load(Ordering::Relaxed),
            ),
            (
                "pangya_protocol_errors_total",
                "service=\"login\",class=\"decode_limit\"",
                self.protocol_decode_limit.load(Ordering::Relaxed),
            ),
            (
                "pangya_protocol_errors_total",
                "service=\"login\",class=\"decode_overflow\"",
                self.protocol_decode_overflow.load(Ordering::Relaxed),
            ),
            (
                "pangya_protocol_errors_total",
                "service=\"login\",class=\"decode_missing_terminator\"",
                self.protocol_decode_missing_terminator
                    .load(Ordering::Relaxed),
            ),
            (
                "pangya_protocol_errors_total",
                "service=\"login\",class=\"decode_invalid\"",
                self.protocol_decode_invalid.load(Ordering::Relaxed),
            ),
            (
                "pangya_protocol_errors_total",
                "service=\"login\",class=\"io\"",
                self.protocol_io.load(Ordering::Relaxed),
            ),
            (
                "pangya_protocol_errors_total",
                "service=\"login\",class=\"crypto\"",
                self.protocol_crypto.load(Ordering::Relaxed),
            ),
            (
                "pangya_protocol_errors_total",
                "service=\"login\",class=\"encode_or_compress\"",
                self.protocol_encode.load(Ordering::Relaxed),
            ),
            (
                "pangya_protocol_errors_total",
                "service=\"login\",class=\"invalid_state\"",
                self.protocol_invalid_state.load(Ordering::Relaxed),
            ),
            (
                "pangya_protocol_errors_total",
                "service=\"login\",class=\"unknown_opcode\"",
                self.protocol_unknown_opcode.load(Ordering::Relaxed),
            ),
            (
                "pangya_unknown_opcodes_total",
                "service=\"login\",range=\"0000_00ff\"",
                self.unknown_low.load(Ordering::Relaxed),
            ),
            (
                "pangya_unknown_opcodes_total",
                "service=\"login\",range=\"other\"",
                self.unknown_other.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"accept_global\"",
                self.rate_accept_global.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"accept_source\"",
                self.rate_accept_source.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"connection_global\"",
                self.rate_connection_global.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"connection_source\"",
                self.rate_connection_source.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"login_global\"",
                self.rate_login_global.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"login_source\"",
                self.rate_login_source.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"login_username\"",
                self.rate_login_username.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"packet_global\"",
                self.rate_packet_global.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"packet_source\"",
                self.rate_packet_source.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"packet_or_bytes_connection\"",
                self.rate_packet_connection.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"bytes_global\"",
                self.rate_bytes_global.load(Ordering::Relaxed),
            ),
            (
                "pangya_rate_limit_total",
                "class=\"bytes_source\"",
                self.rate_bytes_source.load(Ordering::Relaxed),
            ),
            (
                "pangya_db_query_latency_total",
                "class=\"fast\"",
                self.db_query_fast.load(Ordering::Relaxed),
            ),
            (
                "pangya_db_query_latency_total",
                "class=\"slow\"",
                self.db_query_slow.load(Ordering::Relaxed),
            ),
            (
                "pangya_db_query_latency_total",
                "class=\"error\"",
                self.db_query_error.load(Ordering::Relaxed),
            ),
            (
                "pangya_db_pool_ready",
                "class=\"primary\"",
                self.db_ready.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_accepted_total",
                "service=\"game\"",
                self.game_accepted.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_active",
                "service=\"game\"",
                self.game_active.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"game\",reason=\"peer_closed\"",
                self.game_closed_peer.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"game\",reason=\"cancelled\"",
                self.game_closed_cancelled.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"game\",reason=\"rejected\"",
                self.game_closed_rejected.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"game\",reason=\"timeout\"",
                self.game_closed_timeout.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"game\",reason=\"limited\"",
                self.game_closed_limited.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"game\",reason=\"protocol\"",
                self.game_closed_protocol.load(Ordering::Relaxed),
            ),
            (
                "pangya_connections_closed_total",
                "service=\"game\",reason=\"error\"",
                self.game_closed_error.load(Ordering::Relaxed),
            ),
            (
                "pangya_packets_total",
                "service=\"game\",direction=\"in\"",
                self.game_packets_in.load(Ordering::Relaxed),
            ),
            (
                "pangya_packets_total",
                "service=\"game\",direction=\"out\"",
                self.game_packets_out.load(Ordering::Relaxed),
            ),
            (
                "pangya_packet_plaintext_bytes_total",
                "service=\"game\",direction=\"in\"",
                self.game_bytes_in.load(Ordering::Relaxed),
            ),
            (
                "pangya_packet_plaintext_bytes_total",
                "service=\"game\",direction=\"out\"",
                self.game_bytes_out.load(Ordering::Relaxed),
            ),
            (
                "pangya_game_auth_total",
                "outcome=\"success\"",
                self.game_auth_success.load(Ordering::Relaxed),
            ),
            (
                "pangya_game_auth_total",
                "outcome=\"rejected\"",
                self.game_auth_rejected.load(Ordering::Relaxed),
            ),
            (
                "pangya_game_auth_total",
                "outcome=\"identity_mismatch\"",
                self.game_auth_mismatch.load(Ordering::Relaxed),
            ),
            (
                "pangya_game_auth_total",
                "outcome=\"duplicate\"",
                self.game_auth_duplicate.load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"accept_global\"",
                self.game_rate[0].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"accept_source\"",
                self.game_rate[1].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"connection_global\"",
                self.game_rate[2].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"connection_source\"",
                self.game_rate[3].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"auth_global\"",
                self.game_rate[4].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"auth_source\"",
                self.game_rate[5].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"packet_global\"",
                self.game_rate[6].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"packet_source\"",
                self.game_rate[7].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"packet_or_bytes_connection\"",
                self.game_rate[8].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"bytes_global\"",
                self.game_rate[9].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"bytes_source\"",
                self.game_rate[10].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"room_commands_connection\"",
                self.game_rate[11].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rate_limit_total",
                "class=\"chat_connection\"",
                self.game_rate[12].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_room_events_total",
                "event=\"listed\"",
                self.game_room[0].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_room_events_total",
                "event=\"created\"",
                self.game_room[1].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_room_events_total",
                "event=\"joined\"",
                self.game_room[2].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_room_events_total",
                "event=\"left\"",
                self.game_room[3].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_room_events_total",
                "event=\"settings_changed\"",
                self.game_room[4].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_room_events_total",
                "event=\"ready_changed\"",
                self.game_room[5].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_room_events_total",
                "event=\"kicked\"",
                self.game_room[6].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_room_events_total",
                "event=\"state_sent\"",
                self.game_room[7].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_room_events_total",
                "event=\"closed\"",
                self.game_room[8].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_rooms_active",
                "service=\"game\"",
                self.game_active_rooms.load(Ordering::Relaxed),
            ),
            (
                "pangya_game_queue_events_total",
                "event=\"lobby_rejected\"",
                self.game_queue[0].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_queue_events_total",
                "event=\"outbound_dropped\"",
                self.game_queue[1].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_chat_events_total",
                "event=\"accepted\"",
                self.game_chat[0].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_chat_events_total",
                "event=\"delivered\"",
                self.game_chat[1].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_chat_events_total",
                "event=\"rate_limited\"",
                self.game_chat[2].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_unknown_opcode_actions_total",
                "action=\"disconnected\"",
                self.game_unknown[0].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_unknown_opcode_actions_total",
                "action=\"ignored\"",
                self.game_unknown[1].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_unknown_opcode_actions_total",
                "action=\"captured\"",
                self.game_unknown[2].load(Ordering::Relaxed),
            ),
            (
                "pangya_game_unknown_opcode_actions_total",
                "action=\"strike_limit\"",
                self.game_unknown[3].load(Ordering::Relaxed),
            ),
        ];
        for (name, labels, value) in values {
            let _ = writeln!(output, "{name}{{{labels}}} {value}");
        }
        output
    }
}

impl LoginObserver for M2Metrics {
    fn accepted(&self, connection_id: ConnectionId, source: &SourceAddressPrefix) {
        self.accepted_login.fetch_add(1, Ordering::Relaxed);
        self.active_login.fetch_add(1, Ordering::Relaxed);
        tracing::info!(connection_id = connection_id.get(), service = "login", client_profile = "us_852", source_prefix = %source, "connection accepted");
    }

    fn closed(&self, outcome: ConnectionTermination) {
        self.active_login.fetch_sub(1, Ordering::Relaxed);
        match outcome {
            ConnectionTermination::Completed => &self.closed_complete,
            ConnectionTermination::Rejected => &self.closed_rejected,
            ConnectionTermination::Cancelled => &self.closed_cancelled,
            ConnectionTermination::PeerClosed => &self.closed_peer_closed,
            ConnectionTermination::Timeout => &self.closed_timeout,
            ConnectionTermination::Limited => &self.closed_limited,
            ConnectionTermination::Protocol => &self.closed_protocol,
            ConnectionTermination::Error => &self.closed_error,
        }
        .fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            service = "login",
            reason = outcome.label(),
            "connection closed"
        );
    }

    fn frame(&self, direction: &'static str, opcode: u16, bytes: usize) {
        let bytes = u64::try_from(bytes).map_or(u64::MAX, |value| value);
        if direction == "in" {
            self.packets_in.fetch_add(1, Ordering::Relaxed);
            self.plaintext_bytes_in.fetch_add(bytes, Ordering::Relaxed);
        } else {
            self.packets_out.fetch_add(1, Ordering::Relaxed);
            self.plaintext_bytes_out.fetch_add(bytes, Ordering::Relaxed);
        }
        tracing::debug!(service = "login", direction, opcode, "packet");
    }

    fn login(&self, outcome: &'static str) {
        match outcome {
            "success" => &self.login_success,
            "duplicate" => &self.login_duplicate,
            _ => &self.login_rejected,
        }
        .fetch_add(1, Ordering::Relaxed);
    }

    fn authenticated(&self, account_id: AccountId) {
        tracing::Span::current().record("account_id", account_id.get());
        tracing::info!(
            service = "login",
            account_id = account_id.get(),
            "account authenticated"
        );
    }

    fn credential_worker(&self, outcome: CredentialWorkerOutcome) {
        match outcome {
            CredentialWorkerOutcome::Overload => &self.credential_overload,
            CredentialWorkerOutcome::Timeout => &self.credential_timeout,
            CredentialWorkerOutcome::OperationalError => &self.credential_operational,
        }
        .fetch_add(1, Ordering::Relaxed);
    }

    fn protocol_error(&self, class: ProtocolMetricClass) {
        match class {
            ProtocolMetricClass::Decode => &self.protocol_decode,
            ProtocolMetricClass::DecodeTruncated => &self.protocol_decode_truncated,
            ProtocolMetricClass::DecodeLimit => &self.protocol_decode_limit,
            ProtocolMetricClass::DecodeOverflow => &self.protocol_decode_overflow,
            ProtocolMetricClass::DecodeMissingTerminator => {
                &self.protocol_decode_missing_terminator
            }
            ProtocolMetricClass::DecodeInvalid => &self.protocol_decode_invalid,
            ProtocolMetricClass::Io => &self.protocol_io,
            ProtocolMetricClass::Crypto => &self.protocol_crypto,
            ProtocolMetricClass::EncodeOrCompress => &self.protocol_encode,
            ProtocolMetricClass::InvalidState => &self.protocol_invalid_state,
            ProtocolMetricClass::UnknownOpcode => &self.protocol_unknown_opcode,
        }
        .fetch_add(1, Ordering::Relaxed);
    }

    fn unknown_opcode(&self, bucket: UnknownOpcodeBucket) {
        match bucket {
            UnknownOpcodeBucket::Low => &self.unknown_low,
            UnknownOpcodeBucket::Other => &self.unknown_other,
        }
        .fetch_add(1, Ordering::Relaxed);
    }

    fn db_query(&self, class: DbQueryClass) {
        match class {
            DbQueryClass::Fast => &self.db_query_fast,
            DbQueryClass::Slow => &self.db_query_slow,
            DbQueryClass::Error => &self.db_query_error,
        }
        .fetch_add(1, Ordering::Relaxed);
    }

    fn rate_limited(&self, class: RateLimitClass) {
        match class {
            RateLimitClass::AcceptGlobal => &self.rate_accept_global,
            RateLimitClass::AcceptSource => &self.rate_accept_source,
            RateLimitClass::ConnectionGlobal => &self.rate_connection_global,
            RateLimitClass::ConnectionSource => &self.rate_connection_source,
            RateLimitClass::LoginGlobal => &self.rate_login_global,
            RateLimitClass::LoginSource => &self.rate_login_source,
            RateLimitClass::LoginUsername => &self.rate_login_username,
            RateLimitClass::PacketGlobal => &self.rate_packet_global,
            RateLimitClass::PacketSource => &self.rate_packet_source,
            RateLimitClass::PacketOrBytesConnection => &self.rate_packet_connection,
            RateLimitClass::BytesGlobal => &self.rate_bytes_global,
            RateLimitClass::BytesSource => &self.rate_bytes_source,
        }
        .fetch_add(1, Ordering::Relaxed);
    }
}

impl GameObserver for M2Metrics {
    fn accepted(&self, connection_id: GameConnectionId, source: &SourceAddressPrefix) {
        self.game_accepted.fetch_add(1, Ordering::Relaxed);
        self.game_active.fetch_add(1, Ordering::Relaxed);
        tracing::info!(connection_id = connection_id.get(), service = "game", client_profile = "us_852_synthetic_m3", source_prefix = %source, "connection accepted");
    }

    fn closed(&self, outcome: GameTermination) {
        self.game_active.fetch_sub(1, Ordering::Relaxed);
        match outcome {
            GameTermination::PeerClosed => &self.game_closed_peer,
            GameTermination::Cancelled => &self.game_closed_cancelled,
            GameTermination::Rejected => &self.game_closed_rejected,
            GameTermination::Timeout => &self.game_closed_timeout,
            GameTermination::Limited => &self.game_closed_limited,
            GameTermination::Protocol => &self.game_closed_protocol,
            GameTermination::Error => &self.game_closed_error,
        }
        .fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            service = "game",
            reason = outcome.label(),
            "connection closed"
        );
    }

    fn frame(&self, direction: &'static str, opcode: u16, bytes: usize) {
        let bytes = u64::try_from(bytes).map_or(u64::MAX, |value| value);
        if direction == "in" {
            self.game_packets_in.fetch_add(1, Ordering::Relaxed);
            self.game_bytes_in.fetch_add(bytes, Ordering::Relaxed);
        } else {
            self.game_packets_out.fetch_add(1, Ordering::Relaxed);
            self.game_bytes_out.fetch_add(bytes, Ordering::Relaxed);
        }
        tracing::debug!(service = "game", direction, opcode, "packet");
    }

    fn authentication(&self, outcome: &'static str) {
        match outcome {
            "success" => &self.game_auth_success,
            "identity_mismatch" => &self.game_auth_mismatch,
            "duplicate" => &self.game_auth_duplicate,
            _ => &self.game_auth_rejected,
        }
        .fetch_add(1, Ordering::Relaxed);
    }

    fn rate_limited(&self, class: GameRateClass) {
        let index = match class {
            GameRateClass::AcceptGlobal => 0,
            GameRateClass::AcceptSource => 1,
            GameRateClass::ConnectionGlobal => 2,
            GameRateClass::ConnectionSource => 3,
            GameRateClass::AuthGlobal => 4,
            GameRateClass::AuthSource => 5,
            GameRateClass::PacketGlobal => 6,
            GameRateClass::PacketSource => 7,
            GameRateClass::PacketOrBytesConnection => 8,
            GameRateClass::BytesGlobal => 9,
            GameRateClass::BytesSource => 10,
            GameRateClass::RoomCommandsConnection => 11,
            GameRateClass::ChatConnection => 12,
        };
        self.game_rate[index].fetch_add(1, Ordering::Relaxed);
    }

    fn room(&self, event: GameRoomObservation) {
        let index = match event {
            GameRoomObservation::Listed => 0,
            GameRoomObservation::Created => 1,
            GameRoomObservation::Joined => 2,
            GameRoomObservation::Left => 3,
            GameRoomObservation::SettingsChanged => 4,
            GameRoomObservation::ReadyChanged => 5,
            GameRoomObservation::Kicked => 6,
            GameRoomObservation::StateSent => 7,
            GameRoomObservation::Closed => 8,
        };
        self.game_room[index].fetch_add(1, Ordering::Relaxed);
        match event {
            GameRoomObservation::Created => {
                self.game_active_rooms.fetch_add(1, Ordering::Relaxed);
            }
            GameRoomObservation::Closed => {
                let _ = self.game_active_rooms.fetch_update(
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                    |active| Some(active.saturating_sub(1)),
                );
            }
            GameRoomObservation::Listed
            | GameRoomObservation::Joined
            | GameRoomObservation::Left
            | GameRoomObservation::SettingsChanged
            | GameRoomObservation::ReadyChanged
            | GameRoomObservation::Kicked
            | GameRoomObservation::StateSent => {}
        }
    }

    fn queue(&self, event: GameQueueObservation) {
        let index = match event {
            GameQueueObservation::LobbyRejected => 0,
            GameQueueObservation::OutboundDropped => 1,
        };
        self.game_queue[index].fetch_add(1, Ordering::Relaxed);
    }

    fn chat(&self, event: GameChatObservation) {
        let index = match event {
            GameChatObservation::Accepted => 0,
            GameChatObservation::Delivered => 1,
            GameChatObservation::RateLimited => 2,
        };
        self.game_chat[index].fetch_add(1, Ordering::Relaxed);
    }

    fn unknown(&self, event: GameUnknownObservation) {
        let index = match event {
            GameUnknownObservation::Disconnected => 0,
            GameUnknownObservation::Ignored => 1,
            GameUnknownObservation::Captured => 2,
            GameUnknownObservation::StrikeLimit => 3,
        };
        self.game_unknown[index].fetch_add(1, Ordering::Relaxed);
    }

    fn authenticated(&self, account_id: AccountId) {
        tracing::Span::current().record("account_id", account_id.get());
        tracing::info!(
            service = "game",
            account_id = account_id.get(),
            "handover authenticated"
        );
    }
}

/// Readiness/liveness state shared with the admin HTTP server.
#[derive(Debug)]
pub struct HealthState {
    config_valid: AtomicBool,
    database_migrated: AtomicBool,
    login_bound: AtomicBool,
    game_required: AtomicBool,
    catalog_loaded: AtomicBool,
    game_bound: AtomicBool,
    shutting_down: AtomicBool,
    heartbeat_millis: AtomicU64,
    heartbeat_stale_after: Duration,
    metrics_enabled: bool,
    metrics: Arc<M2Metrics>,
}

impl HealthState {
    /// Creates non-ready health state with an initial live heartbeat.
    #[must_use]
    pub fn new(
        metrics: Arc<M2Metrics>,
        heartbeat_stale_after: Duration,
        metrics_enabled: bool,
    ) -> Self {
        Self {
            config_valid: AtomicBool::new(false),
            database_migrated: AtomicBool::new(false),
            login_bound: AtomicBool::new(false),
            game_required: AtomicBool::new(false),
            catalog_loaded: AtomicBool::new(false),
            game_bound: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            heartbeat_millis: AtomicU64::new(now_millis()),
            heartbeat_stale_after,
            metrics_enabled,
            metrics,
        }
    }

    /// Sets validated-config readiness.
    pub fn set_config_valid(&self, value: bool) {
        self.config_valid.store(value, Ordering::Release);
    }
    /// Sets migrated-database readiness.
    pub fn set_database_migrated(&self, value: bool) {
        self.database_migrated.store(value, Ordering::Release);
        self.metrics.set_db_ready(value);
    }
    /// Sets required LoginService-listener readiness.
    pub fn set_login_bound(&self, value: bool) {
        self.login_bound.store(value, Ordering::Release);
    }
    /// Configures whether M3 catalog/GameService gates are required.
    pub fn set_game_required(&self, value: bool) {
        self.game_required.store(value, Ordering::Release);
    }
    /// Sets immutable catalog readiness.
    pub fn set_catalog_loaded(&self, value: bool) {
        self.catalog_loaded.store(value, Ordering::Release);
    }
    /// Sets required GameService listener readiness.
    pub fn set_game_bound(&self, value: bool) {
        self.game_bound.store(value, Ordering::Release);
    }
    /// Marks shutdown before listener cancellation, forcing readiness false.
    pub fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
    }
    /// Updates event-loop heartbeat time.
    pub fn heartbeat(&self) {
        self.heartbeat_millis.store(now_millis(), Ordering::Release);
    }
    /// Returns true only when base and any enabled M3 dependencies/listeners are ready.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.config_valid.load(Ordering::Acquire)
            && self.database_migrated.load(Ordering::Acquire)
            && self.login_bound.load(Ordering::Acquire)
            && (!self.game_required.load(Ordering::Acquire)
                || (self.catalog_loaded.load(Ordering::Acquire)
                    && self.game_bound.load(Ordering::Acquire)))
            && !self.shutting_down.load(Ordering::Acquire)
    }
    /// Returns event-loop liveness independent of DB readiness.
    #[must_use]
    pub fn live(&self) -> bool {
        let age = now_millis().saturating_sub(self.heartbeat_millis.load(Ordering::Acquire));
        age <= duration_millis(self.heartbeat_stale_after)
    }
}

#[derive(Serialize)]
struct HealthBody {
    status: &'static str,
}

/// Builds the read-only admin router.
pub fn admin_router(state: Arc<HealthState>) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/metrics", get(metrics))
        .with_state(state)
}

async fn live(State(state): State<Arc<HealthState>>) -> Response {
    health_response(state.live())
}

async fn ready(State(state): State<Arc<HealthState>>) -> Response {
    health_response(state.ready())
}

async fn metrics(State(state): State<Arc<HealthState>>) -> Response {
    if !state.metrics_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        state.metrics.render(),
    )
        .into_response()
}

fn health_response(healthy: bool) -> Response {
    let status = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = if healthy { "ok" } else { "unavailable" };
    (status, Json(HealthBody { status: body })).into_response()
}

/// Serves the admin router until cancellation using Axum 0.8's listener API.
///
/// # Errors
/// Returns an HTTP serving I/O failure.
pub async fn serve_admin(
    listener: TcpListener,
    state: Arc<HealthState>,
    shutdown: CancellationToken,
) -> Result<(), std::io::Error> {
    axum::serve(listener, admin_router(state))
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
}

fn now_millis() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0_u128, |duration| duration.as_millis());
    u64::try_from(millis).map_or(u64::MAX, |value| value)
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).map_or(u64::MAX, |value| value)
}

/// Marker retained for the M1 crate-boundary test.
#[must_use]
pub const fn crate_boundary() -> &'static str {
    "observability"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_transitions_and_liveness_ignore_database() {
        let metrics = Arc::new(M2Metrics::default());
        let health = HealthState::new(metrics, Duration::from_secs(10), true);
        assert!(!health.ready());
        assert!(health.live());
        health.set_config_valid(true);
        health.set_database_migrated(true);
        health.set_login_bound(true);
        assert!(health.ready());
        health.begin_shutdown();
        assert!(!health.ready());
        assert!(health.live());
    }

    #[test]
    fn optional_game_readiness_requires_catalog_and_listener_only_when_enabled() {
        let metrics = Arc::new(M2Metrics::default());
        let health = HealthState::new(metrics, Duration::from_secs(10), true);
        health.set_config_valid(true);
        health.set_database_migrated(true);
        health.set_login_bound(true);
        assert!(health.ready());
        health.set_game_required(true);
        assert!(!health.ready());
        health.set_catalog_loaded(true);
        assert!(!health.ready());
        health.set_game_bound(true);
        assert!(health.ready());
    }

    #[test]
    fn metrics_never_contain_synthetic_secret_or_token() {
        let metrics = M2Metrics::default();
        metrics.login("rejected");
        let rendered = metrics.render();
        assert!(!rendered.contains("0123456789abcdef0123456789abcdef"));
        assert!(!rendered.contains("synthetic-bearer"));
        assert!(rendered.contains("pangya_login_attempts_total"));
    }

    #[test]
    fn connection_termination_metrics_distinguish_completion_cancellation_and_eof() {
        let metrics = M2Metrics::default();
        metrics.active_login.store(3, Ordering::Relaxed);
        LoginObserver::closed(&metrics, ConnectionTermination::Completed);
        LoginObserver::closed(&metrics, ConnectionTermination::Cancelled);
        LoginObserver::closed(&metrics, ConnectionTermination::PeerClosed);
        let rendered = metrics.render();
        for reason in ["complete", "cancelled", "peer_closed"] {
            assert!(rendered.contains(&format!("reason=\"{reason}\"}} 1")));
        }
    }

    #[test]
    fn io_and_detailed_decode_metrics_have_fixed_labels_and_counters() {
        let metrics = M2Metrics::default();
        metrics.protocol_error(ProtocolMetricClass::Io);
        metrics.protocol_error(ProtocolMetricClass::DecodeTruncated);
        let rendered = metrics.render();
        assert!(
            rendered.contains("pangya_protocol_errors_total{service=\"login\",class=\"io\"} 1")
        );
        assert!(rendered.contains(
            "pangya_protocol_errors_total{service=\"login\",class=\"decode_truncated\"} 1"
        ));
    }

    #[test]
    fn game_m4_metrics_use_only_fixed_labels_and_active_rooms_never_underflows() {
        let metrics = M2Metrics::default();
        GameObserver::rate_limited(&metrics, GameRateClass::RoomCommandsConnection);
        GameObserver::rate_limited(&metrics, GameRateClass::ChatConnection);
        GameObserver::room(&metrics, GameRoomObservation::Closed);
        GameObserver::room(&metrics, GameRoomObservation::Created);
        GameObserver::room(&metrics, GameRoomObservation::SettingsChanged);
        GameObserver::queue(&metrics, GameQueueObservation::OutboundDropped);
        GameObserver::chat(&metrics, GameChatObservation::Delivered);
        GameObserver::unknown(&metrics, GameUnknownObservation::Captured);

        let rendered = metrics.render();
        for expected in [
            "pangya_game_rate_limit_total{class=\"room_commands_connection\"} 1",
            "pangya_game_rate_limit_total{class=\"chat_connection\"} 1",
            "pangya_game_rooms_active{service=\"game\"} 1",
            "pangya_game_room_events_total{event=\"closed\"} 1",
            "pangya_game_room_events_total{event=\"created\"} 1",
            "pangya_game_room_events_total{event=\"settings_changed\"} 1",
            "pangya_game_queue_events_total{event=\"outbound_dropped\"} 1",
            "pangya_game_chat_events_total{event=\"delivered\"} 1",
            "pangya_game_unknown_opcode_actions_total{action=\"captured\"} 1",
        ] {
            assert!(rendered.contains(expected), "missing {expected}");
        }
        for secret_or_unbounded_label in [
            "0123456789abcdef0123456789abcdef",
            "synthetic-bearer",
            "room_id=",
            "user_id=",
            "account_id=",
            "text=",
        ] {
            assert!(!rendered.contains(secret_or_unbounded_label));
        }
    }
}
