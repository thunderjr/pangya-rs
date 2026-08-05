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
use pangya_domain::{ItemTypeId, PlayerSnapshot, StarterGrant};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use thiserror::Error;

/// Supported manifest schema version.
pub const MANIFEST_VERSION: u32 = 1;
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
    /// Remaining unattested record bytes.
    pub opaque: Arc<[u8]>,
}

#[derive(Debug)]
struct CatalogInner {
    records: BTreeMap<CatalogKind, BTreeMap<u32, CatalogRecord>>,
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
        validate_relative(manifest)?;
        let root = Dir::open_ambient_dir(directory, ambient_authority())
            .map_err(|_| CatalogError::Path)?;
        let manifest_bytes = read_bounded_regular(&root, manifest, MAX_MANIFEST_BYTES)?;
        let manifest_text =
            std::str::from_utf8(&manifest_bytes).map_err(|_| CatalogError::Manifest)?;
        let parsed: CatalogManifest =
            toml::from_str(manifest_text).map_err(|_| CatalogError::Manifest)?;
        Self::load_manifest_from_dir(&root, parsed)
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
        if manifest.manifest_version != MANIFEST_VERSION
            || manifest.files.is_empty()
            || manifest.files.len() > MAX_MANIFEST_FILES
        {
            return Err(CatalogError::Manifest);
        }
        let mut records = BTreeMap::new();
        let mut type_ids = BTreeSet::new();
        for entry in &manifest.files {
            validate_manifest_entry(entry)?;
            if records.contains_key(&entry.kind) {
                return Err(CatalogError::DuplicateKind);
            }
            let bytes = read_bounded_regular(root, &entry.filename, MAX_IFF_BYTES)?;
            if sha256_hex(&bytes) != entry.sha256 {
                return Err(CatalogError::Digest);
            }
            let parsed = parse_iff_bytes(entry, &bytes)?;
            if parsed.keys().any(|type_id| !type_ids.insert(*type_id)) {
                return Err(CatalogError::DuplicateTypeId);
            }
            records.insert(entry.kind, parsed);
        }
        for required in [
            CatalogKind::Character,
            CatalogKind::ClubSet,
            CatalogKind::Ball,
        ] {
            if !records.contains_key(&required) {
                return Err(CatalogError::MissingKind);
            }
        }
        Ok(Self(Arc::new(CatalogInner { records })))
    }

    /// Returns whether a type ID exists in one required family.
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
            if !is_club && !is_ball {
                return Err(CatalogError::Binding);
            }
            if snapshot.equipment.club_item_id == Some(item.id) && !is_club {
                return Err(CatalogError::Binding);
            }
            if snapshot.equipment.ball_item_id == Some(item.id) && !is_ball {
                return Err(CatalogError::Binding);
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
        let value = CatalogRecord {
            type_id: ItemTypeId::new(type_id),
            opaque: Arc::from(&record[4..]),
        };
        if records.insert(type_id, value).is_some() {
            return Err(CatalogError::DuplicateTypeId);
        }
    }
    Ok(records)
}

fn validate_manifest_entry(entry: &ManifestFile) -> Result<(), CatalogError> {
    if entry.count == 0
        || !(4..=MAX_RECORD_SIZE).contains(&entry.record_size)
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

fn validate_relative(path: &Path) -> Result<(), CatalogError> {
    if path.as_os_str().is_empty() || path.as_os_str().len() > 240 || path.is_absolute() {
        return Err(CatalogError::Path);
    }
    if path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        Ok(())
    } else {
        Err(CatalogError::Path)
    }
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
    }
}
