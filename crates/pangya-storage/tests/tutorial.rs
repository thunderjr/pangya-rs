//! PostgreSQL tutorial progression and exactly-once reward acceptance tests.

use pangya_domain::{PlayerRepository, RepositoryError, TutorialKind, tutorial_completion_rewards};
use pangya_storage::{MIGRATOR, PgRepository};
use sqlx::PgPool;

fn account(username: &str) -> pangya_domain::NewAccount {
    pangya_domain::NewAccount {
        username: pangya_domain::Username::parse(username).expect("username"),
        credential_hash: pangya_domain::CredentialHash::new("tutorial-test".to_owned()),
        nickname: Some(pangya_domain::Nickname::parse(username).expect("nickname")),
        starter: pangya_domain::StarterGrant {
            character: pangya_domain::StarterCharacter {
                key: pangya_domain::StarterKey::parse("starter.character").expect("key"),
                item_type_id: pangya_domain::ItemTypeId::new(0x0400_0000),
            },
            items: Vec::new(),
            equipped_club_key: None,
            equipped_ball_key: None,
        },
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tutorial_progress_survives_restart_and_mission_retries(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository.create_account(account("TutorialRestart")).await.expect("account");
    let id = aggregate.account.id;

    assert_eq!(repository.load_tutorial_progress(id).await.expect("initial"), Default::default());
    for mission in [1, 2, 4, 8, 16, 32, 64] {
        let result = repository.apply_tutorial_mission(id, TutorialKind::Rookie, mission).await.expect("mission");
        assert!(result.newly_completed);
        assert_eq!(result.completed_option, None);
    }
    let complete = repository.apply_tutorial_mission(id, TutorialKind::Rookie, 128).await.expect("final mission");
    assert_eq!(complete.progress.rookie, 0xff);
    assert_eq!(complete.completed_option, Some(1));

    let restarted = PgRepository::new(pool.clone());
    assert_eq!(restarted.load_tutorial_progress(id).await.expect("restart"), complete.progress);
    let retry = restarted.apply_tutorial_mission(id, TutorialKind::Rookie, 128).await.expect("retry");
    assert!(!retry.newly_completed);
    assert_eq!(retry.completed_option, None);
    let mission_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM tutorial_mission_rewards WHERE account_id = $1")
        .bind(id.get()).fetch_one(&pool).await.expect("mission ledger");
    assert_eq!(mission_rows, 8);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tutorial_completion_claim_is_concurrent_and_exactly_once(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository.create_account(account("TutorialConcurrent")).await.expect("account");
    let id = aggregate.account.id;
    for mission in [1, 2, 4, 8, 16, 32, 64, 128] {
        repository.apply_tutorial_mission(id, TutorialKind::Rookie, mission).await.expect("mission");
    }
    let rewards = tutorial_completion_rewards(1).expect("rookie rewards");
    let (first, second) = tokio::join!(
        repository.claim_tutorial_completion(id, 1, rewards),
        repository.claim_tutorial_completion(id, 1, rewards),
    );
    let claimed = [first.expect("first claim"), second.expect("second claim")];
    assert_eq!(claimed.iter().filter(|value| **value).count(), 1);
    let reward_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inventory_items WHERE account_id = $1 AND starter_key LIKE 'tutorial.1.%'",
    ).bind(id.get()).fetch_one(&pool).await.expect("reward rows");
    assert_eq!(reward_rows, 2);
    let claim_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM tutorial_reward_claims WHERE account_id = $1")
        .bind(id.get()).fetch_one(&pool).await.expect("claim rows");
    assert_eq!(claim_rows, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tutorial_completion_cannot_be_claimed_before_progress(pool: PgPool) {
    let repository = PgRepository::new(pool.clone());
    let aggregate = repository.create_account(account("TutorialNoPremature")).await.expect("account");
    let rewards = tutorial_completion_rewards(1).expect("rookie rewards");
    assert_eq!(
        repository.claim_tutorial_completion(aggregate.account.id, 1, rewards).await,
        Err(RepositoryError::CorruptData)
    );
}
