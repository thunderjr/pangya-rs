#![allow(missing_docs)]

use pangya_message::{ChannelInfo, MessageStore, PostgresStore, Presence};
use pangya_storage::MIGRATOR;
use sqlx::{PgPool, Row};

#[sqlx::test(migrator = "MIGRATOR")]
async fn postgres_social_state_survives_store_restart_and_claims_once(pool: PgPool) {
    for (id, username, nickname) in [(1_i64, "alice", "Alice"), (2, "bob", "Bob")] {
        sqlx::query(
            "INSERT INTO accounts(id, username_normalized, username_display) VALUES($1,$2,$2)",
        )
        .bind(id)
        .bind(username)
        .execute(&pool)
        .await
        .expect("account");
        sqlx::query("INSERT INTO profiles(account_id,nickname_display,nickname_normalized,setup_state) VALUES($1,$2,lower($2),'complete')")
            .bind(id)
            .bind(nickname)
            .execute(&pool)
            .await
            .expect("profile");
    }
    let store = PostgresStore::new(pool.clone());
    assert!(store.authenticate(1, b"Alice").await.expect("auth"));
    store.add_friend(1, 2).await.expect("request");
    store.confirm_friend(1, 2).await.expect("confirm");
    store
        .set_online(2, Presence::Online, ChannelInfo::offline())
        .await
        .expect("online");
    store
        .queue_message(1, 2, b"durable".to_vec())
        .await
        .expect("queue");

    // A fresh store instance models a process restart while retaining the pool/database.
    let restarted = PostgresStore::new(pool.clone());
    let messages = restarted.take_live_messages(2).await.expect("claim");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, b"durable");
    assert!(
        restarted
            .take_messages(2)
            .await
            .expect("second claim")
            .is_empty()
    );
    let row = sqlx::query("SELECT count(*) AS count FROM message_offline_messages")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(row.get::<i64, _>("count"), 0);
}
