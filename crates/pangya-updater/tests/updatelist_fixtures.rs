//! Golden and directory-scan coverage for the client patch contract.
//!
//! The golden pair was produced by an independent implementation of the same format and
//! cipher, so a match here means the emitter agrees with an encoder the client is known to
//! accept — not merely with itself.

use pangya_updater::{
    EntrySelection, FileEntry, UpdateList, UpdateListRegion, build_from_directory,
    decipher_trim_nul,
};

const PLAINTEXT: &[u8] = include_bytes!("fixtures/us852_updatelist/plaintext.xml");
const CIPHERTEXT: &[u8] = include_bytes!("fixtures/us852_updatelist/fixture.bin");

fn fixture_list() -> UpdateList {
    UpdateList {
        patch_version: "FakeVer".to_owned(),
        patch_number: 9999,
        entries: vec![
            FileEntry {
                name: "projectg700gb+.pak".to_owned(),
                size: 1_131_201_576,
                checksum: -1_234_567,
                sha256: String::new(),
                date: "2016-04-15".to_owned(),
                time: "09:41:58".to_owned(),
            },
            FileEntry {
                name: "projectg851gb.pak".to_owned(),
                size: 690_331,
                checksum: 42,
                sha256: String::new(),
                date: "2016-11-02".to_owned(),
                time: "00:30:00".to_owned(),
            },
        ],
    }
}

#[test]
fn plaintext_matches_the_golden_document() {
    assert_eq!(fixture_list().to_xml().as_bytes(), PLAINTEXT);
}

#[test]
fn ciphertext_matches_the_golden_encryption() {
    let key = UpdateListRegion::Us.key();
    assert_eq!(fixture_list().to_encrypted(key), CIPHERTEXT);
}

#[test]
fn golden_ciphertext_decrypts_to_the_golden_plaintext() {
    let key = UpdateListRegion::Us.key();
    let decrypted = decipher_trim_nul(key, CIPHERTEXT).expect("block aligned");
    assert_eq!(decrypted, PLAINTEXT);
}

#[test]
fn a_wrong_region_key_does_not_recover_the_document() {
    let decrypted = decipher_trim_nul(UpdateListRegion::Jp.key(), CIPHERTEXT).expect("aligned");
    assert_ne!(decrypted, PLAINTEXT);
}

/// A scan must be reproducible and must checksum what is actually on disk.
#[test]
fn directory_scan_is_sorted_filtered_and_checksummed() {
    let root = tempdir();
    let directory = cap_std::fs::Dir::open_ambient_dir(&root, cap_std::ambient_authority())
        .expect("open temp dir");

    std::fs::write(root.join("projectg851gb.pak"), b"second").expect("write");
    std::fs::write(root.join("projectg700gb+.pak"), b"first").expect("write");
    std::fs::write(root.join("ijl15.dll"), b"not a pak").expect("write");
    std::fs::create_dir(root.join("GameGuard")).expect("mkdir");

    let paks = build_from_directory(&directory, EntrySelection::PakSeriesOnly, "v", 1)
        .expect("scan pak series");
    let names: Vec<&str> = paks
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, ["projectg700gb+.pak", "projectg851gb.pak"]);
    assert_eq!(paks.entries[0].size, 5);
    assert_eq!(
        paks.entries[0].checksum,
        pangya_updater::checksum(b"first") as i32
    );

    let all = build_from_directory(&directory, EntrySelection::AllFiles, "v", 1).expect("scan all");
    let all_names: Vec<&str> = all
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    // The directory is skipped; only regular files are listed.
    assert_eq!(
        all_names,
        ["ijl15.dll", "projectg700gb+.pak", "projectg851gb.pak"]
    );

    // Scanning twice must produce identical bytes, or a golden test could never hold.
    let again =
        build_from_directory(&directory, EntrySelection::PakSeriesOnly, "v", 1).expect("rescan");
    assert_eq!(paks.to_xml(), again.to_xml());

    std::fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn an_empty_directory_produces_a_zero_count_document() {
    let root = tempdir();
    let directory = cap_std::fs::Dir::open_ambient_dir(&root, cap_std::ambient_authority())
        .expect("open temp dir");
    let list =
        build_from_directory(&directory, EntrySelection::PakSeriesOnly, "v", 1).expect("scan");
    assert!(list.entries.is_empty());
    assert!(list.to_xml().contains("<updatefiles count=\"0\">"));
    std::fs::remove_dir_all(&root).expect("cleanup");
}

/// A unique scratch directory without pulling in a temp-file dependency.
fn tempdir() -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("pangya-updater-{}-{index}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}
