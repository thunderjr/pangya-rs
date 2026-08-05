//! Generated, non-proprietary synthetic catalog golden and failure coverage.

use std::{fs, path::Path};

use pangya_data::{Catalog, CatalogError, CatalogKind};
use pangya_domain::{ItemTypeId, StarterCharacter, StarterGrant, StarterItem, StarterKey};
use sha2::{Digest as _, Sha256};

fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/synthetic-catalog"
    ))
}

fn key(value: &str) -> StarterKey {
    StarterKey::parse(value).expect("key")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn generated_golden_catalog_loads_and_cross_checks_starter() {
    let catalog = Catalog::load(fixtures(), Path::new("manifest.toml")).expect("catalog");
    assert!(catalog.contains(CatalogKind::Character, ItemTypeId::new(0x0400_0000)));
    assert_eq!(
        catalog
            .record(CatalogKind::ClubSet, ItemTypeId::new(0x1000_0000))
            .expect("club")
            .opaque
            .as_ref(),
        b"CLUB"
    );
    let starter = StarterGrant {
        character: StarterCharacter {
            key: key("character"),
            item_type_id: ItemTypeId::new(0x0400_0000),
        },
        items: vec![
            StarterItem {
                key: key("club"),
                item_type_id: ItemTypeId::new(0x1000_0000),
                quantity: 1,
            },
            StarterItem {
                key: key("ball"),
                item_type_id: ItemTypeId::new(0x1800_0000),
                quantity: 1,
            },
        ],
        equipped_club_key: Some(key("club")),
        equipped_ball_key: Some(key("ball")),
    };
    catalog.validate_starter(&starter).expect("starter");
}

#[test]
fn cross_family_duplicate_type_id_is_rejected_globally() {
    let base = std::env::temp_dir().join(format!(
        "pangya-catalog-cross-family-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("temp");
    for name in ["character.bin", "club_set.bin", "ball.bin"] {
        fs::copy(fixtures().join(name), base.join(name)).expect("copy");
    }
    let mut ball = fs::read(base.join("ball.bin")).expect("ball");
    ball[8..12].copy_from_slice(&0x1000_0000_u32.to_le_bytes());
    fs::write(base.join("ball.bin"), &ball).expect("duplicate ball");
    let manifest = fs::read_to_string(fixtures().join("manifest.toml"))
        .expect("manifest")
        .replace(
            "7f270c607407c9fecedefa12ae5c69408a41badfd82c989d4cbc67ab4765045e",
            &sha256_hex(&ball),
        );
    fs::write(base.join("manifest.toml"), manifest).expect("manifest");
    assert_eq!(
        Catalog::load(&base, Path::new("manifest.toml")).expect_err("duplicate type ID"),
        CatalogError::DuplicateTypeId
    );
    fs::remove_dir_all(base).expect("cleanup");
}

#[test]
fn digest_and_duplicate_kind_errors_are_redacted_and_typed() {
    let base = std::env::temp_dir().join(format!("pangya-catalog-errors-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("temp");
    for name in ["character.bin", "club_set.bin", "ball.bin"] {
        fs::copy(fixtures().join(name), base.join(name)).expect("copy");
    }
    let manifest = fs::read_to_string(fixtures().join("manifest.toml")).expect("manifest");
    fs::write(
        base.join("manifest.toml"),
        manifest.replace("8e634d84", "00000000"),
    )
    .expect("bad digest");
    assert_eq!(
        Catalog::load(&base, Path::new("manifest.toml")).expect_err("digest"),
        CatalogError::Digest
    );
    fs::write(
        base.join("manifest.toml"),
        fs::read_to_string(fixtures().join("manifest.toml"))
            .expect("manifest")
            .replace("kind = \"club_set\"", "kind = \"character\""),
    )
    .expect("duplicate kind");
    assert_eq!(
        Catalog::load(&base, Path::new("manifest.toml")).expect_err("kind"),
        CatalogError::DuplicateKind
    );
    fs::remove_dir_all(base).expect("cleanup");
}
