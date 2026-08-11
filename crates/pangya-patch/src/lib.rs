#![allow(missing_docs)] // Public JSON fields mirror the signed wire schema; module docs define it.
//! Authenticated, incremental reconstruction of a PangYa client PAK.
//!
//! This crate deliberately accepts only the PAK variants documented by the pinned
//! `pangfiles` reference and only ordinary, non-encrypted ZIP members.  It is used by the
//! bundle producer and launcher so the two sides cannot drift on the security boundary.
//!
//! # Retail-format provenance
//! PAK trailer, entry flags, legacy XOR and XTEA metadata are derived from
//! `/Users/thunderjr/projects/pangya-rs/opensource-references/pangbox--pangfiles/pak/format.go:3-46`
//! and `pak/reader.go:66-127`. LZ/LZ2 token layout and pads are derived from
//! `/Users/thunderjr/projects/pangya-rs/opensource-references/pangbox--pangfiles/pak/decompress.go:8-74`.
//! The XTEA round function is derived from
//! `/Users/thunderjr/projects/pangya-rs/opensource-references/pangbox--pangfiles/crypto/pyxtea/xtea.go:20-46`.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::io::Read as _;

const TRAILER: usize = 9;
const ENTRY: usize = 14;
const SIGNATURE: u8 = 0x12;
const TYPE_MASK: u8 = 0x0f;
const META_MASK: u8 = 0xf0;
const BASIC: u8 = 0;
const LZ: u8 = 1;
const DIRECTORY: u8 = 2;
const LZ2: u8 = 3;
const XOR: u8 = 0x10;
const XTEA: u8 = 0x20;
const PLAIN: u8 = 0x80;
const ZIP_LOCAL: u32 = 0x0403_4b50;
const ZIP_CENTRAL: u32 = 0x0201_4b50;
const ZIP_END: u32 = 0x0605_4b50;
const MAX_ENTRIES: usize = 4096;
const MAX_OUTPUT: usize = 512 * 1024 * 1024;

/// PAK and IFF reconstruction failures. No malformed input is allowed to panic.
#[derive(Debug, thiserror::Error)]
pub enum PatchError {
    #[error("input is truncated or has an out-of-range offset")]
    Truncated,
    #[error("bad PAK signature")]
    BadPakSignature,
    #[error("unsupported PAK entry variant")]
    UnsupportedPak,
    #[error("PAK has too many entries or produces too much output")]
    TooLarge,
    #[error("PAK entry ranges overlap or enter its table")]
    Overlap,
    #[error("PAK path is unsafe, duplicated, or case-collides")]
    UnsafePath,
    #[error("malformed LZ/LZ2 back-reference")]
    BadLz,
    #[error("bad ZIP/IFF structure or signature")]
    BadZip,
    #[error("ZIP member is unsafe, duplicate, case-colliding, encrypted, or unsupported")]
    UnsafeZip,
    #[error("a declared old/new hash or length did not match")]
    DigestMismatch,
    #[error("signature is invalid or belongs to a different pinned key")]
    BadSignature,
    #[error("manifest is invalid")]
    Manifest,
}

/// Public key and key id pinned by a launcher profile; never learned from a server.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TrustKey {
    pub key_id: String,
    /// Lowercase/uppercase hex is accepted at profile validation, then decoded here.
    pub public_key: String,
}

/// Whole file compatibility metadata.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FileMetadata {
    pub size: u64,
    pub pangya_crc: i32,
    pub sha256: String,
}

/// A changed IFF ZIP member. Payloads are always raw member bytes, never deltas.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct MemberChange {
    pub name: String,
    pub old_sha256: String,
    pub old_length: u64,
    pub new_sha256: String,
    pub new_length: u64,
}

/// Canonical signed release manifest.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub tool_version: String,
    pub release_id: u64,
    pub key_id: String,
    pub target_pak: String,
    pub base_pak: FileMetadata,
    pub current_iff_sha256: String,
    pub current_iff_size: u64,
    pub members: Vec<MemberChange>,
    pub result_iff_sha256: String,
    pub result_iff_size: u64,
    pub result_pak: FileMetadata,
}

impl ReleaseManifest {
    /// Stable JSON encoding. Struct declaration order and `BTreeMap`-free fields are the
    /// canonical contract; producer and launcher use exactly this routine.
    pub fn canonical_json(&self) -> Result<Vec<u8>, PatchError> {
        validate_manifest(self)?;
        serde_json::to_vec(self).map_err(|_| PatchError::Manifest)
    }

    /// Bytes covered by Ed25519: canonical metadata plus unambiguous payload digest lines.
    pub fn signing_message(&self) -> Result<Vec<u8>, PatchError> {
        let mut out = self.canonical_json()?;
        out.push(b'\n');
        for member in &self.members {
            out.extend_from_slice(member.name.as_bytes());
            out.push(b':');
            out.extend_from_slice(member.new_sha256.as_bytes());
            out.push(b'\n');
        }
        Ok(out)
    }
}

/// Verifies the pinned key id, key encoding, manifest signature, and payload hashes.
pub fn verify_bundle(
    manifest: &ReleaseManifest,
    signature: &[u8],
    trust: &TrustKey,
    payloads: &BTreeMap<String, Vec<u8>>,
) -> Result<(), PatchError> {
    if manifest.key_id != trust.key_id || signature.len() != 64 {
        return Err(PatchError::BadSignature);
    }
    let bytes = decode_hex(&trust.public_key, 32).ok_or(PatchError::BadSignature)?;
    let key = VerifyingKey::from_bytes(
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| PatchError::BadSignature)?,
    )
    .map_err(|_| PatchError::BadSignature)?;
    key.verify(
        &manifest.signing_message()?,
        &Signature::from_slice(signature).map_err(|_| PatchError::BadSignature)?,
    )
    .map_err(|_| PatchError::BadSignature)?;
    if payloads.len() != manifest.members.len() {
        return Err(PatchError::DigestMismatch);
    }
    for change in &manifest.members {
        let bytes = payloads
            .get(&change.name)
            .ok_or(PatchError::DigestMismatch)?;
        if bytes.len() as u64 != change.new_length || hash(bytes) != change.new_sha256 {
            return Err(PatchError::DigestMismatch);
        }
    }
    Ok(())
}

/// Reconstructs a target PAK after all signature/base/current checks have completed.
/// The changed member is emitted as a basic PAK record; every unrelated packed record remains
/// byte-for-byte unchanged. The final metadata is verified independently before returning.
/// Produced release metadata, signature, and raw changed-member payloads.
pub type ProducedRelease = (ReleaseManifest, Vec<u8>, BTreeMap<String, Vec<u8>>);

/// Produces a release from two operator-owned PAKs. Only differing IFF member bytes are
/// returned in `payloads`; neither input PAK is written to the release directory.
pub fn produce_release(
    base: &[u8],
    result: &[u8],
    key: [u32; 4],
    release_id: u64,
    key_id: String,
    signer: &SigningKey,
) -> Result<ProducedRelease, PatchError> {
    let base_pak = Pak::parse(base, key)?;
    let result_pak = Pak::parse(result, key)?;
    let base_iff = base_pak.read("data/pangya_gb.iff")?;
    let result_iff = result_pak.read("data/pangya_gb.iff")?;
    let before = Iff::parse(&base_iff)?;
    let after = Iff::parse(&result_iff)?;
    let mut before_names = BTreeMap::new();
    for entry in &before.entries {
        before_names.insert(entry.name.clone(), entry);
    }
    let mut members = Vec::new();
    let mut payloads = BTreeMap::new();
    for entry in &after.entries {
        let old = before_names
            .remove(&entry.name)
            .ok_or(PatchError::DigestMismatch)?;
        let old_bytes = old.data(&base_iff)?;
        let new_bytes = entry.data(&result_iff)?;
        if old_bytes != new_bytes {
            members.push(MemberChange {
                name: entry.name.clone(),
                old_sha256: hash(&old_bytes),
                old_length: old_bytes.len() as u64,
                new_sha256: hash(&new_bytes),
                new_length: new_bytes.len() as u64,
            });
            payloads.insert(entry.name.clone(), new_bytes.to_vec());
        }
    }
    // An add/remove is not a member replacement and would make preservation ambiguous.
    if !before_names.is_empty() || members.is_empty() {
        return Err(PatchError::Manifest);
    }
    members.sort_by(|left, right| left.name.cmp(&right.name));
    let manifest = ReleaseManifest {
        schema_version: 1,
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        release_id,
        key_id,
        target_pak: "projectg851gb.pak".to_owned(),
        base_pak: metadata(base),
        current_iff_sha256: hash(&base_iff),
        current_iff_size: base_iff.len() as u64,
        members,
        result_iff_sha256: hash(&result_iff),
        result_iff_size: result_iff.len() as u64,
        result_pak: metadata(result),
    };
    let signature = signer
        .sign(&manifest.signing_message()?)
        .to_bytes()
        .to_vec();
    Ok((manifest, signature, payloads))
}

/// Reconstructs a target PAK after all signature/base/current checks have completed.
pub fn reconstruct(
    base: &[u8],
    key: [u32; 4],
    manifest: &ReleaseManifest,
    payloads: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, PatchError> {
    validate_manifest(manifest)?;
    if base.len() as u64 != manifest.base_pak.size
        || hash(base) != manifest.base_pak.sha256
        || pangya_crc(base) != manifest.base_pak.pangya_crc
    {
        return Err(PatchError::DigestMismatch);
    }
    let pak = Pak::parse(base, key)?;
    let iff = pak.read("data/pangya_gb.iff")?;
    if iff.len() as u64 != manifest.current_iff_size || hash(&iff) != manifest.current_iff_sha256 {
        return Err(PatchError::DigestMismatch);
    }
    let rebuilt_iff = rebuild_iff(&iff, &manifest.members, payloads)?;
    if rebuilt_iff.len() as u64 != manifest.result_iff_size
        || hash(&rebuilt_iff) != manifest.result_iff_sha256
    {
        return Err(PatchError::DigestMismatch);
    }
    let result = pak.replace_basic("data/pangya_gb.iff", &rebuilt_iff)?;
    if result.len() as u64 != manifest.result_pak.size
        || hash(&result) != manifest.result_pak.sha256
        || pangya_crc(&result) != manifest.result_pak.pangya_crc
    {
        return Err(PatchError::DigestMismatch);
    }
    // Independent parse/re-read catches writer bugs, not merely matching a writer-side buffer.
    let check = Pak::parse(&result, key)?;
    let final_iff = check.read("data/pangya_gb.iff")?;
    if final_iff != rebuilt_iff {
        return Err(PatchError::DigestMismatch);
    }
    Ok(result)
}

#[derive(Clone)]
struct PakEntry {
    path: String,
    typ: u8,
    raw_path: Vec<u8>,
    /// On-disk path-length field; XTEA path padding is not part of this value.
    path_length: u8,
    packed: Vec<u8>,
    real: u32,
}
struct Pak {
    key: [u32; 4],
    entries: Vec<PakEntry>,
}

impl Pak {
    fn parse(bytes: &[u8], key: [u32; 4]) -> Result<Self, PatchError> {
        if bytes.len() < TRAILER {
            return Err(PatchError::Truncated);
        }
        let t = &bytes[bytes.len() - TRAILER..];
        if t[8] != SIGNATURE {
            return Err(PatchError::BadPakSignature);
        }
        let table = le32(&t[..4])? as usize;
        let count = le32(&t[4..8])? as usize;
        if count > MAX_ENTRIES || table > bytes.len() - TRAILER {
            return Err(PatchError::TooLarge);
        }
        let mut at = table;
        let mut entries = Vec::with_capacity(count);
        let mut names = HashSet::with_capacity(count);
        let mut ranges = Vec::with_capacity(count);
        for _ in 0..count {
            let raw = get(bytes, at, ENTRY)?.to_vec();
            at += ENTRY;
            let typ = raw[1];
            let meta = typ & META_MASK;
            if !(meta == 0 || meta == XOR || meta == XTEA || meta == PLAIN) {
                return Err(PatchError::UnsupportedPak);
            }
            let mut decoded = raw;
            if meta == XTEA {
                let mut block = [0_u8; 8];
                block[..4].copy_from_slice(&decoded[2..6]);
                block[4..].copy_from_slice(&decoded[10..14]);
                xtea_decrypt(key, &mut block);
                decoded[2..6].copy_from_slice(&block[..4]);
                decoded[10..14].copy_from_slice(&block[4..]);
            }
            let len = decoded[0] as usize;
            let payload_type = typ & TYPE_MASK;
            if !matches!(payload_type, BASIC | LZ | DIRECTORY | LZ2) {
                return Err(PatchError::UnsupportedPak);
            }
            let path_len = len
                .checked_add(if meta == XTEA { 0 } else { 1 })
                .ok_or(PatchError::Truncated)?;
            let mut raw_path = get(bytes, at, path_len)?.to_vec();
            at += path_len;
            let (offset, packed, real) = (
                le32(&decoded[2..6])? as usize,
                le32(&decoded[6..10])? as usize,
                le32(&decoded[10..14])? as usize,
            );
            let real = if meta == 0 || meta == XOR {
                real ^ 0x71
            } else {
                real
            };
            if real > MAX_OUTPUT || packed > MAX_OUTPUT {
                return Err(PatchError::TooLarge);
            }
            let path_bytes = match meta {
                XTEA => {
                    if !raw_path.len().is_multiple_of(8) {
                        return Err(PatchError::UnsupportedPak);
                    }
                    xtea_stream(key, &mut raw_path, false);
                    trim_path(&raw_path).to_vec()
                }
                XOR | 0 => raw_path[..len].iter().map(|b| b ^ 0x71).collect(),
                PLAIN => raw_path[..len].to_vec(),
                _ => return Err(PatchError::UnsupportedPak),
            };
            let path = std::str::from_utf8(&path_bytes)
                .map_err(|_| PatchError::UnsafePath)?
                .to_owned();
            if !safe_name(&path) || !names.insert(path.to_ascii_lowercase()) {
                return Err(PatchError::UnsafePath);
            }
            if payload_type != DIRECTORY {
                let end = offset.checked_add(packed).ok_or(PatchError::Truncated)?;
                if end > table {
                    return Err(PatchError::Overlap);
                }
                ranges.push((offset, end));
            }
            let packed_data = if payload_type == DIRECTORY {
                Vec::new()
            } else {
                get(bytes, offset, packed)?.to_vec()
            };
            let entry = PakEntry {
                path,
                typ,
                raw_path,
                path_length: decoded[0],
                packed: packed_data,
                real: real as u32,
            };
            if payload_type == BASIC && packed != real {
                return Err(PatchError::Truncated);
            }
            entries.push(entry);
        }
        ranges.sort_unstable();
        if ranges.windows(2).any(|w| w[0].1 > w[1].0) {
            return Err(PatchError::Overlap);
        }
        Ok(Self { key, entries })
    }
    fn read(&self, name: &str) -> Result<Vec<u8>, PatchError> {
        let e = self
            .entries
            .iter()
            .find(|e| e.path.eq_ignore_ascii_case(name))
            .ok_or(PatchError::UnsafePath)?;
        let kind = e.typ & TYPE_MASK;
        let output = if kind == BASIC {
            e.packed.clone()
        } else if kind == DIRECTORY {
            Vec::new()
        } else {
            decompress(&e.packed, kind, e.real as usize)?
        };
        if output.len() != e.real as usize {
            return Err(PatchError::DigestMismatch);
        }
        Ok(output)
    }
    fn replace_basic(&self, name: &str, replacement: &[u8]) -> Result<Vec<u8>, PatchError> {
        if replacement.len() > MAX_OUTPUT {
            return Err(PatchError::TooLarge);
        }
        let mut data = Vec::new();
        let mut records = Vec::new();
        let mut found = false;
        for e in &self.entries {
            let offset = u32::try_from(data.len()).map_err(|_| PatchError::TooLarge)?;
            let changing = e.path.eq_ignore_ascii_case(name);
            found |= changing;
            let packed = if changing { replacement } else { &e.packed };
            data.extend_from_slice(packed);
            records.push((e, offset, packed.len() as u32, changing));
        }
        if !found {
            return Err(PatchError::UnsafePath);
        }
        let table = u32::try_from(data.len()).map_err(|_| PatchError::TooLarge)?;
        for (e, offset, size, changing) in records {
            let meta = e.typ & META_MASK;
            let typ = if changing { meta | BASIC } else { e.typ };
            let real = if changing { size } else { e.real };
            let stored_real = if meta == XOR { real ^ 0x71 } else { real };
            let mut header = vec![e.path_length, typ];
            header.extend_from_slice(&offset.to_le_bytes());
            header.extend_from_slice(&size.to_le_bytes());
            header.extend_from_slice(&stored_real.to_le_bytes());
            if meta == XTEA {
                let mut block = [0_u8; 8];
                block[..4].copy_from_slice(&header[2..6]);
                block[4..].copy_from_slice(&header[10..14]);
                xtea_encrypt(self.key, &mut block);
                header[2..6].copy_from_slice(&block[..4]);
                header[10..14].copy_from_slice(&block[4..]);
            }
            data.extend_from_slice(&header);
            let mut path = e.raw_path.clone();
            if meta == XTEA {
                xtea_stream(self.key, &mut path, true);
            }
            data.extend_from_slice(&path);
        }
        data.extend_from_slice(&table.to_le_bytes());
        data.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        data.push(SIGNATURE);
        Ok(data)
    }
}

fn decompress(input: &[u8], kind: u8, expected: usize) -> Result<Vec<u8>, PatchError> {
    if !matches!(kind, LZ | LZ2) {
        return Err(PatchError::UnsupportedPak);
    }
    // Exact retail LZ/LZ2 controls and pads, source cited in this module's provenance block.
    const PAD: [u16; 8] = [
        0xff21, 0x834f, 0x675f, 0x0034, 0xf237, 0x815f, 0x4765, 0x0233,
    ];
    let mut out = Vec::new();
    let (mut at, mut bits, mut seq, mut raw) = (0usize, 0u8, 0u8, 0u8);
    while at < input.len() {
        if bits == 0 {
            raw = *input.get(at).ok_or(PatchError::Truncated)?;
            at += 1;
            seq = if kind == LZ2 { raw ^ 0xc8 } else { raw };
            bits = 8;
        }
        if seq & 1 == 0 {
            out.push(*input.get(at).ok_or(PatchError::Truncated)?);
            at += 1;
            if out.len() > expected {
                return Err(PatchError::TooLarge);
            }
        } else {
            let b = get(input, at, 2)?;
            at += 2;
            let mut value = u16::from_le_bytes([b[0], b[1]]);
            if kind == LZ2 {
                value ^= PAD[(raw >> 3 & 7) as usize];
            }
            let offset = (value & 0x0fff) as usize;
            let size = (value >> 12) as usize + 2;
            if offset.checked_add(size).ok_or(PatchError::BadLz)? > out.len()
                || out.len().checked_add(size).ok_or(PatchError::TooLarge)? > MAX_OUTPUT
                || out.len().checked_add(size).ok_or(PatchError::TooLarge)? > expected
            {
                return Err(PatchError::BadLz);
            }
            let start = out.len() - offset - size;
            let copy = out[start..start + size].to_vec();
            out.extend_from_slice(&copy);
        }
        seq >>= 1;
        bits -= 1;
    }
    if out.len() != expected {
        return Err(PatchError::DigestMismatch);
    }
    Ok(out)
}

fn rebuild_iff(
    input: &[u8],
    changes: &[MemberChange],
    payloads: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, PatchError> {
    let zip = Iff::parse(input)?;
    let mut wanted = BTreeMap::new();
    for c in changes {
        wanted.insert(c.name.as_str(), c);
    }
    if wanted.len() != changes.len() {
        return Err(PatchError::Manifest);
    }
    let mut out = Vec::new();
    let prefix = zip
        .entries
        .iter()
        .map(|e| e.local)
        .min()
        .unwrap_or(zip.central);
    out.extend_from_slice(get(input, 0, prefix)?);
    let mut central = Vec::new();
    let mut seen = HashSet::new();
    for e in &zip.entries {
        let change = wanted.get(e.name.as_str());
        let offset = u32::try_from(out.len()).map_err(|_| PatchError::TooLarge)?;
        if let Some(c) = change {
            let old = e.data(input)?;
            if old.len() as u64 != c.old_length || hash(&old) != c.old_sha256 {
                return Err(PatchError::DigestMismatch);
            }
            let payload = payloads.get(&c.name).ok_or(PatchError::DigestMismatch)?;
            if payload.len() as u64 != c.new_length || hash(payload) != c.new_sha256 {
                return Err(PatchError::DigestMismatch);
            }
            out.extend_from_slice(&e.local_replacement(payload)?);
            central.extend_from_slice(&e.central_replacement(payload, offset)?);
            seen.insert(c.name.as_str());
        } else {
            out.extend_from_slice(e.raw_local(input)?);
            central.extend_from_slice(&e.central_replacement_raw(offset)?);
        }
    }
    if seen.len() != changes.len() {
        return Err(PatchError::DigestMismatch);
    }
    let cd = u32::try_from(out.len()).map_err(|_| PatchError::TooLarge)?;
    out.extend_from_slice(&central);
    let cd_len = u32::try_from(central.len()).map_err(|_| PatchError::TooLarge)?;
    out.extend_from_slice(&zip.end(cd, cd_len)?);
    if out.len() > MAX_OUTPUT {
        return Err(PatchError::TooLarge);
    }
    Ok(out)
}

struct Iff {
    entries: Vec<ZipEntry>,
    central: usize,
    end_raw: Vec<u8>,
    count: u16,
}
struct ZipEntry {
    name: String,
    flags: u16,
    method: u16,
    local: usize,
    central_raw: Vec<u8>,
    local_head: Vec<u8>,
    /// Original local name and extra fields; central fields are not interchangeable.
    local_name_extra: Vec<u8>,
    data_start: usize,
    data_len: usize,
    local_end: usize,
}
impl Iff {
    fn parse(input: &[u8]) -> Result<Self, PatchError> {
        let start = input.len().saturating_sub(65_557);
        let end_at = (start..input.len().saturating_sub(3))
            .rev()
            .find(|&i| le32(&input[i..i + 4]).ok() == Some(ZIP_END))
            .ok_or(PatchError::BadZip)?;
        let end = get(input, end_at, 22)?;
        let disk = le16(&end[4..6])?;
        let cd_disk = le16(&end[6..8])?;
        let count = le16(&end[8..10])?;
        if disk != 0 || cd_disk != 0 || count != le16(&end[10..12])? || count as usize > MAX_ENTRIES
        {
            return Err(PatchError::BadZip);
        }
        let cd_len = le32(&end[12..16])? as usize;
        let central = le32(&end[16..20])? as usize;
        if central.checked_add(cd_len).ok_or(PatchError::BadZip)? != end_at {
            return Err(PatchError::BadZip);
        }
        let mut at = central;
        let mut entries = Vec::with_capacity(count as usize);
        let mut names = HashSet::new();
        for _ in 0..count {
            let fixed = get(input, at, 46)?;
            if le32(fixed)? != ZIP_CENTRAL {
                return Err(PatchError::BadZip);
            }
            let nl = le16(&fixed[28..30])? as usize;
            let xl = le16(&fixed[30..32])? as usize;
            let cl = le16(&fixed[32..34])? as usize;
            let raw = get(input, at, 46 + nl + xl + cl)?.to_vec();
            at += raw.len();
            let flags = le16(&raw[8..10])?;
            let method = le16(&raw[10..12])?;
            if flags & (1 | 8) != 0
                || !matches!(method, 0 | 8)
                || raw[34..46].contains(&0xff)
                || le32(&raw[20..24])? == u32::MAX
                || le32(&raw[24..28])? == u32::MAX
            {
                return Err(PatchError::UnsafeZip);
            }
            let name = std::str::from_utf8(&raw[46..46 + nl])
                .map_err(|_| PatchError::UnsafeZip)?
                .to_owned();
            if !safe_name(&name) || !names.insert(name.to_ascii_lowercase()) {
                return Err(PatchError::UnsafeZip);
            }
            let local = le32(&raw[42..46])? as usize;
            let lh = get(input, local, 30)?.to_vec();
            if le32(&lh)? != ZIP_LOCAL {
                return Err(PatchError::BadZip);
            }
            let lnl = le16(&lh[26..28])? as usize;
            let lxl = le16(&lh[28..30])? as usize;
            let local_name_extra = get(input, local + 30, lnl + lxl)?.to_vec();
            if le16(&lh[6..8])? != flags
                || le16(&lh[8..10])? != method
                || local_name_extra[..lnl] != raw[46..46 + nl]
                || le32(&lh[14..18])? != le32(&raw[16..20])?
                || le32(&lh[18..22])? != le32(&raw[20..24])?
                || le32(&lh[22..26])? != le32(&raw[24..28])?
            {
                return Err(PatchError::BadZip);
            }
            let data_start = local
                .checked_add(30 + lnl + lxl)
                .ok_or(PatchError::BadZip)?;
            let data_len = le32(&raw[20..24])? as usize;
            get(input, data_start, data_len)?;
            entries.push(ZipEntry {
                name,
                flags,
                method,
                local,
                central_raw: raw,
                local_head: lh,
                local_name_extra,
                data_start,
                data_len,
                local_end: 0,
            });
        }
        if at != central + cd_len {
            return Err(PatchError::BadZip);
        }
        let mut locs: Vec<(usize, usize)> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.local, i))
            .collect();
        locs.sort_unstable();
        if locs.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err(PatchError::BadZip);
        }
        for (n, (pos, index)) in locs.iter().enumerate() {
            entries[*index].local_end = locs.get(n + 1).map_or(central, |x| x.0);
            if *pos > entries[*index].data_start
                || entries[*index].local_end < entries[*index].data_start + entries[*index].data_len
            {
                return Err(PatchError::BadZip);
            }
        }
        Ok(Self {
            entries,
            central,
            end_raw: input[end_at..].to_vec(),
            count,
        })
    }
    fn end(&self, offset: u32, length: u32) -> Result<Vec<u8>, PatchError> {
        let mut e = self.end_raw.clone();
        if e.len() < 22 || self.count as usize != self.entries.len() {
            return Err(PatchError::BadZip);
        }
        e[12..16].copy_from_slice(&length.to_le_bytes());
        e[16..20].copy_from_slice(&offset.to_le_bytes());
        Ok(e)
    }
}
impl ZipEntry {
    fn raw_local<'a>(&self, input: &'a [u8]) -> Result<&'a [u8], PatchError> {
        get(input, self.local, self.local_end - self.local)
    }
    fn data(&self, input: &[u8]) -> Result<Vec<u8>, PatchError> {
        let wire = get(input, self.data_start, self.data_len)?;
        let expected = le32(&self.central_raw[24..28])? as usize;
        let out = if self.method == 0 {
            wire.to_vec()
        } else {
            let decoder = flate2::read::DeflateDecoder::new(wire);
            let mut out = Vec::with_capacity(expected.min(MAX_OUTPUT));
            decoder
                .take((MAX_OUTPUT + 1) as u64)
                .read_to_end(&mut out)
                .map_err(|_| PatchError::BadZip)?;
            out
        };
        if out.len() != expected || crc32fast::hash(&out) != le32(&self.central_raw[16..20])? {
            return Err(PatchError::BadZip);
        }
        Ok(out)
    }
    fn local_replacement(&self, p: &[u8]) -> Result<Vec<u8>, PatchError> {
        let mut h = self.local_head.clone();
        let flags = self.flags & !8;
        h[6..8].copy_from_slice(&flags.to_le_bytes());
        h[8..10].copy_from_slice(&0u16.to_le_bytes());
        let crc = crc32fast::hash(p);
        h[14..18].copy_from_slice(&crc.to_le_bytes());
        h[18..22].copy_from_slice(&(p.len() as u32).to_le_bytes());
        h[22..26].copy_from_slice(&(p.len() as u32).to_le_bytes());
        let nl = le16(&h[26..28])? as usize;
        let xl = le16(&h[28..30])? as usize;
        let mut out = h;
        out.extend_from_slice(&self.local_name_extra);
        if self.local_name_extra.len() != nl + xl {
            return Err(PatchError::BadZip);
        }
        out.extend_from_slice(p);
        Ok(out)
    }
    fn central_replacement(&self, p: &[u8], offset: u32) -> Result<Vec<u8>, PatchError> {
        let mut c = self.central_raw.clone();
        let flags = self.flags & !8;
        c[8..10].copy_from_slice(&flags.to_le_bytes());
        c[10..12].copy_from_slice(&0u16.to_le_bytes());
        let crc = crc32fast::hash(p);
        c[16..20].copy_from_slice(&crc.to_le_bytes());
        c[20..24].copy_from_slice(&(p.len() as u32).to_le_bytes());
        c[24..28].copy_from_slice(&(p.len() as u32).to_le_bytes());
        c[42..46].copy_from_slice(&offset.to_le_bytes());
        Ok(c)
    }
    fn central_replacement_raw(&self, offset: u32) -> Result<Vec<u8>, PatchError> {
        let mut c = self.central_raw.clone();
        c[42..46].copy_from_slice(&offset.to_le_bytes());
        Ok(c)
    }
}

fn validate_manifest(m: &ReleaseManifest) -> Result<(), PatchError> {
    if m.schema_version != 1
        || m.tool_version.is_empty()
        || !safe_pak(&m.target_pak)
        || m.members.is_empty()
    {
        return Err(PatchError::Manifest);
    }
    let mut seen = HashSet::new();
    for c in &m.members {
        if !safe_name(&c.name)
            || !seen.insert(c.name.to_ascii_lowercase())
            || c.new_length as usize > MAX_OUTPUT
            || !digest(&c.old_sha256)
            || !digest(&c.new_sha256)
        {
            return Err(PatchError::Manifest);
        }
    }
    for d in [
        &m.base_pak.sha256,
        &m.current_iff_sha256,
        &m.result_iff_sha256,
        &m.result_pak.sha256,
    ] {
        if !digest(d) {
            return Err(PatchError::Manifest);
        }
    }
    Ok(())
}
fn safe_name(s: &str) -> bool {
    !s.is_empty() && !s.contains(['/', '\\']) && !s.contains("..") && s.is_ascii()
}
fn safe_pak(s: &str) -> bool {
    safe_name(s) && s.to_ascii_lowercase().ends_with(".pak")
}
fn digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn metadata(v: &[u8]) -> FileMetadata {
    FileMetadata {
        size: v.len() as u64,
        pangya_crc: pangya_crc(v),
        sha256: hash(v),
    }
}
fn hash(v: &[u8]) -> String {
    format!("{:x}", Sha256::digest(v))
}
fn pangya_crc(v: &[u8]) -> i32 {
    let mut state = !0u32;
    for &byte in v {
        let mut x = (state ^ byte as u32) & 255;
        for _ in 0..8 {
            x = if x & 1 != 0 {
                (x >> 1) ^ 0x04c1_1db7
            } else {
                x >> 1
            };
        }
        state = x ^ (state >> 8);
    }
    (!state) as i32
}
fn get(v: &[u8], at: usize, n: usize) -> Result<&[u8], PatchError> {
    v.get(at..at.checked_add(n).ok_or(PatchError::Truncated)?)
        .ok_or(PatchError::Truncated)
}
fn le16(v: &[u8]) -> Result<u16, PatchError> {
    let b = get(v, 0, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}
fn le32(v: &[u8]) -> Result<u32, PatchError> {
    let b = get(v, 0, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
fn trim_path(v: &[u8]) -> &[u8] {
    let mut end = v.len();
    while end > 0 && matches!(v[end - 1], 0 | 0xcd) {
        end -= 1;
    }
    &v[..end]
}
fn decode_hex(value: &str, expected: usize) -> Option<Vec<u8>> {
    if value.len() != expected * 2 {
        return None;
    }
    let mut out = Vec::with_capacity(expected);
    for pair in value.as_bytes().chunks_exact(2) {
        let digit = |b: u8| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        };
        out.push(
            digit(pair[0])?
                .checked_mul(16)?
                .checked_add(digit(pair[1])?)?,
        );
    }
    Some(out)
}
fn xtea_stream(key: [u32; 4], data: &mut [u8], encrypt: bool) {
    for part in data.chunks_exact_mut(8) {
        let mut b = [0; 8];
        b.copy_from_slice(part);
        if encrypt {
            xtea_encrypt(key, &mut b)
        } else {
            xtea_decrypt(key, &mut b)
        }
        part.copy_from_slice(&b)
    }
}
fn mix(v: u32, sum: u32, key: u32) -> u32 {
    ((v << 4 ^ v >> 5).wrapping_add(v)) ^ sum.wrapping_add(key)
}
fn xtea_encrypt(key: [u32; 4], b: &mut [u8; 8]) {
    let (mut a, mut z) = (
        u32::from_le_bytes(b[..4].try_into().unwrap_or_default()),
        u32::from_le_bytes(b[4..].try_into().unwrap_or_default()),
    );
    let mut sum = 0;
    for _ in 0..16 {
        a = a.wrapping_add(mix(z, sum, key[(sum & 3) as usize]));
        sum = sum.wrapping_sub(0x61c8_8647);
        z = z.wrapping_add(mix(a, sum, key[((sum >> 11) & 3) as usize]));
    }
    b[..4].copy_from_slice(&a.to_le_bytes());
    b[4..].copy_from_slice(&z.to_le_bytes());
}
fn xtea_decrypt(key: [u32; 4], b: &mut [u8; 8]) {
    let (mut a, mut z) = (
        u32::from_le_bytes(b[..4].try_into().unwrap_or_default()),
        u32::from_le_bytes(b[4..].try_into().unwrap_or_default()),
    );
    let mut sum = 0xe377_9b90;
    for _ in 0..16 {
        z = z.wrapping_sub(mix(a, sum, key[((sum >> 11) & 3) as usize]));
        sum = sum.wrapping_add(0x61c8_8647);
        a = a.wrapping_sub(mix(z, sum, key[(sum & 3) as usize]));
    }
    b[..4].copy_from_slice(&a.to_le_bytes());
    b[4..].copy_from_slice(&z.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lz_rejects_a_backreference_before_output() {
        // Retail token layout: /Users/thunderjr/projects/pangya-rs/opensource-references/pangbox--pangfiles/pak/decompress.go:48-63
        assert!(matches!(
            decompress(&[1, 0, 0], LZ, 2),
            Err(PatchError::BadLz)
        ));
    }
    #[test]
    fn lz2_and_output_length_are_strict() {
        // LZ2 selector/pad and token lengths: /Users/thunderjr/projects/pangya-rs/opensource-references/pangbox--pangfiles/pak/decompress.go:41-63.
        assert!(decompress(&[0xc9, 0, 0], LZ2, 2).is_err());
        assert!(
            decompress(&[0, b'a'], LZ, 2).is_err(),
            "short output must not be accepted"
        );
        assert!(
            decompress(&[0, b'a', b'b'], LZ, 1).is_err(),
            "oversized output must not be accepted"
        );
    }

    #[test]
    fn manifest_rejects_duplicate_and_case_colliding_members() {
        let member = |name: &str| MemberChange {
            name: name.to_owned(),
            old_sha256: "0".repeat(64),
            old_length: 1,
            new_sha256: "1".repeat(64),
            new_length: 1,
        };
        let manifest = ReleaseManifest {
            schema_version: 1,
            tool_version: "test".into(),
            release_id: 1,
            key_id: "key".into(),
            target_pak: "projectg851gb.pak".into(),
            base_pak: FileMetadata {
                size: 1,
                pangya_crc: 0,
                sha256: "0".repeat(64),
            },
            current_iff_sha256: "0".repeat(64),
            current_iff_size: 1,
            members: vec![member("Item.iff"), member("item.iff")],
            result_iff_sha256: "0".repeat(64),
            result_iff_size: 1,
            result_pak: FileMetadata {
                size: 1,
                pangya_crc: 0,
                sha256: "0".repeat(64),
            },
        };
        assert!(matches!(
            manifest.canonical_json(),
            Err(PatchError::Manifest)
        ));
    }

    #[test]
    fn manifest_rejects_traversal() {
        let m = ReleaseManifest {
            schema_version: 1,
            tool_version: "x".into(),
            release_id: 1,
            key_id: "k".into(),
            target_pak: "../x.pak".into(),
            base_pak: FileMetadata {
                size: 0,
                pangya_crc: 0,
                sha256: "0".repeat(64),
            },
            current_iff_sha256: "0".repeat(64),
            current_iff_size: 0,
            members: vec![],
            result_iff_sha256: "0".repeat(64),
            result_iff_size: 0,
            result_pak: FileMetadata {
                size: 0,
                pangya_crc: 0,
                sha256: "0".repeat(64),
            },
        };
        assert!(validate_manifest(&m).is_err());
    }
}
