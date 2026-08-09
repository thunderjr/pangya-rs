//! Account listing, detail, and audited mutations.
//!
//! Every mutating handler follows the same shape, and the order matters: perform the
//! repository call first, then write the audit row, then answer. A mutation whose audit row
//! cannot be written is reported as a failure rather than a success, because an unrecorded
//! admin action is worse than a refused one.
//!
//! Nothing here reimplements a domain rule. Balance credits go through
//! `AccountRepository::grant_balance`, which already takes a row lock, refuses on overflow and
//! writes an operator audit line; status changes go through `AccountRepository::set_status`,
//! which also revokes outstanding handovers. This module supplies the HTTP shape and the
//! attribution, not the invariants.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse as _, Response},
};
use pangya_domain::{
    AccountBalances, AccountId, AccountRole, AccountStatus, AdminAccountQuery, AdminAccountSort,
    AdminAccountSummary, AdminLedgerSource, BalanceAssignment, BalanceGrant, Nickname,
};
use pangya_login::CanonicalTransportSecret;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{AdminError, AdminState, Operator, audit, page_from, rfc3339};

#[derive(Debug, Deserialize)]
pub(crate) struct ListQuery {
    q: Option<String>,
    status: Option<String>,
    role: Option<String>,
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
struct AccountRow {
    id: i64,
    username: String,
    nickname: Option<String>,
    status: &'static str,
    role: &'static str,
    setup_state: &'static str,
    rank: u32,
    experience: u64,
    pang: u64,
    points: u64,
    character_count: i64,
    inventory_count: i64,
    created_at: String,
}

fn summary_row(summary: AdminAccountSummary) -> AccountRow {
    AccountRow {
        id: summary.id.get(),
        username: summary.username,
        nickname: summary.nickname,
        status: status_text(summary.status),
        role: summary.role.as_str(),
        setup_state: setup_text(summary.setup_state),
        rank: summary.rank,
        experience: summary.experience,
        pang: summary.pang,
        points: summary.points,
        character_count: summary.character_count,
        inventory_count: summary.inventory_count,
        created_at: rfc3339(summary.created_at),
    }
}

/// `GET /accounts`
pub(crate) async fn list(
    State(state): State<AdminState>,
    _operator: Operator,
    Query(query): Query<ListQuery>,
) -> Result<Response, AdminError> {
    let request = AdminAccountQuery {
        // Trimmed and length-capped: the value becomes an `ILIKE` pattern, and an unbounded
        // one is a cheap way to make PostgreSQL work hard.
        search: query
            .q
            .map(|value| value.trim().chars().take(64).collect::<String>())
            .filter(|value| !value.is_empty()),
        status: query.status.as_deref().map(parse_status).transpose()?,
        role: query.role.as_deref().map(parse_role).transpose()?,
        sort: parse_sort(query.sort.as_deref())?,
        page: page_from(query.limit, query.offset)?,
    };
    let rows = state.repository.list_accounts(request).await?;
    Ok(Json(rows.into_iter().map(summary_row).collect::<Vec<_>>()).into_response())
}

#[derive(Serialize)]
struct CharacterRow {
    id: i64,
    item_type_id: u32,
    starter_key: String,
}

#[derive(Serialize)]
struct InventoryRow {
    id: i64,
    item_type_id: u32,
    quantity: u32,
    class: &'static str,
    starter_key: String,
    durability: Option<u32>,
    expires_at: Option<String>,
    /// Equipped state, resolved here so the panel does not re-derive it from three fields.
    equipped_as: Option<&'static str>,
}

#[derive(Serialize)]
struct EquipmentBody {
    character_id: i64,
    club_item_id: Option<i64>,
    ball_item_id: Option<i64>,
    version: u32,
}

#[derive(Serialize)]
struct AccountDetailBody {
    account: AccountRow,
    selected_character_id: Option<i64>,
    equipment: Option<EquipmentBody>,
    characters: Vec<CharacterRow>,
    inventory: Vec<InventoryRow>,
}

/// `GET /accounts/{id}`
pub(crate) async fn detail(
    State(state): State<AdminState>,
    _operator: Operator,
    Path(id): Path<i64>,
) -> Result<Response, AdminError> {
    let account_id = account_id(id)?;
    let detail = state.repository.load_account_detail(account_id).await?;
    let equipment = detail.equipment.as_ref();
    let body = AccountDetailBody {
        selected_character_id: detail
            .selected_character_id
            .map(pangya_domain::CharacterId::get),
        equipment: equipment.map(|set| EquipmentBody {
            character_id: set.character_id.get(),
            club_item_id: set.club_item_id.map(pangya_domain::InventoryItemId::get),
            ball_item_id: set.ball_item_id.map(pangya_domain::InventoryItemId::get),
            version: set.version,
        }),
        characters: detail
            .characters
            .iter()
            .map(|character| CharacterRow {
                id: character.id.get(),
                item_type_id: character.item_type_id.get(),
                starter_key: character.starter_key.as_str().to_owned(),
            })
            .collect(),
        inventory: detail
            .inventory
            .iter()
            .map(|item| InventoryRow {
                id: item.id.get(),
                item_type_id: item.item_type_id.get(),
                quantity: item.quantity,
                class: inventory_class_text(item.class),
                starter_key: item.starter_key.as_str().to_owned(),
                durability: match item.durability {
                    pangya_domain::InventoryDurability::Durable(value) => Some(value),
                    pangya_domain::InventoryDurability::Nondurable => None,
                },
                expires_at: item.expires_at.map(rfc3339),
                equipped_as: equipment.and_then(|set| {
                    if set.club_item_id == Some(item.id) {
                        Some("club_set")
                    } else if set.ball_item_id == Some(item.id) {
                        Some("ball")
                    } else {
                        None
                    }
                }),
            })
            .collect(),
        account: summary_row(detail.summary),
    };
    Ok(Json(body).into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct PageQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
struct LedgerRow {
    source: &'static str,
    delta: i64,
    balance_after: Option<i64>,
    reason: String,
    reference: String,
    created_at: String,
}

/// `GET /accounts/{id}/ledger`
pub(crate) async fn ledger(
    State(state): State<AdminState>,
    _operator: Operator,
    Path(id): Path<i64>,
    Query(query): Query<PageQuery>,
) -> Result<Response, AdminError> {
    let rows = state
        .repository
        .list_account_ledger(account_id(id)?, page_from(query.limit, query.offset)?)
        .await?;
    let body = rows
        .into_iter()
        .map(|row| LedgerRow {
            source: AdminLedgerSource::as_str(row.source),
            delta: row.delta,
            balance_after: row.balance_after,
            reason: row.reason,
            reference: row.reference,
            created_at: rfc3339(row.created_at),
        })
        .collect::<Vec<_>>();
    Ok(Json(body).into_response())
}

#[derive(Serialize)]
struct MatchRow {
    match_id: String,
    mode: String,
    course_id: u32,
    status: String,
    strokes: Option<i16>,
    score: Option<i16>,
    place: Option<i16>,
    completion: Option<String>,
    pang_reward: Option<i64>,
    experience_reward: Option<i64>,
    created_at: String,
}

/// `GET /accounts/{id}/matches`
pub(crate) async fn matches(
    State(state): State<AdminState>,
    _operator: Operator,
    Path(id): Path<i64>,
    Query(query): Query<PageQuery>,
) -> Result<Response, AdminError> {
    let rows = state
        .repository
        .list_account_matches(account_id(id)?, page_from(query.limit, query.offset)?)
        .await?;
    let body = rows
        .into_iter()
        .map(|row| MatchRow {
            match_id: row.match_id.get().to_string(),
            mode: row.mode,
            course_id: row.course_id.get(),
            status: row.status,
            strokes: row.strokes,
            score: row.score,
            place: row.place,
            completion: row.completion,
            pang_reward: row.pang_reward,
            experience_reward: row.experience_reward,
            created_at: rfc3339(row.created_at),
        })
        .collect::<Vec<_>>();
    Ok(Json(body).into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct PatchAccount {
    status: Option<String>,
    role: Option<String>,
    nickname: Option<String>,
}

/// `PATCH /accounts/{id}`
pub(crate) async fn patch(
    State(state): State<AdminState>,
    Operator(session): Operator,
    Path(id): Path<i64>,
    Json(body): Json<PatchAccount>,
) -> Result<Response, AdminError> {
    let account_id = account_id(id)?;
    if body.status.is_none() && body.role.is_none() && body.nickname.is_none() {
        return Err(AdminError::BadRequest("empty_patch"));
    }
    let now = std::time::SystemTime::now();

    if let Some(status) = body.status.as_deref() {
        let status = parse_status(status)?;
        // An operator locking themselves out is a support call, not a feature.
        if status != AccountStatus::Active && account_id == session.account_id {
            return Err(AdminError::Conflict("cannot_disable_self"));
        }
        // Goes through the domain method, which also revokes outstanding game handovers.
        state.repository.set_status(account_id, status, now).await?;
        if status != AccountStatus::Active {
            // A banned account must not keep an open console either.
            state
                .repository
                .revoke_admin_sessions_for_account(account_id, now)
                .await?;
        }
        audit(
            &state,
            session.account_id,
            "account.status.set",
            Some(account_id),
            &serde_json::json!({ "status": status_text(status) }),
        )
        .await?;
    }

    if let Some(role) = body.role.as_deref() {
        let role = parse_role(role)?;
        // Same reasoning: demoting yourself removes the authority you would need to undo it.
        if role != AccountRole::Admin && account_id == session.account_id {
            return Err(AdminError::Conflict("cannot_demote_self"));
        }
        state
            .repository
            .set_account_role(account_id, role, now)
            .await?;
        audit(
            &state,
            session.account_id,
            "account.role.set",
            Some(account_id),
            &serde_json::json!({ "role": role.as_str() }),
        )
        .await?;
    }

    if let Some(nickname) = body.nickname.as_deref() {
        let nickname =
            Nickname::parse(nickname).map_err(|_| AdminError::BadRequest("invalid_nickname"))?;
        let display = nickname.display().to_owned();
        state.repository.set_nickname(account_id, nickname).await?;
        audit(
            &state,
            session.account_id,
            "account.nickname.set",
            Some(account_id),
            &serde_json::json!({ "nickname": display }),
        )
        .await?;
    }

    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct BalanceBody {
    /// `"grant"` credits; `"set"` assigns an exact value.
    mode: String,
    pang: Option<u64>,
    points: Option<u64>,
}

#[derive(Serialize)]
struct Balances {
    pang: u64,
    points: u64,
}

/// `POST /accounts/{id}/balance`
pub(crate) async fn balance(
    State(state): State<AdminState>,
    Operator(session): Operator,
    Path(id): Path<i64>,
    Json(body): Json<BalanceBody>,
) -> Result<Response, AdminError> {
    let account_id = account_id(id)?;
    let balances: AccountBalances = match body.mode.as_str() {
        "grant" => {
            let grant = BalanceGrant {
                pang: body.pang.unwrap_or(0),
                points: body.points.unwrap_or(0),
            };
            if grant.is_empty() {
                return Err(AdminError::BadRequest("empty_grant"));
            }
            // Reuses the CLI's path: row lock, refuse-on-overflow, operator audit line.
            state.repository.grant_balance(account_id, grant).await?
        }
        "set" => {
            let assignment = BalanceAssignment {
                pang: body.pang,
                points: body.points,
            };
            if assignment.is_empty() {
                return Err(AdminError::BadRequest("empty_assignment"));
            }
            state
                .repository
                .set_balances(account_id, assignment)
                .await?
        }
        _ => return Err(AdminError::BadRequest("invalid_mode")),
    };
    audit(
        &state,
        session.account_id,
        "account.balance.set",
        Some(account_id),
        &serde_json::json!({
            "mode": body.mode,
            "pang": body.pang,
            "points": body.points,
            "pang_after": balances.pang,
            "points_after": balances.points,
        }),
    )
    .await?;
    Ok(Json(Balances {
        pang: balances.pang,
        points: balances.points,
    })
    .into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct PasswordBody {
    #[serde(deserialize_with = "crate::auth::zeroizing_string")]
    password: Zeroizing<String>,
}

/// `POST /accounts/{id}/password`
///
/// Takes a plaintext password and derives the client's legacy 32-hex transport secret from it,
/// so the account can afterwards log in from a real client with the same password. Resetting
/// it to a value the client cannot produce would leave the account unusable.
pub(crate) async fn password(
    State(state): State<AdminState>,
    Operator(session): Operator,
    Path(id): Path<i64>,
    Json(body): Json<PasswordBody>,
) -> Result<Response, AdminError> {
    let account_id = account_id(id)?;
    let secret: CanonicalTransportSecret = crate::auth::derive_transport_secret(&body.password)
        .map_err(|_| AdminError::BadRequest("invalid_password"))?;
    let hash = state
        .credentials
        .hash(secret)
        .await
        .map_err(|_| AdminError::Storage)?;
    state.repository.set_credential(account_id, hash).await?;
    // Whoever held a session under the old password loses it.
    state
        .repository
        .revoke_admin_sessions_for_account(account_id, std::time::SystemTime::now())
        .await?;
    // The password never reaches the audit row, only the fact that it changed.
    audit(
        &state,
        session.account_id,
        "account.password.reset",
        Some(account_id),
        &serde_json::json!({}),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn account_id(value: i64) -> Result<AccountId, AdminError> {
    AccountId::new(value).map_err(|_| AdminError::BadRequest("invalid_account_id"))
}

fn parse_status(value: &str) -> Result<AccountStatus, AdminError> {
    match value {
        "active" => Ok(AccountStatus::Active),
        "banned" => Ok(AccountStatus::Banned),
        "disabled" => Ok(AccountStatus::Disabled),
        _ => Err(AdminError::BadRequest("invalid_status")),
    }
}

const fn status_text(value: AccountStatus) -> &'static str {
    match value {
        AccountStatus::Active => "active",
        AccountStatus::Banned => "banned",
        AccountStatus::Disabled => "disabled",
    }
}

const fn setup_text(value: pangya_domain::SetupState) -> &'static str {
    match value {
        pangya_domain::SetupState::NeedsNickname => "needs_nickname",
        pangya_domain::SetupState::NeedsStarter => "needs_starter",
        pangya_domain::SetupState::Complete => "complete",
    }
}

const fn inventory_class_text(value: pangya_domain::InventoryClass) -> &'static str {
    match value {
        pangya_domain::InventoryClass::Legacy => "legacy",
        pangya_domain::InventoryClass::ClubSet => "club_set",
        pangya_domain::InventoryClass::Ball => "ball",
        pangya_domain::InventoryClass::Consumable => "consumable",
        pangya_domain::InventoryClass::CharacterPart => "character_part",
        pangya_domain::InventoryClass::Caddie => "caddie",
        pangya_domain::InventoryClass::CaddieItem => "caddie_item",
        pangya_domain::InventoryClass::Mascot => "mascot",
        pangya_domain::InventoryClass::Card => "card",
        pangya_domain::InventoryClass::Furniture => "furniture",
        pangya_domain::InventoryClass::Skin => "skin",
        pangya_domain::InventoryClass::HairStyle => "hair_style",
        pangya_domain::InventoryClass::SetItem => "set_item",
    }
}

fn parse_role(value: &str) -> Result<AccountRole, AdminError> {
    AccountRole::parse(value).map_err(|_| AdminError::BadRequest("invalid_role"))
}

fn parse_sort(value: Option<&str>) -> Result<AdminAccountSort, AdminError> {
    match value {
        None | Some("created_desc") => Ok(AdminAccountSort::CreatedDesc),
        Some("created_asc") => Ok(AdminAccountSort::CreatedAsc),
        Some("pang_desc") => Ok(AdminAccountSort::PangDesc),
        Some("experience_desc") => Ok(AdminAccountSort::ExperienceDesc),
        Some("username_asc") => Ok(AdminAccountSort::UsernameAsc),
        Some(_) => Err(AdminError::BadRequest("invalid_sort")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sort_vocabulary_is_closed() {
        assert_eq!(parse_sort(None), Ok(AdminAccountSort::CreatedDesc));
        assert_eq!(
            parse_sort(Some("pang_desc")),
            Ok(AdminAccountSort::PangDesc)
        );
        // An unknown value is refused rather than silently falling back, so a typo in the
        // panel surfaces as a 400 instead of a quietly wrong ordering.
        assert_eq!(
            parse_sort(Some("pang; DROP TABLE accounts")),
            Err(AdminError::BadRequest("invalid_sort"))
        );
    }

    #[test]
    fn status_and_role_vocabularies_are_closed() {
        assert_eq!(parse_status("banned"), Ok(AccountStatus::Banned));
        assert_eq!(
            parse_status("Banned"),
            Err(AdminError::BadRequest("invalid_status"))
        );
        assert_eq!(parse_role("admin"), Ok(AccountRole::Admin));
        assert_eq!(
            parse_role("root"),
            Err(AdminError::BadRequest("invalid_role"))
        );
    }

    #[test]
    fn a_nonpositive_account_id_is_a_bad_request_not_a_lookup() {
        assert_eq!(
            account_id(0),
            Err(AdminError::BadRequest("invalid_account_id"))
        );
        assert_eq!(
            account_id(-1),
            Err(AdminError::BadRequest("invalid_account_id"))
        );
    }
}
