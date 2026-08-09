//! Driving the client's own shop tables from the console.
//!
//! [`shop`](crate::shop) makes the shop admin-controlled on the server side: it decides what is
//! charged and what is permitted, live. It cannot change what the client *displays*, because the
//! client renders shop names, prices and listing from IFF tables inside its own PAK series. This
//! module closes that half.
//!
//! It does so without ever authoring anything itself. Authoring reads proprietary client
//! archives, writes a multi-megabyte PAK and stages it into a served tree — a filesystem job that
//! has no business running behind an admin cookie on the one HTTP surface that also mutates
//! player state. So the console renders the document and enqueues it; a worker outside this
//! process claims it and does the work. The database stays the single point of control.
//!
//! The document is the same `catalog.json` `scripts/author-client-iff.py` has always consumed, so
//! a hand-run authoring and a console-driven one produce identical output by construction rather
//! than by discipline.

use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse as _, Response},
};
use pangya_domain::{ItemSale, NewShopPublishRequest, ShopOverlay};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{AdminError, AdminState, Operator, audit};

/// Schema version of the rendered document, matching `author-client-iff.py`.
const DOCUMENT_VERSION: u32 = 1;

/// How many publish attempts the history view returns.
const HISTORY_LIMIT: i64 = 20;

/// One offer line, in the shape the authoring script parses.
#[derive(Clone, Debug, Serialize)]
struct DocumentOffer {
    table: &'static str,
    /// Lowercase `0x`-prefixed, because that is what the script's fixtures and every
    /// hand-written catalog already use.
    type_id: String,
    pang: u64,
}

/// The rendered `catalog.json`.
#[derive(Clone, Debug, Serialize)]
struct Document {
    version: u32,
    /// Rows the client never sold carry no shop metadata at all, so authoring one means
    /// inventing its currency and display nibble. Always on here: an operator enabling an item
    /// in the console means it to appear, and refusing silently would leave them staring at a
    /// published shop that is missing exactly the rows they added.
    invent_shop_metadata: bool,
    /// Only the tables this document actually touches. Naming a table makes the authoring script
    /// rewrite every row in it, including clearing rows this document omits — so listing one
    /// with no offers would disable that whole family.
    managed_tables: Vec<&'static str>,
    offers: Vec<DocumentOffer>,
}

/// Renders the document the client's tables should be authored from.
///
/// Every catalog record whose *effective* sale — the client's own row with the overlay applied —
/// is a Pang price becomes an offer. That is exactly the set the server will trade on, which is
/// what makes a completed publish bring the two halves into agreement.
fn render(catalog: &pangya_data::Catalog, overlay: &ShopOverlay) -> Document {
    let mut offers = Vec::new();
    let mut managed_tables = Vec::new();
    for (kind, record) in catalog.records() {
        // `None` marks a family that must never be authored: a character is not inventory and a
        // course is not an item. See `CatalogKind::authorable_client_table`.
        let Some(table) = kind.authorable_client_table() else {
            continue;
        };
        let Some(definition) = record.definition().copied() else {
            continue;
        };
        let Some(resolved) = overlay.resolve(definition) else {
            continue;
        };
        let ItemSale::Pang(pang) = resolved.sale else {
            continue;
        };
        if !managed_tables.contains(&table) {
            managed_tables.push(table);
        }
        offers.push(DocumentOffer {
            table,
            type_id: format!("0x{:08x}", definition.type_id.get()),
            pang,
        });
    }
    // Deterministic ordering: the same overlay must render byte-identical bytes, or the digest
    // that proves a worker authored what the operator approved would change for no reason.
    managed_tables.sort_unstable();
    offers.sort_unstable_by(|left, right| {
        (left.table, &left.type_id).cmp(&(right.table, &right.type_id))
    });
    Document {
        version: DOCUMENT_VERSION,
        invent_shop_metadata: true,
        managed_tables,
        offers,
    }
}

/// Serializes a document to the exact bytes a worker will author, with its digest.
fn serialize(document: &Document) -> Result<(String, [u8; 32]), AdminError> {
    let text = serde_json::to_string(document).map_err(|_| AdminError::Storage)?;
    let digest = Sha256::digest(text.as_bytes()).into();
    Ok((text, digest))
}

#[derive(Serialize)]
struct TableSummary {
    table: &'static str,
    offers: usize,
}

#[derive(Serialize)]
struct PublishSummaryBody {
    id: i64,
    requested_by: i64,
    overlay_revision: i64,
    offer_count: i32,
    status: String,
    detail: Option<String>,
    client_pak_name: Option<String>,
    requested_at: String,
    started_at: Option<String>,
    finished_at: Option<String>,
}

#[derive(Serialize)]
struct StatusBody {
    /// The overlay the console would publish right now.
    overlay_revision: i64,
    /// How many offers that publish would carry.
    pending_offer_count: usize,
    /// Per-table breakdown, so an operator can see a family about to be emptied.
    tables: Vec<TableSummary>,
    /// Digest of the document as it would be rendered now. Equal to the newest successful
    /// publish's digest exactly when the client is up to date.
    document_sha256: String,
    /// True when the newest successful publish authored a different document than the overlay
    /// would render now — that is, when players' clients are showing a stale shop.
    ///
    /// Computed from digests rather than revisions: an overlay edit that cancels out an earlier
    /// one leaves the revision higher but the shop identical, and telling an operator to
    /// republish for a no-op change trains them to ignore the flag.
    client_behind: bool,
    /// True when a request is queued or running, so the console can refuse a second.
    in_flight: bool,
    /// Newest attempts, newest first.
    history: Vec<PublishSummaryBody>,
}

fn summarize(row: pangya_domain::ShopPublishSummary) -> PublishSummaryBody {
    PublishSummaryBody {
        id: row.id,
        requested_by: row.requested_by.get(),
        overlay_revision: row.overlay_revision,
        offer_count: row.offer_count,
        status: row.status,
        detail: row.detail,
        client_pak_name: row.client_pak_name,
        requested_at: crate::rfc3339(row.requested_at),
        started_at: row.started_at.map(crate::rfc3339),
        finished_at: row.finished_at.map(crate::rfc3339),
    }
}

/// `GET /shop/publish`
pub(crate) async fn status(
    State(state): State<AdminState>,
    _operator: Operator,
) -> Result<Response, AdminError> {
    let catalog = state
        .catalog
        .as_ref()
        .ok_or(AdminError::Conflict("catalog_not_loaded"))?;
    let overlay = state.repository.load_shop_overlay().await?;
    let document = render(catalog, &overlay);
    let (_, digest) = serialize(&document)?;
    let digest_hex = hex(&digest);

    let history = state.repository.list_shop_publishes(HISTORY_LIMIT).await?;
    let in_flight = history
        .iter()
        .any(|row| row.status == "pending" || row.status == "running");
    // Nothing published yet reads as behind: the client is showing whatever archive was authored
    // by hand, which this server has no way to attribute to an overlay.
    let client_behind = history
        .iter()
        .find(|row| row.status == "published")
        .is_none_or(|row| row.document_sha256 != digest);

    let mut tables: Vec<TableSummary> = Vec::new();
    for offer in &document.offers {
        match tables.iter_mut().find(|entry| entry.table == offer.table) {
            Some(entry) => entry.offers += 1,
            None => tables.push(TableSummary {
                table: offer.table,
                offers: 1,
            }),
        }
    }

    Ok(Json(StatusBody {
        overlay_revision: overlay.revision(),
        pending_offer_count: document.offers.len(),
        tables,
        document_sha256: digest_hex,
        client_behind,
        in_flight,
        history: history.into_iter().map(summarize).collect(),
    })
    .into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct DocumentQuery {
    /// Set by the worker, which needs the exact bytes rather than a summary.
    download: Option<bool>,
}

/// `GET /shop/publish/document`
///
/// The rendered document, for review before publishing and for a worker that would rather
/// re-render than trust a queued copy.
pub(crate) async fn document(
    State(state): State<AdminState>,
    _operator: Operator,
    Query(query): Query<DocumentQuery>,
) -> Result<Response, AdminError> {
    let catalog = state
        .catalog
        .as_ref()
        .ok_or(AdminError::Conflict("catalog_not_loaded"))?;
    let overlay = state.repository.load_shop_overlay().await?;
    let document = render(catalog, &overlay);
    let (text, digest) = serialize(&document)?;
    if query.download.unwrap_or(false) {
        return Ok((
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            text,
        )
            .into_response());
    }
    Ok(Json(serde_json::json!({
        "overlay_revision": overlay.revision(),
        "document_sha256": hex(&digest),
        "offer_count": document.offers.len(),
        "managed_tables": document.managed_tables,
    }))
    .into_response())
}

/// `POST /shop/publish`
pub(crate) async fn enqueue(
    State(state): State<AdminState>,
    Operator(session): Operator,
) -> Result<Response, AdminError> {
    let catalog = state
        .catalog
        .as_ref()
        .ok_or(AdminError::Conflict("catalog_not_loaded"))?;
    let overlay = state.repository.load_shop_overlay().await?;
    let document = render(catalog, &overlay);
    if document.offers.is_empty() {
        // Publishing an empty document would author every managed table into an empty shop.
        // That is a legitimate thing to want and a catastrophic thing to do by accident, so it
        // needs a deliberate path rather than an empty overlay and a button.
        return Err(AdminError::BadRequest("empty_document"));
    }
    let (text, digest) = serialize(&document)?;
    let offer_count = i32::try_from(document.offers.len())
        .map_err(|_| AdminError::Conflict("document_too_large"))?;

    let id = state
        .repository
        .enqueue_shop_publish(
            session.account_id,
            NewShopPublishRequest {
                overlay_revision: overlay.revision(),
                document: text,
                document_sha256: digest,
                offer_count,
            },
        )
        .await?;
    audit(
        &state,
        session.account_id,
        "shop.publish.enqueue",
        None,
        &serde_json::json!({
            "request_id": id,
            "overlay_revision": overlay.revision(),
            "offer_count": offer_count,
            "document_sha256": hex(&digest),
        }),
    )
    .await?;
    Ok(Json(serde_json::json!({
        "id": id,
        "offer_count": offer_count,
        "document_sha256": hex(&digest),
    }))
    .into_response())
}

/// `POST /shop/publish/claim`
///
/// Hands the outstanding request to a worker. Returns `204` when nothing is queued, so a polling
/// worker can distinguish "nothing to do" from a failure without parsing a body.
///
/// This is still only bookkeeping: the claim moves a row from `pending` to `running` and returns
/// the document. Nothing here reads a client archive or writes a file — that is the worker's job,
/// deliberately outside this process.
pub(crate) async fn claim(
    State(state): State<AdminState>,
    Operator(session): Operator,
) -> Result<Response, AdminError> {
    let Some(claimed) = state.repository.claim_shop_publish().await? else {
        return Ok(axum::http::StatusCode::NO_CONTENT.into_response());
    };
    audit(
        &state,
        session.account_id,
        "shop.publish.claim",
        None,
        &serde_json::json!({
            "request_id": claimed.id,
            "overlay_revision": claimed.overlay_revision,
        }),
    )
    .await?;
    Ok(Json(serde_json::json!({
        "id": claimed.id,
        "overlay_revision": claimed.overlay_revision,
        "document_sha256": hex(&claimed.document_sha256),
        // Raw JSON text, not a re-serialized value: the digest above is over these exact bytes
        // and a parse/re-emit round trip is free to move them.
        "document": claimed.document,
    }))
    .into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct FinishBody {
    /// `published` or `failed`.
    status: String,
    /// Required on failure; the reason an operator will read.
    detail: Option<String>,
    /// The archive name the client downloads, on success.
    client_pak_name: Option<String>,
    /// Its SHA-256 as lowercase hex, on success.
    client_pak_sha256: Option<String>,
}

/// `POST /shop/publish/{id}/finish`
pub(crate) async fn finish(
    State(state): State<AdminState>,
    Operator(session): Operator,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(body): Json<FinishBody>,
) -> Result<Response, AdminError> {
    let outcome = match body.status.as_str() {
        "published" => {
            let name = body
                .client_pak_name
                .ok_or(AdminError::BadRequest("missing_pak_name"))?;
            // The name reaches a `client_pak_name` column that operator tooling compares against
            // real filenames, so it is held to the same shape the launcher's denylist enforces:
            // a bare archive name, never a path.
            if name.is_empty()
                || name.len() > 64
                || !name.ends_with(".pak")
                || name.contains('/')
                || name.contains('\\')
            {
                return Err(AdminError::BadRequest("invalid_pak_name"));
            }
            let digest = body
                .client_pak_sha256
                .ok_or(AdminError::BadRequest("missing_pak_digest"))?;
            let digest =
                parse_hex32(&digest).ok_or(AdminError::BadRequest("invalid_pak_digest"))?;
            pangya_domain::ShopPublishOutcome::Published {
                client_pak_name: name,
                client_pak_sha256: digest,
            }
        }
        "failed" => {
            let detail = body.detail.unwrap_or_default();
            if detail.is_empty() {
                // A failure that says nothing is indistinguishable from a worker that never ran.
                return Err(AdminError::BadRequest("missing_detail"));
            }
            pangya_domain::ShopPublishOutcome::Failed {
                detail: detail.chars().take(2000).collect(),
            }
        }
        _ => return Err(AdminError::BadRequest("invalid_status")),
    };
    let audited = serde_json::json!({
        "request_id": id,
        "status": body.status,
    });
    state.repository.finish_shop_publish(id, outcome).await?;
    audit(
        &state,
        session.account_id,
        "shop.publish.finish",
        None,
        &audited,
    )
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

/// Parses 64 lowercase hex characters into a digest, refusing anything else.
fn parse_hex32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

fn hex(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_through_the_parser_the_worker_reports_with() {
        let mut bytes = [0u8; 32];
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = u8::try_from(index).expect("index fits");
        }
        assert_eq!(parse_hex32(&hex(&bytes)), Some(bytes));
    }

    #[test]
    fn parse_hex32_refuses_anything_that_is_not_64_lowercase_hex_digits() {
        assert_eq!(parse_hex32(""), None);
        assert_eq!(parse_hex32(&"a".repeat(63)), None);
        assert_eq!(parse_hex32(&"a".repeat(65)), None);
        // Non-hex inside the right length is the case a length check alone would let through.
        assert_eq!(parse_hex32(&"g".repeat(64)), None);
        // Multi-byte characters make byte slicing fall on a character boundary; the parser must
        // return `None` rather than panic.
        assert_eq!(parse_hex32(&"é".repeat(32)), None);
    }

    #[test]
    fn hex_pads_every_byte_to_two_digits() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x0a;
        bytes[31] = 0xff;
        let text = hex(&bytes);
        assert_eq!(text.len(), 64);
        assert!(text.starts_with("0a"));
        assert!(text.ends_with("ff"));
    }

    #[test]
    fn serializing_is_stable_for_one_document() {
        let document = Document {
            version: DOCUMENT_VERSION,
            invent_shop_metadata: true,
            managed_tables: vec!["Ball.iff", "ClubSet.iff"],
            offers: vec![DocumentOffer {
                table: "Ball.iff",
                type_id: "0x14000000".to_owned(),
                pang: 1,
            }],
        };
        let (first, first_digest) = serialize(&document).expect("serialize");
        let (second, second_digest) = serialize(&document).expect("serialize");
        assert_eq!(first, second);
        assert_eq!(first_digest, second_digest);
        // The authoring script parses these exact keys; renaming one silently produces a
        // document it refuses.
        assert!(first.contains("\"managed_tables\""));
        assert!(first.contains("\"invent_shop_metadata\":true"));
        assert!(first.contains("\"type_id\":\"0x14000000\""));
    }
}
