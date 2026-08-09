#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Immutable, bounded synthetic IFF catalog loading for the local M3 GameService slice.
//!
//! U.S. 852 record layouts and sizes remain unattested. M3 validates only an explicit
//! eight-byte synthetic header and the first little-endian `u32` type identifier of
//! each manifest-sized record; every remaining record byte stays opaque.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read as _,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use cap_std::{ambient_authority, fs::Dir};
use pangya_domain::{
    CatalogFingerprint, CourseId, InventoryClass, InventoryDurability, ItemCompatibility,
    ItemDefinition, ItemDurability, ItemKind, ItemSale, ItemStacking, ItemTypeId, OneHoleConfig,
    PlayerSnapshot, StarterGrant,
};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use unicode_normalization::is_nfc;

/// Original M3-M6 manifest schema version (kept byte/fingerprint compatible).
pub const MANIFEST_VERSION: u32 = 1;
/// Exact typed synthetic M7 manifest schema version.
pub const M7_MANIFEST_VERSION: u32 = 2;
/// Real U.S. client catalog schema version.
///
/// Records carry a small-valued word at offset 0 and the type ID at offset 4, and family
/// identity comes from the type ID's high byte rather than the header's binding value.
/// Measured against the acquired client; see `docs/data/US_CLIENT_IFF_STRUCTURE.md`.
pub const CLIENT_MANIFEST_VERSION: u32 = 3;
/// Byte offset of the type ID inside a real client record.
pub const CLIENT_TYPE_ID_OFFSET: usize = 4;
/// Offset of the Pang price in a real client item record.
///
/// Every client table this server reads shares the item header `pangbox/server`
/// (`pangya/iff/item.go`) documents: a four-byte active flag and id, a 40-byte name, a rank byte,
/// a 40-byte icon, then price. Verified against the acquired client by reading known shop rows
/// back out — "Air Knight Utility Set" at 10000 and "Candy Club Set" at 7500 match what the
/// client's own shop displays.
/// Byte offset of a real client record's fixed-width display name.
///
/// Every family shares one 0x90-byte record base — `enabled:u32 @0x00`, `iffId:u32 @0x04`,
/// `name:[40] @0x08`, `minLevel @0x30`, `icon:[40] @0x31`, `price:u32 @0x5c`,
/// `shopFlag @0x68` — confirmed independently by two references:
/// `hsreina--pangya-server/src/Iff/IffManager.IffEntry.pas` (`TIffbase`) and
/// `pangbox--server/pangya/iff/item.go`, the latter already this project's cited source for
/// the price and shop-flag offsets.
pub const CLIENT_NAME_OFFSET: usize = 0x08;
/// Fixed width of that name field.
pub const CLIENT_NAME_BYTES: usize = 40;
/// Byte offset of a real client record's Pang price.
pub const CLIENT_PRICE_OFFSET: usize = 0x5c;
/// Offset of the shop availability flag, immediately after price/discount/condition.
pub const CLIENT_SHOP_FLAG_OFFSET: usize = 0x68;
/// Smallest client record this server can price.
pub const CLIENT_PRICED_RECORD_BYTES: usize = CLIENT_SHOP_FLAG_OFFSET + 2;
/// Price the client tables use to mark a row that is not really for sale.
///
/// Rows carrying it always pair it with a zero shop flag, so it is a belt-and-braces check
/// rather than the primary signal.
pub const CLIENT_UNAVAILABLE_PRICE: u32 = 10_000_000;
/// Stack ceiling applied to client consumables.
///
/// The per-item stack limit has not been located in the client records, so this is a stated
/// server policy rather than data read from the table.
pub const CLIENT_CONSUMABLE_MAX_STACK: u32 = 100;
/// Hard catalog stack bound independent of operator input.
pub const MAX_CATALOG_STACK: u32 = 10_000;
/// Maximum manifest bytes read from an operator mount.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
/// Maximum bytes read from one synthetic IFF file.
pub const MAX_IFF_BYTES: usize = 64 * 1024 * 1024;
/// Maximum manifest entries.
pub const MAX_MANIFEST_FILES: usize = 16;
/// Maximum synthetic record size.
pub const MAX_RECORD_SIZE: usize = 64 * 1024;
/// Synthetic IFF header byte length: `count:u16`, `binding:u16`, `version:u32`.
pub const IFF_HEADER_BYTES: usize = 8;

/// Minimum catalog families required by local synthetic M3.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    /// Character records.
    Character,
    /// Club-set inventory records.
    ClubSet,
    /// Ball inventory records.
    Ball,
    /// Stackable consumable records (required by v2).
    Consumable,
    /// Character-compatible part records (required by v2).
    CharacterPart,
    /// Optional locally generated one-hole course records.
    Course,
}

/// One versioned manifest entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
    /// Canonical relative filename below the configured IFF directory.
    pub filename: PathBuf,
    /// Lowercase SHA-256 hex of the complete file.
    pub sha256: String,
    /// Catalog family.
    pub kind: CatalogKind,
    /// Expected synthetic header record count.
    pub count: u16,
    /// Expected synthetic header binding value.
    pub binding: u16,
    /// Expected synthetic header version value.
    pub version: u32,
    /// Exact byte width of each record, including its four-byte type ID.
    pub record_size: usize,
}

/// Versioned operator manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CatalogManifest {
    /// Manifest schema version.
    pub manifest_version: u32,
    /// Explicit file declarations.
    pub files: Vec<ManifestFile>,
}

/// One minimally parsed catalog record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogRecord {
    /// First four record bytes interpreted as little-endian type ID.
    pub type_id: ItemTypeId,
    /// Remaining unattested v1 record bytes. Exact v2 records always leave this empty.
    pub opaque: Arc<[u8]>,
    local_one_hole_par: Option<u8>,
    definition: Option<ItemDefinition>,
    character_part_slot: Option<u8>,
    name: Option<Box<str>>,
}

impl CatalogRecord {
    /// Returns the explicit local one-hole par only for a generated Course record.
    #[must_use]
    pub const fn local_one_hole_par(&self) -> Option<u8> {
        self.local_one_hole_par
    }

    /// Returns the client's own display name, when the record carries one.
    ///
    /// Present only for real client tables: the synthetic schemas have no name field, and a
    /// record narrower than the shared base yields `None` rather than a guess. Nothing on the
    /// wire is derived from this — it exists so an operator surface can show an item as
    /// something other than a bare hexadecimal type id.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the exact immutable v2 economy definition, if this is an item family.
    #[must_use]
    pub const fn definition(&self) -> Option<&ItemDefinition> {
        self.definition.as_ref()
    }

    /// Returns the closed generated character-part slot (`0..=7`) when applicable.
    #[must_use]
    pub const fn character_part_slot(&self) -> Option<u8> {
        self.character_part_slot
    }
}

#[derive(Debug)]
struct CatalogInner {
    records: BTreeMap<CatalogKind, BTreeMap<u32, CatalogRecord>>,
    offers: Arc<[ItemDefinition]>,
    fingerprint: CatalogFingerprint,
    manifest_version: u32,
}

/// Immutable validated catalog shared across GameService connections.
#[derive(Clone, Debug)]
pub struct Catalog(Arc<CatalogInner>);

impl Catalog {
    /// Loads a manifest and all declared files from an operator-controlled directory.
    ///
    /// # Errors
    /// Rejects path escapes, symlink escapes, non-regular files, size/arithmetic/header/hash
    /// mismatches, trailing bytes, missing required kinds, and duplicate kinds/type IDs.
    pub fn load(directory: &Path, manifest: &Path) -> Result<Self, CatalogError> {
        Self::load_with_pricing(directory, manifest, CatalogPricing::Client)
    }

    /// Loads a catalog and applies an operator pricing policy to it.
    ///
    /// # Errors
    /// Applies the same file and catalog validation as [`Self::load`].
    pub fn load_with_pricing(
        directory: &Path,
        manifest: &Path,
        pricing: CatalogPricing,
    ) -> Result<Self, CatalogError> {
        validate_relative(manifest)?;
        let root = Dir::open_ambient_dir(directory, ambient_authority())
            .map_err(|_| CatalogError::Path)?;
        let manifest_bytes = read_bounded_regular(&root, manifest, MAX_MANIFEST_BYTES)?;
        let manifest_text =
            std::str::from_utf8(&manifest_bytes).map_err(|_| CatalogError::Manifest)?;
        let parsed: CatalogManifest =
            toml::from_str(manifest_text).map_err(|_| CatalogError::Manifest)?;
        let catalog = Self::load_manifest_from_dir(&root, parsed)?;
        Ok(catalog.repriced(pricing))
    }

    /// Returns this catalog with an operator pricing policy applied.
    ///
    /// Repricing deliberately cannot make an unsold item sellable: it rewrites the amount on
    /// rows the client already sells and leaves everything else alone.
    #[must_use]
    pub fn repriced(self, pricing: CatalogPricing) -> Self {
        let CatalogPricing::FlatPang(price) = pricing else {
            return self;
        };
        let reprice = |definition: &ItemDefinition| {
            let mut updated = *definition;
            if matches!(definition.sale, ItemSale::Pang(_)) {
                updated.sale = ItemSale::Pang(price);
            }
            updated
        };
        let inner = &*self.0;
        let records = inner
            .records
            .iter()
            .map(|(kind, table)| {
                let table = table
                    .iter()
                    .map(|(id, record)| {
                        let mut record = record.clone();
                        record.definition = record.definition.as_ref().map(reprice);
                        (*id, record)
                    })
                    .collect();
                (*kind, table)
            })
            .collect();
        let offers = inner.offers.iter().map(reprice).collect::<Vec<_>>();
        Self(Arc::new(CatalogInner {
            records,
            offers: Arc::from(offers),
            fingerprint: inner.fingerprint,
            manifest_version: inner.manifest_version,
        }))
    }

    /// Loads already parsed manifest metadata below one opened directory capability.
    ///
    /// # Errors
    /// Applies the same file and catalog validation as [`Self::load`].
    pub fn load_manifest(root: &Path, manifest: CatalogManifest) -> Result<Self, CatalogError> {
        let root =
            Dir::open_ambient_dir(root, ambient_authority()).map_err(|_| CatalogError::Path)?;
        Self::load_manifest_from_dir(&root, manifest)
    }

    fn load_manifest_from_dir(root: &Dir, manifest: CatalogManifest) -> Result<Self, CatalogError> {
        if !matches!(
            manifest.manifest_version,
            MANIFEST_VERSION | M7_MANIFEST_VERSION | CLIENT_MANIFEST_VERSION
        ) || manifest.files.is_empty()
            || manifest.files.len() > MAX_MANIFEST_FILES
        {
            return Err(CatalogError::Manifest);
        }
        for entry in &manifest.files {
            validate_manifest_entry(entry)?;
        }
        let fingerprint = canonical_fingerprint(&manifest)?;
        let mut records = BTreeMap::new();
        let mut type_ids = BTreeSet::new();
        for entry in &manifest.files {
            if records.contains_key(&entry.kind) {
                return Err(CatalogError::DuplicateKind);
            }
            let bytes = read_bounded_regular(root, &entry.filename, MAX_IFF_BYTES)?;
            if sha256_hex(&bytes) != entry.sha256 {
                return Err(CatalogError::Digest);
            }
            let parsed = parse_iff_bytes_for_schema(manifest.manifest_version, entry, &bytes)?;
            if parsed.keys().any(|type_id| !type_ids.insert(*type_id)) {
                return Err(CatalogError::DuplicateTypeId);
            }
            records.insert(entry.kind, parsed);
        }
        let required: &[CatalogKind] = if manifest.manifest_version == CLIENT_MANIFEST_VERSION {
            &[
                CatalogKind::Character,
                CatalogKind::ClubSet,
                CatalogKind::Ball,
            ]
        } else if manifest.manifest_version == M7_MANIFEST_VERSION {
            &[
                CatalogKind::Character,
                CatalogKind::ClubSet,
                CatalogKind::Ball,
                CatalogKind::Consumable,
                CatalogKind::CharacterPart,
            ]
        } else {
            &[
                CatalogKind::Character,
                CatalogKind::ClubSet,
                CatalogKind::Ball,
            ]
        };
        if required.iter().any(|kind| !records.contains_key(kind)) {
            return Err(CatalogError::MissingKind);
        }
        if manifest.manifest_version == M7_MANIFEST_VERSION {
            validate_v2_cross_references(&records)?;
        }
        let mut sold = records
            .values()
            .flat_map(BTreeMap::values)
            .filter_map(|record| record.definition)
            .filter(|definition| matches!(definition.sale, ItemSale::Pang(_)))
            .collect::<Vec<_>>();
        sold.sort_by_key(|definition| definition.type_id);
        let offers: Arc<[ItemDefinition]> = sold.into();
        Ok(Self(Arc::new(CatalogInner {
            records,
            offers,
            fingerprint,
            manifest_version: manifest.manifest_version,
        })))
    }

    /// Returns the stable SHA-256 of canonical declared manifest metadata.
    #[must_use]
    pub fn fingerprint(&self) -> CatalogFingerprint {
        self.0.fingerprint
    }

    /// Returns the validated manifest schema version.
    #[must_use]
    pub fn manifest_version(&self) -> u32 {
        self.0.manifest_version
    }

    /// Returns sold Pang offers in deterministic global type-ID order.
    #[must_use]
    pub fn shop_offers(&self) -> &[ItemDefinition] {
        &self.0.offers
    }

    /// Looks up a sold Pang offer by globally unique type ID.
    #[must_use]
    pub fn shop_offer(&self, type_id: ItemTypeId) -> Option<&ItemDefinition> {
        self.0
            .offers
            .binary_search_by_key(&type_id, |definition| definition.type_id)
            .ok()
            .map(|index| &self.0.offers[index])
    }

    /// Looks up one record in any family, sold or not.
    ///
    /// The operator catalog browser needs rows the shop does not sell — that is precisely the
    /// set an overlay might make sellable — so this is deliberately wider than
    /// [`Self::shop_offer`].
    #[must_use]
    pub fn find_record(&self, type_id: ItemTypeId) -> Option<(CatalogKind, &CatalogRecord)> {
        self.0
            .records
            .iter()
            .find_map(|(kind, table)| table.get(&type_id.get()).map(|record| (*kind, record)))
    }

    /// Iterates every loaded record, family by family, in deterministic type-ID order.
    pub fn records(&self) -> impl Iterator<Item = (CatalogKind, &CatalogRecord)> {
        self.0
            .records
            .iter()
            .flat_map(|(kind, table)| table.values().map(move |record| (*kind, record)))
    }

    /// Returns how many records each family contributed.
    #[must_use]
    pub fn table_counts(&self) -> Vec<(CatalogKind, usize)> {
        self.0
            .records
            .iter()
            .map(|(kind, table)| (*kind, table.len()))
            .collect()
    }

    /// Looks up any exact v2 item definition, including a not-sold item.
    #[must_use]
    pub fn item_definition(&self, type_id: ItemTypeId) -> Option<&ItemDefinition> {
        self.0
            .records
            .values()
            .find_map(|records| records.get(&type_id.get())?.definition())
    }

    /// Checks a generated character-part compatibility edge.
    #[must_use]
    pub fn part_is_compatible(
        &self,
        part_type_id: ItemTypeId,
        character_type_id: ItemTypeId,
    ) -> bool {
        self.item_definition(part_type_id)
            .is_some_and(|definition| {
                matches!(
                    definition.compatibility,
                    ItemCompatibility::Character(expected) if expected == character_type_id
                )
            })
    }

    /// Returns whether a type ID exists in one declared family.
    #[must_use]
    pub fn contains(&self, kind: CatalogKind, type_id: ItemTypeId) -> bool {
        self.0
            .records
            .get(&kind)
            .is_some_and(|records| records.contains_key(&type_id.get()))
    }

    /// Returns an immutable minimally parsed record.
    #[must_use]
    pub fn record(&self, kind: CatalogKind, type_id: ItemTypeId) -> Option<&CatalogRecord> {
        self.0.records.get(&kind)?.get(&type_id.get())
    }

    /// Returns a checked local one-hole configuration from the optional Course family.
    ///
    /// Only the generated schemas carry a par byte. A real client catalog has none, so this
    /// rejects it rather than inventing one; use [`Self::declared_one_hole_course`] there.
    ///
    /// # Errors
    /// Rejects a missing course, a zero/out-of-range course ID, or invalid generated par.
    pub fn one_hole_course(&self, course_id: CourseId) -> Result<OneHoleConfig, CatalogError> {
        let record = self
            .record(CatalogKind::Course, ItemTypeId::new(course_id.get()))
            .ok_or(CatalogError::Binding)?;
        let par = record.local_one_hole_par().ok_or(CatalogError::Structure)?;
        OneHoleConfig::new(course_id, par).map_err(|_| CatalogError::Structure)
    }

    /// Returns a checked one-hole configuration whose par the operator declared.
    ///
    /// The real U.S. client's `Course.iff` record is a presentation row: identifier, display
    /// and Korean names, map directory, short name, a length-prefixed property XML filename,
    /// and one float. Per-hole par is not in it — it lives in the course's own data inside the
    /// PAK series. So the catalog can prove a course *exists* but cannot supply its par, and
    /// par has to come from configuration. The catalog check is still what makes the value
    /// meaningful: a par declared for a course the client does not have is rejected here
    /// rather than surfacing later as a match against a course that cannot load.
    ///
    /// # Errors
    /// Rejects a course absent from the Course family, or a par outside the domain's range.
    pub fn declared_one_hole_course(
        &self,
        course_id: CourseId,
        par: u8,
    ) -> Result<OneHoleConfig, CatalogError> {
        if !self.contains(CatalogKind::Course, ItemTypeId::new(course_id.get())) {
            return Err(CatalogError::Binding);
        }
        OneHoleConfig::new(course_id, par).map_err(|_| CatalogError::Structure)
    }

    /// Cross-checks configured starter IDs against the minimum catalog.
    ///
    /// # Errors
    /// Requires the character in Character, every item in ClubSet or Ball, and explicitly
    /// equipped stable keys in their corresponding family.
    pub fn validate_starter(&self, starter: &StarterGrant) -> Result<(), CatalogError> {
        if !self.contains(CatalogKind::Character, starter.character.item_type_id) {
            return Err(CatalogError::Binding);
        }
        for item in &starter.items {
            if !self.contains(CatalogKind::ClubSet, item.item_type_id)
                && !self.contains(CatalogKind::Ball, item.item_type_id)
            {
                return Err(CatalogError::Binding);
            }
        }
        if let Some(key) = &starter.equipped_club_key {
            let item = starter
                .items
                .iter()
                .find(|item| &item.key == key)
                .ok_or(CatalogError::Binding)?;
            if !self.contains(CatalogKind::ClubSet, item.item_type_id) {
                return Err(CatalogError::Binding);
            }
        }
        if let Some(key) = &starter.equipped_ball_key {
            let item = starter
                .items
                .iter()
                .find(|item| &item.key == key)
                .ok_or(CatalogError::Binding)?;
            if !self.contains(CatalogKind::Ball, item.item_type_id) {
                return Err(CatalogError::Binding);
            }
        }
        Ok(())
    }

    /// Cross-checks a coherent player snapshot before any bootstrap packet is emitted.
    ///
    /// # Errors
    /// Rejects character or equipped/inventory type IDs absent from the minimum catalog.
    pub fn validate_snapshot(&self, snapshot: &PlayerSnapshot) -> Result<(), CatalogError> {
        if snapshot
            .characters
            .iter()
            .any(|character| !self.contains(CatalogKind::Character, character.item_type_id))
        {
            return Err(CatalogError::Binding);
        }
        for item in &snapshot.inventory {
            let is_club = self.contains(CatalogKind::ClubSet, item.item_type_id);
            let is_ball = self.contains(CatalogKind::Ball, item.item_type_id);
            let is_consumable = self.contains(CatalogKind::Consumable, item.item_type_id);
            let is_part = self.contains(CatalogKind::CharacterPart, item.item_type_id);
            if !is_club && !is_ball && !is_consumable && !is_part {
                return Err(CatalogError::Binding);
            }
            if snapshot.equipment.club_item_id == Some(item.id) && !is_club {
                return Err(CatalogError::Binding);
            }
            if snapshot.equipment.ball_item_id == Some(item.id) && !is_ball {
                return Err(CatalogError::Binding);
            }
            if self.0.manifest_version == M7_MANIFEST_VERSION {
                let definition = self
                    .item_definition(item.item_type_id)
                    .ok_or(CatalogError::Binding)?;
                let expected_class = inventory_class(definition.kind);
                if item.class != InventoryClass::Legacy && item.class != expected_class {
                    return Err(CatalogError::Binding);
                }
                match (definition.durability, item.durability) {
                    (ItemDurability::Nondurable, InventoryDurability::Nondurable)
                    | (ItemDurability::Durable { .. }, InventoryDurability::Durable(_)) => {}
                    _ => return Err(CatalogError::Binding),
                }
            }
        }
        Ok(())
    }
}

/// Redacted catalog loading/validation failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CatalogError {
    /// A path was noncanonical, escaped the root, or was not a regular file/directory.
    #[error("catalog path is invalid")]
    Path,
    /// Manifest syntax, version, size, or entry bounds were invalid.
    #[error("catalog manifest is invalid")]
    Manifest,
    /// File digest did not match its manifest declaration.
    #[error("catalog digest does not match")]
    Digest,
    /// Synthetic header/length/arithmetic/trailing-byte validation failed.
    #[error("synthetic IFF structure is invalid")]
    Structure,
    /// A required catalog family was absent.
    #[error("required catalog kind is missing")]
    MissingKind,
    /// A catalog family appeared more than once.
    #[error("catalog kind is duplicated")]
    DuplicateKind,
    /// A type ID appeared more than once anywhere across the required families.
    #[error("catalog type identifier is duplicated")]
    DuplicateTypeId,
    /// Exact v2 sale/price/stack/durability/slot metadata is invalid.
    #[error("catalog item semantics are invalid")]
    Semantics,
    /// A v2 character-part reference does not bind to Character.
    #[error("catalog cross-reference is invalid")]
    CrossReference,
    /// Starter/snapshot type IDs did not bind to the required catalog family.
    #[error("catalog binding is invalid")]
    Binding,
}

/// Parses one bounded synthetic IFF byte slice according to one manifest entry.
///
/// # Errors
/// Rejects zero counts, header mismatches, invalid record widths, checked arithmetic
/// failure, exact-length mismatch/trailing bytes, and duplicate type IDs.
pub fn parse_iff_bytes(
    entry: &ManifestFile,
    bytes: &[u8],
) -> Result<BTreeMap<u32, CatalogRecord>, CatalogError> {
    validate_manifest_entry(entry)?;
    if bytes.len() > MAX_IFF_BYTES || bytes.len() < IFF_HEADER_BYTES {
        return Err(CatalogError::Structure);
    }
    let count = u16::from_le_bytes([bytes[0], bytes[1]]);
    let binding = u16::from_le_bytes([bytes[2], bytes[3]]);
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if count == 0 || count != entry.count || binding != entry.binding || version != entry.version {
        return Err(CatalogError::Structure);
    }
    let records_bytes = usize::from(count)
        .checked_mul(entry.record_size)
        .ok_or(CatalogError::Structure)?;
    let expected = IFF_HEADER_BYTES
        .checked_add(records_bytes)
        .ok_or(CatalogError::Structure)?;
    if bytes.len() != expected {
        return Err(CatalogError::Structure);
    }
    let mut records = BTreeMap::new();
    for record in bytes[IFF_HEADER_BYTES..].chunks_exact(entry.record_size) {
        let type_id = u32::from_le_bytes([record[0], record[1], record[2], record[3]]);
        let (local_one_hole_par, opaque) = if entry.kind == CatalogKind::Course {
            let course_id = CourseId::new(type_id).map_err(|_| CatalogError::Structure)?;
            let par = *record.get(4).ok_or(CatalogError::Structure)?;
            OneHoleConfig::new(course_id, par).map_err(|_| CatalogError::Structure)?;
            (Some(par), &record[5..])
        } else {
            (None, &record[4..])
        };
        let value = CatalogRecord {
            type_id: ItemTypeId::new(type_id),
            opaque: Arc::from(opaque),
            local_one_hole_par,
            definition: None,
            character_part_slot: None,
            // The synthetic schemas carry no name field.
            name: None,
        };
        if records.insert(type_id, value).is_some() {
            return Err(CatalogError::DuplicateTypeId);
        }
    }
    Ok(records)
}

/// Builds an economy definition from one real client record.
///
/// Only identity, sellability and price are taken from the table. Durability and part
/// compatibility have not been located in these records, so they are stated conservatively
/// rather than guessed: nothing is durable and every part is compatible with any character.
/// Characters and courses are not sellable items and yield no definition.
fn client_definition(
    kind: CatalogKind,
    type_id: ItemTypeId,
    record: &[u8],
) -> Result<Option<ItemDefinition>, CatalogError> {
    let item_kind = match kind {
        CatalogKind::Character | CatalogKind::Course => return Ok(None),
        CatalogKind::ClubSet => ItemKind::ClubSet,
        CatalogKind::Ball => ItemKind::Ball,
        CatalogKind::Consumable => ItemKind::Consumable,
        CatalogKind::CharacterPart => ItemKind::CharacterPart,
    };
    // A table narrower than the priced header carries no price to read. That is not a
    // structural fault — identity still parses and the rest of the catalog stays usable — so
    // such a record simply yields no economy definition.
    if record.len() < CLIENT_PRICED_RECORD_BYTES {
        return Ok(None);
    }
    let price = u64::from(le_u32(record, CLIENT_PRICE_OFFSET)?);
    let shop_flag = record[CLIENT_SHOP_FLAG_OFFSET];
    let sale = if shop_flag == 0 || price == u64::from(CLIENT_UNAVAILABLE_PRICE) || price == 0 {
        ItemSale::NotSold
    } else {
        ItemSale::Pang(price)
    };
    let stacking = if matches!(item_kind, ItemKind::Consumable) {
        ItemStacking::Stackable {
            max_stack: CLIENT_CONSUMABLE_MAX_STACK,
        }
    } else {
        ItemStacking::Unique
    };
    Ok(Some(ItemDefinition {
        type_id,
        kind: item_kind,
        sale,
        stacking,
        durability: ItemDurability::Nondurable,
        compatibility: ItemCompatibility::Any,
    }))
}

/// Reads a real client record's fixed-width display name.
///
/// Returns `None` for a record too narrow to carry one, and for a name that is empty after
/// trimming. Bytes are taken up to the first NUL, and decoded lossily: the U.S. tables are
/// ASCII, but a byte outside it must not cost the whole catalog its load — the name is
/// cosmetic, and every gameplay decision is made from the type id.
fn client_name(record: &[u8]) -> Option<Box<str>> {
    let end = CLIENT_NAME_OFFSET.checked_add(CLIENT_NAME_BYTES)?;
    let field = record.get(CLIENT_NAME_OFFSET..end)?;
    let bytes = field.split(|byte| *byte == 0).next().unwrap_or(field);
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.into())
}

/// How an operator wants the loaded catalog priced.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CatalogPricing {
    /// Prices are whatever the client's own tables say.
    #[default]
    Client,
    /// Every item the client sells is repriced to this many Pang.
    ///
    /// An operator aid for local testing, so the whole shop is reachable without grinding a
    /// balance. It only reprices rows the client already marks as for sale: it never makes an
    /// unavailable item purchasable, so the shop the player sees stays the shop the client
    /// renders.
    FlatPang(u64),
}

/// Family tags accepted for a real client table, as the high byte of each type ID.
///
/// A single client table may legitimately span more than one tag, so this is a set rather
/// than a single value. Measured from the acquired client.
const fn client_family_tags(kind: CatalogKind) -> &'static [u8] {
    match kind {
        CatalogKind::Character => &[0x04],
        CatalogKind::CharacterPart => &[0x08],
        CatalogKind::ClubSet => &[0x10],
        CatalogKind::Ball => &[0x14],
        // The client's Item table spans several tags. Measured against the tables the client
        // actually loads (extracted from the newest PAK): 0x18, 0x1a and 0x1b all appear, the
        // last only on rows added after the revision this was first measured from.
        CatalogKind::Consumable => &[0x18, 0x1a, 0x1b],
        CatalogKind::Course => &[0x28],
    }
}

/// Parses one real U.S. client IFF table.
///
/// Differs from the synthetic schemas in three measured ways: the type ID lives at offset
/// four rather than zero, the header's binding value carries no family meaning, and record
/// width is whatever the header arithmetic yields rather than a fixed per-family constant.
///
/// # Errors
/// Rejects a table whose header, length arithmetic, digest, family tags, or type IDs are
/// inconsistent.
pub fn parse_client_iff_bytes(
    entry: &ManifestFile,
    bytes: &[u8],
) -> Result<BTreeMap<u32, CatalogRecord>, CatalogError> {
    validate_manifest_entry(entry)?;
    if bytes.len() > MAX_IFF_BYTES || bytes.len() < IFF_HEADER_BYTES {
        return Err(CatalogError::Structure);
    }
    let count = u16::from_le_bytes([bytes[0], bytes[1]]);
    let binding = u16::from_le_bytes([bytes[2], bytes[3]]);
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    // Binding is validated against the operator's manifest for change detection only; it
    // is deliberately not used to derive family identity.
    if count == 0 || count != entry.count || binding != entry.binding || version != entry.version {
        return Err(CatalogError::Structure);
    }
    if entry.record_size < CLIENT_TYPE_ID_OFFSET + 4 {
        return Err(CatalogError::Manifest);
    }
    let records_bytes = usize::from(count)
        .checked_mul(entry.record_size)
        .ok_or(CatalogError::Structure)?;
    let expected = IFF_HEADER_BYTES
        .checked_add(records_bytes)
        .ok_or(CatalogError::Structure)?;
    if bytes.len() != expected {
        return Err(CatalogError::Structure);
    }
    let tags = client_family_tags(entry.kind);
    let mut records = BTreeMap::new();
    for record in bytes[IFF_HEADER_BYTES..].chunks_exact(entry.record_size) {
        // A zero at offset zero marks an inactive row. Real tables carry them throughout,
        // and their type IDs are sentinels that do not respect family tagging, so they are
        // skipped rather than rejected. Measured across every client table: skipping these
        // leaves every remaining record correctly tagged.
        if le_u32(record, 0)? == 0 {
            continue;
        }
        let type_id = le_u32(record, CLIENT_TYPE_ID_OFFSET)?;
        if type_id == 0 {
            return Err(CatalogError::Structure);
        }
        let tag = u8::try_from(type_id >> 24).map_err(|_| CatalogError::Structure)?;
        if !tags.contains(&tag) {
            return Err(CatalogError::Structure);
        }
        let value = CatalogRecord {
            type_id: ItemTypeId::new(type_id),
            opaque: Arc::from(&record[CLIENT_TYPE_ID_OFFSET + 4..]),
            local_one_hole_par: None,
            definition: client_definition(entry.kind, ItemTypeId::new(type_id), record)?,
            character_part_slot: None,
            name: client_name(record),
        };
        if records.insert(type_id, value).is_some() {
            return Err(CatalogError::DuplicateTypeId);
        }
    }
    if records.is_empty() {
        return Err(CatalogError::Structure);
    }
    Ok(records)
}

fn parse_iff_bytes_for_schema(
    manifest_version: u32,
    entry: &ManifestFile,
    bytes: &[u8],
) -> Result<BTreeMap<u32, CatalogRecord>, CatalogError> {
    if manifest_version == MANIFEST_VERSION {
        if matches!(
            entry.kind,
            CatalogKind::Consumable | CatalogKind::CharacterPart
        ) {
            return Err(CatalogError::Manifest);
        }
        return parse_iff_bytes(entry, bytes);
    }
    if manifest_version == CLIENT_MANIFEST_VERSION {
        return parse_client_iff_bytes(entry, bytes);
    }
    if manifest_version != M7_MANIFEST_VERSION {
        return Err(CatalogError::Manifest);
    }
    let exact_size = match entry.kind {
        CatalogKind::Character => 4,
        CatalogKind::ClubSet => 21,
        CatalogKind::Ball => 13,
        CatalogKind::Consumable => 17,
        CatalogKind::CharacterPart => 18,
        CatalogKind::Course => 5,
    };
    if entry.record_size != exact_size {
        return Err(CatalogError::Manifest);
    }
    // Reuse all bounded header/count/length/global-in-file checks, then replace the
    // intentionally minimal v1 interpretation with exact v2 semantics.
    let minimally_parsed = parse_iff_bytes(entry, bytes)?;
    let mut parsed = BTreeMap::new();
    for record_bytes in bytes[IFF_HEADER_BYTES..].chunks_exact(entry.record_size) {
        let type_id = le_u32(record_bytes, 0)?;
        let minimal = minimally_parsed
            .get(&type_id)
            .ok_or(CatalogError::Structure)?;
        if type_id == 0 {
            return Err(CatalogError::Semantics);
        }
        let (definition, slot) = parse_v2_definition(entry.kind, record_bytes)?;
        let record = CatalogRecord {
            type_id: minimal.type_id,
            opaque: Arc::from([]),
            local_one_hole_par: minimal.local_one_hole_par,
            definition,
            character_part_slot: slot,
            // The synthetic schemas carry no name field.
            name: None,
        };
        parsed.insert(record.type_id.get(), record);
    }
    Ok(parsed)
}

fn parse_v2_definition(
    kind: CatalogKind,
    record: &[u8],
) -> Result<(Option<ItemDefinition>, Option<u8>), CatalogError> {
    let type_id = ItemTypeId::new(le_u32(record, 0)?);
    let unique = ItemStacking::Unique;
    let any = ItemCompatibility::Any;
    let make = |kind, sale, stacking, durability, compatibility| {
        Some(ItemDefinition {
            type_id,
            kind,
            sale,
            stacking,
            durability,
            compatibility,
        })
    };
    let output = match kind {
        CatalogKind::Character | CatalogKind::Course => (None, None),
        CatalogKind::ClubSet => {
            let sale = parse_sale(record[4], le_u64(record, 5)?)?;
            let max = le_u32(record, 13)?;
            let repair_pang_per_point = le_u32(record, 17)?;
            let durability = match (max, repair_pang_per_point) {
                (0, 0) => ItemDurability::Nondurable,
                (1.., 1..) => ItemDurability::Durable {
                    max,
                    repair_pang_per_point,
                },
                _ => return Err(CatalogError::Semantics),
            };
            (make(ItemKind::ClubSet, sale, unique, durability, any), None)
        }
        CatalogKind::Ball => (
            make(
                ItemKind::Ball,
                parse_sale(record[4], le_u64(record, 5)?)?,
                unique,
                ItemDurability::Nondurable,
                any,
            ),
            None,
        ),
        CatalogKind::Consumable => {
            let max_stack = le_u32(record, 13)?;
            if !(1..=MAX_CATALOG_STACK).contains(&max_stack) {
                return Err(CatalogError::Semantics);
            }
            (
                make(
                    ItemKind::Consumable,
                    parse_sale(record[4], le_u64(record, 5)?)?,
                    ItemStacking::Stackable { max_stack },
                    ItemDurability::Nondurable,
                    any,
                ),
                None,
            )
        }
        CatalogKind::CharacterPart => {
            let compatible = ItemTypeId::new(le_u32(record, 4)?);
            let slot = record[8];
            if slot > 7 || compatible.get() == 0 {
                return Err(CatalogError::Semantics);
            }
            (
                make(
                    ItemKind::CharacterPart,
                    parse_sale(record[9], le_u64(record, 10)?)?,
                    unique,
                    ItemDurability::Nondurable,
                    ItemCompatibility::Character(compatible),
                ),
                Some(slot),
            )
        }
    };
    Ok(output)
}

fn parse_sale(tag: u8, pang_price: u64) -> Result<ItemSale, CatalogError> {
    match tag {
        0 if pang_price == 0 => Ok(ItemSale::NotSold),
        1 if (1..=i64::MAX as u64).contains(&pang_price) => Ok(ItemSale::Pang(pang_price)),
        _ => Err(CatalogError::Semantics),
    }
}

fn le_u32(bytes: &[u8], offset: usize) -> Result<u32, CatalogError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(CatalogError::Structure)?
        .try_into()
        .map_err(|_| CatalogError::Structure)?;
    Ok(u32::from_le_bytes(value))
}

fn le_u64(bytes: &[u8], offset: usize) -> Result<u64, CatalogError> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or(CatalogError::Structure)?
        .try_into()
        .map_err(|_| CatalogError::Structure)?;
    Ok(u64::from_le_bytes(value))
}

fn validate_v2_cross_references(
    records: &BTreeMap<CatalogKind, BTreeMap<u32, CatalogRecord>>,
) -> Result<(), CatalogError> {
    let characters = records
        .get(&CatalogKind::Character)
        .ok_or(CatalogError::MissingKind)?;
    if records
        .get(&CatalogKind::CharacterPart)
        .ok_or(CatalogError::MissingKind)?
        .values()
        .any(|record| {
            !matches!(
                record.definition.map(|definition| definition.compatibility),
                Some(ItemCompatibility::Character(character))
                    if characters.contains_key(&character.get())
            )
        })
    {
        return Err(CatalogError::CrossReference);
    }
    Ok(())
}

const fn inventory_class(kind: ItemKind) -> InventoryClass {
    match kind {
        ItemKind::Character => InventoryClass::Legacy,
        ItemKind::ClubSet => InventoryClass::ClubSet,
        ItemKind::Ball => InventoryClass::Ball,
        ItemKind::Consumable => InventoryClass::Consumable,
        ItemKind::CharacterPart => InventoryClass::CharacterPart,
    }
}

fn validate_manifest_entry(entry: &ManifestFile) -> Result<(), CatalogError> {
    let minimum_record_size = if entry.kind == CatalogKind::Course {
        5
    } else {
        4
    };
    if entry.count == 0
        || !(minimum_record_size..=MAX_RECORD_SIZE).contains(&entry.record_size)
        || entry.sha256.len() != 64
        || !entry
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CatalogError::Manifest);
    }
    validate_relative(&entry.filename)
}

fn canonical_fingerprint(manifest: &CatalogManifest) -> Result<CatalogFingerprint, CatalogError> {
    let mut declarations = manifest
        .files
        .iter()
        .cloned()
        .map(|entry| {
            let kind = catalog_kind_tag(entry.kind);
            let filename = canonical_utf8_filename(&entry.filename)?;
            Ok((kind, filename, entry))
        })
        .collect::<Result<Vec<_>, CatalogError>>()?;
    declarations.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.as_bytes().cmp(right.1.as_bytes()))
    });

    let mut hasher = Sha256::new();
    hash_fingerprint_field(&mut hasher, b"pangya-rs-catalog-fingerprint-v2")?;
    hash_fingerprint_field(&mut hasher, &manifest.manifest_version.to_be_bytes())?;
    let count = u32::try_from(declarations.len()).map_err(|_| CatalogError::Manifest)?;
    hash_fingerprint_field(&mut hasher, &count.to_be_bytes())?;
    for (kind, filename, entry) in declarations {
        hash_fingerprint_field(&mut hasher, &[kind])?;
        hash_fingerprint_field(&mut hasher, filename.as_bytes())?;
        hash_fingerprint_field(&mut hasher, entry.sha256.as_bytes())?;
        hash_fingerprint_field(&mut hasher, &entry.count.to_be_bytes())?;
        hash_fingerprint_field(&mut hasher, &entry.binding.to_be_bytes())?;
        hash_fingerprint_field(&mut hasher, &entry.version.to_be_bytes())?;
        let record_size = u64::try_from(entry.record_size).map_err(|_| CatalogError::Manifest)?;
        hash_fingerprint_field(&mut hasher, &record_size.to_be_bytes())?;
    }
    Ok(CatalogFingerprint::new(hasher.finalize().into()))
}

fn hash_fingerprint_field(hasher: &mut Sha256, field: &[u8]) -> Result<(), CatalogError> {
    let length = u64::try_from(field.len()).map_err(|_| CatalogError::Manifest)?;
    hasher.update(length.to_be_bytes());
    hasher.update(field);
    Ok(())
}

const fn catalog_kind_tag(kind: CatalogKind) -> u8 {
    match kind {
        CatalogKind::Character => 1,
        CatalogKind::ClubSet => 2,
        CatalogKind::Ball => 3,
        CatalogKind::Course => 4,
        CatalogKind::Consumable => 5,
        CatalogKind::CharacterPart => 6,
    }
}

fn canonical_utf8_filename(path: &Path) -> Result<String, CatalogError> {
    let filename = path.to_str().ok_or(CatalogError::Path)?;
    if filename.is_empty()
        || filename.len() > 240
        || path.is_absolute()
        || filename.contains('\\')
        || filename.chars().any(char::is_control)
        || !is_nfc(filename)
    {
        return Err(CatalogError::Path);
    }
    let mut canonical = String::with_capacity(filename.len());
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(CatalogError::Path);
        };
        let component = component.to_str().ok_or(CatalogError::Path)?;
        if !canonical.is_empty() {
            canonical.push('/');
        }
        canonical.push_str(component);
    }
    if canonical != filename {
        return Err(CatalogError::Path);
    }
    Ok(canonical)
}

fn validate_relative(path: &Path) -> Result<(), CatalogError> {
    canonical_utf8_filename(path).map(drop)
}

fn read_bounded_regular(
    root: &Dir,
    relative: &Path,
    maximum: usize,
) -> Result<Vec<u8>, CatalogError> {
    validate_relative(relative)?;
    let file = root.open(relative).map_err(|_| CatalogError::Path)?;
    if !file.metadata().map_err(|_| CatalogError::Path)?.is_file() {
        return Err(CatalogError::Path);
    }
    let sentinel = u64::try_from(maximum)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(CatalogError::Manifest)?;
    let mut bytes = Vec::with_capacity(maximum.min(64 * 1024));
    file.take(sentinel)
        .read_to_end(&mut bytes)
        .map_err(|_| CatalogError::Path)?;
    if bytes.len() > maximum {
        return Err(CatalogError::Manifest);
    }
    Ok(bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Marker retained for the crate-boundary test.
#[must_use]
pub const fn crate_boundary() -> &'static str {
    "data"
}

#[cfg(test)]
mod tests {
    /// The client tables are the only source of item identity and price, so a regression here
    /// silently empties the shop. This builds a record with the measured header layout and
    /// checks both that a sellable row is priced and that the two "not really for sale" markers
    /// are honoured.
    #[test]
    fn client_records_yield_priced_definitions() {
        let mut record = vec![0_u8; CLIENT_PRICED_RECORD_BYTES + 2];
        record[0] = 1;
        record[CLIENT_TYPE_ID_OFFSET..CLIENT_TYPE_ID_OFFSET + 4]
            .copy_from_slice(&0x1000_0001_u32.to_le_bytes());
        record[CLIENT_PRICE_OFFSET..CLIENT_PRICE_OFFSET + 4]
            .copy_from_slice(&6000_u32.to_le_bytes());
        record[CLIENT_SHOP_FLAG_OFFSET] = 33;
        let definition =
            client_definition(CatalogKind::ClubSet, ItemTypeId::new(0x1000_0001), &record)
                .expect("parses")
                .expect("club sets are items");
        assert_eq!(definition.sale, ItemSale::Pang(6000));
        assert_eq!(definition.kind, ItemKind::ClubSet);

        // A zero shop flag means the client does not offer it, whatever the price says.
        let mut unsold = record.clone();
        unsold[CLIENT_SHOP_FLAG_OFFSET] = 0;
        assert_eq!(
            client_definition(CatalogKind::ClubSet, ItemTypeId::new(1), &unsold)
                .expect("parses")
                .expect("definition")
                .sale,
            ItemSale::NotSold
        );

        // The sentinel price marks an unavailable row even if a flag survived.
        let mut sentinel = record.clone();
        sentinel[CLIENT_PRICE_OFFSET..CLIENT_PRICE_OFFSET + 4]
            .copy_from_slice(&CLIENT_UNAVAILABLE_PRICE.to_le_bytes());
        assert_eq!(
            client_definition(CatalogKind::ClubSet, ItemTypeId::new(1), &sentinel)
                .expect("parses")
                .expect("definition")
                .sale,
            ItemSale::NotSold
        );

        // Characters and courses are not purchasable items in any table.
        assert!(
            client_definition(CatalogKind::Character, ItemTypeId::new(1), &record)
                .expect("parses")
                .is_none()
        );
    }

    use std::{env, fs, time::SystemTime};

    use proptest::prelude::*;

    use super::*;

    fn entry(count: u16, record_size: usize) -> ManifestFile {
        ManifestFile {
            filename: PathBuf::from("synthetic.bin"),
            sha256: "0".repeat(64),
            kind: CatalogKind::Character,
            count,
            binding: 7,
            version: 1,
            record_size,
        }
    }

    fn iff(ids: &[u32], record_size: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&u16::try_from(ids.len()).expect("count").to_le_bytes());
        bytes.extend_from_slice(&7_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        for id in ids {
            bytes.extend_from_slice(&id.to_le_bytes());
            bytes.resize(bytes.len() + record_size - 4, 0xa5);
        }
        bytes
    }

    #[test]
    fn minimally_parses_type_id_and_preserves_opaque_bytes() {
        let parsed = parse_iff_bytes(&entry(2, 8), &iff(&[11, 22], 8)).expect("catalog");
        assert_eq!(parsed[&11].type_id, ItemTypeId::new(11));
        assert_eq!(parsed[&22].opaque.as_ref(), [0xa5; 4]);
        assert_eq!(parsed[&22].local_one_hole_par(), None);
    }

    #[test]
    fn course_par_is_explicit_and_checked() {
        let mut course_entry = entry(1, 5);
        course_entry.kind = CatalogKind::Course;
        let mut bytes = iff(&[7], 5);
        bytes[12] = 3;
        let parsed = parse_iff_bytes(&course_entry, &bytes).expect("course");
        assert_eq!(parsed[&7].local_one_hole_par(), Some(3));
        bytes[12] = 0;
        assert_eq!(
            parse_iff_bytes(&course_entry, &bytes),
            Err(CatalogError::Structure)
        );
    }

    #[test]
    fn rejects_duplicate_ids_header_mismatch_and_trailing_bytes() {
        assert_eq!(
            parse_iff_bytes(&entry(2, 8), &iff(&[1, 1], 8)),
            Err(CatalogError::DuplicateTypeId)
        );
        assert_eq!(
            parse_iff_bytes(&entry(1, 8), &iff(&[1, 2], 8)),
            Err(CatalogError::Structure)
        );
        let mut trailing = iff(&[1], 8);
        trailing.push(0);
        assert_eq!(
            parse_iff_bytes(&entry(1, 8), &trailing),
            Err(CatalogError::Structure)
        );
    }

    #[test]
    fn v2_semantics_are_closed_bounded_and_exact() {
        fn declaration(kind: CatalogKind, size: usize) -> ManifestFile {
            let mut value = entry(1, size);
            value.kind = kind;
            value.version = 2;
            value
        }
        fn bytes(record: &[u8]) -> Vec<u8> {
            let mut value = Vec::new();
            value.extend_from_slice(&1_u16.to_le_bytes());
            value.extend_from_slice(&7_u16.to_le_bytes());
            value.extend_from_slice(&2_u32.to_le_bytes());
            value.extend_from_slice(record);
            value
        }

        let mut club = Vec::new();
        club.extend_from_slice(&1_u32.to_le_bytes());
        club.push(1);
        club.extend_from_slice(&50_u64.to_le_bytes());
        club.extend_from_slice(&100_u32.to_le_bytes());
        club.extend_from_slice(&3_u32.to_le_bytes());
        assert!(
            parse_iff_bytes_for_schema(
                M7_MANIFEST_VERSION,
                &declaration(CatalogKind::ClubSet, 21),
                &bytes(&club)
            )
            .is_ok()
        );
        for (offset, replacement) in [(4, 2_u8), (4, 0_u8)] {
            let mut invalid = club.clone();
            invalid[offset] = replacement;
            assert_eq!(
                parse_iff_bytes_for_schema(
                    M7_MANIFEST_VERSION,
                    &declaration(CatalogKind::ClubSet, 21),
                    &bytes(&invalid)
                ),
                Err(CatalogError::Semantics)
            );
        }
        let mut invalid_durability = club.clone();
        invalid_durability[13..17].copy_from_slice(&0_u32.to_le_bytes());
        assert_eq!(
            parse_iff_bytes_for_schema(
                M7_MANIFEST_VERSION,
                &declaration(CatalogKind::ClubSet, 21),
                &bytes(&invalid_durability)
            ),
            Err(CatalogError::Semantics)
        );
        let mut consumable = Vec::new();
        consumable.extend_from_slice(&2_u32.to_le_bytes());
        consumable.push(1);
        consumable.extend_from_slice(&1_u64.to_le_bytes());
        consumable.extend_from_slice(&(MAX_CATALOG_STACK + 1).to_le_bytes());
        assert_eq!(
            parse_iff_bytes_for_schema(
                M7_MANIFEST_VERSION,
                &declaration(CatalogKind::Consumable, 17),
                &bytes(&consumable)
            ),
            Err(CatalogError::Semantics)
        );
        assert_eq!(
            parse_iff_bytes_for_schema(
                M7_MANIFEST_VERSION,
                &declaration(CatalogKind::Ball, 14),
                &bytes(&[0; 14])
            ),
            Err(CatalogError::Manifest)
        );
    }

    #[test]
    fn v2_character_part_cross_reference_is_closed() {
        let character = CatalogRecord {
            type_id: ItemTypeId::new(10),
            opaque: Arc::from([]),
            local_one_hole_par: None,
            definition: None,
            character_part_slot: None,
            name: None,
        };
        let part = CatalogRecord {
            type_id: ItemTypeId::new(20),
            opaque: Arc::from([]),
            local_one_hole_par: None,
            definition: Some(ItemDefinition {
                type_id: ItemTypeId::new(20),
                kind: ItemKind::CharacterPart,
                sale: ItemSale::NotSold,
                stacking: ItemStacking::Unique,
                durability: ItemDurability::Nondurable,
                compatibility: ItemCompatibility::Character(ItemTypeId::new(11)),
            }),
            character_part_slot: Some(0),
            name: None,
        };
        let records = BTreeMap::from([
            (CatalogKind::Character, BTreeMap::from([(10, character)])),
            (CatalogKind::CharacterPart, BTreeMap::from([(20, part)])),
        ]);
        assert_eq!(
            validate_v2_cross_references(&records),
            Err(CatalogError::CrossReference)
        );
    }

    #[test]
    fn catalog_fingerprint_is_invariant_to_manifest_declaration_order() {
        let mut character = entry(1, 8);
        character.filename = PathBuf::from("Character.bin");
        let mut ball = entry(2, 12);
        ball.filename = PathBuf::from("Ball.bin");
        ball.kind = CatalogKind::Ball;
        ball.sha256 = "a".repeat(64);
        let ordered = CatalogManifest {
            manifest_version: MANIFEST_VERSION,
            files: vec![character.clone(), ball.clone()],
        };
        let reordered = CatalogManifest {
            manifest_version: MANIFEST_VERSION,
            files: vec![ball, character],
        };
        assert_eq!(
            canonical_fingerprint(&ordered),
            canonical_fingerprint(&reordered)
        );
    }

    #[test]
    fn catalog_filenames_are_canonical_utf8_with_security_boundaries() {
        assert_eq!(
            canonical_utf8_filename(Path::new("nested/Coursé.bin")),
            Ok("nested/Coursé.bin".to_owned())
        );
        for invalid in [
            "",
            "/absolute.bin",
            "../escape.bin",
            "nested/../escape.bin",
            "nested//duplicate-separator.bin",
            "nested\\platform-ambiguous.bin",
            "control\n.bin",
            "Cafe\u{301}.bin",
        ] {
            assert_eq!(
                canonical_utf8_filename(Path::new(invalid)),
                Err(CatalogError::Path),
                "accepted noncanonical filename {invalid:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn catalog_filenames_reject_non_utf8() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt as _;

        let filename = PathBuf::from(OsString::from_vec(vec![b'f', 0x80, b'.', b'b']));
        assert_eq!(canonical_utf8_filename(&filename), Err(CatalogError::Path));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_parent_components_and_symlink_escapes() {
        use std::os::unix::fs::symlink;

        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let base = env::temp_dir().join(format!("pangya-data-{}-{unique}", std::process::id()));
        let root = base.join("root");
        fs::create_dir_all(&root).expect("root");
        fs::write(base.join("outside.bin"), iff(&[1], 8)).expect("outside");
        symlink(base.join("outside.bin"), root.join("escape.bin")).expect("symlink");
        let capability =
            Dir::open_ambient_dir(&root, ambient_authority()).expect("root capability");
        assert_eq!(
            read_bounded_regular(&capability, Path::new("../outside.bin"), MAX_IFF_BYTES),
            Err(CatalogError::Path)
        );
        assert_eq!(
            read_bounded_regular(
                &capability,
                Path::new("nested/../outside.bin"),
                MAX_IFF_BYTES
            ),
            Err(CatalogError::Path)
        );
        assert_eq!(
            read_bounded_regular(&capability, Path::new("escape.bin"), MAX_IFF_BYTES),
            Err(CatalogError::Path)
        );
        fs::remove_dir_all(base).expect("cleanup");
    }

    proptest! {
        #[test]
        fn arbitrary_iff_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..4096), count in any::<u16>(), size in 0usize..70000) {
            let _ = parse_iff_bytes(&entry(count, size), &bytes);
        }

        #[test]
        fn arbitrary_v2_records_never_panic(
            bytes in proptest::collection::vec(any::<u8>(), 0..4096),
            count in any::<u16>(),
            kind in prop_oneof![
                Just(CatalogKind::Character),
                Just(CatalogKind::ClubSet),
                Just(CatalogKind::Ball),
                Just(CatalogKind::Consumable),
                Just(CatalogKind::CharacterPart),
                Just(CatalogKind::Course),
            ],
        ) {
            let size = match kind {
                CatalogKind::Character => 4,
                CatalogKind::ClubSet => 21,
                CatalogKind::Ball => 13,
                CatalogKind::Consumable => 17,
                CatalogKind::CharacterPart => 18,
                CatalogKind::Course => 5,
            };
            let mut declaration = entry(count, size);
            declaration.kind = kind;
            declaration.version = 2;
            let _ = parse_iff_bytes_for_schema(M7_MANIFEST_VERSION, &declaration, &bytes);
        }
    }
}

#[cfg(test)]
mod name_tests {
    use super::*;

    /// The shared 0x90-byte record base, with a name at 0x08 and a price at 0x5c.
    fn priced_record(name: &[u8]) -> Vec<u8> {
        let mut record = vec![0_u8; 0x90];
        record[0] = 1; // active
        record[CLIENT_TYPE_ID_OFFSET..CLIENT_TYPE_ID_OFFSET + 4]
            .copy_from_slice(&0x1000_0061_u32.to_le_bytes());
        record[CLIENT_NAME_OFFSET..CLIENT_NAME_OFFSET + name.len()].copy_from_slice(name);
        record[CLIENT_PRICE_OFFSET..CLIENT_PRICE_OFFSET + 4]
            .copy_from_slice(&1234_u32.to_le_bytes());
        record[CLIENT_SHOP_FLAG_OFFSET] = 0x22;
        record
    }

    #[test]
    fn a_name_is_read_up_to_its_first_nul_and_trimmed() {
        let record = priced_record(b"Papel Training Club Set\0trailing garbage");
        assert_eq!(
            client_name(&record).as_deref(),
            Some("Papel Training Club Set")
        );
    }

    #[test]
    fn an_empty_or_blank_name_is_absent_rather_than_an_empty_string() {
        assert_eq!(client_name(&priced_record(b"")), None);
        assert_eq!(client_name(&priced_record(b"    ")), None);
    }

    #[test]
    fn a_record_too_narrow_to_hold_a_name_yields_none_rather_than_panicking() {
        // Real tables include families narrower than the priced base; they must still load.
        assert_eq!(client_name(&[0_u8; 8]), None);
        assert_eq!(client_name(&[]), None);
    }

    #[test]
    fn a_non_ascii_byte_is_decoded_lossily_rather_than_failing_the_load() {
        // A name is cosmetic. Losing the whole catalog over one unexpected byte would take
        // the server down for something no gameplay decision depends on.
        let mut name = b"Caf\xE9 Set".to_vec();
        name.resize(CLIENT_NAME_BYTES, 0);
        let parsed = client_name(&priced_record(&name)).expect("a lossy name is still a name");
        assert!(parsed.starts_with("Caf"), "got {parsed:?}");
    }
}
