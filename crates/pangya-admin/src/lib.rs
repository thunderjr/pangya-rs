#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Authenticated operator admin API, mounted on the existing `[http]` listener.
//!
//! This is the first *mutating* HTTP surface in the project, so three rules are structural
//! rather than conventional.
//!
//! *Authority comes from the same accounts the game uses.* An operator signs in with the
//! credentials a player signs in with, and `accounts.role` decides whether that is enough.
//! There is no second identity system and no second credential policy to keep correct.
//!
//! *Every mutation is audited.* Handlers write one `admin_audit_events` row per action, and a
//! mutation that cannot be audited is not performed.
//!
//! *Failures are uniform at the boundary.* A wrong password, an unknown username, a
//! non-admin account and a banned account all produce the same response, so this cannot be
//! turned into an account-enumeration oracle.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use axum::{
    Json, Router,
    extract::{FromRequestParts, Request, State},
    http::{StatusCode, header, request::Parts},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post, put},
};
use pangya_domain::{
    AccountId, AccountRepository, AccountRole, AdminPage, AdminRepository, AdminSession,
    NewAdminAuditEvent, RepositoryError, SourceAddressPrefix,
};
use pangya_login::{BoundedCredentialExecutor, FixedWindowLimiter, RateDecision};
use serde::Serialize;

mod accounts;
mod audit;
mod auth;
mod catalog;
mod inventory;
mod server_status;
pub mod session;
mod shop;

pub use auth::LoginRequest;

/// Path every route in this crate is mounted under.
pub const ADMIN_PREFIX: &str = "/admin/v1";
/// Name of the session cookie.
pub const SESSION_COOKIE: &str = "pangya_admin_session";
/// Bounded key storage for the sign-in rate limiter.
const LOGIN_LIMITER_CAPACITY: usize = 1024;

/// The repository surface an admin handler needs.
///
/// Two traits rather than one because the split is meaningful: `AccountRepository` is the
/// gameplay contract — its `grant_balance` and `set_status` already take the right locks and
/// write the right audit rows — and `AdminRepository` adds only what the console needs on top.
/// Handlers reach for the gameplay method whenever one exists, so an operator action and the
/// equivalent in-game action cannot drift apart.
pub trait AdminSurface: AdminRepository + AccountRepository {}

impl<T: AdminRepository + AccountRepository> AdminSurface for T {}

/// Validated admin API policy.
#[derive(Clone, Copy, Debug)]
pub struct AdminApiConfig {
    /// How long an issued session remains valid.
    pub session_lifetime: Duration,
    /// Sign-in attempts permitted per source prefix per window.
    pub logins_per_window: u32,
    /// Sign-in rate window.
    pub rate_window: Duration,
}

/// Shared state for every admin handler.
#[derive(Clone)]
pub struct AdminState {
    repository: Arc<dyn AdminSurface>,
    credentials: Arc<BoundedCredentialExecutor>,
    /// The immutable catalog, present only when `[game]` is enabled.
    catalog: Option<pangya_data::Catalog>,
    /// Publishes overlay snapshots to live game connections. Absent when `[game]` is off.
    shop_overlay: Option<Arc<tokio::sync::watch::Sender<pangya_domain::ShopOverlay>>>,
    /// Readiness and liveness, shared with the health router.
    health: Option<Arc<pangya_observability::HealthState>>,
    /// The same counters `/metrics` renders.
    metrics: Option<Arc<pangya_observability::M2Metrics>>,
    config: AdminApiConfig,
    logins: Arc<FixedWindowLimiter<SourceAddressPrefix>>,
}

impl AdminState {
    /// Builds shared state from composed dependencies.
    #[must_use]
    pub fn new(
        repository: Arc<dyn AdminSurface>,
        credentials: Arc<BoundedCredentialExecutor>,
        catalog: Option<pangya_data::Catalog>,
        shop_overlay: Option<Arc<tokio::sync::watch::Sender<pangya_domain::ShopOverlay>>>,
        health: Option<Arc<pangya_observability::HealthState>>,
        metrics: Option<Arc<pangya_observability::M2Metrics>>,
        config: AdminApiConfig,
    ) -> Self {
        let logins = Arc::new(FixedWindowLimiter::new(
            LOGIN_LIMITER_CAPACITY,
            config.logins_per_window,
            config.rate_window,
        ));
        Self {
            repository,
            credentials,
            catalog,
            shop_overlay,
            health,
            metrics,
            config,
            logins,
        }
    }
}

impl std::fmt::Debug for AdminState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdminState")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Builds the authenticated admin router.
///
/// Sign-in and sign-out are outside the authentication layer; everything else is behind it.
pub fn router(state: AdminState) -> Router {
    let authenticated = Router::new()
        .route("/auth/me", get(auth::me))
        .route("/audit", get(audit::list))
        .route("/accounts", get(accounts::list))
        .route(
            "/accounts/{id}",
            get(accounts::detail).patch(accounts::patch),
        )
        .route("/accounts/{id}/ledger", get(accounts::ledger))
        .route("/accounts/{id}/matches", get(accounts::matches))
        .route("/accounts/{id}/balance", post(accounts::balance))
        .route("/accounts/{id}/password", post(accounts::password))
        .route("/catalog", get(catalog::list))
        .route("/catalog/meta", get(catalog::meta))
        .route("/shop", get(shop::list))
        .route("/shop/{type_id}", put(shop::put).delete(shop::delete))
        .route("/accounts/{id}/inventory", post(inventory::grant))
        .route(
            "/accounts/{id}/inventory/{item_id}",
            patch(inventory::patch).delete(inventory::delete),
        )
        .route(
            "/accounts/{id}/characters",
            post(inventory::grant_character),
        )
        .route("/accounts/{id}/equipment", put(inventory::set_equipment))
        .route("/server/status", get(server_status::status))
        .route("/leaderboard", get(server_status::leaderboard))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            authenticate_layer,
        ));
    Router::new()
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .merge(authenticated)
        .with_state(state)
}

/// A uniform admin API failure.
///
/// Variants map to status codes and to a stable machine-readable `error` string. They
/// deliberately do not carry repository detail: an operator UI needs to know that something
/// was refused and why in broad terms, not which constraint fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminError {
    /// Credentials were absent, malformed, expired, or insufficient.
    Unauthorized,
    /// Request shape or values violated policy.
    BadRequest(&'static str),
    /// The addressed record does not exist.
    NotFound,
    /// The request was refused by a domain rule.
    Conflict(&'static str),
    /// Too many attempts from this source.
    RateLimited,
    /// Persistence failed. Deliberately opaque.
    Storage,
}

impl AdminError {
    const fn parts(self) -> (StatusCode, &'static str) {
        match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::BadRequest(reason) => (StatusCode::BAD_REQUEST, reason),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict(reason) => (StatusCode::CONFLICT, reason),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            Self::Storage => (StatusCode::INTERNAL_SERVER_ERROR, "storage"),
        }
    }
}

impl From<RepositoryError> for AdminError {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::NotFound => Self::NotFound,
            RepositoryError::AccountInactive => Self::Conflict("account_inactive"),
            RepositoryError::DuplicateUsername => Self::Conflict("duplicate_username"),
            RepositoryError::DuplicateNickname => Self::Conflict("duplicate_nickname"),
            _ => Self::Storage,
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

impl IntoResponse for AdminError {
    fn into_response(self) -> Response {
        let (status, error) = self.parts();
        if matches!(self, Self::Storage) {
            // The detail stays in the log; the client is told only that it failed.
            tracing::error!(surface = "admin_api", "admin request failed in storage");
        }
        (status, Json(ErrorBody { error })).into_response()
    }
}

/// The authenticated session an admin handler runs under.
///
/// Extracting this type is what proves a handler is behind [`authenticate_layer`]: the
/// extractor reads a value the layer inserted and cannot succeed without it.
#[derive(Clone, Debug)]
pub struct Operator(pub AdminSession);

impl<S> FromRequestParts<S> for Operator
where
    S: Send + Sync,
{
    type Rejection = AdminError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Self>()
            .cloned()
            .ok_or(AdminError::Unauthorized)
    }
}

impl Operator {
    /// Returns the acting account.
    #[must_use]
    pub const fn account_id(&self) -> AccountId {
        self.0.account_id
    }
}

async fn authenticate_layer(
    State(state): State<AdminState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AdminError> {
    let presented = cookie_value(&request, SESSION_COOKIE).ok_or(AdminError::Unauthorized)?;
    let parsed = session::parse(&presented).map_err(|_| AdminError::Unauthorized)?;
    let resolved = state
        .repository
        .resolve_admin_session(pangya_domain::ResolveAdminSession {
            id: parsed.id,
            digest: parsed.digest,
            now: SystemTime::now(),
        })
        .await?
        .ok_or(AdminError::Unauthorized)?;
    debug_assert_eq!(resolved.role, AccountRole::Admin);
    request.extensions_mut().insert(Operator(resolved));
    Ok(next.run(request).await)
}

/// Reads one cookie value from a request without pulling in a cookie-jar dependency.
///
/// Returns an owned value because the borrow cannot outlive the header while the request is
/// being mutated by the layer.
fn cookie_value(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(key, value)| (key == name).then(|| value.to_owned()))
}

/// The peer address, when the listener was served with connection info.
///
/// An extractor rather than a bare helper so a handler cannot silently forget to look, and so
/// a listener served without `ConnectInfo` degrades to the loopback prefix instead of failing.
#[derive(Clone, Copy, Debug)]
pub struct MaybePeer(pub Option<SocketAddr>);

/// Reduces a peer address to the same privacy-minimized prefix the login path persists.
fn source_prefix(address: Option<SocketAddr>) -> SourceAddressPrefix {
    SourceAddressPrefix::from_ip(
        address.map_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), |value| {
            value.ip()
        }),
    )
}

/// Records one audit row, mapping a failure to a refusal rather than a silent success.
async fn audit(
    state: &AdminState,
    actor: AccountId,
    action: &str,
    target: Option<AccountId>,
    detail: &serde_json::Value,
) -> Result<(), AdminError> {
    debug_assert!(detail.is_object(), "audit detail must be a JSON object");
    state
        .repository
        .record_admin_audit(NewAdminAuditEvent {
            actor_account_id: actor,
            action: action.to_owned(),
            target_account_id: target,
            detail: detail.to_string(),
        })
        .await
        .map_err(AdminError::from)
}

fn admit_login(state: &AdminState, prefix: &SourceAddressPrefix) -> Result<(), AdminError> {
    match state.logins.check(prefix.clone(), Instant::now()) {
        RateDecision::Allowed => Ok(()),
        RateDecision::Limited | RateDecision::Capacity => Err(AdminError::RateLimited),
    }
}

fn page_from(limit: Option<i64>, offset: Option<i64>) -> Result<AdminPage, AdminError> {
    AdminPage::new(limit.unwrap_or(50), offset.unwrap_or(0))
        .map_err(|_| AdminError::BadRequest("invalid_page"))
}

/// Renders a timestamp as RFC 3339 UTC, the one wire format the panel parses.
fn rfc3339(value: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(value).to_rfc3339()
}
