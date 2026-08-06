//! Real PostgreSQL acceptance tests for the M2 storage foundation.

use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use pangya_domain::{
    AbortMatch, AbortMatchOutcome, AccountRepository, AccountStatus, BeginSoloMatch,
    BeginSoloMatchOutcome, CatalogFingerprint, CommitSoloHole, ConsumeHandover, CourseId,
    CredentialHash, HandoverDigest, HandoverError, HandoverRepository, IncompleteMatchAbortLimit,
    ItemTypeId, MAX_STARTER_ITEMS, MatchAbortReason, MatchId, MatchRepository,
    MatchRepositoryError, MatchResultKey, MatchSeed, NewAccount, Nickname, NormalizedUsername,
    OneHoleConfig, PlayerRepository, RepositoryError, ServiceKind, SourceAddressPrefix,
    StarterCharacter, StarterGrant, StarterItem, StarterKey, StrokeCount, Username, Weather,
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

fn solo_begin(account_id: pangya_domain::AccountId) -> BeginSoloMatch {
    BeginSoloMatch::new(
        MatchId::new(Uuid::new_v4()),
        MatchResultKey::new(Uuid::new_v4()),
        account_id,
        OneHoleConfig::new(CourseId::new(7).expect("course"), 3).expect("configuration"),
        CatalogFingerprint::new([0x42; 32]),
        MatchSeed::new([0x24; 32]),
        Weather::Clear,
        WindConditions::new(87, 231).expect("wind"),
    )
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
async fn disconnect_abort_is_idempotent_and_never_rewards(pool: PgPool) {
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
        MatchAbortReason::Disconnect,
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
        OneHoleConfig::new(CourseId::new(8).expect("course"), 3).expect("config"),
        StrokeCount::new(3).expect("strokes"),
    );
    assert_eq!(
        repository.commit_solo_hole(wrong_config).await,
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
    assert_eq!(state, ("loading".to_owned(), i64::MAX, 0));
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
        Err(MatchRepositoryError::Storage)
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
            Err(MatchRepositoryError::Storage),
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
            Err(MatchRepositoryError::Storage),
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
        ("match terminal", "matches", "UPDATE", ""),
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
            Err(MatchRepositoryError::Storage),
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
        assert_eq!(state, ("loading".to_owned(), None, 0, 0), "stage {stage}");
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
