//! Real PostgreSQL acceptance tests for the M2 storage foundation.

use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Utc};
use pangya_domain::{
    AbortMatch, AbortMatchOutcome, AbortStrokeMatch, AbortStrokeMatchOutcome, AccountId,
    AccountRepository, AccountStatus, BalanceGrant, BeginSoloMatch, BeginSoloMatchOutcome,
    BeginStrokeMatch, BeginStrokeMatchOutcome, CatalogFingerprint, CharacterId, CommitSoloHole,
    CommitStrokeMatch, ConsumeHandover, ConsumeItem, CourseId, CredentialHash, EconomyCommit,
    EconomyError, EconomyItemSelector, EconomyOperationId, EconomyRepository, EquipmentChange,
    HandoverDigest, HandoverError, HandoverRepository, IncompleteMatchAbortLimit,
    ItemCompatibility, ItemDefinition, ItemDurability, ItemKind, ItemSale, ItemStacking,
    ItemTypeId, MAX_STARTER_ITEMS, MarkSoloInGame, MarkSoloInGameOutcome, MarkStrokeInGame,
    MarkStrokeInGameOutcome, MatchAbortReason, MatchId, MatchPlan, MatchRepository,
    MatchRepositoryError, MatchResultKey, MatchSeed, NewAccount, Nickname, NormalizedUsername,
    OfflineNoteClaim, OfflineNoteRequest, PlayerRepository, PurchaseRequest, RepairItem,
    RepositoryError, RetailEquipmentChange, ServiceKind, SourceAddressPrefix, StarterCharacter,
    StarterGrant, StarterItem, StarterKey, StorageFault, StorageObserver, StrokeCompletion,
    StrokeCount, StrokePlace, StrokePlayerCommit, StrokeRosterOrder, Username, Weather,
    WindConditions,
};
use pangya_login::{generate_handover, parse_handover};
use pangya_storage::{MIGRATOR, PgRepository, migrate};
use sqlx::PgPool;
use uuid::Uuid;

fn source() -> SourceAddressPrefix {
    SourceAddressPrefix::from_ip("198.51.100.77".parse().expect("test source IP"))
}

fn key(value: &str) -> StarterKey {
    StarterKey::parse(value).expect("test starter key")
}

fn oversized_starter() -> StarterGrant {
    let mut grant = starter();
    grant.items = (0..=MAX_STARTER_ITEMS)
        .map(|index| StarterItem {
            key: key(&format!("oversized.item.{index}")),
            item_type_id: ItemTypeId::new(0x1000_0000 + u32::try_from(index).expect("item index")),
            quantity: 1,
        })
        .collect();
    grant.equipped_club_key = None;
    grant.equipped_ball_key = None;
    grant
}

fn starter() -> StarterGrant {
    StarterGrant {
        character: StarterCharacter {
            key: key("starter.character.nuri"),
            item_type_id: ItemTypeId::new(0x0400_0000),
        },
        items: vec![
            StarterItem {
                key: key("starter.club.air_knight"),
                item_type_id: ItemTypeId::new(0x1000_0001),
                quantity: 1,
            },
            StarterItem {
                key: key("starter.ball.default"),
                item_type_id: ItemTypeId::new(0x1800_0001),
                quantity: 99,
            },
        ],
        equipped_club_key: Some(key("starter.club.air_knight")),
        equipped_ball_key: Some(key("starter.ball.default")),
    }
}

#[derive(Debug, Eq, PartialEq)]
struct StorageSnapshot {
    characters: Vec<(i64, i64, String, DateTime<Utc>)>,
    inventory: Vec<(i64, i64, String, i64, DateTime<Utc>)>,
    equipment: (i64, i64, Option<i64>, Option<i64>, i64, DateTime<Utc>),
    profile: (Option<i64>, String, DateTime<Utc>),
}

async fn storage_snapshot(pool: &PgPool, account_id: i64) -> StorageSnapshot {
    StorageSnapshot {
        characters: sqlx::query_as(
            "SELECT id, item_type_id, starter_key, created_at \
             FROM characters WHERE account_id = $1 ORDER BY id",
        )
        .bind(account_id)
        .fetch_all(pool)
        .await
        .expect("character snapshot"),
        inventory: sqlx::query_as(
            "SELECT id, item_type_id, starter_key, quantity, updated_at \
             FROM inventory_items WHERE account_id = $1 ORDER BY id",
        )
        .bind(account_id)
        .fetch_all(pool)
        .await
        .expect("inventory snapshot"),
        equipment: sqlx::query_as(
            "SELECT id, character_id, club_item_id, ball_item_id, version, updated_at \
             FROM equipment_sets WHERE account_id = $1",
        )
        .bind(account_id)
        .fetch_one(pool)
        .await
        .expect("equipment snapshot"),
        profile: sqlx::query_as(
            "SELECT selected_character_id, setup_state, updated_at \
             FROM profiles WHERE account_id = $1",
        )
        .bind(account_id)
        .fetch_one(pool)
        .await
        .expect("profile snapshot"),
    }
}

async fn assert_no_aggregate_rows(pool: &PgPool) {
    for table in [
        "accounts",
        "credentials",
        "profiles",
        "characters",
        "inventory_items",
        "equipment_sets",
    ] {
        let query = format!("SELECT count(*) FROM {table}");
        let count: i64 = sqlx::query_scalar(&query)
            .fetch_one(pool)
            .await
            .expect("count aggregate table");
        assert_eq!(count, 0, "{table} retained a partial row");
    }
}

fn account(username: &str, nickname: Option<&str>) -> NewAccount {
    NewAccount {
        username: Username::parse(username).expect("test username"),
        credential_hash: CredentialHash::new(
            "$argon2id$v=19$m=19456,t=2,p=1$c3RvcmFnZXRlc3RzYWx0MTIzNA$2nLtyYaBUcFpTzFeQpjRPYkbRmGjPcW+PvR7YfIkV8A"
                .to_owned(),
        ),
        nickname: nickname.map(|value| Nickname::parse(value).expect("test nickname")),
        starter: starter(),
    }
}

#[sqlx::test]
async fn retail_equipment_update_is_owned_and_transactional(pool: PgPool) {
    migrate(&pool).await.expect("migration");
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_operator_account(account("retailslots", Some("RetailSlots")))
        .await
        .expect("account");
    let account_id = aggregate.account.id;
    let caddie_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity, inventory_class) \
         VALUES ($1, 469762049, 'test.caddie', 1, 'caddie') RETURNING id",
    )
    .bind(account_id.get())
    .fetch_one(&pool)
    .await
    .expect("owned caddie");
    let state = repository
        .update_retail_equipment(
            account_id,
            EconomyOperationId::new(uuid::Uuid::new_v4()),
            aggregate.equipment.version,
            RetailEquipmentChange::Caddie(u32::try_from(caddie_id).expect("caddie id")),
        )
        .await
        .expect("owned equip");
    let state = match state {
        EconomyCommit::Committed(state) => state,
        EconomyCommit::Replayed(_) => panic!("fresh operation replayed"),
    };
    assert_eq!(state.caddie.map(|(_, type_id)| type_id), Some(469762049));
    let version: i64 =
        sqlx::query_scalar("SELECT version FROM equipment_sets WHERE account_id = $1")
            .bind(account_id.get())
            .fetch_one(&pool)
            .await
            .expect("version");
    assert_eq!(version, i64::from(aggregate.equipment.version) + 1);
    let consumable_type = 402_653_185_u32;
    let skin_type = 1_879_048_961_u32;
    let mascot_type = 1_073_741_825_u32;
    let _consumable_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity, inventory_class) \
         VALUES ($1, $2, 'test.consumable', 5, 'consumable') RETURNING id",
    )
    .bind(account_id.get()).bind(i64::from(consumable_type)).fetch_one(&pool).await.expect("consumable");
    let _skin_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity, inventory_class) \
         VALUES ($1, $2, 'test.skin', 1, 'skin') RETURNING id",
    )
    .bind(account_id.get()).bind(i64::from(skin_type)).fetch_one(&pool).await.expect("skin");
    let mascot_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity, inventory_class) \
         VALUES ($1, $2, 'test.mascot', 1, 'mascot') RETURNING id",
    )
    .bind(account_id.get()).bind(i64::from(mascot_type)).fetch_one(&pool).await.expect("mascot");
    let part_type = 134_217_729_u32;
    let part_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity, inventory_class) \
         VALUES ($1, $2, 'test.character-part', 1, 'character_part') RETURNING id",
    )
    .bind(account_id.get()).bind(i64::from(part_type)).fetch_one(&pool).await.expect("character part");
    let state = repository
        .update_retail_equipment(
            account_id,
            EconomyOperationId::new(uuid::Uuid::new_v4()),
            u32::try_from(version).expect("version"),
            RetailEquipmentChange::Consumables([consumable_type, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        )
        .await
        .expect("consumables");
    let state = match state {
        EconomyCommit::Committed(state) => state,
        EconomyCommit::Replayed(_) => panic!("fresh operation replayed"),
    };
    assert_eq!(state.consumables[0], consumable_type);
    let version = version + 1;
    let state = repository
        .update_retail_equipment(
            account_id,
            EconomyOperationId::new(uuid::Uuid::new_v4()),
            u32::try_from(version).expect("version"),
            RetailEquipmentChange::Decoration([skin_type, 0, 0, 0, 0, 0]),
        )
        .await
        .expect("decoration");
    let state = match state {
        EconomyCommit::Committed(state) => state,
        EconomyCommit::Replayed(_) => panic!("fresh operation replayed"),
    };
    assert_eq!(state.decoration[0], skin_type);
    let version = version + 1;
    let state = repository
        .update_retail_equipment(
            account_id,
            EconomyOperationId::new(uuid::Uuid::new_v4()),
            u32::try_from(version).expect("version"),
            RetailEquipmentChange::Mascot(u32::try_from(mascot_id).expect("mascot id")),
        )
        .await
        .expect("mascot");
    let state = match state {
        EconomyCommit::Committed(state) => state,
        EconomyCommit::Replayed(_) => panic!("fresh operation replayed"),
    };
    assert_eq!(state.mascot.map(|(_, type_id)| type_id), Some(mascot_type));
    let version = version + 1;
    let state = repository
        .update_retail_equipment(
            account_id,
            EconomyOperationId::new(uuid::Uuid::new_v4()),
            u32::try_from(version).expect("version"),
            RetailEquipmentChange::CutIn {
                character_id: aggregate.equipment.character_id,
                data: [7; 16],
            },
        )
        .await
        .expect("cut-in");
    let state = match state {
        EconomyCommit::Committed(state) => state,
        EconomyCommit::Replayed(_) => panic!("fresh operation replayed"),
    };
    assert_eq!(state.cut_in.map(|(_, data)| data), Some([7; 16]));
    let version = version + 1;
    let mut part_types = [0_u32; 24];
    let mut part_ids = [0_u32; 24];
    part_types[23] = part_type;
    part_ids[23] = u32::try_from(part_id).expect("part id");
    let state = repository
        .update_retail_equipment(
            account_id,
            EconomyOperationId::new(uuid::Uuid::new_v4()),
            u32::try_from(version).expect("version"),
            RetailEquipmentChange::CharacterParts {
                character_id: aggregate.equipment.character_id,
                type_ids: part_types,
                inventory_ids: part_ids,
                hair_color: 0,
            },
        )
        .await
        .expect("character parts");
    let state = match state {
        EconomyCommit::Committed(state) => state,
        EconomyCommit::Replayed(_) => panic!("fresh operation replayed"),
    };
    assert_eq!(
        state.character_parts.map(|(_, types, _)| types[23]),
        Some(part_type)
    );
    let version = version + 1;
    assert!(
        repository
            .update_retail_equipment(
                account_id,
                EconomyOperationId::new(uuid::Uuid::new_v4()),
                u32::try_from(version).expect("version"),
                RetailEquipmentChange::Caddie(999_999)
            )
            .await
            .is_err()
    );
    let retained: i64 = sqlx::query_scalar("SELECT count(*) FROM player_equipment_slots WHERE account_id = $1 AND slot_family = 'caddie'")
        .bind(account_id.get()).fetch_one(&pool).await.expect("slot");
    assert_eq!(
        retained, 1,
        "rejected ownership must not clear the prior selection"
    );
    let missing_character = CharacterId::new(9_999_999).expect("character id");
    assert!(
        repository
            .update_retail_equipment(
                account_id,
                EconomyOperationId::new(uuid::Uuid::new_v4()),
                u32::try_from(version).expect("version"),
                RetailEquipmentChange::CutIn {
                    character_id: missing_character,
                    data: [0; 16],
                },
            )
            .await
            .is_err(),
        "an empty cut-in still must name an owned character"
    );
}

#[sqlx::test]
async fn issue11_my_room_and_mascot_message_survive_projection_reload(pool: PgPool) {
    migrate(&pool).await.expect("migration");
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_operator_account(account("myroom", Some("MyRoom")))
        .await
        .expect("account");
    let account_id = aggregate.account.id;
    let mascot_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity, inventory_class) \
         VALUES ($1, 1073741825, 'test.myroom.mascot', 1, 'mascot') RETURNING id",
    )
    .bind(account_id.get())
    .fetch_one(&pool)
    .await
    .expect("mascot");
    repository
        .update_retail_equipment(
            account_id,
            EconomyOperationId::new(Uuid::new_v4()),
            aggregate.equipment.version,
            RetailEquipmentChange::Mascot(u32::try_from(mascot_id).expect("mascot id")),
        )
        .await
        .expect("equip mascot");
    sqlx::query(
        "INSERT INTO my_room_furniture (account_id, slot_index, item_type_id, unknown_prefix, unknown_suffix) \
         VALUES ($1, 0, 134217780, $2, $3)",
    )
    .bind(account_id.get())
    .bind([1_u8, 2, 3, 4].as_slice())
    .bind([5_u8; 19].as_slice())
    .execute(&pool)
    .await
    .expect("furniture");
    repository
        .save_mascot_message(
            account_id,
            MascotMessageUpdate {
                inventory_item_id: pangya_domain::InventoryItemId::new(mascot_id).expect("id"),
                message: b"hello visitor".to_vec(),
            },
        )
        .await
        .expect("mascot message");
    let projection = repository
        .load_my_room(account_id)
        .await
        .expect("projection");
    assert_eq!(projection.furniture.len(), 1);
    assert_eq!(projection.furniture[0].item_type_id, 134217780);
    assert_eq!(
        projection.mascot_message.as_deref(),
        Some(&b"hello visitor"[..])
    );
}

#[sqlx::test]
async fn retail_equipment_replay_is_durable_and_serialized(pool: PgPool) {
    migrate(&pool).await.expect("migration");
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_operator_account(account("retailreplay", Some("RetailReplay")))
        .await
        .expect("account");
    let account_id = aggregate.account.id;
    let first_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity, inventory_class) \
         VALUES ($1, 469762049, 'test.replay.caddie.one', 1, 'caddie') RETURNING id",
    )
    .bind(account_id.get())
    .fetch_one(&pool)
    .await
    .expect("first caddie");
    let second_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity, inventory_class) \
         VALUES ($1, 469762050, 'test.replay.caddie.two', 1, 'caddie') RETURNING id",
    )
    .bind(account_id.get())
    .fetch_one(&pool)
    .await
    .expect("second caddie");
    let first = u32::try_from(first_id).expect("first id");
    let second = u32::try_from(second_id).expect("second id");
    let operation = EconomyOperationId::new(uuid::Uuid::new_v4());
    let initial_version = aggregate.equipment.version;
    let committed = repository
        .update_retail_equipment(
            account_id,
            operation,
            initial_version,
            RetailEquipmentChange::Caddie(first),
        )
        .await
        .expect("initial update");
    let committed_state = match committed {
        EconomyCommit::Committed(state) => state,
        EconomyCommit::Replayed(_) => panic!("initial update unexpectedly replayed"),
    };
    assert_eq!(
        committed_state.caddie.map(|(_, kind)| kind),
        Some(469762049)
    );
    let version_after_commit: i64 =
        sqlx::query_scalar("SELECT version FROM equipment_sets WHERE account_id = $1")
            .bind(account_id.get())
            .fetch_one(&pool)
            .await
            .expect("version after commit");
    let replayed = repository
        .update_retail_equipment(
            account_id,
            operation,
            initial_version,
            RetailEquipmentChange::Caddie(first),
        )
        .await
        .expect("exact replay");
    let replayed_state = match replayed {
        EconomyCommit::Replayed(state) => state,
        EconomyCommit::Committed(_) => panic!("exact retry unexpectedly committed"),
    };
    assert_eq!(replayed_state, committed_state);
    assert_eq!(version_after_commit, i64::from(initial_version) + 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM retail_equipment_operations WHERE operation_id = $1",
        )
        .bind(operation.get())
        .fetch_one(&pool)
        .await
        .expect("operation ledger count"),
        1
    );

    let intervening_operation = EconomyOperationId::new(uuid::Uuid::new_v4());
    let intervening = repository
        .update_retail_equipment(
            account_id,
            intervening_operation,
            u32::try_from(version_after_commit).expect("version"),
            RetailEquipmentChange::Caddie(second),
        )
        .await
        .expect("intervening update");
    // A fresh repository instance must use the durable ledger, not connection-local replay state.
    let restarted = PgRepository::new(pool.clone());
    let replay_after_mutation = restarted
        .update_retail_equipment(
            account_id,
            operation,
            initial_version,
            RetailEquipmentChange::Caddie(first),
        )
        .await
        .expect("replay after mutation");
    let replay_after_mutation_state = match replay_after_mutation {
        EconomyCommit::Replayed(state) => state,
        EconomyCommit::Committed(_) => panic!("replay after mutation unexpectedly committed"),
    };
    assert_eq!(replay_after_mutation_state, committed_state);
    assert!(
        restarted
            .update_retail_equipment(
                account_id,
                operation,
                initial_version,
                RetailEquipmentChange::Caddie(second),
            )
            .await
            .is_err(),
        "reusing an operation key with changed input must be rejected"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT version FROM equipment_sets WHERE account_id = $1",)
            .bind(account_id.get())
            .fetch_one(&pool)
            .await
            .expect("version after replay"),
        version_after_commit + 1
    );
    let intervening_state = match intervening {
        EconomyCommit::Committed(state) => state,
        EconomyCommit::Replayed(_) => panic!("intervening update unexpectedly replayed"),
    };
    assert_eq!(
        repository
            .load_retail_equipment(account_id)
            .await
            .expect("projection")
            .caddie,
        intervening_state.caddie
    );

    let concurrent_operation = EconomyOperationId::new(uuid::Uuid::new_v4());
    let expected_version = u32::try_from(version_after_commit + 1).expect("version");
    let left = repository.clone();
    let right = repository.clone();
    let (left, right) = tokio::join!(
        left.update_retail_equipment(
            account_id,
            concurrent_operation,
            expected_version,
            RetailEquipmentChange::Caddie(first),
        ),
        right.update_retail_equipment(
            account_id,
            concurrent_operation,
            expected_version,
            RetailEquipmentChange::Caddie(first),
        )
    );
    let left = left.expect("left concurrent retry");
    let right = right.expect("right concurrent retry");
    assert_ne!(
        matches!(left, EconomyCommit::Committed(_)),
        matches!(right, EconomyCommit::Committed(_)),
        "same-key concurrency must expose exactly one newly-applied result"
    );
    let left_state = match left {
        EconomyCommit::Committed(state) | EconomyCommit::Replayed(state) => state,
    };
    let right_state = match right {
        EconomyCommit::Committed(state) | EconomyCommit::Replayed(state) => state,
    };
    assert_eq!(left_state, right_state);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT version FROM equipment_sets WHERE account_id = $1",)
            .bind(account_id.get())
            .fetch_one(&pool)
            .await
            .expect("version after concurrent retries"),
        version_after_commit + 2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM retail_equipment_operations WHERE operation_id = $1",
        )
        .bind(concurrent_operation.get())
        .fetch_one(&pool)
        .await
        .expect("concurrent operation ledger count"),
        1
    );
}

#[sqlx::test]
async fn empty_database_runs_embedded_migration(pool: PgPool) {
    migrate(&pool).await.expect("embedded migration succeeds");
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_name = 'accounts')",
    )
    .fetch_one(&pool)
    .await
    .expect("catalog query");
    assert!(table_exists);
}

#[sqlx::test(migrations = false)]
async fn upgraded_pre_recent_players_database_keeps_migration_0022_path(pool: PgPool) {
    // Reproduce a database released immediately before the recent-player migration, then apply
    // the unchanged 0022 file as the forward upgrade. This deliberately avoids renaming the
    // already-applied migration, which SQLx checksums and rejects as drift.
    for migration in MIGRATOR.iter().filter(|migration| migration.version <= 21) {
        sqlx::raw_sql(&migration.sql)
            .execute(&pool)
            .await
            .expect("previous released migration");
    }
    sqlx::raw_sql(include_str!("../migrations/0022_retail_recent_players.sql"))
        .execute(&pool)
        .await
        .expect("0022 forward upgrade");
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_name = 'retail_recent_players')",
    )
    .fetch_one(&pool)
    .await
    .expect("recent-player catalog query");
    assert!(table_exists);
    assert_eq!(
        MIGRATOR
            .iter()
            .find(|migration| migration.version == 22)
            .map(|migration| migration.description.as_ref()),
        Some("retail recent players"),
        "the released migration remains version 0022",
    );
}

#[sqlx::test(migrations = false)]
async fn m6_forward_migration_preserves_committed_and_incomplete_m5_rows(pool: PgPool) {
    for migration in [
        include_str!("../migrations/0001_m2_account_foundation.sql"),
        include_str!("../migrations/0002_operator_audit.sql"),
        include_str!("../migrations/0003_m5_solo_matches.sql"),
        include_str!("../migrations/0004_m5_match_wind.sql"),
        include_str!("../migrations/0005_m5_persistence_failure_abort.sql"),
    ] {
        sqlx::raw_sql(migration)
            .execute(&pool)
            .await
            .expect("released migration");
    }
    let account_id: i64 = sqlx::query_scalar(
        "INSERT INTO accounts (username_normalized, username_display) \
         VALUES ('upgrade_m5', 'UpgradeM5') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("account");
    sqlx::query(
        "INSERT INTO profiles (account_id, nickname_display, nickname_normalized, setup_state, pang, experience) \
         VALUES ($1, 'UpgradeNick', 'upgradenick', 'complete', 12, 5)",
    )
    .bind(account_id)
    .execute(&pool)
    .await
    .expect("profile");
    let committed_id = Uuid::new_v4();
    let committed_key = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO matches \
         (id, result_commit_key, course_id, hole, par, catalog_sha256, seed, weather, \
          reward_formula, status, committed_at, wind_speed_tenths, wind_angle_degrees) \
         VALUES ($1, $2, 7, 1, 3, decode(repeat('42', 32), 'hex'), \
          decode(repeat('24', 32), 'hex'), 'clear', 'solo-v1', 'committed', now(), 87, 231)",
    )
    .bind(committed_id)
    .bind(committed_key)
    .execute(&pool)
    .await
    .expect("committed match");
    sqlx::query(
        "INSERT INTO match_players \
         (match_id, account_id, strokes, score, pang_reward, experience_reward, \
          pang_balance_after, experience_balance_after) \
         VALUES ($1, $2, 2, -1, 12, 5, 12, 5)",
    )
    .bind(committed_id)
    .bind(account_id)
    .execute(&pool)
    .await
    .expect("committed player");
    sqlx::query(
        "INSERT INTO currency_ledger \
         (account_id, match_id, idempotency_key, currency, delta, reason, balance_after) \
         VALUES ($1, $2, $3, 'pang', 12, 'solo-v1', 12)",
    )
    .bind(account_id)
    .bind(committed_id)
    .bind(committed_key)
    .execute(&pool)
    .await
    .expect("currency ledger");
    sqlx::query(
        "INSERT INTO progression_ledger \
         (account_id, match_id, idempotency_key, progression, delta, reason, balance_after) \
         VALUES ($1, $2, $3, 'experience', 5, 'solo-v1', 5)",
    )
    .bind(account_id)
    .bind(committed_id)
    .bind(committed_key)
    .execute(&pool)
    .await
    .expect("progression ledger");
    for event in ["started", "committed"] {
        sqlx::query(
            "INSERT INTO match_audit_events (match_id, account_id, event, outcome) \
             VALUES ($1, $2, $3, 'success')",
        )
        .bind(committed_id)
        .bind(account_id)
        .bind(event)
        .execute(&pool)
        .await
        .expect("audit");
    }
    let incomplete_id = Uuid::new_v4();
    let incomplete_key = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO matches \
         (id, result_commit_key, course_id, hole, par, catalog_sha256, seed, weather, \
          reward_formula, status, wind_speed_tenths, wind_angle_degrees) \
         VALUES ($1, $2, 7, 1, 3, decode(repeat('42', 32), 'hex'), \
          decode(repeat('24', 32), 'hex'), 'clear', 'solo-v1', 'loading', 87, 231)",
    )
    .bind(incomplete_id)
    .bind(incomplete_key)
    .execute(&pool)
    .await
    .expect("incomplete match");
    sqlx::query("INSERT INTO match_players (match_id, account_id) VALUES ($1, $2)")
        .bind(incomplete_id)
        .bind(account_id)
        .execute(&pool)
        .await
        .expect("incomplete player");
    sqlx::query(
        "INSERT INTO match_audit_events (match_id, account_id, event, outcome) \
         VALUES ($1, $2, 'started', 'success')",
    )
    .bind(incomplete_id)
    .bind(account_id)
    .execute(&pool)
    .await
    .expect("incomplete audit");

    sqlx::raw_sql(include_str!("../migrations/0006_m6_stroke_records.sql"))
        .execute(&pool)
        .await
        .expect("M6 migration");

    type PreservedM5Row = (Uuid, String, i16, Uuid, Option<i16>, Option<String>);
    let preserved: Vec<PreservedM5Row> = sqlx::query_as(
        "SELECT m.id, m.status, mp.participant_order, mp.player_result_key, mp.place, mp.completion \
         FROM matches m JOIN match_players mp ON mp.match_id = m.id ORDER BY m.status",
    )
    .fetch_all(&pool)
    .await
    .expect("preserved rows");
    assert_eq!(preserved.len(), 2);
    assert!(preserved.iter().any(|row| {
        row.0 == committed_id
            && row.1 == "committed"
            && row.2 == 0
            && row.3 == committed_key
            && row.4 == Some(1)
            && row.5.as_deref() == Some("holed")
    }));
    assert!(preserved.iter().any(|row| {
        row.0 == incomplete_id
            && row.1 == "loading"
            && row.2 == 0
            && row.3 == incomplete_key
            && row.4.is_none()
            && row.5.is_none()
    }));
    let history: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM currency_ledger), \
                (SELECT count(*) FROM progression_ledger), \
                (SELECT count(*) FROM match_audit_events)",
    )
    .fetch_one(&pool)
    .await
    .expect("history");
    assert_eq!(history, (1, 1, 3));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn oversized_starter_is_rejected_at_create_and_grant_boundaries_without_writes(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let mut oversized_account = account("OversizedCreate", Some("OversizedNick"));
    oversized_account.starter = oversized_starter();
    assert_eq!(
        repository.create_account(oversized_account).await,
        Err(RepositoryError::InvalidStarterGrant)
    );
    assert_no_aggregate_rows(&pool).await;

    let mut oversized_operator = account("OversizedOperator", Some("OperatorNick"));
    oversized_operator.starter = oversized_starter();
    assert_eq!(
        repository.create_operator_account(oversized_operator).await,
        Err(RepositoryError::InvalidStarterGrant)
    );
    assert_no_aggregate_rows(&pool).await;

    let aggregate = repository
        .create_account(account("BoundedAccount", Some("BoundedNick")))
        .await
        .expect("bounded account");
    let before = storage_snapshot(&pool, aggregate.account.id.get()).await;
    assert_eq!(
        repository
            .grant_starter(aggregate.account.id, oversized_starter())
            .await,
        Err(RepositoryError::InvalidStarterGrant)
    );
    assert_eq!(
        storage_snapshot(&pool, aggregate.account.id.get()).await,
        before
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn aggregate_creation_is_atomic_and_duplicate_username_is_friendly(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("PlayerOne", Some("Pang-Ya")))
        .await
        .expect("aggregate created");
    assert_eq!(aggregate.account.username_normalized.as_str(), "playerone");
    assert_eq!(aggregate.inventory.len(), 2);
    assert_eq!(aggregate.equipment.character_id, aggregate.character.id);
    let authentication = repository
        .load_authentication(&NormalizedUsername::parse("PLAYERONE").expect("normalized username"))
        .await
        .expect("authentication query")
        .expect("authentication record");
    assert_eq!(authentication.account.id, aggregate.account.id);
    assert_eq!(authentication.account.status, AccountStatus::Active);

    let duplicate = repository
        .create_account(account(" playerone ", Some("OtherNick")))
        .await;
    assert_eq!(duplicate, Err(RepositoryError::DuplicateUsername));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn operator_success_audit_failure_rolls_back_whole_aggregate(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    sqlx::query(
        "CREATE FUNCTION test_fail_operator_audit() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected audit failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("failure function");
    sqlx::query(
        "CREATE TRIGGER test_fail_operator_audit BEFORE INSERT ON operator_audit_events \
         FOR EACH ROW EXECUTE FUNCTION test_fail_operator_audit()",
    )
    .execute(&pool)
    .await
    .expect("failure trigger");

    assert_eq!(
        repository
            .create_operator_account(account("AuditRollback", Some("AuditNick")))
            .await,
        Err(RepositoryError::Storage(StorageFault::PlPgSqlRaise))
    );
    assert_no_aggregate_rows(&pool).await;
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_audit_events")
        .fetch_one(&pool)
        .await
        .expect("audit count");
    assert_eq!(audits, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn offline_note_acceptance_is_atomic_idempotent_and_leased_by_account_id(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let sender = repository
        .create_account(account("NoteSender", Some("SenderNick")))
        .await
        .expect("sender");
    let recipient = repository
        .create_account(account("NoteRecipient", Some("RecipientNick")))
        .await
        .expect("recipient");
    sqlx::query("UPDATE profiles SET pang = 25 WHERE account_id = $1")
        .bind(sender.account.id.get())
        .execute(&pool)
        .await
        .expect("fund sender");
    let request = OfflineNoteRequest {
        sender_id: sender.account.id,
        recipient_id: recipient.account.id,
        operation_id: [7; 32],
        message: b"offline fixture".to_vec(),
    };
    let first = repository
        .accept_offline_note(request.clone())
        .await
        .expect("accept");
    assert_eq!(first.pang, 15);
    assert!(first.accepted);
    let replay = repository
        .accept_offline_note(request)
        .await
        .expect("replay");
    assert_eq!(replay.pang, 15);
    assert!(!replay.accepted);
    let pending = repository
        .claim_offline_notes(recipient.account.id)
        .await
        .expect("claim pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].sender_nickname, b"SenderNick");
    assert_eq!(pending[0].message, b"offline fixture");
    // A second consumer cannot claim a live lease. Expiry makes it claimable again, which is the
    // crash/disconnect recovery path; only the replacement lease can acknowledge it.
    let pending_again = repository
        .claim_offline_notes(recipient.account.id)
        .await
        .expect("claim once");
    assert!(pending_again.is_empty());
    // An expired claimant must not be able to acknowledge the replacement lease. The affected
    // row count is the fence's observable result, and the note remains pending for the new token.
    sqlx::query("UPDATE offline_notes SET delivery_lease_until = now() - interval '1 second'")
        .execute(&pool)
        .await
        .expect("expire lease");
    let recovered = repository
        .claim_offline_notes(recipient.account.id)
        .await
        .expect("claim after disconnect");
    assert_eq!(recovered.len(), 1);
    assert_ne!(recovered[0].lease_token, pending[0].lease_token);
    assert!(
        !repository
            .ack_offline_note(OfflineNoteClaim {
                id: recovered[0].id,
                lease_token: pending[0].lease_token,
            })
            .await
            .expect("stale ack")
    );
    let still_pending = repository
        .claim_offline_notes(recipient.account.id)
        .await
        .expect("stale ack leaves lease");
    assert!(still_pending.is_empty(), "replacement lease remains live");
    assert!(
        repository
            .ack_offline_note(OfflineNoteClaim {
                id: recovered[0].id,
                lease_token: recovered[0].lease_token,
            })
            .await
            .expect("ack pending")
    );
    let acknowledged = repository
        .claim_offline_notes(recipient.account.id)
        .await
        .expect("claim acknowledged");
    assert!(acknowledged.is_empty());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM offline_notes")
        .fetch_one(&pool)
        .await
        .expect("note count");
    assert_eq!(count, 1);
    sqlx::query("UPDATE profiles SET pang = 5 WHERE account_id = $1")
        .bind(sender.account.id.get())
        .execute(&pool)
        .await
        .expect("empty sender");
    assert_eq!(
        repository
            .accept_offline_note(OfflineNoteRequest {
                sender_id: sender.account.id,
                recipient_id: recipient.account.id,
                operation_id: [8; 32],
                message: b"must not charge".to_vec(),
            })
            .await,
        Err(RepositoryError::BalanceInsufficient)
    );
    let (pang, rows): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT pang FROM profiles WHERE account_id = $1), count(*) FROM offline_notes",
    )
    .bind(sender.account.id.get())
    .fetch_one(&pool)
    .await
    .expect("unchanged failed note");
    assert_eq!((pang, rows), (5, 1));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn ban_serializes_before_nickname_and_starter_mutations(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("BanSetup", None))
        .await
        .expect("account");
    let account_id = aggregate.account.id;
    let before = storage_snapshot(&pool, account_id.get()).await;

    let mut ban = pool.begin().await.expect("ban transaction");
    sqlx::query("SELECT id FROM accounts WHERE id = $1 FOR UPDATE")
        .bind(account_id.get())
        .fetch_one(&mut *ban)
        .await
        .expect("account lock");
    let nickname_repository = repository.clone();
    let nickname_task = tokio::spawn(async move {
        nickname_repository
            .set_nickname(
                account_id,
                Nickname::parse("BlockedNick").expect("nickname"),
            )
            .await
    });
    let starter_repository = repository.clone();
    let starter_task = tokio::spawn(async move {
        starter_repository
            .grant_starter(account_id, starter())
            .await
    });
    tokio::task::yield_now().await;
    sqlx::query("UPDATE accounts SET status = 'banned' WHERE id = $1")
        .bind(account_id.get())
        .execute(&mut *ban)
        .await
        .expect("ban");
    ban.commit().await.expect("ban commit");

    assert_eq!(
        nickname_task.await.expect("nickname join"),
        Err(RepositoryError::AccountInactive)
    );
    assert_eq!(
        starter_task.await.expect("starter join"),
        Err(RepositoryError::AccountInactive)
    );
    let nickname: Option<String> =
        sqlx::query_scalar("SELECT nickname_display FROM profiles WHERE account_id = $1")
            .bind(account_id.get())
            .fetch_one(&pool)
            .await
            .expect("nickname state");
    assert_eq!(nickname, None);
    let after = storage_snapshot(&pool, account_id.get()).await;
    assert_eq!(after, before);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_aggregate_mutation_stage_rolls_back_everything(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    sqlx::query(
        "CREATE FUNCTION test_fail_mutation() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected aggregate mutation failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("create test-only failure function");

    let stages = [
        ("account", "accounts", "INSERT"),
        ("credential", "credentials", "INSERT"),
        ("profile", "profiles", "INSERT"),
        ("character", "characters", "INSERT"),
        ("inventory", "inventory_items", "INSERT"),
        ("equipment", "equipment_sets", "INSERT"),
        ("setup", "profiles", "UPDATE OF selected_character_id"),
    ];
    for (index, (stage, table, operation)) in stages.into_iter().enumerate() {
        let create_trigger = format!(
            "CREATE TRIGGER test_fail_stage BEFORE {operation} ON {table} \
             FOR EACH ROW EXECUTE FUNCTION test_fail_mutation()"
        );
        sqlx::query(&create_trigger)
            .execute(&pool)
            .await
            .expect("create test-only trigger");
        let result = repository
            .create_account(account(
                &format!("Rollback{index}"),
                Some(&format!("RollNick{index}")),
            ))
            .await;
        assert_eq!(
            result,
            Err(RepositoryError::Storage(StorageFault::PlPgSqlRaise)),
            "stage {stage}"
        );
        assert_no_aggregate_rows(&pool).await;
        let drop_trigger = format!("DROP TRIGGER test_fail_stage ON {table}");
        sqlx::query(&drop_trigger)
            .execute(&pool)
            .await
            .expect("drop test-only trigger");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn player_snapshot_is_coherent_active_complete_and_race_safe(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("SnapshotUser", Some("SnapshotNick")))
        .await
        .expect("account");
    let snapshot = repository
        .load_player_snapshot(aggregate.account.id)
        .await
        .expect("snapshot");
    assert_eq!(snapshot.account.id, aggregate.account.id);
    assert_eq!(
        snapshot.profile.setup_state,
        pangya_domain::SetupState::Complete
    );
    assert_eq!(snapshot.characters.len(), 1);
    assert_eq!(snapshot.inventory.len(), 2);
    assert_eq!(snapshot.equipment.character_id, snapshot.characters[0].id);

    let account_id = aggregate.account.id;
    let (loaded, banned) = tokio::join!(
        repository.load_player_snapshot(account_id),
        repository.set_status(account_id, AccountStatus::Banned, SystemTime::now())
    );
    banned.expect("ban");
    assert!(
        loaded.is_ok() || loaded == Err(RepositoryError::AccountInactive),
        "repeatable read must be coherent: {loaded:?}"
    );
    assert_eq!(
        repository.load_player_snapshot(account_id).await,
        Err(RepositoryError::AccountInactive)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn player_snapshot_rejects_valid_but_incomplete_persisted_setup(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("CorruptSnapshot", Some("CorruptNick")))
        .await
        .expect("account");
    sqlx::query("UPDATE profiles SET setup_state = 'needs_starter' WHERE account_id = $1")
        .bind(aggregate.account.id.get())
        .execute(&pool)
        .await
        .expect("corrupt setup");
    assert_eq!(
        repository.load_player_snapshot(aggregate.account.id).await,
        Err(RepositoryError::CorruptData)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn nickname_unique_constraint_resolves_race_once(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let first = repository
        .create_account(account("NickRaceOne", None))
        .await
        .expect("first account");
    let second = repository
        .create_account(account("NickRaceTwo", None))
        .await
        .expect("second account");
    let nickname_a = Nickname::parse("Same-Nick").expect("nickname");
    let normalized_nickname = nickname_a.normalized().clone();
    assert!(
        repository
            .nickname_available(&normalized_nickname)
            .await
            .expect("availability before race")
    );
    let nickname_b = nickname_a.clone();
    let (left, right) = tokio::join!(
        repository.set_nickname(first.account.id, nickname_a),
        repository.set_nickname(second.account.id, nickname_b)
    );
    let outcomes = [left, right];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| **result == Err(RepositoryError::DuplicateNickname))
            .count(),
        1
    );
    assert!(
        !repository
            .nickname_available(&normalized_nickname)
            .await
            .expect("availability after race")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn starter_replay_is_write_free_and_all_configuration_drift_is_rejected(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("StarterReplay", Some("StarterNick")))
        .await
        .expect("account");
    let account_id = aggregate.account.id;
    let before = storage_snapshot(&pool, account_id.get()).await;
    let (left, right) = tokio::join!(
        repository.grant_starter(account_id, starter()),
        repository.grant_starter(account_id, starter())
    );
    assert_eq!(left.expect("left replay"), aggregate);
    assert_eq!(right.expect("right replay"), aggregate);
    assert_eq!(storage_snapshot(&pool, account_id.get()).await, before);

    let mut character_key = starter();
    character_key.character.key = key("starter.character.changed");
    let mut character_type = starter();
    character_type.character.item_type_id = ItemTypeId::new(99);
    let mut item_key = starter();
    item_key.items[0].key = key("starter.club.changed");
    item_key.equipped_club_key = Some(key("starter.club.changed"));
    let mut item_type = starter();
    item_type.items[0].item_type_id = ItemTypeId::new(100);
    let mut quantity = starter();
    quantity.items[1].quantity = 1;
    let mut equipment = starter();
    equipment.equipped_ball_key = None;
    let mut missing_item = starter();
    missing_item.items.pop();
    missing_item.equipped_ball_key = None;
    let mut added_item = starter();
    added_item.items.push(StarterItem {
        key: key("starter.extra"),
        item_type_id: ItemTypeId::new(101),
        quantity: 1,
    });
    let mut duplicate_key = starter();
    duplicate_key.items[1].key = duplicate_key.items[0].key.clone();

    for (case, drifted) in [
        ("character key", character_key),
        ("character type", character_type),
        ("item key", item_key),
        ("item type", item_type),
        ("quantity", quantity),
        ("equipment", equipment),
        ("missing item", missing_item),
        ("added item", added_item),
        ("duplicate key", duplicate_key),
    ] {
        assert_eq!(
            repository.grant_starter(account_id, drifted).await,
            Err(RepositoryError::InvalidStarterGrant),
            "drift case {case}"
        );
        assert_eq!(
            storage_snapshot(&pool, account_id.get()).await,
            before,
            "drift case {case} changed storage"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn handover_rejects_target_expiry_replay_and_wrong_digest(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let aggregate = repository
        .create_account(account("HandoverCases", Some("HandoverNick")))
        .await
        .expect("account");
    let now = SystemTime::now();

    let wrong_digest = generate_handover(aggregate.account.id, ServiceKind::Game, source(), now)
        .expect("generate digest case");
    let parsed_wrong_digest =
        parse_handover(wrong_digest.token.expose_secret()).expect("parse digest case");
    repository.issue(wrong_digest.record).await.expect("issue");
    assert_eq!(
        repository
            .consume(ConsumeHandover {
                id: parsed_wrong_digest.id,
                digest: HandoverDigest::new([0; 32]),
                target: ServiceKind::Game,
                now,
            })
            .await,
        Err(HandoverError::Invalid)
    );

    let wrong_target = generate_handover(aggregate.account.id, ServiceKind::Game, source(), now)
        .expect("generate target case");
    let parsed = parse_handover(wrong_target.token.expose_secret()).expect("parse");
    repository.issue(wrong_target.record).await.expect("issue");
    assert_eq!(
        repository
            .consume(ConsumeHandover {
                id: parsed.id,
                digest: parsed.digest,
                target: ServiceKind::Message,
                now,
            })
            .await,
        Err(HandoverError::WrongTarget)
    );

    let expiring = generate_handover(aggregate.account.id, ServiceKind::Game, source(), now)
        .expect("generate expiry case");
    let parsed_expiring = parse_handover(expiring.token.expose_secret()).expect("parse");
    repository.issue(expiring.record).await.expect("issue");
    assert_eq!(
        repository
            .consume(ConsumeHandover {
                id: parsed_expiring.id,
                digest: parsed_expiring.digest,
                target: ServiceKind::Game,
                now: now + Duration::from_secs(60),
            })
            .await,
        Err(HandoverError::Expired)
    );

    let replay = generate_handover(aggregate.account.id, ServiceKind::Game, source(), now)
        .expect("generate replay case");
    let parsed_replay = parse_handover(replay.token.expose_secret()).expect("parse");
    repository.issue(replay.record).await.expect("issue");
    let request = ConsumeHandover {
        id: parsed_replay.id,
        digest: parsed_replay.digest,
        target: ServiceKind::Game,
        now,
    };
    repository
        .consume(request.clone())
        .await
        .expect("first consume");
    assert_eq!(
        repository.consume(request).await,
        Err(HandoverError::AlreadyConsumed)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_handover_consume_has_exactly_one_winner(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let aggregate = repository
        .create_account(account("ConsumeRace", Some("ConsumeNick")))
        .await
        .expect("account");
    let now = SystemTime::now();
    let generated = generate_handover(aggregate.account.id, ServiceKind::Game, source(), now)
        .expect("generate");
    let parsed = parse_handover(generated.token.expose_secret()).expect("parse");
    repository.issue(generated.record).await.expect("issue");
    let request = ConsumeHandover {
        id: parsed.id,
        digest: parsed.digest,
        target: ServiceKind::Game,
        now,
    };
    let (left, right) = tokio::join!(
        repository.consume(request.clone()),
        repository.consume(request)
    );
    let outcomes = [left, right];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| **result == Err(HandoverError::AlreadyConsumed))
            .count(),
        1
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn banning_account_revokes_outstanding_handover(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("BanRevoke", Some("BanNick")))
        .await
        .expect("account");
    let now = SystemTime::now();
    let generated = generate_handover(aggregate.account.id, ServiceKind::Game, source(), now)
        .expect("generate");
    let parsed = parse_handover(generated.token.expose_secret()).expect("parse");
    repository.issue(generated.record).await.expect("issue");
    repository
        .set_status(aggregate.account.id, AccountStatus::Banned, now)
        .await
        .expect("ban");
    let state: (Option<DateTime<Utc>>, Option<DateTime<Utc>>) =
        sqlx::query_as("SELECT consumed_at, revoked_at FROM handover_sessions WHERE id = $1")
            .bind(parsed.id.get())
            .fetch_one(&pool)
            .await
            .expect("revocation state");
    assert!(state.0.is_none());
    assert!(state.1.is_some(), "ban must persist revoked_at");
    assert_eq!(
        repository
            .consume(ConsumeHandover {
                id: parsed.id,
                digest: parsed.digest.clone(),
                target: ServiceKind::Game,
                now,
            })
            .await,
        Err(HandoverError::AccountInactive)
    );
    repository
        .set_status(
            aggregate.account.id,
            AccountStatus::Active,
            now + Duration::from_secs(1),
        )
        .await
        .expect("reactivate");
    assert_eq!(
        repository
            .consume(ConsumeHandover {
                id: parsed.id,
                digest: parsed.digest,
                target: ServiceKind::Game,
                now: now + Duration::from_secs(1),
            })
            .await,
        Err(HandoverError::AlreadyConsumed),
        "reactivation must not restore a revoked token"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn status_and_revocation_mutations_roll_back_together(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("BanRollback", Some("BanRollNick")))
        .await
        .expect("account");
    let now = SystemTime::now();
    let generated = generate_handover(aggregate.account.id, ServiceKind::Game, source(), now)
        .expect("generate");
    let selector = generated.record.id;
    repository.issue(generated.record).await.expect("issue");
    sqlx::query(
        "CREATE FUNCTION test_fail_revocation() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected revocation failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("failure function");
    sqlx::query(
        "CREATE TRIGGER test_fail_revocation BEFORE UPDATE OF revoked_at ON handover_sessions \
         FOR EACH ROW EXECUTE FUNCTION test_fail_revocation()",
    )
    .execute(&pool)
    .await
    .expect("failure trigger");
    assert_eq!(
        repository
            .set_status(aggregate.account.id, AccountStatus::Banned, now)
            .await,
        Err(RepositoryError::Storage(StorageFault::PlPgSqlRaise))
    );
    let status: String = sqlx::query_scalar("SELECT status FROM accounts WHERE id = $1")
        .bind(aggregate.account.id.get())
        .fetch_one(&pool)
        .await
        .expect("status");
    let revoked: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT revoked_at FROM handover_sessions WHERE id = $1")
            .bind(selector.get())
            .fetch_one(&pool)
            .await
            .expect("revoked state");
    assert_eq!(status, "active");
    assert!(revoked.is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_ban_and_consume_leave_one_terminal_handover_state(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("BanConsumeRace", Some("BanRaceNick")))
        .await
        .expect("account");
    let now = SystemTime::now();
    let generated = generate_handover(aggregate.account.id, ServiceKind::Game, source(), now)
        .expect("generate");
    let parsed = parse_handover(generated.token.expose_secret()).expect("parse");
    repository.issue(generated.record).await.expect("issue");
    let request = ConsumeHandover {
        id: parsed.id,
        digest: parsed.digest,
        target: ServiceKind::Game,
        now,
    };
    let (ban, consume) = tokio::join!(
        repository.set_status(aggregate.account.id, AccountStatus::Banned, now),
        repository.consume(request.clone())
    );
    ban.expect("ban succeeds");
    assert!(
        consume.is_ok()
            || matches!(
                consume,
                Err(HandoverError::AccountInactive | HandoverError::AlreadyConsumed)
            )
    );
    let state: (Option<DateTime<Utc>>, Option<DateTime<Utc>>) =
        sqlx::query_as("SELECT consumed_at, revoked_at FROM handover_sessions WHERE id = $1")
            .bind(parsed.id.get())
            .fetch_one(&pool)
            .await
            .expect("terminal state");
    assert_ne!(state.0.is_some(), state.1.is_some());
    assert!(repository.consume(request).await.is_err());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn handover_stores_only_canonical_privacy_minimized_source_prefix(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("SourcePrefix", Some("SourceNick")))
        .await
        .expect("account");
    let raw = "2001:db8:1234:56ff:abcd::42";
    let prefix = SourceAddressPrefix::from_ip(raw.parse().expect("IPv6"));
    let generated = generate_handover(
        aggregate.account.id,
        ServiceKind::Game,
        prefix.clone(),
        SystemTime::now(),
    )
    .expect("generate");
    let selector = generated.record.id;
    repository.issue(generated.record).await.expect("issue");
    let stored: String =
        sqlx::query_scalar("SELECT source_address_prefix FROM handover_sessions WHERE id = $1")
            .bind(selector.get())
            .fetch_one(&pool)
            .await
            .expect("stored prefix");
    assert_eq!(stored, prefix.as_str());
    assert_eq!(stored, "2001:db8:1234:5600::/56");
    assert!(!stored.contains("abcd"));
}

fn solo_begin(account_id: pangya_domain::AccountId) -> BeginSoloMatch {
    BeginSoloMatch::new(
        MatchId::new(Uuid::new_v4()),
        MatchResultKey::new(Uuid::new_v4()),
        account_id,
        MatchPlan::with_holes(CourseId::new(7).expect("course"), 1, 0, 3).expect("configuration"),
        CatalogFingerprint::new([0x42; 32]),
        MatchSeed::new([0x24; 32]),
        Weather::Clear,
        WindConditions::new(87, 231).expect("wind"),
    )
}

fn solo_mark(begin: &BeginSoloMatch) -> MarkSoloInGame {
    MarkSoloInGame::new(begin.match_id(), begin.result_key(), begin.account_id())
}

fn solo_commit(begin: &BeginSoloMatch, strokes: u16) -> CommitSoloHole {
    CommitSoloHole::new(
        begin.match_id(),
        begin.result_key(),
        begin.account_id(),
        begin.config(),
        StrokeCount::new(strokes).expect("strokes"),
    )
}

fn stroke_begin_with_config(
    first: AccountId,
    second: AccountId,
    config: MatchPlan,
) -> BeginStrokeMatch {
    BeginStrokeMatch::new(
        MatchId::new(Uuid::new_v4()),
        MatchResultKey::new(Uuid::new_v4()),
        [
            pangya_domain::StrokeParticipant::new(
                first,
                StrokeRosterOrder::First,
                MatchResultKey::new(Uuid::new_v4()),
            ),
            pangya_domain::StrokeParticipant::new(
                second,
                StrokeRosterOrder::Second,
                MatchResultKey::new(Uuid::new_v4()),
            ),
        ],
        config,
        CatalogFingerprint::new([0x62; 32]),
        MatchSeed::new([0x26; 32]),
        Weather::Cloudy,
        WindConditions::new(51, 180).expect("wind"),
    )
    .expect("stroke begin")
}

fn stroke_begin(first: AccountId, second: AccountId) -> BeginStrokeMatch {
    stroke_begin_with_config(
        first,
        second,
        MatchPlan::with_holes(CourseId::new(17).expect("course"), 1, 0, 3).expect("configuration"),
    )
}

fn stroke_commit(
    begin: &BeginStrokeMatch,
    first: (u16, StrokePlace, StrokeCompletion),
    second: (u16, StrokePlace, StrokeCompletion),
) -> CommitStrokeMatch {
    CommitStrokeMatch::new(
        begin.match_id(),
        begin.result_key(),
        begin.config(),
        [
            StrokePlayerCommit::new(begin.participants()[0], first.0, first.1, first.2)
                .expect("first settlement"),
            StrokePlayerCommit::new(begin.participants()[1], second.0, second.1, second.2)
                .expect("second settlement"),
        ],
    )
    .expect("stroke commit")
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn match_wind_columns_are_required_bounded_and_store_authoritative_input(pool: PgPool) {
    let columns: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT column_name, data_type, is_nullable, column_default \
         FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'matches' \
           AND column_name IN ('wind_speed_tenths', 'wind_angle_degrees') \
         ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .expect("wind column schema");
    assert_eq!(
        columns,
        vec![
            (
                "wind_angle_degrees".to_owned(),
                "smallint".to_owned(),
                "NO".to_owned(),
                None,
            ),
            (
                "wind_speed_tenths".to_owned(),
                "smallint".to_owned(),
                "NO".to_owned(),
                None,
            ),
        ]
    );

    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("SoloWindSchema", Some("SoloWindNick")))
        .await
        .expect("account");
    let begin = solo_begin(aggregate.account.id);
    repository
        .begin_solo(begin.clone())
        .await
        .expect("begin with wind");
    let stored: (i16, i16) =
        sqlx::query_as("SELECT wind_speed_tenths, wind_angle_degrees FROM matches WHERE id = $1")
            .bind(begin.match_id().get())
            .fetch_one(&pool)
            .await
            .expect("stored wind");
    assert_eq!(stored, (87, 231));

    for (column, value) in [
        ("wind_speed_tenths", -1_i16),
        ("wind_speed_tenths", 151_i16),
        ("wind_angle_degrees", -1_i16),
        ("wind_angle_degrees", 360_i16),
    ] {
        let statement = format!("UPDATE matches SET {column} = $2 WHERE id = $1");
        assert!(
            sqlx::query(&statement)
                .bind(begin.match_id().get())
                .bind(value)
                .execute(&pool)
                .await
                .is_err(),
            "{column} accepted out-of-range value {value}"
        );
    }
}

async fn match_counts(pool: &PgPool, match_id: MatchId) -> (i64, i64, i64, i64) {
    let players = sqlx::query_scalar("SELECT count(*) FROM match_players WHERE match_id = $1")
        .bind(match_id.get())
        .fetch_one(pool)
        .await
        .expect("players");
    let currency = sqlx::query_scalar("SELECT count(*) FROM currency_ledger WHERE match_id = $1")
        .bind(match_id.get())
        .fetch_one(pool)
        .await
        .expect("currency ledger");
    let progression =
        sqlx::query_scalar("SELECT count(*) FROM progression_ledger WHERE match_id = $1")
            .bind(match_id.get())
            .fetch_one(pool)
            .await
            .expect("progression ledger");
    let audits = sqlx::query_scalar("SELECT count(*) FROM match_audit_events WHERE match_id = $1")
        .bind(match_id.get())
        .fetch_one(pool)
        .await
        .expect("audits");
    (players, currency, progression, audits)
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn solo_in_game_transition_is_checked_idempotent_and_atomic(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let owner = repository
        .create_account(account("SoloMarkOwner", Some("MarkOwner")))
        .await
        .expect("owner");
    let other = repository
        .create_account(account("SoloMarkOther", Some("MarkOther")))
        .await
        .expect("other");

    let committed = solo_begin(owner.account.id);
    repository
        .begin_solo(committed.clone())
        .await
        .expect("begin committed candidate");
    assert_eq!(
        repository
            .mark_solo_in_game(MarkSoloInGame::new(
                committed.match_id(),
                committed.result_key(),
                other.account.id,
            ))
            .await,
        Err(MatchRepositoryError::WrongAccount)
    );
    assert_eq!(
        repository
            .mark_solo_in_game(MarkSoloInGame::new(
                committed.match_id(),
                MatchResultKey::new(Uuid::new_v4()),
                committed.account_id(),
            ))
            .await,
        Err(MatchRepositoryError::WrongResultKey)
    );
    assert_eq!(
        repository.mark_solo_in_game(solo_mark(&committed)).await,
        Ok(MarkSoloInGameOutcome::Marked)
    );
    assert_eq!(
        repository.mark_solo_in_game(solo_mark(&committed)).await,
        Ok(MarkSoloInGameOutcome::Existing)
    );
    repository
        .commit_solo_hole(solo_commit(&committed, 2))
        .await
        .expect("commit");
    assert_eq!(
        repository.mark_solo_in_game(solo_mark(&committed)).await,
        Err(MatchRepositoryError::InvalidStatus)
    );

    let aborted = solo_begin(owner.account.id);
    repository
        .begin_solo(aborted.clone())
        .await
        .expect("begin aborted candidate");
    repository
        .abort(AbortMatch::new(
            aborted.match_id(),
            aborted.result_key(),
            aborted.account_id(),
            MatchAbortReason::Disconnect,
        ))
        .await
        .expect("abort");
    assert_eq!(
        repository.mark_solo_in_game(solo_mark(&aborted)).await,
        Err(MatchRepositoryError::InvalidStatus)
    );

    let wrong_status = solo_begin(owner.account.id);
    repository
        .begin_solo(wrong_status.clone())
        .await
        .expect("begin wrong status");
    sqlx::query("UPDATE matches SET status = 'in_game' WHERE id = $1")
        .bind(wrong_status.match_id().get())
        .execute(&pool)
        .await
        .expect("set in-game status");
    sqlx::query("UPDATE matches SET status = 'results_pending' WHERE id = $1")
        .bind(wrong_status.match_id().get())
        .execute(&pool)
        .await
        .expect("set wrong status");
    assert_eq!(
        repository.mark_solo_in_game(solo_mark(&wrong_status)).await,
        Err(MatchRepositoryError::InvalidStatus)
    );

    let rollback = solo_begin(owner.account.id);
    repository
        .begin_solo(rollback.clone())
        .await
        .expect("begin rollback candidate");
    sqlx::query(
        "CREATE FUNCTION test_fail_mark_in_game() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected mark failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("mark failure function");
    sqlx::query(
        "CREATE TRIGGER test_fail_mark_in_game BEFORE UPDATE ON matches \
         FOR EACH ROW WHEN (NEW.status = 'in_game') EXECUTE FUNCTION test_fail_mark_in_game()",
    )
    .execute(&pool)
    .await
    .expect("mark failure trigger");
    assert_eq!(
        repository.mark_solo_in_game(solo_mark(&rollback)).await,
        Err(MatchRepositoryError::Storage(StorageFault::PlPgSqlRaise))
    );
    let status: String = sqlx::query_scalar("SELECT status FROM matches WHERE id = $1")
        .bind(rollback.match_id().get())
        .fetch_one(&pool)
        .await
        .expect("rollback status");
    assert_eq!(status, "loading");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn solo_match_commit_is_exactly_once_for_sequential_and_concurrent_replay(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("SoloHappy", Some("SoloHappyNick")))
        .await
        .expect("account");
    let begin = solo_begin(aggregate.account.id);
    assert_eq!(
        repository.begin_solo(begin.clone()).await,
        Ok(BeginSoloMatchOutcome::Begun)
    );
    assert_eq!(
        repository.begin_solo(begin.clone()).await,
        Ok(BeginSoloMatchOutcome::Existing)
    );
    assert_eq!(
        repository.mark_solo_in_game(solo_mark(&begin)).await,
        Ok(MarkSoloInGameOutcome::Marked)
    );
    let commit = solo_commit(&begin, 2);
    let (left, right) = tokio::join!(
        repository.commit_solo_hole(commit),
        repository.commit_solo_hole(commit)
    );
    let left = left.expect("left commit");
    let right = right.expect("right replay");
    assert_eq!(left, right);
    assert_eq!(
        (
            left.score(),
            left.pang_reward(),
            left.experience_reward(),
            left.pang_balance(),
            left.experience_balance()
        ),
        (-1, 12, 5, 12, 5)
    );
    assert_eq!(repository.commit_solo_hole(commit).await, Ok(left));
    assert_eq!(match_counts(&pool, begin.match_id()).await, (1, 1, 1, 2));
    let profile: (i64, i64) =
        sqlx::query_as("SELECT pang, experience FROM profiles WHERE account_id = $1")
            .bind(aggregate.account.id.get())
            .fetch_one(&pool)
            .await
            .expect("balances");
    assert_eq!(profile, (12, 5));
    assert_eq!(
        repository
            .abort(AbortMatch::new(
                begin.match_id(),
                begin.result_key(),
                begin.account_id(),
                MatchAbortReason::Disconnect,
            ))
            .await,
        Ok(AbortMatchOutcome::AlreadyCommitted(left))
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn distinct_matches_commit_concurrently_for_one_account_without_lost_rewards(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("SoloConcurrent", Some("ConcurrentNick")))
        .await
        .expect("account");
    let first = solo_begin(aggregate.account.id);
    let second = solo_begin(aggregate.account.id);
    repository
        .begin_solo(first.clone())
        .await
        .expect("first begin");
    repository
        .begin_solo(second.clone())
        .await
        .expect("second begin");
    repository
        .mark_solo_in_game(solo_mark(&first))
        .await
        .expect("first in game");
    repository
        .mark_solo_in_game(solo_mark(&second))
        .await
        .expect("second in game");

    let (first_result, second_result) = tokio::join!(
        repository.commit_solo_hole(solo_commit(&first, 2)),
        repository.commit_solo_hole(solo_commit(&second, 4))
    );
    let first_result = first_result.expect("first commit");
    let second_result = second_result.expect("second commit");
    assert_eq!(
        (
            first_result.score(),
            first_result.pang_reward(),
            first_result.experience_reward()
        ),
        (-1, 12, 5)
    );
    assert_eq!(
        (
            second_result.score(),
            second_result.pang_reward(),
            second_result.experience_reward()
        ),
        (1, 10, 5)
    );

    for (begin, pang_delta) in [(&first, 12_i64), (&second, 10_i64)] {
        let ledger: (i64, i64, i64) = sqlx::query_as(
            "SELECT c.delta, p.delta, \
                    (SELECT count(*) FROM match_players mp WHERE mp.match_id = $1 \
                     AND mp.pang_reward = c.delta AND mp.experience_reward = p.delta) \
             FROM currency_ledger c JOIN progression_ledger p ON p.match_id = c.match_id \
             WHERE c.match_id = $1",
        )
        .bind(begin.match_id().get())
        .fetch_one(&pool)
        .await
        .expect("exact ledger row");
        assert_eq!(ledger, (pang_delta, 5, 1));
        assert_eq!(match_counts(&pool, begin.match_id()).await, (1, 1, 1, 2));
    }
    let balances: (i64, i64) =
        sqlx::query_as("SELECT pang, experience FROM profiles WHERE account_id = $1")
            .bind(aggregate.account.id.get())
            .fetch_one(&pool)
            .await
            .expect("summed balances");
    assert_eq!(balances, (22, 10));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn persistence_failure_abort_is_idempotent_and_never_rewards(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("SoloAbort", Some("SoloAbortNick")))
        .await
        .expect("account");
    let begin = solo_begin(aggregate.account.id);
    repository.begin_solo(begin.clone()).await.expect("begin");
    let abort = AbortMatch::new(
        begin.match_id(),
        begin.result_key(),
        begin.account_id(),
        MatchAbortReason::PersistenceFailure,
    );
    assert_eq!(
        repository.abort(abort).await,
        Ok(AbortMatchOutcome::Aborted)
    );
    assert_eq!(
        repository.abort(abort).await,
        Ok(AbortMatchOutcome::AlreadyAborted)
    );
    assert_eq!(
        repository.commit_solo_hole(solo_commit(&begin, 3)).await,
        Err(MatchRepositoryError::Aborted)
    );
    assert_eq!(match_counts(&pool, begin.match_id()).await, (1, 0, 0, 2));
    let balances: (i64, i64) =
        sqlx::query_as("SELECT pang, experience FROM profiles WHERE account_id = $1")
            .bind(aggregate.account.id.get())
            .fetch_one(&pool)
            .await
            .expect("balances");
    assert_eq!(balances, (0, 0));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn solo_match_rejects_begin_drift_and_wrong_authority_or_config(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let first = repository
        .create_account(account("SoloAuthorityA", Some("SoloAuthA")))
        .await
        .expect("first account");
    let second = repository
        .create_account(account("SoloAuthorityB", Some("SoloAuthB")))
        .await
        .expect("second account");
    let begin = solo_begin(first.account.id);
    repository.begin_solo(begin.clone()).await.expect("begin");
    repository
        .mark_solo_in_game(solo_mark(&begin))
        .await
        .expect("in game");
    let drift = BeginSoloMatch::new(
        begin.match_id(),
        begin.result_key(),
        begin.account_id(),
        begin.config(),
        begin.catalog_fingerprint(),
        begin.seed(),
        begin.weather(),
        WindConditions::new(
            begin.wind().speed_tenths() + 1,
            begin.wind().angle_degrees(),
        )
        .expect("drift wind"),
    );
    assert_eq!(
        repository.begin_solo(drift).await,
        Err(MatchRepositoryError::InputDrift)
    );
    let wrong_account = CommitSoloHole::new(
        begin.match_id(),
        begin.result_key(),
        second.account.id,
        begin.config(),
        StrokeCount::new(3).expect("strokes"),
    );
    assert_eq!(
        repository.commit_solo_hole(wrong_account).await,
        Err(MatchRepositoryError::WrongAccount)
    );
    let wrong_key = CommitSoloHole::new(
        begin.match_id(),
        MatchResultKey::new(Uuid::new_v4()),
        begin.account_id(),
        begin.config(),
        StrokeCount::new(3).expect("strokes"),
    );
    assert_eq!(
        repository.commit_solo_hole(wrong_key).await,
        Err(MatchRepositoryError::WrongResultKey)
    );
    let wrong_config = CommitSoloHole::new(
        begin.match_id(),
        begin.result_key(),
        begin.account_id(),
        MatchPlan::with_holes(CourseId::new(8).expect("course"), 1, 0, 3).expect("config"),
        StrokeCount::new(3).expect("strokes"),
    );
    assert_eq!(
        repository.commit_solo_hole(wrong_config).await,
        Err(MatchRepositoryError::WrongConfig)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn stroke_commit_rejects_hole_mode_drift(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let first = repository
        .create_account(account("StrokeModeA", Some("StrokeModeA")))
        .await
        .expect("first account");
    let second = repository
        .create_account(account("StrokeModeB", Some("StrokeModeB")))
        .await
        .expect("second account");
    let plan = MatchPlan::with_holes(CourseId::new(17).expect("course"), 18, 1, 3)
        .expect("full-card plan");
    let begin = stroke_begin_with_config(first.account.id, second.account.id, plan);
    assert_eq!(
        repository.begin_stroke(begin.clone()).await,
        Ok(BeginStrokeMatchOutcome::Begun)
    );
    repository
        .mark_stroke_in_game(MarkStrokeInGame::new(begin.match_id(), begin.result_key()))
        .await
        .expect("mark in game");
    let wrong_plan =
        MatchPlan::with_holes(begin.config().course_id(), 18, 2, 3).expect("different progression");
    let wrong_commit = CommitStrokeMatch::new(
        begin.match_id(),
        begin.result_key(),
        wrong_plan,
        [
            StrokePlayerCommit::new(
                begin.participants()[0],
                2,
                StrokePlace::First,
                StrokeCompletion::Holed,
            )
            .expect("first commit"),
            StrokePlayerCommit::new(
                begin.participants()[1],
                4,
                StrokePlace::Second,
                StrokeCompletion::StrokeCap,
            )
            .expect("second commit"),
        ],
    )
    .expect("wrong mode commit shape");
    assert_eq!(
        repository.commit_stroke_match(wrong_commit).await,
        Err(MatchRepositoryError::WrongConfig)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn balance_overflow_rolls_back_result_ledgers_audit_and_profile(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("SoloOverflow", Some("SoloOverflowNick")))
        .await
        .expect("account");
    let begin = solo_begin(aggregate.account.id);
    repository.begin_solo(begin.clone()).await.expect("begin");
    repository
        .mark_solo_in_game(solo_mark(&begin))
        .await
        .expect("in game");
    sqlx::query("UPDATE profiles SET pang = $2 WHERE account_id = $1")
        .bind(aggregate.account.id.get())
        .bind(i64::MAX)
        .execute(&pool)
        .await
        .expect("set maximum balance");
    assert_eq!(
        repository.commit_solo_hole(solo_commit(&begin, 3)).await,
        Err(MatchRepositoryError::BalanceOverflow)
    );
    assert_eq!(match_counts(&pool, begin.match_id()).await, (1, 0, 0, 1));
    let state: (String, i64, i64) = sqlx::query_as(
        "SELECT m.status, p.pang, p.experience FROM matches m \
         JOIN match_players mp ON mp.match_id = m.id \
         JOIN profiles p ON p.account_id = mp.account_id WHERE m.id = $1",
    )
    .bind(begin.match_id().get())
    .fetch_one(&pool)
    .await
    .expect("state");
    assert_eq!(state, ("in_game".to_owned(), i64::MAX, 0));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn stale_startup_recovery_aborts_all_with_audit_and_no_reward(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    for index in 0..3 {
        let aggregate = repository
            .create_account(account(
                &format!("StaleSolo{index}"),
                Some(&format!("StaleNick{index}")),
            ))
            .await
            .expect("account");
        repository
            .begin_solo(solo_begin(aggregate.account.id))
            .await
            .expect("begin stale match");
    }
    assert_eq!(
        repository
            .abort_incomplete_matches(IncompleteMatchAbortLimit::new(3).expect("limit"))
            .await,
        Ok(3)
    );
    assert_eq!(
        repository
            .abort_incomplete_matches(IncompleteMatchAbortLimit::new(3).expect("limit"))
            .await,
        Ok(0)
    );
    let states: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM matches WHERE status = 'aborted' \
         AND abort_reason = 'startup_recovery'",
    )
    .fetch_one(&pool)
    .await
    .expect("aborted matches");
    let abort_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM match_audit_events WHERE event = 'aborted' \
         AND reason = 'startup_recovery'",
    )
    .fetch_one(&pool)
    .await
    .expect("abort audits");
    let ledgers: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM currency_ledger) + \
                (SELECT count(*) FROM progression_ledger)",
    )
    .fetch_one(&pool)
    .await
    .expect("ledgers");
    assert_eq!((states, abort_audits, ledgers), (3, 3, 0));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn startup_recovery_second_row_failure_rolls_back_first_row(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    for index in 0..2 {
        let aggregate = repository
            .create_account(account(
                &format!("RecoveryRollback{index}"),
                Some(&format!("RecoveryRoll{index}")),
            ))
            .await
            .expect("account");
        repository
            .begin_solo(solo_begin(aggregate.account.id))
            .await
            .expect("begin");
    }
    let ordered_match_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM matches WHERE status = 'loading' ORDER BY created_at, id",
    )
    .fetch_all(&pool)
    .await
    .expect("ordered recovery rows");
    assert_eq!(ordered_match_ids.len(), 2);
    let second = ordered_match_ids[1];
    sqlx::query(
        "CREATE FUNCTION test_fail_second_recovery() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected second recovery failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("failure function");
    sqlx::query(&format!(
        "CREATE TRIGGER test_fail_second_recovery_audit BEFORE INSERT ON match_audit_events \
         FOR EACH ROW WHEN (NEW.event = 'aborted' AND NEW.match_id = '{second}'::uuid) \
         EXECUTE FUNCTION test_fail_second_recovery()"
    ))
    .execute(&pool)
    .await
    .expect("failure trigger");

    assert_eq!(
        repository
            .abort_incomplete_matches(IncompleteMatchAbortLimit::new(2).expect("limit"))
            .await,
        Err(MatchRepositoryError::Storage(StorageFault::PlPgSqlRaise))
    );
    let state: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
             (SELECT count(*) FROM matches WHERE status = 'loading'), \
             (SELECT count(*) FROM match_players WHERE quit), \
             (SELECT count(*) FROM match_audit_events WHERE event = 'aborted')",
    )
    .fetch_one(&pool)
    .await
    .expect("rolled back recovery state");
    assert_eq!(state, (2, 0, 0));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn startup_recovery_cap_rejects_without_partial_abort(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    for index in 0..2 {
        let aggregate = repository
            .create_account(account(
                &format!("CappedSolo{index}"),
                Some(&format!("CappedNick{index}")),
            ))
            .await
            .expect("account");
        repository
            .begin_solo(solo_begin(aggregate.account.id))
            .await
            .expect("begin");
    }
    assert_eq!(
        repository
            .abort_incomplete_matches(IncompleteMatchAbortLimit::new(1).expect("limit"))
            .await,
        Err(MatchRepositoryError::RecoveryLimitExceeded)
    );
    let loading: i64 = sqlx::query_scalar("SELECT count(*) FROM matches WHERE status = 'loading'")
        .fetch_one(&pool)
        .await
        .expect("loading matches");
    assert_eq!(loading, 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn begin_and_abort_stage_failures_leave_no_partial_lifecycle(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("LifecycleRollback", Some("LifecycleRoll")))
        .await
        .expect("account");
    sqlx::query(
        "CREATE FUNCTION test_fail_match_lifecycle() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected match lifecycle failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("failure function");

    for (stage, table, condition) in [
        ("match", "matches", ""),
        ("player", "match_players", ""),
        (
            "started audit",
            "match_audit_events",
            "WHEN (NEW.event = 'started')",
        ),
    ] {
        let begin = solo_begin(aggregate.account.id);
        let trigger = format!(
            "CREATE TRIGGER test_fail_match_lifecycle_stage BEFORE INSERT ON {table} \
             FOR EACH ROW {condition} EXECUTE FUNCTION test_fail_match_lifecycle()"
        );
        sqlx::query(&trigger).execute(&pool).await.expect("trigger");
        assert_eq!(
            repository.begin_solo(begin.clone()).await,
            Err(MatchRepositoryError::Storage(StorageFault::PlPgSqlRaise)),
            "begin stage {stage}"
        );
        sqlx::query(&format!(
            "DROP TRIGGER test_fail_match_lifecycle_stage ON {table}"
        ))
        .execute(&pool)
        .await
        .expect("drop trigger");
        let matches: i64 = sqlx::query_scalar("SELECT count(*) FROM matches WHERE id = $1")
            .bind(begin.match_id().get())
            .fetch_one(&pool)
            .await
            .expect("match count");
        assert_eq!(matches, 0, "begin stage {stage} retained a match");
    }

    for (index, (stage, table, condition)) in [
        ("player", "match_players", ""),
        ("match", "matches", ""),
        (
            "abort audit",
            "match_audit_events",
            "WHEN (NEW.event = 'aborted')",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let begin = solo_begin(aggregate.account.id);
        repository.begin_solo(begin.clone()).await.expect("begin");
        let operation = if table == "match_audit_events" {
            "INSERT"
        } else {
            "UPDATE"
        };
        let trigger = format!(
            "CREATE TRIGGER test_fail_match_lifecycle_stage BEFORE {operation} ON {table} \
             FOR EACH ROW {condition} EXECUTE FUNCTION test_fail_match_lifecycle()"
        );
        sqlx::query(&trigger).execute(&pool).await.expect("trigger");
        let abort = AbortMatch::new(
            begin.match_id(),
            begin.result_key(),
            begin.account_id(),
            if index == 0 {
                MatchAbortReason::Disconnect
            } else {
                MatchAbortReason::Shutdown
            },
        );
        assert_eq!(
            repository.abort(abort).await,
            Err(MatchRepositoryError::Storage(StorageFault::PlPgSqlRaise)),
            "abort stage {stage}"
        );
        sqlx::query(&format!(
            "DROP TRIGGER test_fail_match_lifecycle_stage ON {table}"
        ))
        .execute(&pool)
        .await
        .expect("drop trigger");
        let state: (String, bool, i64) = sqlx::query_as(
            "SELECT m.status, mp.quit, \
                    (SELECT count(*) FROM match_audit_events a \
                     WHERE a.match_id = m.id AND a.event = 'aborted') \
             FROM matches m JOIN match_players mp ON mp.match_id = m.id WHERE m.id = $1",
        )
        .bind(begin.match_id().get())
        .fetch_one(&pool)
        .await
        .expect("state");
        assert_eq!(
            state,
            ("loading".to_owned(), false, 0),
            "abort stage {stage}"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_solo_commit_mutation_stage_failure_rolls_back_transaction(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    sqlx::query(
        "CREATE FUNCTION test_fail_match_commit() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected match commit failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("failure function");
    let stages = [
        (
            "match pending",
            "matches",
            "UPDATE",
            "WHEN (NEW.status = 'results_pending')",
        ),
        ("profile", "profiles", "UPDATE", ""),
        ("pang ledger", "currency_ledger", "INSERT", ""),
        ("experience ledger", "progression_ledger", "INSERT", ""),
        ("player result", "match_players", "UPDATE", ""),
        (
            "commit audit",
            "match_audit_events",
            "INSERT",
            "WHEN (NEW.event = 'committed')",
        ),
        (
            "match terminal",
            "matches",
            "UPDATE",
            "WHEN (NEW.status = 'committed')",
        ),
    ];
    for (index, (stage, table, operation, condition)) in stages.into_iter().enumerate() {
        let aggregate = repository
            .create_account(account(
                &format!("CommitRollback{index}"),
                Some(&format!("CommitRoll{index}")),
            ))
            .await
            .expect("account");
        let begin = solo_begin(aggregate.account.id);
        repository.begin_solo(begin.clone()).await.expect("begin");
        repository
            .mark_solo_in_game(solo_mark(&begin))
            .await
            .expect("in game");
        let trigger = format!(
            "CREATE TRIGGER test_fail_match_stage BEFORE {operation} ON {table} \
             FOR EACH ROW {condition} EXECUTE FUNCTION test_fail_match_commit()"
        );
        sqlx::query(&trigger)
            .execute(&pool)
            .await
            .expect("failure trigger");
        assert_eq!(
            repository.commit_solo_hole(solo_commit(&begin, 2)).await,
            Err(MatchRepositoryError::Storage(StorageFault::PlPgSqlRaise)),
            "stage {stage}"
        );
        let drop_trigger = format!("DROP TRIGGER test_fail_match_stage ON {table}");
        sqlx::query(&drop_trigger)
            .execute(&pool)
            .await
            .expect("drop failure trigger");
        assert_eq!(
            match_counts(&pool, begin.match_id()).await,
            (1, 0, 0, 1),
            "stage {stage} retained history"
        );
        let state: (String, Option<i16>, i64, i64) = sqlx::query_as(
            "SELECT m.status, mp.strokes, p.pang, p.experience FROM matches m \
             JOIN match_players mp ON mp.match_id = m.id \
             JOIN profiles p ON p.account_id = mp.account_id WHERE m.id = $1",
        )
        .bind(begin.match_id().get())
        .fetch_one(&pool)
        .await
        .expect("rolled back state");
        assert_eq!(state, ("in_game".to_owned(), None, 0, 0), "stage {stage}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn match_history_composite_foreign_keys_reject_a_different_account(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let owner = repository
        .create_account(account("HistoryOwner", Some("HistoryOwnerNick")))
        .await
        .expect("owner");
    let other = repository
        .create_account(account("HistoryOther", Some("HistoryOtherNick")))
        .await
        .expect("other");
    let begin = solo_begin(owner.account.id);
    repository.begin_solo(begin.clone()).await.expect("begin");

    assert!(
        sqlx::query(
            "INSERT INTO currency_ledger \
             (account_id, match_id, idempotency_key, currency, delta, reason, balance_after) \
             VALUES ($1, $2, $3, 'pang', 1, 'solo-v1', 1)",
        )
        .bind(other.account.id.get())
        .bind(begin.match_id().get())
        .bind(begin.result_key().get())
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            "INSERT INTO progression_ledger \
             (account_id, match_id, idempotency_key, progression, delta, reason, balance_after) \
             VALUES ($1, $2, $3, 'experience', 1, 'solo-v1', 1)",
        )
        .bind(other.account.id.get())
        .bind(begin.match_id().get())
        .bind(begin.result_key().get())
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            "INSERT INTO match_audit_events (match_id, account_id, event, outcome) \
             VALUES ($1, $2, 'committed', 'success')",
        )
        .bind(begin.match_id().get())
        .bind(other.account.id.get())
        .execute(&pool)
        .await
        .is_err()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn ledgers_and_match_audit_reject_updates_and_deletes(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("ImmutableHistory", Some("ImmutableNick")))
        .await
        .expect("account");
    let begin = solo_begin(aggregate.account.id);
    repository.begin_solo(begin.clone()).await.expect("begin");
    repository
        .mark_solo_in_game(solo_mark(&begin))
        .await
        .expect("in game");
    repository
        .commit_solo_hole(solo_commit(&begin, 2))
        .await
        .expect("commit");

    for statement in [
        "UPDATE currency_ledger SET balance_after = balance_after WHERE match_id = $1",
        "DELETE FROM currency_ledger WHERE match_id = $1",
        "UPDATE progression_ledger SET balance_after = balance_after WHERE match_id = $1",
        "DELETE FROM progression_ledger WHERE match_id = $1",
        "UPDATE match_audit_events SET outcome = outcome WHERE match_id = $1",
        "DELETE FROM match_audit_events WHERE match_id = $1",
    ] {
        assert!(
            sqlx::query(statement)
                .bind(begin.match_id().get())
                .execute(&pool)
                .await
                .is_err(),
            "immutable history mutation succeeded: {statement}"
        );
    }
    assert_eq!(match_counts(&pool, begin.match_id()).await, (1, 1, 1, 2));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn solo_lifecycle_apis_reject_stroke_authority_without_mutation(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let first = repository
        .create_account(account("CrossApiStrokeA", Some("CrossApiStrokeA")))
        .await
        .expect("first account");
    let second = repository
        .create_account(account("CrossApiStrokeB", Some("CrossApiStrokeB")))
        .await
        .expect("second account");
    let stroke = stroke_begin(first.account.id, second.account.id);
    repository
        .begin_stroke(stroke.clone())
        .await
        .expect("stroke begin");
    let solo = BeginSoloMatch::new(
        stroke.match_id(),
        stroke.result_key(),
        first.account.id,
        stroke.config(),
        stroke.catalog_fingerprint(),
        stroke.seed(),
        stroke.weather(),
        stroke.wind(),
    );
    let snapshot = || async {
        sqlx::query_as::<_, (String, i64, i64, i64)>(
            "SELECT m.status, \
               (SELECT count(*) FROM match_players WHERE match_id = m.id AND quit), \
               (SELECT count(*) FROM match_players WHERE match_id = m.id AND strokes IS NOT NULL), \
               (SELECT count(*) FROM match_audit_events WHERE match_id = m.id) \
             FROM matches m WHERE m.id = $1",
        )
        .bind(stroke.match_id().get())
        .fetch_one(&pool)
        .await
        .expect("stroke snapshot")
    };
    let before = snapshot().await;
    assert_eq!(
        repository.begin_solo(solo.clone()).await,
        Err(MatchRepositoryError::WrongMode)
    );
    assert_eq!(
        repository
            .mark_solo_in_game(MarkSoloInGame::new(
                stroke.match_id(),
                stroke.result_key(),
                first.account.id,
            ))
            .await,
        Err(MatchRepositoryError::WrongMode)
    );
    assert_eq!(
        repository
            .abort(AbortMatch::new(
                stroke.match_id(),
                stroke.result_key(),
                first.account.id,
                MatchAbortReason::Disconnect,
            ))
            .await,
        Err(MatchRepositoryError::WrongMode)
    );
    assert_eq!(
        repository
            .commit_solo_hole(CommitSoloHole::new(
                stroke.match_id(),
                stroke.result_key(),
                first.account.id,
                stroke.config(),
                StrokeCount::new(2).expect("strokes"),
            ))
            .await,
        Err(MatchRepositoryError::WrongMode)
    );
    assert_eq!(snapshot().await, before);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn schema_rejects_cross_mode_history_unsettled_records_and_invalid_solo_shape(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let solo_account = repository
        .create_account(account("ConditionalSolo", Some("ConditionalSolo")))
        .await
        .expect("solo account");
    let first = repository
        .create_account(account("ConditionalStrokeA", Some("ConditionalStA")))
        .await
        .expect("first stroke account");
    let second = repository
        .create_account(account("ConditionalStrokeB", Some("ConditionalStB")))
        .await
        .expect("second stroke account");
    let solo = solo_begin(solo_account.account.id);
    let stroke = stroke_begin(first.account.id, second.account.id);
    repository
        .begin_solo(solo.clone())
        .await
        .expect("solo begin");
    repository
        .begin_stroke(stroke.clone())
        .await
        .expect("stroke begin");

    for (match_id, account_id, player_key, reason) in [
        (
            solo.match_id().get(),
            solo.account_id().get(),
            solo.result_key().get(),
            "stroke-two-v1",
        ),
        (
            stroke.match_id().get(),
            stroke.participants()[0].account_id().get(),
            stroke.participants()[0].player_result_key().get(),
            "solo-v1",
        ),
    ] {
        assert!(
            sqlx::query(
                "INSERT INTO currency_ledger \
                 (account_id, match_id, idempotency_key, currency, delta, reason, balance_after) \
                 VALUES ($1, $2, $3, 'pang', 1, $4, 1)",
            )
            .bind(account_id)
            .bind(match_id)
            .bind(player_key)
            .bind(reason)
            .execute(&pool)
            .await
            .is_err(),
            "cross-mode currency ledger was accepted"
        );
        assert!(
            sqlx::query(
                "INSERT INTO progression_ledger \
                 (account_id, match_id, idempotency_key, progression, delta, reason, balance_after) \
                 VALUES ($1, $2, $3, 'experience', 1, $4, 1)",
            )
            .bind(account_id)
            .bind(match_id)
            .bind(player_key)
            .bind(reason)
            .execute(&pool)
            .await
            .is_err(),
            "cross-mode progression ledger was accepted"
        );
    }
    assert!(
        sqlx::query(
            "INSERT INTO match_players \
             (match_id, account_id, participant_order, player_result_key) \
             VALUES ($1, $2, 1, $3)",
        )
        .bind(solo.match_id().get())
        .bind(first.account.id.get())
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .is_err(),
        "solo parent accepted a second participant/order one"
    );
    assert!(
        sqlx::query(
            "UPDATE match_players SET strokes = 0, score = NULL, quit = TRUE, place = 1, \
             completion = 'give_up', pang_reward = 0, experience_reward = 0, \
             pang_balance_after = 0, experience_balance_after = 0 WHERE match_id = $1",
        )
        .bind(solo.match_id().get())
        .execute(&pool)
        .await
        .is_err(),
        "stroke-forfeit settlement shape was accepted for solo"
    );

    sqlx::query("UPDATE matches SET status = 'in_game' WHERE id = $1")
        .bind(stroke.match_id().get())
        .execute(&pool)
        .await
        .expect("in game");
    sqlx::query("UPDATE matches SET status = 'results_pending' WHERE id = $1")
        .bind(stroke.match_id().get())
        .execute(&pool)
        .await
        .expect("results pending");
    assert!(
        sqlx::query(
            "INSERT INTO course_records \
             (account_id, course_id, mode, best_score, best_strokes, rounds_completed, \
              best_match_id, best_player_result_key, first_achieved_at, updated_at) \
             VALUES ($1, 17, 'stroke_two', -1, 2, 1, $2, $3, now(), now())",
        )
        .bind(first.account.id.get())
        .bind(stroke.match_id().get())
        .bind(stroke.participants()[0].player_result_key().get())
        .execute(&pool)
        .await
        .is_err(),
        "unsettled results-pending player established a course record"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn nonterminal_match_identity_and_invalid_lifecycle_transitions_are_immutable(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("MatchIdentity", Some("MatchIdentity")))
        .await
        .expect("account");
    let begin = solo_begin(aggregate.account.id);
    repository.begin_solo(begin.clone()).await.expect("begin");
    let mutations = [
        "UPDATE matches SET id = gen_random_uuid() WHERE id = $1",
        "UPDATE matches SET result_commit_key = gen_random_uuid() WHERE id = $1",
        "UPDATE matches SET mode = 'stroke_two' WHERE id = $1",
        "UPDATE matches SET course_id = course_id + 1 WHERE id = $1",
        "UPDATE matches SET hole = 2 WHERE id = $1",
        "UPDATE matches SET par = par + 1 WHERE id = $1",
        "UPDATE matches SET catalog_sha256 = seed WHERE id = $1",
        "UPDATE matches SET seed = catalog_sha256 WHERE id = $1",
        "UPDATE matches SET weather = 'rain' WHERE id = $1",
        "UPDATE matches SET wind_speed_tenths = wind_speed_tenths + 1 WHERE id = $1",
        "UPDATE matches SET wind_angle_degrees = wind_angle_degrees + 1 WHERE id = $1",
        "UPDATE matches SET reward_formula = 'stroke-two-v1' WHERE id = $1",
        "UPDATE matches SET created_at = created_at - interval '1 second' WHERE id = $1",
        "UPDATE matches SET status = 'results_pending' WHERE id = $1",
    ];
    for mutation in mutations {
        assert!(
            sqlx::query(mutation)
                .bind(begin.match_id().get())
                .execute(&pool)
                .await
                .is_err(),
            "nonterminal match mutation succeeded: {mutation}"
        );
    }
    let state: (String, String, i64, i16) =
        sqlx::query_as("SELECT mode, reward_formula, course_id, par FROM matches WHERE id = $1")
            .bind(begin.match_id().get())
            .fetch_one(&pool)
            .await
            .expect("identity state");
    assert_eq!(
        state,
        ("solo_practice".to_owned(), "solo-v1".to_owned(), 7, 3)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn exact_begin_replays_concurrent_with_settlement_are_bounded_and_deadlock_free(
    pool: PgPool,
) {
    let repository = PgRepository::new(pool.clone());

    let solo_commit_account = repository
        .create_account(account("SoloDeadlockCommit", Some("SoloDeadCommit")))
        .await
        .expect("solo commit account");
    let solo_commit_begin = solo_begin(solo_commit_account.account.id);
    repository
        .begin_solo(solo_commit_begin.clone())
        .await
        .expect("solo commit begin");
    repository
        .mark_solo_in_game(solo_mark(&solo_commit_begin))
        .await
        .expect("solo commit mark");
    let (replay, commit) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(
            repository.begin_solo(solo_commit_begin.clone()),
            repository.commit_solo_hole(solo_commit(&solo_commit_begin, 2))
        )
    })
    .await
    .expect("solo replay/commit deadlocked");
    assert_eq!(replay, Ok(BeginSoloMatchOutcome::Existing));
    commit.expect("solo commit");

    let solo_abort_account = repository
        .create_account(account("SoloDeadlockAbort", Some("SoloDeadAbort")))
        .await
        .expect("solo abort account");
    let solo_abort_begin = solo_begin(solo_abort_account.account.id);
    repository
        .begin_solo(solo_abort_begin.clone())
        .await
        .expect("solo abort begin");
    let (replay, abort) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(
            repository.begin_solo(solo_abort_begin.clone()),
            repository.abort(AbortMatch::new(
                solo_abort_begin.match_id(),
                solo_abort_begin.result_key(),
                solo_abort_begin.account_id(),
                MatchAbortReason::Shutdown,
            ))
        )
    })
    .await
    .expect("solo replay/abort deadlocked");
    assert_eq!(replay, Ok(BeginSoloMatchOutcome::Existing));
    assert_eq!(abort, Ok(AbortMatchOutcome::Aborted));

    let stroke_commit_first = repository
        .create_account(account("StrokeDeadCommitA", Some("StrokeDeadComA")))
        .await
        .expect("stroke commit first");
    let stroke_commit_second = repository
        .create_account(account("StrokeDeadCommitB", Some("StrokeDeadComB")))
        .await
        .expect("stroke commit second");
    let stroke_commit_begin = stroke_begin(
        stroke_commit_first.account.id,
        stroke_commit_second.account.id,
    );
    repository
        .begin_stroke(stroke_commit_begin.clone())
        .await
        .expect("stroke commit begin");
    repository
        .mark_stroke_in_game(MarkStrokeInGame::new(
            stroke_commit_begin.match_id(),
            stroke_commit_begin.result_key(),
        ))
        .await
        .expect("stroke commit mark");
    let (replay, commit) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(
            repository.begin_stroke(stroke_commit_begin.clone()),
            repository.commit_stroke_match(stroke_commit(
                &stroke_commit_begin,
                (2, StrokePlace::First, StrokeCompletion::Holed),
                (4, StrokePlace::Second, StrokeCompletion::StrokeCap),
            ))
        )
    })
    .await
    .expect("stroke replay/commit deadlocked");
    assert_eq!(replay, Ok(BeginStrokeMatchOutcome::Existing));
    commit.expect("stroke commit");

    let stroke_abort_first = repository
        .create_account(account("StrokeDeadAbortA", Some("StrokeDeadAbA")))
        .await
        .expect("stroke abort first");
    let stroke_abort_second = repository
        .create_account(account("StrokeDeadAbortB", Some("StrokeDeadAbB")))
        .await
        .expect("stroke abort second");
    let stroke_abort_begin = stroke_begin(
        stroke_abort_first.account.id,
        stroke_abort_second.account.id,
    );
    repository
        .begin_stroke(stroke_abort_begin.clone())
        .await
        .expect("stroke abort begin");
    let (replay, abort) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(
            repository.begin_stroke(stroke_abort_begin.clone()),
            repository.abort_stroke(AbortStrokeMatch::new(
                stroke_abort_begin.match_id(),
                stroke_abort_begin.result_key(),
                MatchAbortReason::Shutdown,
            ))
        )
    })
    .await
    .expect("stroke replay/abort deadlocked");
    assert_eq!(replay, Ok(BeginStrokeMatchOutcome::Existing));
    assert_eq!(abort, Ok(AbortStrokeMatchOutcome::Aborted));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn stroke_begin_mark_and_normal_commit_are_atomic_and_exactly_once(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let first = repository
        .create_account(account("StrokeNormalA", Some("StrokeNormA")))
        .await
        .expect("first account");
    let second = repository
        .create_account(account("StrokeNormalB", Some("StrokeNormB")))
        .await
        .expect("second account");
    let begin = stroke_begin(first.account.id, second.account.id);
    assert_eq!(
        repository.begin_stroke(begin.clone()).await,
        Ok(BeginStrokeMatchOutcome::Begun)
    );
    assert_eq!(
        repository.begin_stroke(begin.clone()).await,
        Ok(BeginStrokeMatchOutcome::Existing)
    );
    assert_eq!(
        repository
            .mark_stroke_in_game(MarkStrokeInGame::new(begin.match_id(), begin.result_key()))
            .await,
        Ok(MarkStrokeInGameOutcome::Marked)
    );
    assert_eq!(
        repository
            .mark_stroke_in_game(MarkStrokeInGame::new(begin.match_id(), begin.result_key()))
            .await,
        Ok(MarkStrokeInGameOutcome::Existing)
    );
    let commit = stroke_commit(
        &begin,
        (2, StrokePlace::First, StrokeCompletion::Holed),
        (4, StrokePlace::Second, StrokeCompletion::StrokeCap),
    );
    let (left, right) = tokio::join!(
        repository.commit_stroke_match(commit),
        repository.commit_stroke_match(commit)
    );
    let left = left.expect("first commit");
    let right = right.expect("replayed commit");
    assert_eq!(left, right);
    assert_eq!(repository.commit_stroke_match(commit).await, Ok(left));
    assert_eq!(
        left.players()
            .iter()
            .map(|player| (
                player.place(),
                player.score(),
                player.pang_reward(),
                player.experience_reward(),
                player.pang_balance(),
                player.experience_balance(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (StrokePlace::First, Some(-1), 12, 5, 12, 5),
            (StrokePlace::Second, Some(1), 10, 5, 10, 5),
        ]
    );
    assert_eq!(match_counts(&pool, begin.match_id()).await, (2, 2, 2, 2));
    assert!(
        sqlx::query("UPDATE match_players SET strokes = 9 WHERE match_id = $1")
            .bind(begin.match_id().get())
            .execute(&pool)
            .await
            .is_err(),
        "terminal player settlement must be immutable"
    );
    assert!(
        sqlx::query("UPDATE matches SET course_id = 99 WHERE id = $1")
            .bind(begin.match_id().get())
            .execute(&pool)
            .await
            .is_err(),
        "terminal aggregate history must be immutable"
    );
    assert!(
        sqlx::query(
            "INSERT INTO course_records \
             (account_id, course_id, mode, best_score, best_strokes, rounds_completed, \
              best_match_id, best_player_result_key, first_achieved_at, updated_at) \
             VALUES ($1, 17, 'stroke_two', 1, 4, 1, $2, $3, now(), now())",
        )
        .bind(second.account.id.get())
        .bind(begin.match_id().get())
        .bind(begin.participants()[1].player_result_key().get())
        .execute(&pool)
        .await
        .is_err(),
        "stroke-cap settlement must not establish a course record"
    );
    assert_eq!(
        repository
            .abort_stroke(AbortStrokeMatch::new(
                begin.match_id(),
                begin.result_key(),
                MatchAbortReason::Disconnect,
            ))
            .await,
        Ok(AbortStrokeMatchOutcome::AlreadyCommitted(left))
    );

    let drifted = BeginStrokeMatch::new(
        begin.match_id(),
        begin.result_key(),
        *begin.participants(),
        begin.config(),
        begin.catalog_fingerprint(),
        begin.seed(),
        Weather::Rain,
        begin.wind(),
    )
    .expect("drift request");
    assert_eq!(
        repository.begin_stroke(drifted).await,
        Err(MatchRepositoryError::InputDrift)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn stroke_begin_rejects_player_key_reuse_and_schema_rejects_third_participant(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let first = repository
        .create_account(account("StrokeKeysA", Some("StrokeKeysA")))
        .await
        .expect("first account");
    let second = repository
        .create_account(account("StrokeKeysB", Some("StrokeKeysB")))
        .await
        .expect("second account");
    let third = repository
        .create_account(account("StrokeKeysC", Some("StrokeKeysC")))
        .await
        .expect("third account");
    let begin = stroke_begin(first.account.id, second.account.id);
    repository
        .begin_stroke(begin.clone())
        .await
        .expect("first begin");
    let reused = BeginStrokeMatch::new(
        MatchId::new(Uuid::new_v4()),
        MatchResultKey::new(Uuid::new_v4()),
        [
            pangya_domain::StrokeParticipant::new(
                first.account.id,
                StrokeRosterOrder::First,
                begin.participants()[0].player_result_key(),
            ),
            pangya_domain::StrokeParticipant::new(
                third.account.id,
                StrokeRosterOrder::Second,
                MatchResultKey::new(Uuid::new_v4()),
            ),
        ],
        begin.config(),
        begin.catalog_fingerprint(),
        begin.seed(),
        begin.weather(),
        begin.wind(),
    )
    .expect("reused-key request");
    assert_eq!(
        repository.begin_stroke(reused).await,
        Err(MatchRepositoryError::InputDrift)
    );
    assert!(
        sqlx::query(
            "INSERT INTO match_players \
             (match_id, account_id, participant_order, player_result_key) VALUES ($1, $2, 2, $3)",
        )
        .bind(begin.match_id().get())
        .bind(third.account.id.get())
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .is_err(),
        "unique constrained orders must cap a match at two participants"
    );
    assert!(
        sqlx::query(
            "INSERT INTO match_players \
             (match_id, account_id, participant_order, player_result_key) VALUES ($1, $2, 0, $3)",
        )
        .bind(Uuid::new_v4())
        .bind(third.account.id.get())
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .is_err(),
        "participant rows must reference an aggregate"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn stroke_winner_and_forfeit_are_truthful_without_course_record(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let first = repository
        .create_account(account("StrokeForfeitA", Some("StrokeForfeitA")))
        .await
        .expect("first account");
    let second = repository
        .create_account(account("StrokeForfeitB", Some("StrokeForfeitB")))
        .await
        .expect("second account");
    let begin = stroke_begin(first.account.id, second.account.id);
    repository.begin_stroke(begin.clone()).await.expect("begin");
    repository
        .mark_stroke_in_game(MarkStrokeInGame::new(begin.match_id(), begin.result_key()))
        .await
        .expect("in game");
    let result = repository
        .commit_stroke_match(stroke_commit(
            &begin,
            (0, StrokePlace::First, StrokeCompletion::WinnerByForfeit),
            (0, StrokePlace::Second, StrokeCompletion::GiveUp),
        ))
        .await
        .expect("commit");
    let winner = result.players()[0];
    assert_eq!(
        (
            winner.score(),
            winner.pang_reward(),
            winner.experience_reward(),
            winner.pang_balance(),
            winner.experience_balance(),
        ),
        (None, 10, 5, 10, 5)
    );
    let forfeited = result.players()[1];
    assert_eq!(
        (
            forfeited.score(),
            forfeited.pang_reward(),
            forfeited.experience_reward(),
            forfeited.pang_balance(),
            forfeited.experience_balance(),
        ),
        (None, 0, 0, 0, 0)
    );
    assert_eq!(match_counts(&pool, begin.match_id()).await, (2, 1, 1, 2));
    let forfeited_ledgers: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM currency_ledger WHERE account_id = $1) + \
                (SELECT count(*) FROM progression_ledger WHERE account_id = $1)",
    )
    .bind(second.account.id.get())
    .fetch_one(&pool)
    .await
    .expect("forfeit ledgers");
    assert_eq!(forfeited_ledgers, 0);
    let records: i64 = sqlx::query_scalar("SELECT count(*) FROM course_records")
        .fetch_one(&pool)
        .await
        .expect("record count");
    assert_eq!(records, 0, "winner-by-forfeit is record-ineligible");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn schema_defers_and_rejects_malformed_stroke_forfeit_aggregates(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let first = repository
        .create_account(account("StrokePairA", Some("StrokePairA")))
        .await
        .expect("first account");
    let second = repository
        .create_account(account("StrokePairB", Some("StrokePairB")))
        .await
        .expect("second account");
    let begin = stroke_begin(first.account.id, second.account.id);
    repository.begin_stroke(begin.clone()).await.expect("begin");
    repository
        .mark_stroke_in_game(MarkStrokeInGame::new(begin.match_id(), begin.result_key()))
        .await
        .expect("in game");

    for (first_completion, second_completion) in [
        ("winner_by_forfeit", "winner_by_forfeit"),
        ("game_timeout", "winner_by_forfeit"),
        ("winner_by_forfeit", "game_timeout"),
        ("winner_by_forfeit", "holed"),
        ("holed", "give_up"),
    ] {
        let mut transaction = pool.begin().await.expect("transaction");
        sqlx::query(
            "UPDATE match_players AS mp SET \
                place = malformed.place, completion = malformed.completion, \
                strokes = CASE WHEN malformed.completion IN ('holed', 'stroke_cap') THEN 1 ELSE 0 END, \
                score = CASE WHEN malformed.completion IN ('holed', 'stroke_cap') THEN -3 ELSE NULL END, \
                pang_reward = CASE WHEN malformed.completion = 'winner_by_forfeit' THEN 10 \
                                   WHEN malformed.completion IN ('holed', 'stroke_cap') THEN 16 ELSE 0 END, \
                experience_reward = CASE WHEN malformed.completion IN \
                    ('winner_by_forfeit', 'holed', 'stroke_cap') THEN 5 ELSE 0 END, \
                pang_balance_after = 100, experience_balance_after = 100, \
                quit = malformed.completion IN \
                    ('give_up', 'disconnect', 'turn_timeout', 'game_timeout') \
             FROM (VALUES (0::smallint, 1::smallint, $2::text), \
                          (1::smallint, 2::smallint, $3::text)) \
                  AS malformed(participant_order, place, completion) \
             WHERE mp.match_id = $1 AND mp.participant_order = malformed.participant_order",
        )
        .bind(begin.match_id().get())
        .bind(first_completion)
        .bind(second_completion)
        .execute(&mut *transaction)
        .await
        .expect("row-local shape remains valid until aggregate check");
        assert!(
            transaction.commit().await.is_err(),
            "malformed aggregate {first_completion}/{second_completion} must fail at transaction end"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn stroke_course_records_keep_deterministic_best_and_count_only_holed(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let first = repository
        .create_account(account("StrokeRecordA", Some("StrokeRecordA")))
        .await
        .expect("first account");
    let second = repository
        .create_account(account("StrokeRecordB", Some("StrokeRecordB")))
        .await
        .expect("second account");
    let course = CourseId::new(29).expect("course");
    for (index, (par, strokes, completion)) in [
        (4, 4, StrokeCompletion::Holed),
        (4, 5, StrokeCompletion::Holed),
        (4, 4, StrokeCompletion::Holed),
        (3, 3, StrokeCompletion::Holed),
        (5, 2, StrokeCompletion::Holed),
        (5, 1, StrokeCompletion::StrokeCap),
    ]
    .into_iter()
    .enumerate()
    {
        let begin = stroke_begin_with_config(
            first.account.id,
            second.account.id,
            MatchPlan::with_holes(course, 1, 0, par).expect("configuration"),
        );
        repository.begin_stroke(begin.clone()).await.expect("begin");
        repository
            .mark_stroke_in_game(MarkStrokeInGame::new(begin.match_id(), begin.result_key()))
            .await
            .expect("in game");
        repository
            .commit_stroke_match(stroke_commit(
                &begin,
                (strokes, StrokePlace::First, completion),
                (
                    u16::try_from(8 + index).expect("strokes"),
                    StrokePlace::Second,
                    StrokeCompletion::StrokeCap,
                ),
            ))
            .await
            .expect("commit");
    }
    let record: (i16, i16, i64, Uuid, Uuid, bool) = sqlx::query_as(
        "SELECT best_score, best_strokes, rounds_completed, best_match_id, \
                best_player_result_key, first_achieved_at <= updated_at \
         FROM course_records WHERE account_id = $1 AND course_id = $2 AND mode = 'stroke_two'",
    )
    .bind(first.account.id.get())
    .bind(i64::from(course.get()))
    .fetch_one(&pool)
    .await
    .expect("course record");
    assert_eq!((record.0, record.1, record.2, record.5), (-3, 2, 5, true));
    let authority: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM match_players WHERE match_id = $1 AND account_id = $2 \
         AND player_result_key = $3 AND completion = 'holed'",
    )
    .bind(record.3)
    .bind(first.account.id.get())
    .bind(record.4)
    .fetch_one(&pool)
    .await
    .expect("record authority");
    assert_eq!(authority, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_stroke_matches_with_shared_accounts_are_deadlock_free(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let shared = repository
        .create_account(account("StrokeShared", Some("StrokeShared")))
        .await
        .expect("shared account");
    let second = repository
        .create_account(account("StrokePeerB", Some("StrokePeerB")))
        .await
        .expect("second account");
    let third = repository
        .create_account(account("StrokePeerC", Some("StrokePeerC")))
        .await
        .expect("third account");
    let left = stroke_begin(shared.account.id, second.account.id);
    let right = stroke_begin(shared.account.id, third.account.id);
    for begin in [&left, &right] {
        repository.begin_stroke(begin.clone()).await.expect("begin");
        repository
            .mark_stroke_in_game(MarkStrokeInGame::new(begin.match_id(), begin.result_key()))
            .await
            .expect("in game");
    }
    let (left_result, right_result) = tokio::join!(
        repository.commit_stroke_match(stroke_commit(
            &left,
            (2, StrokePlace::First, StrokeCompletion::Holed),
            (4, StrokePlace::Second, StrokeCompletion::StrokeCap),
        )),
        repository.commit_stroke_match(stroke_commit(
            &right,
            (2, StrokePlace::First, StrokeCompletion::Holed),
            (4, StrokePlace::Second, StrokeCompletion::StrokeCap),
        ))
    );
    left_result.expect("left commit");
    right_result.expect("right commit");
    let balances: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT account_id, pang, experience FROM profiles \
         WHERE account_id = ANY($1) ORDER BY account_id",
    )
    .bind(
        &[
            shared.account.id.get(),
            second.account.id.get(),
            third.account.id.get(),
        ][..],
    )
    .fetch_all(&pool)
    .await
    .expect("balances");
    let total_pang: i64 = balances.iter().map(|row| row.1).sum();
    let total_experience: i64 = balances.iter().map(|row| row.2).sum();
    assert_eq!((total_pang, total_experience), (44, 20));
    let shared_record_rounds: i64 = sqlx::query_scalar(
        "SELECT rounds_completed FROM course_records WHERE account_id = $1 AND course_id = 17",
    )
    .bind(shared.account.id.get())
    .fetch_one(&pool)
    .await
    .expect("concurrent record");
    assert_eq!(shared_record_rounds, 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_stroke_commit_stage_failure_rolls_back_both_players(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    sqlx::query(
        "CREATE FUNCTION test_fail_stroke_commit() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected stroke commit failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("failure function");
    let stages = [
        (
            "pending",
            "matches",
            "UPDATE",
            "WHEN (NEW.status = 'results_pending')",
        ),
        ("profile", "profiles", "UPDATE", ""),
        ("pang ledger", "currency_ledger", "INSERT", ""),
        ("experience ledger", "progression_ledger", "INSERT", ""),
        ("player settlement", "match_players", "UPDATE", ""),
        (
            "second participant after first settlement",
            "match_players",
            "UPDATE",
            "WHEN (NEW.participant_order = 1)",
        ),
        ("course record", "course_records", "INSERT", ""),
        (
            "commit audit",
            "match_audit_events",
            "INSERT",
            "WHEN (NEW.event = 'committed')",
        ),
        (
            "terminal",
            "matches",
            "UPDATE",
            "WHEN (NEW.status = 'committed')",
        ),
    ];
    for (index, (stage, table, operation, condition)) in stages.into_iter().enumerate() {
        let first = repository
            .create_account(account(
                &format!("StrokeRollA{index}"),
                Some(&format!("StrokeRA{index}")),
            ))
            .await
            .expect("first account");
        let second = repository
            .create_account(account(
                &format!("StrokeRollB{index}"),
                Some(&format!("StrokeRB{index}")),
            ))
            .await
            .expect("second account");
        let begin = stroke_begin(first.account.id, second.account.id);
        repository.begin_stroke(begin.clone()).await.expect("begin");
        repository
            .mark_stroke_in_game(MarkStrokeInGame::new(begin.match_id(), begin.result_key()))
            .await
            .expect("in game");
        let trigger = format!(
            "CREATE TRIGGER test_fail_stroke_stage BEFORE {operation} ON {table} \
             FOR EACH ROW {condition} EXECUTE FUNCTION test_fail_stroke_commit()"
        );
        sqlx::query(&trigger)
            .execute(&pool)
            .await
            .expect("failure trigger");
        assert_eq!(
            repository
                .commit_stroke_match(stroke_commit(
                    &begin,
                    (2, StrokePlace::First, StrokeCompletion::Holed),
                    (4, StrokePlace::Second, StrokeCompletion::Holed),
                ))
                .await,
            Err(MatchRepositoryError::Storage(StorageFault::PlPgSqlRaise)),
            "stage {stage}"
        );
        sqlx::query(&format!("DROP TRIGGER test_fail_stroke_stage ON {table}"))
            .execute(&pool)
            .await
            .expect("drop trigger");
        let state: (String, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT m.status, \
               (SELECT count(*) FROM match_players WHERE match_id = m.id AND strokes IS NOT NULL), \
               (SELECT count(*) FROM currency_ledger WHERE match_id = m.id), \
               (SELECT count(*) FROM progression_ledger WHERE match_id = m.id), \
               (SELECT count(*) FROM course_records WHERE best_match_id = m.id), \
               (SELECT sum(p.pang + p.experience)::BIGINT FROM profiles p JOIN match_players mp \
                 ON mp.account_id = p.account_id WHERE mp.match_id = m.id) \
             FROM matches m WHERE m.id = $1",
        )
        .bind(begin.match_id().get())
        .fetch_one(&pool)
        .await
        .expect("rollback state");
        assert_eq!(
            state,
            ("in_game".to_owned(), 0, 0, 0, 0, 0),
            "stage {stage}"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn stroke_begin_and_abort_failures_leave_no_partial_aggregate(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let first = repository
        .create_account(account("StrokeLifeA", Some("StrokeLifeA")))
        .await
        .expect("first account");
    let second = repository
        .create_account(account("StrokeLifeB", Some("StrokeLifeB")))
        .await
        .expect("second account");
    sqlx::query(
        "CREATE FUNCTION test_fail_stroke_lifecycle() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected stroke lifecycle failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("failure function");

    let failed_begin = stroke_begin(first.account.id, second.account.id);
    sqlx::query(
        "CREATE TRIGGER test_fail_stroke_begin_second BEFORE INSERT ON match_players \
         FOR EACH ROW WHEN (NEW.participant_order = 1) \
         EXECUTE FUNCTION test_fail_stroke_lifecycle()",
    )
    .execute(&pool)
    .await
    .expect("begin failure trigger");
    assert_eq!(
        repository.begin_stroke(failed_begin.clone()).await,
        Err(MatchRepositoryError::Storage(StorageFault::PlPgSqlRaise))
    );
    let failed_begin_rows: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM matches WHERE id = $1) + \
                (SELECT count(*) FROM match_players WHERE match_id = $1) + \
                (SELECT count(*) FROM match_audit_events WHERE match_id = $1)",
    )
    .bind(failed_begin.match_id().get())
    .fetch_one(&pool)
    .await
    .expect("failed begin rows");
    assert_eq!(failed_begin_rows, 0);
    sqlx::query("DROP TRIGGER test_fail_stroke_begin_second ON match_players")
        .execute(&pool)
        .await
        .expect("drop begin trigger");

    let failed_abort = stroke_begin(first.account.id, second.account.id);
    repository
        .begin_stroke(failed_abort.clone())
        .await
        .expect("begin abort candidate");
    sqlx::query(
        "CREATE TRIGGER test_fail_stroke_abort_audit BEFORE INSERT ON match_audit_events \
         FOR EACH ROW WHEN (NEW.event = 'aborted') \
         EXECUTE FUNCTION test_fail_stroke_lifecycle()",
    )
    .execute(&pool)
    .await
    .expect("abort failure trigger");
    assert_eq!(
        repository
            .abort_stroke(AbortStrokeMatch::new(
                failed_abort.match_id(),
                failed_abort.result_key(),
                MatchAbortReason::Shutdown,
            ))
            .await,
        Err(MatchRepositoryError::Storage(StorageFault::PlPgSqlRaise))
    );
    let failed_abort_state: (String, i64, i64) = sqlx::query_as(
        "SELECT m.status, \
           (SELECT count(*) FROM match_players WHERE match_id = m.id AND quit), \
           (SELECT count(*) FROM match_audit_events WHERE match_id = m.id AND event = 'aborted') \
         FROM matches m WHERE id = $1",
    )
    .bind(failed_abort.match_id().get())
    .fetch_one(&pool)
    .await
    .expect("failed abort state");
    assert_eq!(failed_abort_state, ("loading".to_owned(), 0, 0));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn stroke_abort_and_generic_recovery_cover_all_players_once_per_match(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let first = repository
        .create_account(account("StrokeRecoveryA", Some("StrokeRecoverA")))
        .await
        .expect("first account");
    let second = repository
        .create_account(account("StrokeRecoveryB", Some("StrokeRecoverB")))
        .await
        .expect("second account");
    let explicit = stroke_begin(first.account.id, second.account.id);
    repository
        .begin_stroke(explicit.clone())
        .await
        .expect("explicit begin");
    let abort = AbortStrokeMatch::new(
        explicit.match_id(),
        explicit.result_key(),
        MatchAbortReason::LoadingTimeout,
    );
    assert_eq!(
        repository.abort_stroke(abort).await,
        Ok(AbortStrokeMatchOutcome::Aborted)
    );
    assert_eq!(
        repository.abort_stroke(abort).await,
        Ok(AbortStrokeMatchOutcome::AlreadyAborted)
    );

    for _ in 0..2 {
        repository
            .begin_stroke(stroke_begin(first.account.id, second.account.id))
            .await
            .expect("stale begin");
    }
    assert_eq!(
        repository
            .abort_incomplete_matches(IncompleteMatchAbortLimit::new(2).expect("limit"))
            .await,
        Ok(2)
    );
    let recovered: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT count(*) FROM matches WHERE abort_reason = 'startup_recovery'), \
           (SELECT count(*) FROM match_players mp JOIN matches m ON m.id = mp.match_id \
             WHERE m.abort_reason = 'startup_recovery' AND mp.quit), \
           (SELECT count(*) FROM match_audit_events WHERE reason = 'startup_recovery')",
    )
    .fetch_one(&pool)
    .await
    .expect("recovery state");
    assert_eq!(recovered, (2, 4, 2));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn database_checks_reject_range_sign_digest_and_status_violations(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("CheckRanges", Some("CheckNick")))
        .await
        .expect("account");
    let id = aggregate.account.id.get();

    assert!(
        sqlx::query("UPDATE profiles SET pang = -1 WHERE account_id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query(
            "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity) \
         VALUES ($1, 4294967296, 'range.too_high', 1)"
        )
        .bind(id)
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity) \
         VALUES ($1, 1, 'quantity.zero', 0)"
        )
        .bind(id)
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            "INSERT INTO handover_sessions \
         (id, account_id, token_digest, target, source_address_prefix, issued_at, expires_at) \
         VALUES (gen_random_uuid(), $1, '\\x01'::bytea, 'game', '192.0.2.0/24', \
                 now(), now() + interval '1 minute')"
        )
        .bind(id)
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            "INSERT INTO handover_sessions \
             (id, account_id, token_digest, target, source_address_prefix, issued_at, expires_at) \
             VALUES (gen_random_uuid(), $1, decode(repeat('01', 32), 'hex'), 'game', \
                     '192.0.2.99/24', now(), now() + interval '1 minute')"
        )
        .bind(id)
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query("UPDATE accounts SET status = 'unknown' WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .is_err()
    );
}

fn economy_catalog() -> CatalogFingerprint {
    CatalogFingerprint::new([0x77; 32])
}

fn durable_club_definition() -> ItemDefinition {
    ItemDefinition {
        type_id: ItemTypeId::new(0x1000_1001),
        kind: ItemKind::ClubSet,
        sale: ItemSale::Pang(500),
        stacking: ItemStacking::Unique,
        durability: ItemDurability::Durable {
            max: 100,
            repair_pang_per_point: 3,
        },
        compatibility: ItemCompatibility::Any,
    }
}

fn consumable_definition() -> ItemDefinition {
    ItemDefinition {
        type_id: ItemTypeId::new(0x1a00_1001),
        kind: ItemKind::Consumable,
        sale: ItemSale::Pang(25),
        stacking: ItemStacking::Stackable { max_stack: 3 },
        durability: ItemDurability::Nondurable,
        compatibility: ItemCompatibility::Any,
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn economy_purchase_replay_consume_repair_equip_and_audits_are_atomic(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("EconomyCore", Some("EconomyNick")))
        .await
        .expect("account");
    let account_id = aggregate.account.id;
    sqlx::query("UPDATE profiles SET pang = 2000 WHERE account_id = $1")
        .bind(account_id.get())
        .execute(&pool)
        .await
        .expect("fund account");

    let purchase = PurchaseRequest {
        account_id,
        operation_id: EconomyOperationId::new(Uuid::new_v4()),
        catalog: economy_catalog(),
        definition: durable_club_definition(),
        quantity: 1,
    };
    let committed = repository.purchase(purchase).await.expect("purchase");
    let EconomyCommit::Committed(club) = committed else {
        panic!("first purchase must commit");
    };
    assert_eq!((club.pang_balance, club.durability), (1500, Some(100)));

    let mut changed_catalog = purchase;
    changed_catalog.catalog = CatalogFingerprint::new([0x88; 32]);
    assert_eq!(
        repository.purchase(changed_catalog).await.expect("replay"),
        EconomyCommit::Replayed(club)
    );
    let mut drift = purchase;
    drift.quantity = 2;
    assert_eq!(
        repository.purchase(drift).await,
        Err(EconomyError::IdempotencyDrift)
    );

    let stack_request = PurchaseRequest {
        account_id,
        operation_id: EconomyOperationId::new(Uuid::new_v4()),
        catalog: economy_catalog(),
        definition: consumable_definition(),
        quantity: 2,
    };
    let EconomyCommit::Committed(stack) = repository
        .purchase(stack_request)
        .await
        .expect("stack purchase")
    else {
        panic!("stack purchase must commit");
    };
    assert_eq!((stack.quantity_after, stack.pang_balance), (2, 1450));
    let cap_request = PurchaseRequest {
        operation_id: EconomyOperationId::new(Uuid::new_v4()),
        quantity: 2,
        ..stack_request
    };
    assert_eq!(
        repository.purchase(cap_request).await,
        Err(EconomyError::StackFull)
    );
    let frozen_failure: i64 =
        sqlx::query_scalar("SELECT count(*) FROM economy_operations WHERE operation_id = $1")
            .bind(cap_request.operation_id.get())
            .fetch_one(&pool)
            .await
            .expect("operation count");
    assert_eq!(frozen_failure, 0);

    let consume_one = ConsumeItem {
        account_id,
        operation_id: EconomyOperationId::new(Uuid::new_v4()),
        catalog: economy_catalog(),
        item: EconomyItemSelector {
            inventory_id: stack.inventory_id,
            definition: consumable_definition(),
        },
    };
    let EconomyCommit::Committed(consumed) = repository
        .consume_one(consume_one)
        .await
        .expect("consume first")
    else {
        panic!("consume must commit");
    };
    assert_eq!(consumed.quantity_after, 1);
    assert_eq!(
        repository
            .consume_one(consume_one)
            .await
            .expect("consume replay"),
        EconomyCommit::Replayed(consumed)
    );
    let consume_last = ConsumeItem {
        operation_id: EconomyOperationId::new(Uuid::new_v4()),
        ..consume_one
    };
    let EconomyCommit::Committed(removed) = repository
        .consume_one(consume_last)
        .await
        .expect("consume last")
    else {
        panic!("last consume must commit");
    };
    assert_eq!(removed.quantity_after, 0);
    let row_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_items WHERE id = $1)")
            .bind(stack.inventory_id.get())
            .fetch_one(&pool)
            .await
            .expect("inventory existence");
    assert!(!row_exists);

    sqlx::query("UPDATE inventory_items SET durability = 40 WHERE id = $1")
        .bind(club.inventory_id.get())
        .execute(&pool)
        .await
        .expect("synthetic wear setup");
    let repair = RepairItem {
        account_id,
        operation_id: EconomyOperationId::new(Uuid::new_v4()),
        catalog: economy_catalog(),
        item: EconomyItemSelector {
            inventory_id: club.inventory_id,
            definition: durable_club_definition(),
        },
    };
    let EconomyCommit::Committed(repaired) = repository.repair(repair).await.expect("repair")
    else {
        panic!("repair must commit");
    };
    assert_eq!(
        (
            repaired.durability,
            repaired.pang_cost,
            repaired.pang_balance
        ),
        (100, 180, 1270)
    );
    assert_eq!(
        repository.repair(repair).await.expect("repair replay"),
        EconomyCommit::Replayed(repaired)
    );

    let equipment = EquipmentChange {
        account_id,
        operation_id: EconomyOperationId::new(Uuid::new_v4()),
        catalog: economy_catalog(),
        expected_version: aggregate.equipment.version,
        character_id: aggregate.character.id,
        character_type_id: aggregate.character.item_type_id,
        club: Some(EconomyItemSelector {
            inventory_id: club.inventory_id,
            definition: durable_club_definition(),
        }),
        ball: None,
    };
    let EconomyCommit::Committed(equipped) = repository.equip(equipment).await.expect("equip")
    else {
        panic!("equip must commit");
    };
    assert_eq!(equipped.version, 1);
    // A different equipment commit may advance the live version before an encrypted runtime
    // retry returns. The exact operation must replay its durable result, not compare this current
    // version and must not overwrite the intervening selection.
    let intervening = EquipmentChange {
        operation_id: EconomyOperationId::new(Uuid::new_v4()),
        expected_version: equipped.version,
        club: None,
        ..equipment
    };
    let EconomyCommit::Committed(intervening_result) = repository
        .equip(intervening)
        .await
        .expect("intervening equip")
    else {
        panic!("intervening equip must commit");
    };
    assert_eq!(intervening_result.version, 2);
    assert_eq!(
        repository.equip(equipment).await,
        Ok(EconomyCommit::Replayed(equipped))
    );
    assert_eq!(
        repository
            .load_player_snapshot(account_id)
            .await
            .expect("intervening projection")
            .equipment
            .version,
        2
    );
    let raced = EquipmentChange {
        operation_id: EconomyOperationId::new(Uuid::new_v4()),
        ..equipment
    };
    assert_eq!(
        repository.equip(raced).await,
        Err(EconomyError::VersionConflict)
    );

    let snapshot = repository
        .load_player_snapshot(account_id)
        .await
        .expect("snapshot");
    let projected = snapshot
        .inventory
        .iter()
        .find(|item| item.id == club.inventory_id)
        .expect("club projection");
    assert_eq!(
        projected.durability,
        pangya_domain::InventoryDurability::Durable(100)
    );
    assert_eq!(projected.class, pangya_domain::InventoryClass::ClubSet);

    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM economy_operations), \
                (SELECT count(*) FROM shop_currency_ledger), \
                (SELECT count(*) FROM item_ledger), \
                (SELECT count(*) FROM equipment_ledger)",
    )
    .fetch_one(&pool)
    .await
    .expect("audit counts");
    assert_eq!(counts, (7, 3, 5, 2));
    assert!(
        sqlx::query("UPDATE economy_operations SET command = 'repair' WHERE operation_id = $1")
            .bind(purchase.operation_id.get())
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_same_key_purchases_commit_exactly_once(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("EconomyRace", Some("EconomyRaceNick")))
        .await
        .expect("account");
    sqlx::query("UPDATE profiles SET pang = 1000 WHERE account_id = $1")
        .bind(aggregate.account.id.get())
        .execute(&pool)
        .await
        .expect("fund account");
    let request = PurchaseRequest {
        account_id: aggregate.account.id,
        operation_id: EconomyOperationId::new(Uuid::new_v4()),
        catalog: economy_catalog(),
        definition: durable_club_definition(),
        quantity: 1,
    };
    let (left, right) = tokio::join!(repository.purchase(request), repository.purchase(request));
    let outcomes = [left.expect("left"), right.expect("right")];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, EconomyCommit::Committed(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, EconomyCommit::Replayed(_)))
            .count(),
        1
    );
    let state: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT p.pang, \
                (SELECT count(*) FROM inventory_items i WHERE i.account_id = p.account_id AND i.item_type_id = 268439553), \
                (SELECT count(*) FROM economy_operations o WHERE o.account_id = p.account_id), \
                (SELECT count(*) FROM shop_currency_ledger l WHERE l.account_id = p.account_id) \
         FROM profiles p WHERE p.account_id = $1",
    )
    .bind(aggregate.account.id.get())
    .fetch_one(&pool)
    .await
    .expect("state");
    assert_eq!(state, (500, 1, 1, 1));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn purchase_concurrent_with_m5_reward_preserves_exact_balance_arithmetic(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("EconomyReward", Some("EconReward")))
        .await
        .expect("account");
    sqlx::query("UPDATE profiles SET pang = 1000 WHERE account_id = $1")
        .bind(aggregate.account.id.get())
        .execute(&pool)
        .await
        .expect("fund account");
    let begin = solo_begin(aggregate.account.id);
    assert!(matches!(
        repository.begin_solo(begin.clone()).await.expect("begin"),
        BeginSoloMatchOutcome::Begun
    ));
    assert_eq!(
        repository
            .mark_solo_in_game(solo_mark(&begin))
            .await
            .expect("mark"),
        MarkSoloInGameOutcome::Marked
    );
    let purchase = PurchaseRequest {
        account_id: aggregate.account.id,
        operation_id: EconomyOperationId::new(Uuid::new_v4()),
        catalog: economy_catalog(),
        definition: durable_club_definition(),
        quantity: 1,
    };
    let commit = solo_commit(&begin, 2);
    let (purchase_outcome, reward_outcome) = tokio::join!(
        repository.purchase(purchase),
        repository.commit_solo_hole(commit)
    );
    let EconomyCommit::Committed(purchased) = purchase_outcome.expect("purchase") else {
        panic!("purchase must commit");
    };
    let reward = reward_outcome.expect("reward");
    let final_balance: i64 = sqlx::query_scalar("SELECT pang FROM profiles WHERE account_id = $1")
        .bind(aggregate.account.id.get())
        .fetch_one(&pool)
        .await
        .expect("balance");
    assert_eq!(
        u64::try_from(final_balance).expect("nonnegative"),
        1000 + reward.pang_reward() - 500
    );
    assert!(purchased.pang_balance == 500 || purchased.pang_balance == 500 + reward.pang_reward());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM shop_currency_ledger WHERE account_id = $1"
        )
        .bind(aggregate.account.id.get())
        .fetch_one(&pool)
        .await
        .expect("economy ledger"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM currency_ledger WHERE account_id = $1")
            .bind(aggregate.account.id.get())
            .fetch_one(&pool)
            .await
            .expect("match ledger"),
        1
    );
}

#[sqlx::test(migrations = false)]
async fn m7_forward_migration_preserves_legacy_inventory_projection(pool: PgPool) {
    for migration in [
        include_str!("../migrations/0001_m2_account_foundation.sql"),
        include_str!("../migrations/0002_operator_audit.sql"),
        include_str!("../migrations/0003_m5_solo_matches.sql"),
        include_str!("../migrations/0004_m5_match_wind.sql"),
        include_str!("../migrations/0005_m5_persistence_failure_abort.sql"),
        include_str!("../migrations/0006_m6_stroke_records.sql"),
        include_str!("../migrations/0007_m6_winner_by_forfeit.sql"),
    ] {
        sqlx::raw_sql(migration)
            .execute(&pool)
            .await
            .expect("released migration");
    }
    let account_id: i64 = sqlx::query_scalar(
        "INSERT INTO accounts (username_normalized, username_display) \
         VALUES ('m7_upgrade', 'M7Upgrade') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("account");
    let expires: DateTime<Utc> = "2030-01-02T03:04:05Z".parse().expect("time");
    let inventory_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items \
         (account_id, item_type_id, starter_key, quantity, durability, expires_at) \
         VALUES ($1, 268435457, 'legacy.club', 7, 42, $2) RETURNING id",
    )
    .bind(account_id)
    .bind(expires)
    .fetch_one(&pool)
    .await
    .expect("legacy inventory");
    sqlx::raw_sql(include_str!("../migrations/0008_m7_synthetic_economy.sql"))
        .execute(&pool)
        .await
        .expect("M7 migration");
    let row: (i64, i64, Option<i64>, Option<DateTime<Utc>>, String) = sqlx::query_as(
        "SELECT id, quantity, durability, expires_at, inventory_class \
         FROM inventory_items WHERE id = $1",
    )
    .bind(inventory_id)
    .fetch_one(&pool)
    .await
    .expect("preserved inventory");
    assert_eq!(
        row,
        (
            inventory_id,
            7,
            Some(42),
            Some(expires),
            "legacy".to_owned()
        )
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_purchase_mutation_stage_failure_rolls_back_balance_grant_operation_and_ledgers(
    pool: PgPool,
) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("EconomyFailure", Some("EconFailure")))
        .await
        .expect("account");
    sqlx::query("UPDATE profiles SET pang = 5000 WHERE account_id = $1")
        .bind(aggregate.account.id.get())
        .execute(&pool)
        .await
        .expect("fund account");
    sqlx::query(
        "CREATE FUNCTION test_fail_economy_stage() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected economy stage failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("failure function");
    for (stage, table, operation) in [
        ("grant", "inventory_items", "INSERT"),
        ("deduction", "profiles", "UPDATE"),
        ("operation", "economy_operations", "INSERT"),
        ("currency", "shop_currency_ledger", "INSERT"),
        ("item", "item_ledger", "INSERT"),
    ] {
        let trigger = format!(
            "CREATE TRIGGER test_fail_economy BEFORE {operation} ON {table} \
             FOR EACH ROW EXECUTE FUNCTION test_fail_economy_stage()"
        );
        sqlx::query(&trigger)
            .execute(&pool)
            .await
            .expect("failure trigger");
        let request = PurchaseRequest {
            account_id: aggregate.account.id,
            operation_id: EconomyOperationId::new(Uuid::new_v4()),
            catalog: economy_catalog(),
            definition: durable_club_definition(),
            quantity: 1,
        };
        assert_eq!(
            repository.purchase(request).await,
            Err(EconomyError::Storage(StorageFault::PlPgSqlRaise)),
            "stage {stage}"
        );
        sqlx::query(&format!("DROP TRIGGER test_fail_economy ON {table}"))
            .execute(&pool)
            .await
            .expect("drop trigger");
        let state: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT pang, \
                    (SELECT count(*) FROM inventory_items WHERE account_id = profiles.account_id), \
                    (SELECT count(*) FROM economy_operations WHERE account_id = profiles.account_id), \
                    (SELECT count(*) FROM shop_currency_ledger WHERE account_id = profiles.account_id), \
                    (SELECT count(*) FROM item_ledger WHERE account_id = profiles.account_id) \
             FROM profiles WHERE account_id = $1",
        )
        .bind(aggregate.account.id.get())
        .fetch_one(&pool)
        .await
        .expect("rollback state");
        assert_eq!(state, (5000, 2, 0, 0, 0), "stage {stage}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn equip_consume_and_repair_stage_failures_roll_back_all_authoritative_state(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("EconomyStages", Some("EconStages")))
        .await
        .expect("account");
    let account_id = aggregate.account.id;
    sqlx::query("UPDATE profiles SET pang = 5000 WHERE account_id = $1")
        .bind(account_id.get())
        .execute(&pool)
        .await
        .expect("fund");
    let club_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items \
         (account_id, item_type_id, starter_key, quantity, durability, inventory_class) \
         VALUES ($1, $2, 'test.m7.club', 1, 40, 'club_set') RETURNING id",
    )
    .bind(account_id.get())
    .bind(i64::from(durable_club_definition().type_id.get()))
    .fetch_one(&pool)
    .await
    .expect("club");
    let consumable_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items \
         (account_id, item_type_id, starter_key, quantity, inventory_class) \
         VALUES ($1, $2, 'test.m7.consume', 2, 'consumable') RETURNING id",
    )
    .bind(account_id.get())
    .bind(i64::from(consumable_definition().type_id.get()))
    .fetch_one(&pool)
    .await
    .expect("consumable");
    sqlx::query(
        "CREATE FUNCTION test_fail_other_economy_stage() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected economy stage failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("function");

    for (stage, table, operation) in [
        ("profile", "profiles", "UPDATE"),
        ("equipment", "equipment_sets", "UPDATE"),
        ("operation", "economy_operations", "INSERT"),
        ("ledger", "equipment_ledger", "INSERT"),
    ] {
        sqlx::query(&format!(
            "CREATE TRIGGER test_fail_other_economy BEFORE {operation} ON {table} \
             FOR EACH ROW EXECUTE FUNCTION test_fail_other_economy_stage()"
        ))
        .execute(&pool)
        .await
        .expect("equip trigger");
        let request = EquipmentChange {
            account_id,
            operation_id: EconomyOperationId::new(Uuid::new_v4()),
            catalog: economy_catalog(),
            expected_version: 0,
            character_id: aggregate.character.id,
            character_type_id: aggregate.character.item_type_id,
            club: Some(EconomyItemSelector {
                inventory_id: pangya_domain::InventoryItemId::new(club_id).expect("club id"),
                definition: durable_club_definition(),
            }),
            ball: None,
        };
        assert_eq!(
            repository.equip(request).await,
            Err(EconomyError::Storage(StorageFault::PlPgSqlRaise)),
            "{stage}"
        );
        sqlx::query(&format!("DROP TRIGGER test_fail_other_economy ON {table}"))
            .execute(&pool)
            .await
            .expect("drop equip trigger");
        let state: (Option<i64>, i64, i64, i64) = sqlx::query_as(
            "SELECT selected_character_id, \
                    (SELECT version FROM equipment_sets WHERE account_id = profiles.account_id), \
                    (SELECT count(*) FROM economy_operations WHERE account_id = profiles.account_id), \
                    (SELECT count(*) FROM equipment_ledger WHERE account_id = profiles.account_id) \
             FROM profiles WHERE account_id = $1",
        )
        .bind(account_id.get())
        .fetch_one(&pool)
        .await
        .expect("equip rollback");
        assert_eq!(
            state,
            (Some(aggregate.character.id.get()), 0, 0, 0),
            "{stage}"
        );
    }

    for (stage, table, operation) in [
        ("inventory", "inventory_items", "UPDATE"),
        ("operation", "economy_operations", "INSERT"),
        ("ledger", "item_ledger", "INSERT"),
    ] {
        sqlx::query(&format!(
            "CREATE TRIGGER test_fail_other_economy BEFORE {operation} ON {table} \
             FOR EACH ROW EXECUTE FUNCTION test_fail_other_economy_stage()"
        ))
        .execute(&pool)
        .await
        .expect("consume trigger");
        let request = ConsumeItem {
            account_id,
            operation_id: EconomyOperationId::new(Uuid::new_v4()),
            catalog: economy_catalog(),
            item: EconomyItemSelector {
                inventory_id: pangya_domain::InventoryItemId::new(consumable_id)
                    .expect("consumable id"),
                definition: consumable_definition(),
            },
        };
        assert_eq!(
            repository.consume_one(request).await,
            Err(EconomyError::Storage(StorageFault::PlPgSqlRaise)),
            "{stage}"
        );
        sqlx::query(&format!("DROP TRIGGER test_fail_other_economy ON {table}"))
            .execute(&pool)
            .await
            .expect("drop consume trigger");
        let state: (i64, i64, i64) = sqlx::query_as(
            "SELECT quantity, \
                    (SELECT count(*) FROM economy_operations WHERE account_id = inventory_items.account_id), \
                    (SELECT count(*) FROM item_ledger WHERE account_id = inventory_items.account_id) \
             FROM inventory_items WHERE id = $1",
        )
        .bind(consumable_id)
        .fetch_one(&pool)
        .await
        .expect("consume rollback");
        assert_eq!(state, (2, 0, 0), "{stage}");
    }

    for (stage, table, operation) in [
        ("inventory", "inventory_items", "UPDATE"),
        ("profile", "profiles", "UPDATE"),
        ("operation", "economy_operations", "INSERT"),
        ("currency", "shop_currency_ledger", "INSERT"),
        ("item", "item_ledger", "INSERT"),
    ] {
        sqlx::query(&format!(
            "CREATE TRIGGER test_fail_other_economy BEFORE {operation} ON {table} \
             FOR EACH ROW EXECUTE FUNCTION test_fail_other_economy_stage()"
        ))
        .execute(&pool)
        .await
        .expect("repair trigger");
        let request = RepairItem {
            account_id,
            operation_id: EconomyOperationId::new(Uuid::new_v4()),
            catalog: economy_catalog(),
            item: EconomyItemSelector {
                inventory_id: pangya_domain::InventoryItemId::new(club_id).expect("club id"),
                definition: durable_club_definition(),
            },
        };
        assert_eq!(
            repository.repair(request).await,
            Err(EconomyError::Storage(StorageFault::PlPgSqlRaise)),
            "{stage}"
        );
        sqlx::query(&format!("DROP TRIGGER test_fail_other_economy ON {table}"))
            .execute(&pool)
            .await
            .expect("drop repair trigger");
        let state: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT durability, \
                    (SELECT pang FROM profiles WHERE account_id = inventory_items.account_id), \
                    (SELECT count(*) FROM economy_operations WHERE account_id = inventory_items.account_id), \
                    (SELECT count(*) FROM shop_currency_ledger WHERE account_id = inventory_items.account_id), \
                    (SELECT count(*) FROM item_ledger WHERE account_id = inventory_items.account_id) \
             FROM inventory_items WHERE id = $1",
        )
        .bind(club_id)
        .fetch_one(&pool)
        .await
        .expect("repair rollback");
        assert_eq!(state, (40, 5000, 0, 0, 0), "{stage}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn economy_rejects_not_sold_overflow_insufficient_expired_depleted_and_wrong_family(
    pool: PgPool,
) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("EconomyErrors", Some("EconErrors")))
        .await
        .expect("account");
    let account_id = aggregate.account.id;
    let mut not_sold = durable_club_definition();
    not_sold.sale = ItemSale::NotSold;
    let failed_id = EconomyOperationId::new(Uuid::new_v4());
    assert_eq!(
        repository
            .purchase(PurchaseRequest {
                account_id,
                operation_id: failed_id,
                catalog: economy_catalog(),
                definition: not_sold,
                quantity: 1,
            })
            .await,
        Err(EconomyError::Invalid)
    );
    let insufficient = PurchaseRequest {
        account_id,
        operation_id: failed_id,
        catalog: economy_catalog(),
        definition: durable_club_definition(),
        quantity: 1,
    };
    assert_eq!(
        repository.purchase(insufficient).await,
        Err(EconomyError::InsufficientPang)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM economy_operations WHERE operation_id = $1"
        )
        .bind(failed_id.get())
        .fetch_one(&pool)
        .await
        .expect("failed operation count"),
        0
    );
    sqlx::query("UPDATE profiles SET pang = 2000 WHERE account_id = $1")
        .bind(account_id.get())
        .execute(&pool)
        .await
        .expect("fund retry");
    assert!(matches!(
        repository.purchase(insufficient).await,
        Ok(EconomyCommit::Committed(_))
    ));

    let mut overflow_definition = consumable_definition();
    overflow_definition.sale = ItemSale::Pang(i64::MAX as u64);
    assert_eq!(
        repository
            .purchase(PurchaseRequest {
                account_id,
                operation_id: EconomyOperationId::new(Uuid::new_v4()),
                catalog: economy_catalog(),
                definition: overflow_definition,
                quantity: 2,
            })
            .await,
        Err(EconomyError::ArithmeticOverflow)
    );

    let expired_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items \
         (account_id, item_type_id, starter_key, quantity, expires_at, inventory_class) \
         VALUES ($1, $2, 'test.expired', 1, now() - interval '1 second', 'consumable') \
         RETURNING id",
    )
    .bind(account_id.get())
    .bind(i64::from(consumable_definition().type_id.get()))
    .fetch_one(&pool)
    .await
    .expect("expired row");
    assert_eq!(
        repository
            .consume_one(ConsumeItem {
                account_id,
                operation_id: EconomyOperationId::new(Uuid::new_v4()),
                catalog: economy_catalog(),
                item: EconomyItemSelector {
                    inventory_id: pangya_domain::InventoryItemId::new(expired_id)
                        .expect("expired id"),
                    definition: consumable_definition(),
                },
            })
            .await,
        Err(EconomyError::Expired)
    );

    let depleted_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items \
         (account_id, item_type_id, starter_key, quantity, durability, inventory_class) \
         VALUES ($1, $2, 'test.depleted', 1, 0, 'club_set') RETURNING id",
    )
    .bind(account_id.get())
    .bind(i64::from(durable_club_definition().type_id.get()))
    .fetch_one(&pool)
    .await
    .expect("depleted row");
    assert_eq!(
        repository
            .equip(EquipmentChange {
                account_id,
                operation_id: EconomyOperationId::new(Uuid::new_v4()),
                catalog: economy_catalog(),
                expected_version: 0,
                character_id: aggregate.character.id,
                character_type_id: aggregate.character.item_type_id,
                club: Some(EconomyItemSelector {
                    inventory_id: pangya_domain::InventoryItemId::new(depleted_id)
                        .expect("depleted id"),
                    definition: durable_club_definition(),
                }),
                ball: None,
            })
            .await,
        Err(EconomyError::Depleted)
    );
    let wrong_family = EconomyItemSelector {
        inventory_id: pangya_domain::InventoryItemId::new(depleted_id).expect("id"),
        definition: consumable_definition(),
    };
    assert_eq!(
        repository
            .repair(RepairItem {
                account_id,
                operation_id: EconomyOperationId::new(Uuid::new_v4()),
                catalog: economy_catalog(),
                item: wrong_family,
            })
            .await,
        Err(EconomyError::Incompatible)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn distinct_concurrent_purchases_serialize_without_lost_balance_updates(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("EconomyDistinct", Some("EconDistinct")))
        .await
        .expect("account");
    sqlx::query("UPDATE profiles SET pang = 1000 WHERE account_id = $1")
        .bind(aggregate.account.id.get())
        .execute(&pool)
        .await
        .expect("fund");
    let mut tasks = Vec::new();
    for offset in 0..8_u32 {
        let repository = repository.clone();
        let mut definition = durable_club_definition();
        definition.type_id = ItemTypeId::new(0x1000_2000 + offset);
        definition.sale = ItemSale::Pang(25);
        let request = PurchaseRequest {
            account_id: aggregate.account.id,
            operation_id: EconomyOperationId::new(Uuid::new_v4()),
            catalog: economy_catalog(),
            definition,
            quantity: 1,
        };
        tasks.push(tokio::spawn(
            async move { repository.purchase(request).await },
        ));
    }
    for task in tasks {
        assert!(matches!(
            task.await.expect("join").expect("purchase"),
            EconomyCommit::Committed(_)
        ));
    }
    let state: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT pang, \
                (SELECT count(*) FROM economy_operations WHERE account_id = profiles.account_id), \
                (SELECT count(*) FROM shop_currency_ledger WHERE account_id = profiles.account_id), \
                (SELECT count(*) FROM item_ledger WHERE account_id = profiles.account_id) \
         FROM profiles WHERE account_id = $1",
    )
    .bind(aggregate.account.id.get())
    .fetch_one(&pool)
    .await
    .expect("state");
    assert_eq!(state, (800, 8, 8, 8));
}

/// Records every fault a repository reports, so a test can assert the observed stream.
#[derive(Debug, Default)]
struct RecordingStorageObserver {
    faults: std::sync::Mutex<Vec<StorageFault>>,
}

impl RecordingStorageObserver {
    fn taken(&self) -> Vec<StorageFault> {
        self.faults.lock().expect("fault lock").clone()
    }
}

impl StorageObserver for RecordingStorageObserver {
    fn storage_fault(&self, fault: StorageFault) {
        self.faults.lock().expect("fault lock").push(fault);
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn real_sqlstates_classify_and_reach_the_observer_without_changing_outcomes(pool: PgPool) {
    // Each case raises a genuine SQLSTATE through PostgreSQL and the driver, so this pins
    // the whole chain: server error -> sqlx -> classifier -> error variant -> observer.
    for (errcode, expected) in [
        ("40P01", StorageFault::Deadlock),
        ("40001", StorageFault::Serialization),
        ("53300", StorageFault::InsufficientResources),
        ("57014", StorageFault::OperatorIntervention),
        ("08006", StorageFault::Connection),
        ("XX001", StorageFault::InternalError),
    ] {
        let observer = Arc::new(RecordingStorageObserver::default());
        let repository = PgRepository::with_observer(pool.clone(), Arc::clone(&observer) as Arc<_>);
        sqlx::query(&format!(
            "CREATE OR REPLACE FUNCTION test_sqlstate() RETURNS trigger LANGUAGE plpgsql AS $$              BEGIN RAISE EXCEPTION 'injected' USING ERRCODE = '{errcode}'; END $$"
        ))
        .execute(&pool)
        .await
        .expect("sqlstate function");
        sqlx::query(
            "CREATE TRIGGER test_sqlstate BEFORE INSERT ON accounts              FOR EACH ROW EXECUTE FUNCTION test_sqlstate()",
        )
        .execute(&pool)
        .await
        .expect("sqlstate trigger");

        let outcome = repository
            .create_operator_account(account(
                &format!("S{errcode}"),
                Some(&format!("N{errcode}")),
            ))
            .await;
        assert_eq!(
            outcome,
            Err(RepositoryError::Storage(expected)),
            "SQLSTATE {errcode} classifies as {expected}"
        );
        assert_eq!(
            observer.taken(),
            vec![expected],
            "SQLSTATE {errcode} is observed exactly once"
        );
        assert_no_aggregate_rows(&pool).await;

        sqlx::query("DROP TRIGGER test_sqlstate ON accounts")
            .execute(&pool)
            .await
            .expect("drop trigger");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn successful_operations_report_no_fault_and_a_missing_observer_changes_nothing(
    pool: PgPool,
) {
    let observer = Arc::new(RecordingStorageObserver::default());
    let observed = PgRepository::with_observer(pool.clone(), Arc::clone(&observer) as Arc<_>);
    let created = observed
        .create_operator_account(account("QuietPath", Some("QuietNick")))
        .await
        .expect("account creates");
    assert!(
        observer.taken().is_empty(),
        "a successful path reports no fault"
    );

    // A non-storage failure is not a fault: it must stay off the storage dimension.
    let duplicate = observed
        .create_operator_account(account("QuietPath", Some("OtherNick")))
        .await;
    assert_eq!(duplicate, Err(RepositoryError::DuplicateUsername));
    assert!(
        observer.taken().is_empty(),
        "a typed domain rejection is not a storage fault"
    );

    // The observer is purely a side channel: the unobserved repository agrees exactly.
    let plain = PgRepository::new(pool.clone());
    assert_eq!(
        plain
            .create_operator_account(account("QuietPath", Some("ThirdNick")))
            .await,
        Err(RepositoryError::DuplicateUsername)
    );
    assert_eq!(
        plain
            .load_player_snapshot(created.account.id)
            .await
            .expect("snapshot loads")
            .account
            .id,
        created.account.id
    );
}

/// Operator balance grants are how an account gets funded for shop testing, so they must be
/// additive, must refuse an unknown account rather than silently creating one, and must refuse
/// rather than wrap when a credit would exceed the representable ceiling.
#[sqlx::test(migrator = "MIGRATOR")]
async fn operator_balance_grants_accumulate_and_refuse_overflow(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let created = repository
        .create_operator_account(account("BalanceUser", Some("BalanceNick")))
        .await
        .expect("account created");
    let id = created.account.id;

    let first = repository
        .grant_balance(
            id,
            BalanceGrant {
                pang: 10_000,
                points: 25,
            },
        )
        .await
        .expect("first grant");
    assert_eq!(first.pang, 10_000);
    assert_eq!(first.points, 25);

    // A second grant adds to the first rather than replacing it.
    let second = repository
        .grant_balance(
            id,
            BalanceGrant {
                pang: 5_000,
                points: 0,
            },
        )
        .await
        .expect("second grant");
    assert_eq!(second.pang, 15_000);
    assert_eq!(second.points, 25);

    assert_eq!(
        repository
            .grant_balance(
                id,
                BalanceGrant {
                    pang: u64::MAX,
                    points: 0,
                },
            )
            .await,
        Err(RepositoryError::BalanceOverflow),
        "a credit past the ceiling must refuse, never wrap"
    );
    // The refused grant left the balance untouched.
    let after = repository
        .grant_balance(id, BalanceGrant { pang: 0, points: 1 })
        .await
        .expect("grant after refusal");
    assert_eq!(after.pang, 15_000);
    assert_eq!(after.points, 26);

    assert_eq!(
        repository
            .grant_balance(
                AccountId::new(9_999_999).expect("id"),
                BalanceGrant { pang: 1, points: 0 },
            )
            .await,
        Err(RepositoryError::NotFound)
    );
}
