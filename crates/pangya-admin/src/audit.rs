//! Admin audit log listing.
//!
//! Read-only by construction: `admin_audit_events` carries an append-only trigger, so there is
//! no editing endpoint to write even if one were asked for.

use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse as _, Response},
};
use serde::{Deserialize, Serialize};

use crate::{AdminError, AdminState, Operator, page_from};

/// Bounded page selector shared by admin listings.
#[derive(Debug, Deserialize)]
pub(crate) struct PageQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
struct AuditRow {
    id: i64,
    actor_account_id: i64,
    actor_username: String,
    action: String,
    target_account_id: Option<i64>,
    /// Raw JSON text, forwarded verbatim so the panel can render whatever a verb recorded.
    detail: String,
    occurred_at: String,
}

/// `GET /audit`
pub(crate) async fn list(
    State(state): State<AdminState>,
    _operator: Operator,
    Query(query): Query<PageQuery>,
) -> Result<Response, AdminError> {
    let page = page_from(query.limit, query.offset)?;
    let rows = state.repository.list_admin_audit(page).await?;
    let body = rows
        .into_iter()
        .map(|row| AuditRow {
            id: row.id,
            actor_account_id: row.actor_account_id.get(),
            actor_username: row.actor_username,
            action: row.action,
            target_account_id: row.target_account_id.map(pangya_domain::AccountId::get),
            detail: row.detail,
            occurred_at: crate::rfc3339(row.occurred_at),
        })
        .collect::<Vec<_>>();
    Ok(Json(body).into_response())
}
