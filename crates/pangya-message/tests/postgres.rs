#![allow(missing_docs)]

use pangya_crypto::{client_encrypt, server_decrypt};
use pangya_message::{
    ChannelInfo, ClientPacket, MessageService, MessageStore, PostgresStore, Presence, ServerPacket,
};
use pangya_protocol::CodecLimits;
use pangya_storage::MIGRATOR;
use sqlx::{PgPool, Row};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

async fn send_encrypted_packet(stream: &mut TcpStream, key: u8, salt: u8, packet: ClientPacket) {
    let mut plain = packet.opcode().to_le_bytes().to_vec();
    plain.extend(packet.encode_payload().expect("payload"));
    stream
        .write_all(&client_encrypt(&plain, key, salt).expect("encrypt"))
        .await
        .expect("write");
}

async fn receive_encrypted_packet(stream: &mut TcpStream, key: u8) -> ServerPacket {
    let mut header = [0_u8; 3];
    stream.read_exact(&mut header).await.expect("header");
    let total = usize::from(u16::from_le_bytes([header[1], header[2]])) + 3;
    let mut frame = vec![0_u8; total];
    frame[..3].copy_from_slice(&header);
    stream.read_exact(&mut frame[3..]).await.expect("body");
    let decoded = server_decrypt(&frame, key, 8 * 1024 * 1024, 128).expect("decrypt");
    let opcode = u16::from_le_bytes([decoded[0], decoded[1]]);
    ServerPacket::decode(opcode, &decoded[2..]).expect("server packet")
}

async fn encrypted_connect(address: std::net::SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(address).await.expect("connect");
    let mut hello = [0_u8; 9];
    stream.read_exact(&mut hello).await.expect("hello");
    assert_eq!(&hello[..6], &[0, 6, 0, 0, 0x3f, 0]);
    stream
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn postgres_expired_presence_emits_and_deletes_one_offline_transition(pool: PgPool) {
    for (id, username, nickname) in [
        (1_i64, "alice", "Alice"),
        (2, "bob", "Bob"),
        (3, "cara", "Cara"),
    ] {
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
    store.add_friend(1, 2).await.expect("friend request");
    store
        .confirm_friend(2, 1)
        .await
        .expect("friend confirmation");
    store.add_friend(2, 3).await.expect("second friend request");
    store
        .confirm_friend(3, 2)
        .await
        .expect("second friend confirmation");
    store
        .set_online(2, Presence::Online, ChannelInfo::offline())
        .await
        .expect("online");
    sqlx::query(
        "UPDATE message_presence SET expires_at=now()-interval '1 second' WHERE account_id=2",
    )
    .execute(&pool)
    .await
    .expect("expire presence");

    let first = store
        .take_presence_events(1)
        .await
        .expect("first presence poll");
    assert_eq!(
        first,
        vec![(2, Presence::Offline, ChannelInfo::offline())],
        "expiry emits exactly one offline transition"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM message_presence_events WHERE recipient_account_id=1",
        )
        .fetch_one(&pool)
        .await
        .expect("event deletion count"),
        0,
        "delivered transition is deleted"
    );

    assert!(
        store
            .take_presence_events(1)
            .await
            .expect("second presence poll")
            .is_empty(),
        "the same expired projection is not reinserted on a later poll"
    );
    assert_eq!(
        store
            .take_presence_events(3)
            .await
            .expect("second recipient presence poll"),
        vec![(2, Presence::Offline, ChannelInfo::offline())],
        "expiry fans out to every confirmed friend"
    );
    assert!(
        store
            .take_presence_events(3)
            .await
            .expect("second recipient repeat poll")
            .is_empty(),
        "each recipient deletes its transition exactly once"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM message_presence WHERE account_id=2",)
            .fetch_one(&pool)
            .await
            .expect("expired projection count"),
        0,
        "the expired projection is removed after transition materialization"
    );
}

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
    assert!(
        !store
            .has_pending_friend_request(1, 2)
            .await
            .expect("outgoing")
    );
    assert!(
        store
            .has_pending_friend_request(2, 1)
            .await
            .expect("incoming")
    );
    assert!(store.confirm_friend(1, 2).await.is_err());
    store.confirm_friend(2, 1).await.expect("confirm");
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
    let row = sqlx::query(
        r#"SELECT count(*) AS count,
                  count(*) FILTER (WHERE delivered_at IS NULL) AS pending,
                  count(*) FILTER (WHERE delivery_lease_until IS NOT NULL) AS leased,
                  count(*) FILTER (WHERE delivered_at IS NULL AND (delivery_lease_until IS NULL OR delivery_lease_until <= now())) AS available
           FROM message_offline_messages"#,
    )
    .fetch_one(&pool)
    .await
    .expect("leased count");
    assert_eq!(row.get::<i64, _>("count"), 1);
    assert_eq!(row.get::<i64, _>("pending"), 1);
    assert_eq!(row.get::<i64, _>("leased"), 1);
    assert_eq!(row.get::<i64, _>("available"), 0);

    // Claiming is not delivery: the row remains durable and invisible until the consumer ACKs it.
    restarted.ack_messages(&messages).await.expect("ack");
    let row = sqlx::query(
        r#"SELECT count(*) AS count,
                  count(*) FILTER (WHERE delivered_at IS NULL) AS pending,
                  count(*) FILTER (WHERE delivery_lease_until IS NOT NULL) AS leased,
                  count(*) FILTER (WHERE delivered_at IS NULL AND (delivery_lease_until IS NULL OR delivery_lease_until <= now())) AS available
           FROM message_offline_messages"#,
    )
    .fetch_one(&pool)
    .await
    .expect("ack count");
    assert_eq!(row.get::<i64, _>("count"), 1);
    assert_eq!(row.get::<i64, _>("pending"), 0);
    assert_eq!(row.get::<i64, _>("leased"), 0);
    assert_eq!(row.get::<i64, _>("available"), 0);
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

#[sqlx::test(migrator = "MIGRATOR")]
async fn encrypted_production_multi_client_survives_service_restart(pool: PgPool) {
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
        sqlx::query("INSERT INTO message_login_eligibility(account_id,nickname,peer_ip,issued_at,expires_at) VALUES($1,$2,'127.0.0.1'::inet,now(),now()+interval '60 seconds')")
            .bind(id)
            .bind(nickname)
            .execute(&pool)
            .await
            .expect("eligibility");
    }
    let store = PostgresStore::new(pool.clone());
    store.add_friend(1, 2).await.expect("friend request");
    assert!(store.confirm_friend(1, 2).await.is_err());
    store
        .confirm_friend(2, 1)
        .await
        .expect("friend confirmation");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let shutdown = CancellationToken::new();
    let service = MessageService::with_store(Arc::new(store), 7, CodecLimits::default());
    let server_shutdown = shutdown.clone();
    let task = tokio::spawn(async move { service.serve(listener, server_shutdown).await });
    let mut alice = encrypted_connect(address).await;
    let mut bob = encrypted_connect(address).await;
    send_encrypted_packet(
        &mut alice,
        7,
        1,
        ClientPacket::CredentialDeclaration {
            user_id: 1,
            user_nickname: b"Alice".to_vec(),
        },
    )
    .await;
    send_encrypted_packet(
        &mut bob,
        7,
        1,
        ClientPacket::CredentialDeclaration {
            user_id: 2,
            user_nickname: b"Bob".to_vec(),
        },
    )
    .await;
    assert_eq!(
        receive_encrypted_packet(&mut alice, 7).await,
        ServerPacket::CredentialResponse { user_id: 1 }
    );
    assert_eq!(
        receive_encrypted_packet(&mut bob, 7).await,
        ServerPacket::CredentialResponse { user_id: 2 }
    );

    send_encrypted_packet(
        &mut alice,
        7,
        2,
        ClientPacket::Chat {
            user_id: 2,
            message: b"encrypted multi-client".to_vec(),
        },
    )
    .await;
    let chat = timeout(Duration::from_secs(2), async {
        loop {
            if let ServerPacket::Chat { message, .. } = receive_encrypted_packet(&mut bob, 7).await
            {
                break message;
            }
        }
    })
    .await
    .expect("chat delivery");
    assert_eq!(chat, b"encrypted multi-client");

    drop(alice);
    drop(bob);
    shutdown.cancel();
    task.await.expect("join").expect("serve");

    // Queue while the first service is gone, then use a fresh service/store composition. The
    // encrypted client path must still claim and ACK the durable message after restart.
    sqlx::query("INSERT INTO message_login_eligibility(account_id,nickname,peer_ip,issued_at,expires_at) VALUES(2,'Bob','127.0.0.1'::inet,now(),now()+interval '60 seconds')")
        .execute(&pool)
        .await
        .expect("restart eligibility");
    PostgresStore::new(pool.clone())
        .queue_message(1, 2, b"after restart".to_vec())
        .await
        .expect("durable queue");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("restart listener");
    let address = listener.local_addr().expect("restart address");
    let shutdown = CancellationToken::new();
    let service = MessageService::with_store(
        Arc::new(PostgresStore::new(pool.clone())),
        7,
        CodecLimits::default(),
    );
    let server_shutdown = shutdown.clone();
    let task = tokio::spawn(async move { service.serve(listener, server_shutdown).await });
    let mut bob = encrypted_connect(address).await;
    send_encrypted_packet(
        &mut bob,
        7,
        1,
        ClientPacket::CredentialDeclaration {
            user_id: 2,
            user_nickname: b"Bob".to_vec(),
        },
    )
    .await;
    assert_eq!(
        receive_encrypted_packet(&mut bob, 7).await,
        ServerPacket::CredentialResponse { user_id: 2 }
    );
    let message = timeout(Duration::from_secs(2), async {
        loop {
            if let ServerPacket::Chat { message, .. } = receive_encrypted_packet(&mut bob, 7).await
            {
                break message;
            }
        }
    })
    .await
    .expect("restart message delivery");
    assert_eq!(message, b"after restart");
    for _ in 0..100 {
        let pending: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM message_offline_messages WHERE delivered_at IS NULL",
        )
        .fetch_one(&pool)
        .await
        .expect("pending poll");
        if pending == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    drop(bob);
    shutdown.cancel();
    task.await.expect("restart join").expect("restart serve");

    let row = sqlx::query(
        "SELECT count(*) FILTER (WHERE delivered_at IS NULL) AS pending FROM message_offline_messages",
    )
    .fetch_one(&pool)
    .await
    .expect("restart pending count");
    assert_eq!(row.get::<i64, _>("pending"), 0);
}
