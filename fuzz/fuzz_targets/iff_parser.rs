#![no_main]

use std::path::PathBuf;

use libfuzzer_sys::fuzz_target;
use pangya_data::{CatalogKind, ManifestFile, parse_iff_bytes};

fuzz_target!(|data: &[u8]| {
    let count = data
        .get(..2)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u16::from_le_bytes)
        .unwrap_or(1);
    let record_size = data
        .get(2..4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u16::from_le_bytes)
        .map_or(4, usize::from);
    let entry = ManifestFile {
        filename: PathBuf::from("fuzz.bin"),
        sha256: "0".repeat(64),
        kind: CatalogKind::Character,
        count,
        binding: 0,
        version: 0,
        record_size,
    };
    let _ = parse_iff_bytes(&entry, data);
});
