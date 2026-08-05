//! Real PostgreSQL acceptance tests for the M2 storage foundation.

use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use pangya_domain::{
    AccountRepository, AccountStatus, ConsumeHandover, CredentialHash, HandoverDigest,
    HandoverError, HandoverRepository, ItemTypeId, MAX_STARTER_ITEMS, NewAccount, Nickname,
    NormalizedUsername, PlayerRepository, RepositoryError, ServiceKind, SourceAddressPrefix,
    StarterCharacter, StarterGrant, StarterItem, StarterKey, Username,
};
use pangya_login::{generate_handover, parse_handover};
use pangya_storage::{MIGRATOR, PgRepository, migrate};
use sqlx::PgPool;

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
        Err(RepositoryError::Storage)
    );
    assert_no_aggregate_rows(&pool).await;
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_audit_events")
        .fetch_one(&pool)
        .await
        .expect("audit count");
    assert_eq!(audits, 0);
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
        assert_eq!(result, Err(RepositoryError::Storage), "stage {stage}");
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
        Err(RepositoryError::Storage)
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
