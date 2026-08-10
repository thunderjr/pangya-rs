#![allow(missing_docs)]

use pangya_crypto::{client_encrypt, server_decrypt};
use pangya_message::{
    ChannelInfo, ClientPacket, MessageService, MessageStore, PostgresStore, Presence, ServerPacket,
};
use pangya_protocol::CodecLimits;
use pangya_storage::MIGRATOR;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

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
    assert!(
        !store
            .authenticate(1, b"Alice", "203.0.113.9".parse().expect("peer"))
            .await
            .expect("unissued auth")
    );
    sqlx::query("INSERT INTO message_login_eligibility(account_id,nickname,peer_ip,issued_at,expires_at) VALUES($1,$2,$3::inet,now(),now()+interval '60 seconds')")
        .bind(1_i64).bind("Alice").bind("203.0.113.9")
        .execute(&pool).await.expect("eligibility");
    assert!(
        store
            .authenticate(1, b"Alice", "203.0.113.9".parse().expect("peer"))
            .await
            .expect("auth")
    );
    assert!(
        !store
            .authenticate(1, b"Alice", "203.0.113.9".parse().expect("peer"))
            .await
            .expect("replay rejected")
    );
    sqlx::query("INSERT INTO message_login_eligibility(account_id,nickname,peer_ip,issued_at,expires_at) VALUES($1,$2,$3::inet,now(),now()+interval '60 seconds')")
        .bind(1_i64).bind("Alice").bind("203.0.113.9")
        .execute(&pool).await.expect("eligibility");
    assert!(
        !store
            .authenticate(1, b"Alice", "203.0.113.10".parse().expect("wrong peer"))
            .await
            .expect("wrong peer rejected")
    );
    assert!(
        !store
            .authenticate(1, b"Mallory", "203.0.113.9".parse().expect("peer"))
            .await
            .expect("wrong nickname rejected")
    );
    assert!(
        store
            .authenticate(1, b"Alice", "203.0.113.9".parse().expect("peer"))
            .await
            .expect("correct eligibility retained")
    );
    sqlx::query("INSERT INTO message_login_eligibility(account_id,nickname,peer_ip,issued_at,expires_at) VALUES($1,$2,$3::inet,now()-interval '2 minutes',now()-interval '1 minute')")
        .bind(1_i64).bind("Alice").bind("203.0.113.9")
        .execute(&pool).await.expect("expired eligibility");
    assert!(
        !store
            .authenticate(1, b"Alice", "203.0.113.9".parse().expect("peer"))
            .await
            .expect("expiry rejected")
    );
    sqlx::query("UPDATE accounts SET status='disabled' WHERE id=1")
        .execute(&pool)
        .await
        .expect("disable");
    sqlx::query("INSERT INTO message_login_eligibility(account_id,nickname,peer_ip,issued_at,expires_at) VALUES($1,$2,$3::inet,now(),now()+interval '60 seconds')")
        .bind(1_i64).bind("Alice").bind("203.0.113.9")
        .execute(&pool).await.expect("inactive eligibility");
    assert!(
        !store
            .authenticate(1, b"Alice", "203.0.113.9".parse().expect("peer"))
            .await
            .expect("inactive rejected")
    );
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

#[sqlx::test(migrator = "MIGRATOR")]
async fn encrypted_production_message_auth_uses_shared_one_time_eligibility(pool: PgPool) {
    sqlx::query(
        "INSERT INTO accounts(id, username_normalized, username_display) VALUES(1,'alice','alice')",
    )
    .execute(&pool)
    .await
    .expect("account");
    sqlx::query("INSERT INTO profiles(account_id,nickname_display,nickname_normalized,setup_state) VALUES(1,'Alice','alice','complete')")
        .execute(&pool).await.expect("profile");
    sqlx::query("INSERT INTO message_login_eligibility(account_id,nickname,peer_ip,issued_at,expires_at) VALUES(1,'Alice','127.0.0.1'::inet,now(),now()+interval '60 seconds')")
        .execute(&pool).await.expect("eligibility");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let shutdown = CancellationToken::new();
    let service = MessageService::with_store(
        Arc::new(PostgresStore::new(pool)),
        7,
        CodecLimits::default(),
    );
    let server_shutdown = shutdown.clone();
    let task = tokio::spawn(async move { service.serve(listener, server_shutdown).await });
    let mut stream = TcpStream::connect(address).await.expect("connect");
    let mut hello = [0_u8; 9];
    stream.read_exact(&mut hello).await.expect("hello");
    let packet = ClientPacket::CredentialDeclaration {
        user_id: 1,
        user_nickname: b"Alice".to_vec(),
    };
    let mut plain = packet.opcode().to_le_bytes().to_vec();
    plain.extend(packet.encode_payload().expect("payload"));
    stream
        .write_all(&client_encrypt(&plain, 7, 1).expect("encrypt"))
        .await
        .expect("write");
    let mut header = [0_u8; 3];
    stream
        .read_exact(&mut header)
        .await
        .expect("response header");
    let total = usize::from(u16::from_le_bytes([header[1], header[2]])) + 3;
    let mut frame = vec![0_u8; total];
    frame[..3].copy_from_slice(&header);
    stream
        .read_exact(&mut frame[3..])
        .await
        .expect("response body");
    let decoded = server_decrypt(&frame, 7, 8 * 1024 * 1024, 128).expect("decrypt");
    assert_eq!(u16::from_le_bytes([decoded[0], decoded[1]]), 0x2f);
    assert_eq!(
        ServerPacket::decode(0x2f, &decoded[2..]).expect("credential response"),
        ServerPacket::CredentialResponse { user_id: 1 }
    );
    shutdown.cancel();
    drop(stream);
    task.await.expect("join").expect("serve");
}
