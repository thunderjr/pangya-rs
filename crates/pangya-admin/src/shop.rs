//! The live shop overlay.
//!
//! The catalog is immutable and parsed once at startup, so this is the only way to change
//! what the server sells without a restart. Two things about it are worth stating plainly,
//! because getting either wrong produces a confusing player experience:
//!
//! *It changes what the server charges and permits, never what the client displays.* The
//! client renders shop names, prices and listing from its own IFF tables inside the PAK. An
//! item enabled here that the client does not list is purchasable by the protocol but not
//! reachable through the client's shop UI; a price changed here is charged, while the client
//! goes on showing its own. Both are surfaced in the response so the panel can flag them.
//!
//! *A write republishes immediately.* The game reads a `watch` snapshot rather than the
//! database, so the write path is responsible for pushing the new snapshot — and does so in
//! the same request, before answering.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse as _, Response},
};
use pangya_domain::{ItemSale, ItemTypeId, ShopOverride};
use serde::{Deserialize, Serialize};

use crate::{AdminError, AdminState, Operator, audit};

/// Largest page the overlay browser will return.
const MAX_ITEMS: usize = 300;

#[derive(Debug, Deserialize)]
pub(crate) struct ShopQuery {
    q: Option<String>,
    /// `true` restricts to items the server currently sells.
    offered: Option<bool>,
    /// `true` restricts to items whose server answer differs from the client's.
    drift: Option<bool>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Serialize)]
struct ShopRow {
    type_id: u32,
    type_id_hex: String,
    kind: &'static str,
    name: Option<String>,
    /// The client's own icon stem, for the same reason the catalog carries one.
    icon: Option<String>,
    /// What the client's own tables say, for comparison.
    client_pang: Option<u64>,
    /// The override on this item, if any.
    override_enabled: Option<bool>,
    override_pang: Option<u64>,
    /// What the server will actually charge. `null` means it will refuse to sell.
    effective_pang: Option<u64>,
    /// True when the server's answer differs from the client's own tables.
    ///
    /// Computed here rather than in the panel so one definition of "drift" exists.
    drift: bool,
}

#[derive(Serialize)]
struct ShopBody {
    revision: i64,
    override_count: usize,
    items: Vec<ShopRow>,
}

/// `GET /shop`
pub(crate) async fn list(
    State(state): State<AdminState>,
    _operator: Operator,
    Query(query): Query<ShopQuery>,
) -> Result<Response, AdminError> {
    let catalog = state
        .catalog
        .as_ref()
        .ok_or(AdminError::Conflict("catalog_not_loaded"))?;
    let overlay = state.repository.load_shop_overlay().await?;
    let needle = query
        .q
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());
    let limit = query.limit.unwrap_or(100).clamp(1, MAX_ITEMS);
    let offset = query.offset.unwrap_or(0);

    let items = catalog
        .records()
        .filter_map(|(kind, record)| {
            let definition = record.definition().copied()?;
            let client_pang = match definition.sale {
                ItemSale::Pang(price) => Some(price),
                ItemSale::NotSold => None,
            };
            let entry = overlay.get(definition.type_id);
            let effective_pang =
                overlay
                    .resolve(definition)
                    .and_then(|resolved| match resolved.sale {
                        ItemSale::Pang(price) => Some(price),
                        ItemSale::NotSold => None,
                    });
            Some(ShopRow {
                type_id: definition.type_id.get(),
                type_id_hex: format!("0x{:08x}", definition.type_id.get()),
                kind: crate::catalog::kind_text(kind),
                name: record.name().map(str::to_owned),
                icon: record.icon().map(str::to_owned),
                client_pang,
                override_enabled: entry.and_then(|entry| entry.enabled),
                override_pang: entry.and_then(|entry| entry.pang),
                effective_pang,
                drift: effective_pang != client_pang,
            })
        })
        .filter(|row| {
            query
                .offered
                .is_none_or(|wanted| wanted == row.effective_pang.is_some())
        })
        .filter(|row| query.drift.is_none_or(|wanted| wanted == row.drift))
        .filter(|row| match &needle {
            Some(needle) => {
                row.name
                    .as_ref()
                    .is_some_and(|name| name.to_lowercase().contains(needle))
                    || format!("{:08x}", row.type_id).contains(needle)
            }
            None => true,
        })
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();

    Ok(Json(ShopBody {
        revision: overlay.revision(),
        override_count: overlay.len(),
        items,
    })
    .into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct OverrideBody {
    enabled: Option<bool>,
    pang: Option<u64>,
    note: Option<String>,
}

/// `PUT /shop/{type_id}`
pub(crate) async fn put(
    State(state): State<AdminState>,
    Operator(session): Operator,
    Path(type_id): Path<u32>,
    Json(body): Json<OverrideBody>,
) -> Result<Response, AdminError> {
    if body.enabled.is_none() && body.pang.is_none() {
        // Inheriting both fields is what "no override" means, and the schema refuses it.
        return Err(AdminError::BadRequest("empty_override"));
    }
    if body.pang == Some(0) {
        // Zero is how the client's own tables spell "unavailable"; an override meaning
        // "free" would be indistinguishable from one meaning "not sold".
        return Err(AdminError::BadRequest("zero_price"));
    }
    let item_type_id = ItemTypeId::new(type_id);
    // Refusing an unknown id here rather than storing a row that can never take effect: the
    // overlay resolves against the catalog, so an id the catalog lacks is inert.
    let catalog = state
        .catalog
        .as_ref()
        .ok_or(AdminError::Conflict("catalog_not_loaded"))?;
    if catalog.item_definition(item_type_id).is_none() {
        return Err(AdminError::NotFound);
    }
    if let Some(note) = body.note.as_ref()
        && note.chars().count() > 200
    {
        return Err(AdminError::BadRequest("note_too_long"));
    }

    let revision = state
        .repository
        .set_shop_override(
            session.account_id,
            ShopOverride {
                item_type_id,
                enabled: body.enabled,
                pang: body.pang,
            },
            body.note.clone(),
        )
        .await?;
    republish(&state).await?;
    audit(
        &state,
        session.account_id,
        "shop.override.set",
        None,
        &serde_json::json!({
            "type_id": format!("0x{type_id:08x}"),
            "enabled": body.enabled,
            "pang": body.pang,
            "note": body.note,
            "revision": revision,
        }),
    )
    .await?;
    Ok(Json(serde_json::json!({ "revision": revision })).into_response())
}

/// `DELETE /shop/{type_id}`
pub(crate) async fn delete(
    State(state): State<AdminState>,
    Operator(session): Operator,
    Path(type_id): Path<u32>,
) -> Result<Response, AdminError> {
    let revision = state
        .repository
        .clear_shop_override(ItemTypeId::new(type_id))
        .await?;
    republish(&state).await?;
    audit(
        &state,
        session.account_id,
        "shop.override.clear",
        None,
        &serde_json::json!({ "type_id": format!("0x{type_id:08x}"), "revision": revision }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Reloads the overlay and pushes it to every live game connection.
///
/// Done inside the request, before answering, so an operator who sees a success knows the
/// change is in effect rather than queued. A missing publisher means the game service is not
/// composed in this process, which is normal when `[game]` is disabled.
async fn republish(state: &AdminState) -> Result<(), AdminError> {
    let Some(publisher) = state.shop_overlay.as_ref() else {
        return Ok(());
    };
    let overlay = state.repository.load_shop_overlay().await?;
    // A send failure means every receiver is gone, which is not an operator error.
    let _ = publisher.send(overlay);
    Ok(())
}
