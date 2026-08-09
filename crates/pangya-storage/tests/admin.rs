//! Real PostgreSQL acceptance tests for the operator admin surface.
//!
//! The properties under test are the ones that decide whether a stolen or stale session can
//! still act: authorisation is re-read per request, losing the role revokes sessions in the
//! same transaction, and audit rows cannot be edited away afterwards.

use std::time::{Duration, SystemTime};

use pangya_domain::{
    AccountId, AccountRepository, AccountRole, AccountStatus, AdminAccountQuery, AdminPage,
    AdminRepository, AdminSessionId, CredentialHash, HandoverDigest, NewAccount,
    NewAdminAuditEvent, NewAdminSession, Nickname, NormalizedUsername, RepositoryError,
    ResolveAdminSession, SourceAddressPrefix, StarterCharacter, StarterGrant, StarterKey, Username,
};
use pangya_storage::{MIGRATOR, PgRepository};
use sqlx::PgPool;
use uuid::Uuid;

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2E$\
                   Ym9ndXNvdXRwdXRib2d1c291dHB1dGJvZ3Vzb3V0cHV0YmE";

fn source() -> SourceAddressPrefix {
    SourceAddressPrefix::from_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
}

fn starter() -> StarterGrant {
    StarterGrant {
        character: StarterCharacter {
            key: StarterKey::parse("starter_character").expect("starter key"),
            item_type_id: pangya_domain::ItemTypeId::new(67_108_864),
        },
        items: Vec::new(),
        equipped_club_key: None,
        equipped_ball_key: None,
    }
}

fn account(username: &str) -> NewAccount {
    NewAccount {
        username: Username::parse(username).expect("username"),
        credential_hash: CredentialHash::new(PHC.to_owned()),
        nickname: Some(Nickname::parse(&format!("nick{username}")).expect("nickname")),
        starter: starter(),
    }
}

fn digest(seed: u8) -> HandoverDigest {
    HandoverDigest::new([seed; 32])
}

fn session(id: Uuid, account_id: AccountId, seed: u8, lifetime: Duration) -> NewAdminSession {
    let issued_at = SystemTime::now();
    NewAdminSession {
        id: AdminSessionId::new(id),
        account_id,
        digest: digest(seed),
        source_address_prefix: source(),
        issued_at,
        expires_at: issued_at + lifetime,
    }
}

async fn admin(repository: &PgRepository, username: &str) -> AccountId {
    let aggregate = repository
        .create_account(account(username))
        .await
        .expect("aggregate created");
    repository
        .set_account_role(aggregate.account.id, AccountRole::Admin, SystemTime::now())
        .await
        .expect("role granted");
    aggregate.account.id
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn accounts_default_to_the_player_role_and_cannot_sign_in(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository
        .create_account(account("plainplayer"))
        .await
        .expect("aggregate created");
    let record = repository
        .load_admin_authentication(&NormalizedUsername::parse("plainplayer").expect("username"))
        .await
        .expect("authentication query")
        .expect("record");
    assert_eq!(record.role, AccountRole::Player);
    assert_eq!(record.status, AccountStatus::Active);

    // The role gate is enforced in SQL, not only in the handler, so a caller that forgets to
    // check still cannot mint a session for a non-admin.
    let refused = repository
        .issue_admin_session(session(
            Uuid::new_v4(),
            aggregate.account.id,
            1,
            Duration::from_secs(600),
        ))
        .await;
    assert_eq!(refused, Err(RepositoryError::AccountInactive));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_unknown_username_is_absent_rather_than_an_error(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let record = repository
        .load_admin_authentication(&NormalizedUsername::parse("nobodyhere").expect("username"))
        .await
        .expect("authentication query");
    assert!(record.is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_valid_bearer_resolves_and_a_wrong_digest_does_not(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let account_id = admin(&repository, "rootone").await;
    let id = Uuid::new_v4();
    repository
        .issue_admin_session(session(id, account_id, 7, Duration::from_secs(600)))
        .await
        .expect("session issued");

    let resolved = repository
        .resolve_admin_session(ResolveAdminSession {
            id: AdminSessionId::new(id),
            digest: digest(7),
            now: SystemTime::now(),
        })
        .await
        .expect("resolve query")
        .expect("valid session");
    assert_eq!(resolved.account_id, account_id);
    assert_eq!(resolved.role, AccountRole::Admin);
    assert_eq!(resolved.username_display, "rootone");

    let wrong = repository
        .resolve_admin_session(ResolveAdminSession {
            id: AdminSessionId::new(id),
            digest: digest(8),
            now: SystemTime::now(),
        })
        .await
        .expect("resolve query");
    assert!(wrong.is_none(), "a wrong digest must not resolve");

    let unknown = repository
        .resolve_admin_session(ResolveAdminSession {
            id: AdminSessionId::new(Uuid::new_v4()),
            digest: digest(7),
            now: SystemTime::now(),
        })
        .await
        .expect("resolve query");
    assert!(unknown.is_none(), "an unknown selector must not resolve");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_expired_session_stops_resolving_without_being_revoked(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let account_id = admin(&repository, "roottwo").await;
    let id = Uuid::new_v4();
    let issued = session(id, account_id, 3, Duration::from_secs(1));
    let expires_at = issued.expires_at;
    repository
        .issue_admin_session(issued)
        .await
        .expect("session issued");

    let resolved = repository
        .resolve_admin_session(ResolveAdminSession {
            id: AdminSessionId::new(id),
            digest: digest(3),
            // One second past expiry, supplied rather than slept for, so the test is fast and
            // deterministic.
            now: expires_at + Duration::from_secs(1),
        })
        .await
        .expect("resolve query");
    assert!(resolved.is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn revoking_a_session_takes_effect_immediately_and_is_idempotent(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let account_id = admin(&repository, "rootthree").await;
    let id = Uuid::new_v4();
    repository
        .issue_admin_session(session(id, account_id, 5, Duration::from_secs(600)))
        .await
        .expect("session issued");
    repository
        .revoke_admin_session(AdminSessionId::new(id), SystemTime::now())
        .await
        .expect("revoked");
    // A second revoke must not fail: a browser can send logout twice.
    repository
        .revoke_admin_session(AdminSessionId::new(id), SystemTime::now())
        .await
        .expect("revoked again");

    let resolved = repository
        .resolve_admin_session(ResolveAdminSession {
            id: AdminSessionId::new(id),
            digest: digest(5),
            now: SystemTime::now(),
        })
        .await
        .expect("resolve query");
    assert!(resolved.is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn demotion_revokes_outstanding_sessions_in_the_same_transaction(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let account_id = admin(&repository, "rootfour").await;
    let id = Uuid::new_v4();
    repository
        .issue_admin_session(session(id, account_id, 9, Duration::from_secs(600)))
        .await
        .expect("session issued");

    repository
        .set_account_role(account_id, AccountRole::Player, SystemTime::now())
        .await
        .expect("demoted");

    let resolved = repository
        .resolve_admin_session(ResolveAdminSession {
            id: AdminSessionId::new(id),
            digest: digest(9),
            now: SystemTime::now(),
        })
        .await
        .expect("resolve query");
    assert!(
        resolved.is_none(),
        "a demoted operator's session must stop working at once, not at expiry"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn banning_an_admin_stops_its_live_session_resolving(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let account_id = admin(&repository, "rootfive").await;
    let id = Uuid::new_v4();
    repository
        .issue_admin_session(session(id, account_id, 11, Duration::from_secs(600)))
        .await
        .expect("session issued");

    repository
        .set_status(account_id, AccountStatus::Banned, SystemTime::now())
        .await
        .expect("banned");

    // `set_status` does not touch admin_sessions; the per-request status re-read is what
    // closes this hole, which is precisely why authorisation is not frozen at sign-in.
    let resolved = repository
        .resolve_admin_session(ResolveAdminSession {
            id: AdminSessionId::new(id),
            digest: digest(11),
            now: SystemTime::now(),
        })
        .await
        .expect("resolve query");
    assert!(resolved.is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn setting_a_role_on_a_missing_account_is_not_found(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let missing = AccountId::new(987_654).expect("positive id");
    assert_eq!(
        repository
            .set_account_role(missing, AccountRole::Admin, SystemTime::now())
            .await,
        Err(RepositoryError::NotFound)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn audit_rows_are_appended_newest_first_and_are_immutable(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let account_id = admin(&repository, "rootsix").await;
    for action in ["admin.session.open", "account.balance.grant"] {
        repository
            .record_admin_audit(NewAdminAuditEvent {
                actor_account_id: account_id,
                action: action.to_owned(),
                target_account_id: Some(account_id),
                detail: r#"{"pang":100}"#.to_owned(),
            })
            .await
            .expect("audit appended");
    }

    let rows = repository
        .list_admin_audit(AdminPage::default())
        .await
        .expect("audit listed");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].action, "account.balance.grant");
    assert_eq!(rows[0].actor_username, "rootsix");
    assert_eq!(rows[0].target_account_id, Some(account_id));
    assert!(rows[0].detail.contains("100"));

    for statement in [
        "UPDATE admin_audit_events SET action = 'tampered'",
        "DELETE FROM admin_audit_events",
    ] {
        let refused = sqlx::query(statement).execute(&pool).await;
        assert!(
            refused.is_err(),
            "{statement} must be refused by the append-only trigger"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_non_object_audit_detail_is_refused(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let account_id = admin(&repository, "rootseven").await;
    // The column CHECK is the last line of defence behind the handler's own serialisation.
    let refused = repository
        .record_admin_audit(NewAdminAuditEvent {
            actor_account_id: account_id,
            action: "account.balance.grant".to_owned(),
            target_account_id: None,
            detail: "[1,2,3]".to_owned(),
        })
        .await;
    assert!(refused.is_err());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn audit_paging_is_bounded_regardless_of_the_requested_limit(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let account_id = admin(&repository, "rooteight").await;
    for index in 0..5 {
        repository
            .record_admin_audit(NewAdminAuditEvent {
                actor_account_id: account_id,
                action: format!("test.event.{index}"),
                target_account_id: None,
                detail: "{}".to_owned(),
            })
            .await
            .expect("audit appended");
    }

    // A caller asking for a million rows is clamped, not obeyed.
    let clamped = AdminPage::new(1_000_000, 0).expect("page");
    assert_eq!(clamped.limit(), AdminPage::MAX_LIMIT);
    let rows = repository
        .list_admin_audit(clamped)
        .await
        .expect("audit listed");
    assert_eq!(rows.len(), 5);

    assert!(AdminPage::new(50, -1).is_err());
    assert!(AdminPage::new(50, AdminPage::MAX_OFFSET + 1).is_err());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn revoking_every_session_for_an_account_leaves_other_accounts_alone(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let first = admin(&repository, "rootnine").await;
    let second = admin(&repository, "rootten").await;
    let (first_session, second_session) = (Uuid::new_v4(), Uuid::new_v4());
    repository
        .issue_admin_session(session(first_session, first, 21, Duration::from_secs(600)))
        .await
        .expect("session issued");
    repository
        .issue_admin_session(session(
            second_session,
            second,
            22,
            Duration::from_secs(600),
        ))
        .await
        .expect("session issued");

    repository
        .revoke_admin_sessions_for_account(first, SystemTime::now())
        .await
        .expect("revoked");

    assert!(
        repository
            .resolve_admin_session(ResolveAdminSession {
                id: AdminSessionId::new(first_session),
                digest: digest(21),
                now: SystemTime::now(),
            })
            .await
            .expect("resolve query")
            .is_none()
    );
    assert!(
        repository
            .resolve_admin_session(ResolveAdminSession {
                id: AdminSessionId::new(second_session),
                digest: digest(22),
                now: SystemTime::now(),
            })
            .await
            .expect("resolve query")
            .is_some()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_account_list_filters_searches_and_orders(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let rich = repository
        .create_account(account("wealthyone"))
        .await
        .expect("aggregate created")
        .account
        .id;
    let poor = repository
        .create_account(account("humbletwo"))
        .await
        .expect("aggregate created")
        .account
        .id;
    repository
        .grant_balance(
            rich,
            pangya_domain::BalanceGrant {
                pang: 5_000,
                points: 10,
            },
        )
        .await
        .expect("granted");
    repository
        .set_status(poor, AccountStatus::Banned, SystemTime::now())
        .await
        .expect("banned");

    let all = repository
        .list_accounts(AdminAccountQuery::default())
        .await
        .expect("listed");
    assert_eq!(all.len(), 2);

    // Search matches the normalized username, and the nickname column too.
    let searched = repository
        .list_accounts(AdminAccountQuery {
            search: Some("%wealthy%".to_owned()),
            ..Default::default()
        })
        .await
        .expect("listed");
    assert_eq!(searched.len(), 1);
    assert_eq!(searched[0].id, rich);
    assert_eq!(searched[0].pang, 5_000);
    assert_eq!(searched[0].points, 10);
    // The starter grant creates one character; the count must reflect it without loading it.
    assert_eq!(searched[0].character_count, 1);

    let banned = repository
        .list_accounts(AdminAccountQuery {
            status: Some(AccountStatus::Banned),
            ..Default::default()
        })
        .await
        .expect("listed");
    assert_eq!(banned.len(), 1);
    assert_eq!(banned[0].id, poor);

    let by_pang = repository
        .list_accounts(AdminAccountQuery {
            sort: pangya_domain::AdminAccountSort::PangDesc,
            ..Default::default()
        })
        .await
        .expect("listed");
    assert_eq!(by_pang[0].id, rich);

    let by_name = repository
        .list_accounts(AdminAccountQuery {
            sort: pangya_domain::AdminAccountSort::UsernameAsc,
            ..Default::default()
        })
        .await
        .expect("listed");
    assert_eq!(by_name[0].id, poor, "humbletwo sorts before wealthyone");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn account_detail_carries_the_aggregate_and_a_missing_account_is_not_found(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let account_id = repository
        .create_account(account("detailone"))
        .await
        .expect("aggregate created")
        .account
        .id;

    let detail = repository
        .load_account_detail(account_id)
        .await
        .expect("detail loaded");
    assert_eq!(detail.summary.id, account_id);
    assert_eq!(detail.characters.len(), 1);
    let equipment = detail
        .equipment
        .expect("starter grant creates an equipment set");
    assert_eq!(equipment.character_id, detail.characters[0].id);
    assert_eq!(detail.selected_character_id, Some(detail.characters[0].id));

    assert_eq!(
        repository
            .load_account_detail(AccountId::new(999_999).expect("positive id"))
            .await,
        Err(RepositoryError::NotFound)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn setting_a_balance_can_lower_it_where_granting_cannot(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let account_id = repository
        .create_account(account("balanceone"))
        .await
        .expect("aggregate created")
        .account
        .id;
    repository
        .grant_balance(
            account_id,
            pangya_domain::BalanceGrant {
                pang: 1_000,
                points: 50,
            },
        )
        .await
        .expect("granted");

    // The whole reason `set_balances` exists: a credit can only go up, so correcting a
    // balance downward is not expressible as a grant.
    let after = repository
        .set_balances(
            account_id,
            pangya_domain::BalanceAssignment {
                pang: Some(7),
                points: None,
            },
        )
        .await
        .expect("assigned");
    assert_eq!(after.pang, 7);
    assert_eq!(after.points, 50, "an omitted field must be left alone");

    let both = repository
        .set_balances(
            account_id,
            pangya_domain::BalanceAssignment {
                pang: Some(0),
                points: Some(0),
            },
        )
        .await
        .expect("assigned");
    assert_eq!((both.pang, both.points), (0, 0));

    // An empty assignment is a caller defect, not a silent no-op.
    assert!(
        repository
            .set_balances(
                account_id,
                pangya_domain::BalanceAssignment {
                    pang: None,
                    points: None
                }
            )
            .await
            .is_err()
    );
    assert_eq!(
        repository
            .set_balances(
                AccountId::new(999_999).expect("positive id"),
                pangya_domain::BalanceAssignment {
                    pang: Some(1),
                    points: None
                }
            )
            .await,
        Err(RepositoryError::NotFound)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn replacing_a_credential_changes_what_authentication_returns(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let account_id = repository
        .create_account(account("credone"))
        .await
        .expect("aggregate created")
        .account
        .id;
    let replacement = CredentialHash::new(
        "$argon2id$v=19$m=19456,t=2,p=1$bmV3c2FsdG5ld3NhbHRuZXc$\
         bmV3aGFzaG5ld2hhc2huZXdoYXNobmV3aGFzaG5ld2hh"
            .to_owned(),
    );
    repository
        .set_credential(account_id, replacement.clone())
        .await
        .expect("credential replaced");

    let record = repository
        .load_admin_authentication(&NormalizedUsername::parse("credone").expect("username"))
        .await
        .expect("authentication query")
        .expect("record");
    assert_eq!(
        record.credential_hash.expose_phc(),
        replacement.expose_phc()
    );

    assert_eq!(
        repository
            .set_credential(AccountId::new(999_999).expect("positive id"), replacement)
            .await,
        Err(RepositoryError::NotFound)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_ledger_and_match_listings_are_empty_rather_than_erroring(pool: PgPool) {
    let repository = PgRepository::new(pool);
    let account_id = repository
        .create_account(account("quietone"))
        .await
        .expect("aggregate created")
        .account
        .id;
    // A brand-new account has touched none of the four ledgers. The merged query must return
    // nothing rather than failing on an empty UNION.
    assert!(
        repository
            .list_account_ledger(account_id, AdminPage::default())
            .await
            .expect("ledger listed")
            .is_empty()
    );
    assert!(
        repository
            .list_account_matches(account_id, AdminPage::default())
            .await
            .expect("matches listed")
            .is_empty()
    );
}

fn definition(type_id: u32, sale: pangya_domain::ItemSale) -> pangya_domain::ItemDefinition {
    pangya_domain::ItemDefinition {
        type_id: pangya_domain::ItemTypeId::new(type_id),
        kind: pangya_domain::ItemKind::ClubSet,
        sale,
        stacking: pangya_domain::ItemStacking::Unique,
        durability: pangya_domain::ItemDurability::Nondurable,
        compatibility: pangya_domain::ItemCompatibility::Any,
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_shop_overlay_round_trips_and_bumps_its_revision(pool: PgPool) {
    use pangya_domain::{ItemSale, ItemTypeId, ShopOverride};

    let repository = PgRepository::new(pool);
    let actor = admin(&repository, "shopadmin").await;

    let empty = repository.load_shop_overlay().await.expect("loaded");
    assert!(empty.is_empty());
    let base_revision = empty.revision();

    // Enabling an item the client does not sell is the capability that makes this table
    // worth existing: `price_override_pang` explicitly cannot do it.
    let revision = repository
        .set_shop_override(
            actor,
            ShopOverride {
                item_type_id: ItemTypeId::new(0x1000_0061),
                enabled: Some(true),
                pang: Some(777),
            },
            Some("enabled for testing".to_owned()),
        )
        .await
        .expect("override set");
    assert!(revision > base_revision, "a write must bump the revision");

    let overlay = repository.load_shop_overlay().await.expect("loaded");
    assert_eq!(overlay.len(), 1);
    assert_eq!(overlay.revision(), revision);
    let resolved = overlay
        .resolve(definition(0x1000_0061, ItemSale::NotSold))
        .expect("an unsold item becomes sellable");
    assert_eq!(resolved.sale, ItemSale::Pang(777));

    // Disabling wins over the client's own price.
    repository
        .set_shop_override(
            actor,
            ShopOverride {
                item_type_id: ItemTypeId::new(0x1000_0061),
                enabled: Some(false),
                pang: None,
            },
            None,
        )
        .await
        .expect("override replaced");
    let overlay = repository.load_shop_overlay().await.expect("loaded");
    assert_eq!(overlay.len(), 1, "the same type replaces rather than adds");
    assert!(
        overlay
            .resolve(definition(0x1000_0061, ItemSale::Pang(6000)))
            .is_none()
    );

    // An override that inherits both fields says nothing, and is refused rather than stored.
    assert!(
        repository
            .set_shop_override(
                actor,
                ShopOverride {
                    item_type_id: ItemTypeId::new(0x1000_0062),
                    enabled: None,
                    pang: None,
                },
                None,
            )
            .await
            .is_err()
    );

    let cleared = repository
        .clear_shop_override(ItemTypeId::new(0x1000_0061))
        .await
        .expect("cleared");
    assert!(cleared > revision);
    assert!(
        repository
            .load_shop_overlay()
            .await
            .expect("loaded")
            .is_empty()
    );
    // Clearing an absent override is not an error: a panel can retry a delete safely.
    repository
        .clear_shop_override(ItemTypeId::new(0x1000_0061))
        .await
        .expect("clearing an absent override is a no-op");
}

#[test]
fn an_empty_overlay_resolves_to_exactly_the_catalog_answer() {
    use pangya_domain::{ItemSale, ShopOverlay};

    let overlay = ShopOverlay::default();
    assert_eq!(
        overlay
            .resolve(definition(1, ItemSale::Pang(500)))
            .map(|resolved| resolved.sale),
        Some(ItemSale::Pang(500))
    );
    assert!(overlay.resolve(definition(1, ItemSale::NotSold)).is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn granting_editing_and_deleting_inventory_respects_every_schema_shape(pool: PgPool) {
    use pangya_domain::{
        AdminItemGrant, AdminItemUpdate, AdminMutationError, InventoryClass, ItemTypeId,
    };

    let repository = PgRepository::new(pool);
    let account_id = repository
        .create_account(account("itemowner"))
        .await
        .expect("aggregate created")
        .account
        .id;

    let club = repository
        .grant_item(AdminItemGrant {
            account_id,
            item_type_id: ItemTypeId::new(0x1000_0061),
            class: InventoryClass::ClubSet,
            quantity: 1,
            durability: None,
        })
        .await
        .expect("club granted");
    assert_eq!(club.quantity, 1);
    // The acquisition key must distinguish an operator grant from a starter or a purchase,
    // permanently.
    assert!(club.starter_key.as_str().starts_with("admin."));

    // `ck_inventory_m7_shape` forbids a stacked club set; the refusal must be typed rather
    // than a constraint violation collapsed into an opaque storage fault.
    assert_eq!(
        repository
            .grant_item(AdminItemGrant {
                account_id,
                item_type_id: ItemTypeId::new(0x1000_0062),
                class: InventoryClass::ClubSet,
                quantity: 5,
                durability: None,
            })
            .await,
        Err(AdminMutationError::InvalidShape)
    );

    // Consumables stack onto the existing row rather than tripping the partial unique index.
    let first = repository
        .grant_item(AdminItemGrant {
            account_id,
            item_type_id: ItemTypeId::new(0x1800_0000),
            class: InventoryClass::Consumable,
            quantity: 3,
            durability: None,
        })
        .await
        .expect("consumable granted");
    let second = repository
        .grant_item(AdminItemGrant {
            account_id,
            item_type_id: ItemTypeId::new(0x1800_0000),
            class: InventoryClass::Consumable,
            quantity: 4,
            durability: None,
        })
        .await
        .expect("consumable stacked");
    assert_eq!(second.id, first.id, "the same row is topped up");
    assert_eq!(second.quantity, 7);

    // Quantity zero is `delete`, not an update.
    assert_eq!(
        repository
            .update_item(AdminItemUpdate {
                account_id,
                inventory_id: second.id,
                quantity: Some(0),
                durability: None,
            })
            .await,
        Err(AdminMutationError::InvalidShape)
    );

    // Ownership is checked rather than taken from the caller's URL.
    let other = repository
        .create_account(account("otherowner"))
        .await
        .expect("aggregate created")
        .account
        .id;
    assert_eq!(
        repository.delete_item(other, club.id).await,
        Err(AdminMutationError::NotOwned)
    );

    repository
        .delete_item(account_id, club.id)
        .await
        .expect("deleted");
    assert_eq!(
        repository.delete_item(account_id, club.id).await,
        Err(AdminMutationError::NotFound)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn equipment_writes_are_version_checked_and_bump_the_counter(pool: PgPool) {
    use pangya_domain::{
        AdminEquipmentUpdate, AdminItemGrant, AdminMutationError, InventoryClass, ItemTypeId,
    };

    let repository = PgRepository::new(pool);
    let aggregate = repository
        .create_account(account("equipowner"))
        .await
        .expect("aggregate created");
    let account_id = aggregate.account.id;
    let character_id = aggregate.character.id;

    let ball = repository
        .grant_item(AdminItemGrant {
            account_id,
            item_type_id: ItemTypeId::new(0x1400_00c9),
            class: InventoryClass::Ball,
            quantity: 1,
            durability: None,
        })
        .await
        .expect("ball granted");

    let before = aggregate.equipment.version;
    let updated = repository
        .set_equipment(AdminEquipmentUpdate {
            account_id,
            character_id,
            club_item_id: None,
            ball_item_id: Some(ball.id),
            expected_version: before,
        })
        .await
        .expect("equipment set");
    assert_eq!(updated.ball_item_id, Some(ball.id));
    // The bump is what keeps the player's next in-game equip from being rejected by a version
    // they never saw.
    assert_eq!(updated.version, before + 1);

    // Replaying the same write with the now-stale version is refused, not merged.
    assert_eq!(
        repository
            .set_equipment(AdminEquipmentUpdate {
                account_id,
                character_id,
                club_item_id: None,
                ball_item_id: None,
                expected_version: before,
            })
            .await,
        Err(AdminMutationError::VersionConflict)
    );

    // An equipped item cannot be deleted out from under the equipment set.
    assert_eq!(
        repository.delete_item(account_id, ball.id).await,
        Err(AdminMutationError::Equipped)
    );
}
