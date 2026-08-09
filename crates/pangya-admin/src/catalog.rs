//! Catalog browsing.
//!
//! Read-only. The catalog is parsed once at startup from the client's own IFF tables into an
//! immutable `Arc`, and nothing here can change it — what an operator *can* change is the
//! shop overlay layered on top, which is a separate table and a separate endpoint.
//!
//! The important thing this surface communicates is the difference between the two. The
//! client renders shop names, prices and listing from its own tables inside the PAK; the
//! server decides what it charges and permits. Those can disagree, and an operator needs to
//! see when they do.

use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse as _, Response},
};
use pangya_data::{Catalog, CatalogKind};
use pangya_domain::{ItemSale, ItemStacking};
use serde::{Deserialize, Serialize};

use crate::{AdminError, AdminState, Operator};

/// Largest page the catalog browser will return.
///
/// `Part.iff` alone holds 7,325 rows, so an unbounded listing would be a self-inflicted
/// denial of service on the operator's own browser.
const MAX_ITEMS: usize = 300;

#[derive(Debug, Deserialize)]
pub(crate) struct CatalogQuery {
    kind: Option<String>,
    q: Option<String>,
    /// `true` restricts to items the client sells, `false` to items it does not.
    sold: Option<bool>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Serialize)]
struct CatalogItem {
    type_id: u32,
    /// Hexadecimal spelling, because that is how every other document in this project writes
    /// a type id and a decimal one invites transcription errors.
    type_id_hex: String,
    kind: &'static str,
    /// The client's own display name, absent for a table that carries none.
    name: Option<String>,
    /// The price the client's tables carry, or `null` when the client does not sell it.
    client_pang: Option<u64>,
    max_stack: Option<u32>,
}

#[derive(Serialize)]
struct CatalogMeta {
    /// SHA-256 over canonical declared manifest metadata, as recorded in
    /// `matches.catalog_sha256` and `economy_operations.catalog_sha256`.
    fingerprint: String,
    manifest_version: u32,
    /// Items the client's own tables mark for sale.
    sold_count: usize,
    tables: Vec<TableCount>,
}

#[derive(Serialize)]
struct TableCount {
    kind: &'static str,
    count: usize,
}

/// `GET /catalog`
pub(crate) async fn list(
    State(state): State<AdminState>,
    _operator: Operator,
    Query(query): Query<CatalogQuery>,
) -> Result<Response, AdminError> {
    let catalog = catalog(&state)?;
    let kind = query.kind.as_deref().map(parse_kind).transpose()?;
    let needle = query
        .q
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());
    let limit = query.limit.unwrap_or(100).clamp(1, MAX_ITEMS);
    let offset = query.offset.unwrap_or(0);

    let items = catalog
        .records()
        .filter(|(family, _)| kind.is_none_or(|wanted| *family == wanted))
        .filter(|(_, record)| {
            let sold = matches!(
                record.definition().map(|definition| definition.sale),
                Some(ItemSale::Pang(_))
            );
            query.sold.is_none_or(|wanted| wanted == sold)
        })
        .filter(|(_, record)| match &needle {
            // Matching the hexadecimal spelling too, so pasting a type id out of a log or an
            // evidence document finds the row.
            Some(needle) => {
                record
                    .name()
                    .is_some_and(|name| name.to_lowercase().contains(needle))
                    || format!("{:08x}", record.type_id.get()).contains(needle)
            }
            None => true,
        })
        .skip(offset)
        .take(limit)
        .map(|(family, record)| {
            let definition = record.definition();
            CatalogItem {
                type_id: record.type_id.get(),
                type_id_hex: format!("0x{:08x}", record.type_id.get()),
                kind: kind_text(family),
                name: record.name().map(str::to_owned),
                client_pang: definition.and_then(|definition| match definition.sale {
                    ItemSale::Pang(price) => Some(price),
                    ItemSale::NotSold => None,
                }),
                max_stack: definition.and_then(|definition| match definition.stacking {
                    ItemStacking::Stackable { max_stack } => Some(max_stack),
                    ItemStacking::Unique => None,
                }),
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(items).into_response())
}

/// `GET /catalog/meta`
pub(crate) async fn meta(
    State(state): State<AdminState>,
    _operator: Operator,
) -> Result<Response, AdminError> {
    let catalog = catalog(&state)?;
    let body = CatalogMeta {
        fingerprint: hex(catalog.fingerprint().as_bytes()),
        manifest_version: catalog.manifest_version(),
        sold_count: catalog.shop_offers().len(),
        tables: catalog
            .table_counts()
            .into_iter()
            .map(|(kind, count)| TableCount {
                kind: kind_text(kind),
                count,
            })
            .collect(),
    };
    Ok(Json(body).into_response())
}

/// Returns the loaded catalog, or a refusal explaining why there is none.
///
/// The catalog only exists when `[game]` is enabled. Saying so is more useful than an empty
/// list, which an operator would reasonably read as "the client sells nothing".
fn catalog(state: &AdminState) -> Result<&Catalog, AdminError> {
    state
        .catalog
        .as_ref()
        .ok_or(AdminError::Conflict("catalog_not_loaded"))
}

fn parse_kind(value: &str) -> Result<CatalogKind, AdminError> {
    match value {
        "character" => Ok(CatalogKind::Character),
        "club_set" => Ok(CatalogKind::ClubSet),
        "ball" => Ok(CatalogKind::Ball),
        "consumable" => Ok(CatalogKind::Consumable),
        "character_part" => Ok(CatalogKind::CharacterPart),
        "course" => Ok(CatalogKind::Course),
        _ => Err(AdminError::BadRequest("invalid_kind")),
    }
}

pub(crate) const fn kind_text(value: CatalogKind) -> &'static str {
    match value {
        CatalogKind::Character => "character",
        CatalogKind::ClubSet => "club_set",
        CatalogKind::Ball => "ball",
        CatalogKind::Consumable => "consumable",
        CatalogKind::CharacterPart => "character_part",
        CatalogKind::Course => "course",
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_vocabulary_is_closed_and_round_trips() {
        for name in [
            "character",
            "club_set",
            "ball",
            "consumable",
            "character_part",
            "course",
        ] {
            let kind = parse_kind(name).expect("known kind");
            assert_eq!(kind_text(kind), name);
        }
        assert_eq!(
            parse_kind("caddie"),
            Err(AdminError::BadRequest("invalid_kind")),
            "a family the loader does not parse must be refused, not silently ignored"
        );
    }

    #[test]
    fn the_fingerprint_renders_as_lowercase_hex() {
        assert_eq!(hex(&[0x00, 0x0f, 0xb3, 0xff]), "000fb3ff");
    }
}
