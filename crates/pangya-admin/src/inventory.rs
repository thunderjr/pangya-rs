//! Operator control of characters, inventory and equipment.
//!
//! Everything here writes through `AdminRepository`, which takes the same row locks and
//! satisfies the same triggers the in-game path does. Two invariants are worth calling out
//! because getting them wrong is silent rather than loud:
//!
//! *Equipment writes carry the version the operator read.* A stale version is refused, not
//! merged, exactly as the in-game equip path refuses one — and the write bumps the counter, so
//! a player's next in-game equip is not rejected by a version they never saw.
//!
//! *Item grants are not purchases.* They write `inventory_items` and `admin_audit_events`, and
//! deliberately not `economy_operations` or its ledgers, which record what a *player* did.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse as _, Response},
};
use pangya_domain::{
    AccountId, AdminEquipmentUpdate, AdminItemGrant, AdminItemUpdate, AdminMutationError,
    CharacterId, InventoryClass, InventoryDurability, InventoryItem, InventoryItemId, ItemKind,
    ItemTypeId,
};
use serde::{Deserialize, Serialize};

use crate::{AdminError, AdminState, Operator, audit};

impl From<AdminMutationError> for AdminError {
    fn from(error: AdminMutationError) -> Self {
        match error {
            AdminMutationError::NotFound => Self::NotFound,
            AdminMutationError::NotOwned => Self::Conflict("not_owned"),
            AdminMutationError::InvalidShape => Self::Conflict("invalid_shape"),
            AdminMutationError::AlreadyStacked => Self::Conflict("already_stacked"),
            AdminMutationError::Equipped => Self::Conflict("item_equipped"),
            AdminMutationError::VersionConflict => Self::Conflict("version_conflict"),
            AdminMutationError::CorruptData | AdminMutationError::Storage(_) => Self::Storage,
        }
    }
}

#[derive(Serialize)]
struct ItemBody {
    id: i64,
    item_type_id: u32,
    quantity: u32,
    class: &'static str,
    durability: Option<u32>,
}

fn item_body(item: &InventoryItem) -> ItemBody {
    ItemBody {
        id: item.id.get(),
        item_type_id: item.item_type_id.get(),
        quantity: item.quantity,
        class: class_text(item.class),
        durability: match item.durability {
            InventoryDurability::Durable(value) => Some(value),
            InventoryDurability::Nondurable => None,
        },
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct GrantItemBody {
    type_id: u32,
    quantity: Option<u32>,
    durability: Option<u32>,
}

/// `POST /accounts/{id}/inventory`
///
/// The class is derived from the catalog rather than taken from the request: the schema
/// cross-checks class against quantity and durability, and letting a caller assert a class the
/// item does not have would only produce a constraint violation one layer down.
pub(crate) async fn grant(
    State(state): State<AdminState>,
    Operator(session): Operator,
    Path(id): Path<i64>,
    Json(body): Json<GrantItemBody>,
) -> Result<Response, AdminError> {
    let account_id = account_id(id)?;
    let item_type_id = ItemTypeId::new(body.type_id);
    let catalog = state
        .catalog
        .as_ref()
        .ok_or(AdminError::Conflict("catalog_not_loaded"))?;
    let definition = catalog
        .item_definition(item_type_id)
        .ok_or(AdminError::NotFound)?;
    let class = match definition.kind {
        ItemKind::ClubSet => InventoryClass::ClubSet,
        ItemKind::Ball => InventoryClass::Ball,
        ItemKind::Consumable => InventoryClass::Consumable,
        ItemKind::CharacterPart => InventoryClass::CharacterPart,
        // A character is granted through the character endpoint, which writes a different
        // table; silently doing that here would be surprising.
        ItemKind::Character => return Err(AdminError::Conflict("use_character_endpoint")),
    };
    let item = state
        .repository
        .grant_item(AdminItemGrant {
            account_id,
            item_type_id,
            class,
            quantity: body.quantity.unwrap_or(1),
            durability: body.durability,
        })
        .await?;
    audit(
        &state,
        session.account_id,
        "inventory.item.grant",
        Some(account_id),
        &serde_json::json!({
            "type_id": format!("0x{:08x}", body.type_id),
            "quantity": item.quantity,
            "inventory_id": item.id.get(),
        }),
    )
    .await?;
    Ok(Json(item_body(&item)).into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct PatchItemBody {
    quantity: Option<u32>,
    /// Present-but-null clears durability; absent leaves it alone.
    ///
    /// The double `Option` is the whole point: JSON distinguishes an absent key from an
    /// explicit `null`, and an operator clearing a durability must not be confused with one
    /// who simply did not mention it.
    #[serde(default, deserialize_with = "explicit_option")]
    durability: Option<Option<u32>>,
}

fn explicit_option<'de, D>(deserializer: D) -> Result<Option<Option<u32>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <Option<u32> as Deserialize>::deserialize(deserializer).map(Some)
}

/// `PATCH /accounts/{id}/inventory/{item_id}`
pub(crate) async fn patch(
    State(state): State<AdminState>,
    Operator(session): Operator,
    Path((id, item_id)): Path<(i64, i64)>,
    Json(body): Json<PatchItemBody>,
) -> Result<Response, AdminError> {
    let account_id = account_id(id)?;
    let inventory_id =
        InventoryItemId::new(item_id).map_err(|_| AdminError::BadRequest("invalid_item_id"))?;
    if body.quantity.is_none() && body.durability.is_none() {
        return Err(AdminError::BadRequest("empty_patch"));
    }
    let item = state
        .repository
        .update_item(AdminItemUpdate {
            account_id,
            inventory_id,
            quantity: body.quantity,
            durability: body.durability,
        })
        .await?;
    audit(
        &state,
        session.account_id,
        "inventory.item.update",
        Some(account_id),
        &serde_json::json!({
            "inventory_id": item_id,
            "quantity": body.quantity,
            "durability": body.durability,
        }),
    )
    .await?;
    Ok(Json(item_body(&item)).into_response())
}

/// `DELETE /accounts/{id}/inventory/{item_id}`
pub(crate) async fn delete(
    State(state): State<AdminState>,
    Operator(session): Operator,
    Path((id, item_id)): Path<(i64, i64)>,
) -> Result<Response, AdminError> {
    let account_id = account_id(id)?;
    let inventory_id =
        InventoryItemId::new(item_id).map_err(|_| AdminError::BadRequest("invalid_item_id"))?;
    state
        .repository
        .delete_item(account_id, inventory_id)
        .await?;
    audit(
        &state,
        session.account_id,
        "inventory.item.delete",
        Some(account_id),
        &serde_json::json!({ "inventory_id": item_id }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct GrantCharacterBody {
    type_id: u32,
}

/// `POST /accounts/{id}/characters`
pub(crate) async fn grant_character(
    State(state): State<AdminState>,
    Operator(session): Operator,
    Path(id): Path<i64>,
    Json(body): Json<GrantCharacterBody>,
) -> Result<Response, AdminError> {
    let account_id = account_id(id)?;
    let item_type_id = ItemTypeId::new(body.type_id);
    // Checked against the catalog so an operator cannot create a character the client has no
    // model for, which renders as an invisible player.
    if let Some(catalog) = state.catalog.as_ref()
        && !catalog.contains(pangya_data::CatalogKind::Character, item_type_id)
    {
        return Err(AdminError::NotFound);
    }
    let character = state
        .repository
        .grant_character(account_id, item_type_id)
        .await?;
    audit(
        &state,
        session.account_id,
        "inventory.character.grant",
        Some(account_id),
        &serde_json::json!({
            "type_id": format!("0x{:08x}", body.type_id),
            "character_id": character.id.get(),
        }),
    )
    .await?;
    Ok(Json(serde_json::json!({
        "id": character.id.get(),
        "item_type_id": character.item_type_id.get(),
    }))
    .into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct EquipmentBody {
    character_id: i64,
    club_item_id: Option<i64>,
    ball_item_id: Option<i64>,
    /// The version the operator read. A stale value is refused rather than merged.
    expected_version: u32,
}

/// `PUT /accounts/{id}/equipment`
pub(crate) async fn set_equipment(
    State(state): State<AdminState>,
    Operator(session): Operator,
    Path(id): Path<i64>,
    Json(body): Json<EquipmentBody>,
) -> Result<Response, AdminError> {
    let account_id = account_id(id)?;
    let equipment = state
        .repository
        .set_equipment(AdminEquipmentUpdate {
            account_id,
            character_id: CharacterId::new(body.character_id)
                .map_err(|_| AdminError::BadRequest("invalid_character_id"))?,
            club_item_id: body
                .club_item_id
                .map(InventoryItemId::new)
                .transpose()
                .map_err(|_| AdminError::BadRequest("invalid_item_id"))?,
            ball_item_id: body
                .ball_item_id
                .map(InventoryItemId::new)
                .transpose()
                .map_err(|_| AdminError::BadRequest("invalid_item_id"))?,
            expected_version: body.expected_version,
        })
        .await?;
    audit(
        &state,
        session.account_id,
        "inventory.equipment.set",
        Some(account_id),
        &serde_json::json!({
            "character_id": body.character_id,
            "club_item_id": body.club_item_id,
            "ball_item_id": body.ball_item_id,
            "version_after": equipment.version,
        }),
    )
    .await?;
    Ok(Json(serde_json::json!({
        "character_id": equipment.character_id.get(),
        "club_item_id": equipment.club_item_id.map(InventoryItemId::get),
        "ball_item_id": equipment.ball_item_id.map(InventoryItemId::get),
        "version": equipment.version,
    }))
    .into_response())
}

fn account_id(value: i64) -> Result<AccountId, AdminError> {
    AccountId::new(value).map_err(|_| AdminError::BadRequest("invalid_account_id"))
}

const fn class_text(value: InventoryClass) -> &'static str {
    match value {
        InventoryClass::Legacy => "legacy",
        InventoryClass::ClubSet => "club_set",
        InventoryClass::Ball => "ball",
        InventoryClass::Consumable => "consumable",
        InventoryClass::CharacterPart => "character_part",
    }
}
