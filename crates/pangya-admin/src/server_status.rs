//! Server status and the course-record leaderboard.
//!
//! Status is a projection of the same `HealthState` and metrics counters `/health/ready` and
//! `/metrics` already expose, shaped as JSON so the panel does not have to parse Prometheus
//! text. It deliberately reports no per-account presence: `active_accounts` is a capacity
//! registry holding no player data, and turning it into a presence list is tracked as
//! `DPS-082` rather than improvised here.

use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse as _, Response},
};
use pangya_domain::CourseId;
use serde::{Deserialize, Serialize};

use crate::{AdminError, AdminState, Operator, page_from, rfc3339};

#[derive(Serialize)]
struct StatusBody {
    /// The same value `/health/ready` reports.
    ready: bool,
    /// Whether the event-loop heartbeat is fresh.
    live: bool,
    /// Whether a catalog was loaded, and therefore whether the shop can work at all.
    catalog_loaded: bool,
    /// Overrides currently in force, and the revision they were read at.
    shop_overrides: usize,
    shop_revision: i64,
    /// Selected gauges, so the panel need not parse the Prometheus exposition.
    connections_login: u64,
    connections_game: u64,
    rooms_active: u64,
    matches_active: u64,
}

/// `GET /server/status`
pub(crate) async fn status(
    State(state): State<AdminState>,
    _operator: Operator,
) -> Result<Response, AdminError> {
    let overlay = state.repository.load_shop_overlay().await?;
    let health = state.health.as_ref();
    let metrics = state.metrics.as_ref();
    Ok(Json(StatusBody {
        ready: health.is_some_and(|health| health.ready()),
        live: health.is_some_and(|health| health.live()),
        catalog_loaded: state.catalog.is_some(),
        shop_overrides: overlay.len(),
        shop_revision: overlay.revision(),
        connections_login: metrics.map_or(0, |metrics| metrics.login_connections_active()),
        connections_game: metrics.map_or(0, |metrics| metrics.game_connections_active()),
        rooms_active: metrics.map_or(0, |metrics| metrics.game_active_rooms()),
        matches_active: metrics.map_or(0, |metrics| metrics.game_matches_active()),
    })
    .into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct LeaderboardQuery {
    course_id: Option<u32>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
struct LeaderboardRow {
    account_id: i64,
    username: String,
    course_id: u32,
    mode: String,
    best_score: i16,
    best_strokes: i16,
    rounds_completed: i64,
    first_achieved_at: String,
}

/// `GET /leaderboard`
pub(crate) async fn leaderboard(
    State(state): State<AdminState>,
    _operator: Operator,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Response, AdminError> {
    let course_id = query
        .course_id
        .map(CourseId::new)
        .transpose()
        .map_err(|_| AdminError::BadRequest("invalid_course_id"))?;
    let rows = state
        .repository
        .list_leaderboard(course_id, page_from(query.limit, query.offset)?)
        .await?;
    let body = rows
        .into_iter()
        .map(|row| LeaderboardRow {
            account_id: row.account_id.get(),
            username: row.username,
            course_id: row.course_id.get(),
            mode: row.mode,
            best_score: row.best_score,
            best_strokes: row.best_strokes,
            rounds_completed: row.rounds_completed,
            first_achieved_at: rfc3339(row.first_achieved_at),
        })
        .collect::<Vec<_>>();
    Ok(Json(body).into_response())
}
