//! Generated, non-proprietary synthetic catalog golden and failure coverage.

use std::{fs, path::Path};

use pangya_data::{Catalog, CatalogError, CatalogKind};
use pangya_domain::{
    CourseId, ItemCompatibility, ItemDurability, ItemKind, ItemSale, ItemStacking, ItemTypeId,
    StarterCharacter, StarterGrant, StarterItem, StarterKey,
};
use sha2::{Digest as _, Sha256};

fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/synthetic-catalog"
    ))
}

fn m7_fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/synthetic-catalog-v2"
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
    let course = catalog
        .course_plan(CourseId::new(7).expect("course ID"))
        .expect("generated course");
    assert_eq!(
        (course.course_id().get(), course.hole(), course.par()),
        (7, 1, 3)
    );
    assert_eq!(
        catalog.fingerprint().as_bytes(),
        &[
            0xb3, 0x19, 0x59, 0x5c, 0x4a, 0x00, 0x5d, 0xdf, 0x11, 0x4a, 0x5c, 0xe2, 0x22, 0xee,
            0x7c, 0xa0, 0xee, 0x54, 0x37, 0x6c, 0xec, 0x55, 0xa5, 0xe0, 0xe6, 0x20, 0xd9, 0x37,
            0x3f, 0xf1, 0xb0, 0xd6,
        ]
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
fn generated_v2_catalog_exposes_exact_sorted_economy_semantics() {
    let catalog = Catalog::load(m7_fixtures(), Path::new("manifest.toml")).expect("v2 catalog");
    assert_eq!(catalog.manifest_version(), 2);
    let offer_ids = catalog
        .shop_offers()
        .iter()
        .map(|offer| offer.type_id.get())
        .collect::<Vec<_>>();
    assert_eq!(
        offer_ids,
        vec![0x0800_0001, 0x1000_0001, 0x1800_0001, 0x1a00_0001]
    );
    let club = catalog
        .shop_offer(ItemTypeId::new(0x1000_0001))
        .expect("sold club");
    assert_eq!(club.kind, ItemKind::ClubSet);
    assert_eq!(club.sale, ItemSale::Pang(500));
    assert_eq!(club.stacking, ItemStacking::Unique);
    assert_eq!(
        club.durability,
        ItemDurability::Durable {
            max: 100,
            repair_pang_per_point: 3
        }
    );
    let consumable = catalog
        .shop_offer(ItemTypeId::new(0x1a00_0001))
        .expect("sold consumable");
    assert_eq!(
        consumable.stacking,
        ItemStacking::Stackable { max_stack: 99 }
    );
    let part = catalog
        .item_definition(ItemTypeId::new(0x0800_0001))
        .expect("part");
    assert_eq!(
        part.compatibility,
        ItemCompatibility::Character(ItemTypeId::new(0x0400_0000))
    );
    assert!(catalog.part_is_compatible(ItemTypeId::new(0x0800_0001), ItemTypeId::new(0x0400_0000)));
    assert!(catalog.shop_offer(ItemTypeId::new(0x1a00_0002)).is_none());
    assert_eq!(
        catalog
            .item_definition(ItemTypeId::new(0x1a00_0002))
            .expect("not sold")
            .sale,
        ItemSale::NotSold
    );
}

#[test]
fn original_m3_families_remain_valid_without_optional_course() {
    let base = std::env::temp_dir().join(format!("pangya-catalog-m3-only-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("temp");
    for name in ["character.bin", "club_set.bin", "ball.bin"] {
        fs::copy(fixtures().join(name), base.join(name)).expect("copy");
    }
    let manifest = fs::read_to_string(fixtures().join("manifest.toml")).expect("manifest");
    let course_start = manifest
        .rfind("[[files]]\nfilename = \"Course.bin\"")
        .expect("course declaration");
    fs::write(base.join("manifest.toml"), &manifest[..course_start]).expect("M3 manifest");
    let catalog = Catalog::load(&base, Path::new("manifest.toml")).expect("M3-only catalog");
    assert!(catalog.contains(CatalogKind::Ball, ItemTypeId::new(0x1800_0000)));
    assert!(!catalog.contains(CatalogKind::Course, ItemTypeId::new(7)));
    fs::remove_dir_all(base).expect("cleanup");
}

#[test]
fn cross_family_duplicate_type_id_is_rejected_globally() {
    let base = std::env::temp_dir().join(format!(
        "pangya-catalog-cross-family-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("temp");
    for name in ["character.bin", "club_set.bin", "ball.bin", "Course.bin"] {
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
    for name in ["character.bin", "club_set.bin", "ball.bin", "Course.bin"] {
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

/// Loads the generated fixture that mirrors the real client's record schema.
fn client_schema_catalog() -> Catalog {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic-client-v3");
    Catalog::load(&root, std::path::Path::new("manifest.toml")).expect("client-schema catalog")
}

#[test]
fn client_schema_reads_type_ids_at_record_offset_four() {
    let catalog = client_schema_catalog();
    assert_eq!(
        catalog.manifest_version(),
        pangya_data::CLIENT_MANIFEST_VERSION
    );
    // A synthetic-schema loader would read the activity word at offset zero as the type
    // ID and find 1 everywhere; these ids only appear if offset four is used.
    for (kind, type_id) in [
        (CatalogKind::Character, 0x0400_0000),
        (CatalogKind::Character, 0x0400_0001),
        (CatalogKind::ClubSet, 0x1000_0000),
        (CatalogKind::Ball, 0x1400_0000),
        (CatalogKind::Consumable, 0x1800_0000),
        (CatalogKind::Consumable, 0x1a00_0001),
        (CatalogKind::CharacterPart, 0x0800_0400),
        (CatalogKind::Course, 0x2800_0000),
    ] {
        assert!(
            catalog.contains(kind, ItemTypeId::new(type_id)),
            "missing {kind:?} 0x{type_id:08x}"
        );
    }
    assert!(!catalog.contains(CatalogKind::Character, ItemTypeId::new(1)));
}

#[test]
fn client_schema_skips_rows_flagged_inactive_at_offset_zero() {
    let catalog = client_schema_catalog();
    // The generated tables carry 0x17ffffff sentinels whose family tag matches no
    // declared family. They load only because the zero activity word excludes them.
    assert!(!catalog.contains(CatalogKind::ClubSet, ItemTypeId::new(0x17ff_ffff)));
    assert!(!catalog.contains(CatalogKind::Consumable, ItemTypeId::new(0x17ff_ffff)));
}

#[test]
fn client_schema_rejects_a_record_whose_family_tag_is_wrong() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic-client-v3");
    let mut bytes = std::fs::read(root.join("ball.bin")).expect("ball");
    // Retag the single active ball record as a character; the tag check must reject it.
    bytes[8 + 4..8 + 8].copy_from_slice(&0x0400_0000_u32.to_le_bytes());
    let entry = pangya_data::ManifestFile {
        filename: "ball.bin".into(),
        sha256: "0".repeat(64),
        kind: CatalogKind::Ball,
        count: 1,
        binding: 0,
        version: 13,
        record_size: 16,
    };
    assert!(pangya_data::parse_client_iff_bytes(&entry, &bytes).is_err());
}
