use super::*;
use pangya_crypto::{client_encrypt, server_decrypt};
use pangya_protocol::CodecLimits;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    time::{Duration, sleep},
};
use tokio_util::sync::CancellationToken;

async fn send_packet(stream: &mut TcpStream, key: u8, salt: u8, packet: ClientPacket) {
    let mut plain = packet.opcode().to_le_bytes().to_vec();
    plain.extend(packet.encode_payload().expect("payload"));
    stream
        .write_all(&client_encrypt(&plain, key, salt).expect("encrypt"))
        .await
        .expect("write");
}

async fn receive_packet(stream: &mut TcpStream, key: u8) -> (u16, Vec<u8>) {
    let mut header = [0_u8; 3];
    stream.read_exact(&mut header).await.expect("header");
    let total = usize::from(u16::from_le_bytes([header[1], header[2]])) + 3;
    let mut frame = vec![0_u8; total];
    frame[..3].copy_from_slice(&header);
    stream.read_exact(&mut frame[3..]).await.expect("body");
    let plain = server_decrypt(&frame, key, 8 * 1024 * 1024, 128).expect("decrypt");
    (
        u16::from_le_bytes([plain[0], plain[1]]),
        plain[2..].to_vec(),
    )
}

fn social_store() -> MemoryStore {
    let store = MemoryStore::default();
    store.register_user(User {
        id: 1,
        nickname: b"Alice".to_vec(),
        guild_id: None,
        guild_name: vec![],
    });
    store.register_user(User {
        id: 2,
        nickname: b"Bob".to_vec(),
        guild_id: None,
        guild_name: vec![],
    });
    store.add_friend(1, 2).expect("friends");
    store.confirm_friend(1, 2).expect("friend confirmation");
    store
}

async fn authenticate(stream: &mut TcpStream) {
    let mut hello = [0; 9];
    stream.read_exact(&mut hello).await.expect("hello");
    send_packet(
        stream,
        7,
        1,
        ClientPacket::CredentialDeclaration {
            user_id: 1,
            user_nickname: b"Alice".to_vec(),
        },
    )
    .await;
    let (opcode, payload) = receive_packet(stream, 7).await;
    assert_eq!(opcode, server_opcode::CREDENTIAL_RESPONSE);
    assert_eq!(
        ServerPacket::decode(opcode, &payload).expect("decode"),
        ServerPacket::CredentialResponse { user_id: 1 }
    );
}

async fn wait_for_friend_state(store: &MemoryStore, expected: Presence) {
    for _ in 0..100 {
        if store
            .friends(2)
            .iter()
            .any(|friend| friend.user_id == 1 && friend.state == expected)
        {
            return;
        }
        sleep(Duration::from_millis(10)).await;
    }
    panic!("friend state did not become {expected:?}");
}

#[tokio::test]
async fn encrypted_listener_authenticates_and_closes_on_goodbye() {
    let store = MemoryStore::default();
    store.register_user(User {
        id: 1,
        nickname: b"Alice".to_vec(),
        guild_id: None,
        guild_name: vec![],
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let shutdown = CancellationToken::new();
    let service = MessageService::new(store, 7, CodecLimits::default());
    let server_shutdown = shutdown.clone();
    let task = tokio::spawn(async move { service.serve(listener, server_shutdown).await });

    let mut stream = TcpStream::connect(address).await.expect("connect");
    let mut hello = [0; 9];
    stream.read_exact(&mut hello).await.expect("hello");
    assert_eq!(hello, [0, 6, 0, 0, 0x3f, 0, 1, 1, 7]);
    send_packet(
        &mut stream,
        7,
        1,
        ClientPacket::CredentialDeclaration {
            user_id: 1,
            user_nickname: b"Alice".to_vec(),
        },
    )
    .await;
    let (opcode, payload) = receive_packet(&mut stream, 7).await;
    assert_eq!(opcode, server_opcode::CREDENTIAL_RESPONSE);
    assert_eq!(
        ServerPacket::decode(opcode, &payload).expect("decode"),
        ServerPacket::CredentialResponse { user_id: 1 }
    );
    send_packet(&mut stream, 7, 2, ClientPacket::Goodbye).await;
    drop(stream);
    shutdown.cancel();
    task.await.expect("join").expect("serve");
}

#[tokio::test]
async fn malformed_frame_disconnects_and_cleans_up_presence() {
    let store = social_store();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let shutdown = CancellationToken::new();
    let service = MessageService::new(store.clone(), 7, CodecLimits::default());
    let server_shutdown = shutdown.clone();
    let task = tokio::spawn(async move { service.serve(listener, server_shutdown).await });
    let mut stream = TcpStream::connect(address).await.expect("connect");
    authenticate(&mut stream).await;
    send_packet(
        &mut stream,
        7,
        2,
        ClientPacket::Status {
            status: Presence::Online,
        },
    )
    .await;
    wait_for_friend_state(&store, Presence::Online).await;
    // An encrypted frame with a four-byte total is independently malformed at the transport
    // boundary; it must not leave the authenticated session online.
    stream
        .write_all(&[0, 0, 0, 0])
        .await
        .expect("malformed write");
    wait_for_friend_state(&store, Presence::Offline).await;
    drop(stream);
    shutdown.cancel();
    task.await.expect("join").expect("serve");
}

#[tokio::test]
async fn truncated_eof_disconnects_and_cleans_up_presence() {
    let store = social_store();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let shutdown = CancellationToken::new();
    let service = MessageService::new(store.clone(), 7, CodecLimits::default());
    let server_shutdown = shutdown.clone();
    let task = tokio::spawn(async move { service.serve(listener, server_shutdown).await });
    let mut stream = TcpStream::connect(address).await.expect("connect");
    authenticate(&mut stream).await;
    send_packet(
        &mut stream,
        7,
        2,
        ClientPacket::Status {
            status: Presence::Online,
        },
    )
    .await;
    wait_for_friend_state(&store, Presence::Online).await;
    // This is a literal partial frame: FrameCodec::decode_eof must reject it and still run the
    // connection cleanup path.
    stream
        .write_all(&[1, 5, 0, 0])
        .await
        .expect("partial write");
    drop(stream);
    wait_for_friend_state(&store, Presence::Offline).await;
    shutdown.cancel();
    task.await.expect("join").expect("serve");
}

#[tokio::test]
async fn failed_response_send_disconnects_and_cleans_up_presence() {
    let store = social_store();
    // Enough friend rows force Hello to emit multiple independent response frames. Closing the
    // peer immediately after the request exercises the runtime's send-error cleanup branch.
    for id in 3..=100 {
        store.register_user(User {
            id,
            nickname: format!("Friend{id}").into_bytes(),
            guild_id: None,
            guild_name: vec![],
        });
        store.add_friend(1, id).expect("friend request");
        store.confirm_friend(1, id).expect("friend confirmation");
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let shutdown = CancellationToken::new();
    let service = MessageService::new(store.clone(), 7, CodecLimits::default());
    let server_shutdown = shutdown.clone();
    let task = tokio::spawn(async move { service.serve(listener, server_shutdown).await });
    let mut stream = TcpStream::connect(address).await.expect("connect");
    authenticate(&mut stream).await;
    send_packet(
        &mut stream,
        7,
        2,
        ClientPacket::Status {
            status: Presence::Online,
        },
    )
    .await;
    wait_for_friend_state(&store, Presence::Online).await;
    send_packet(&mut stream, 7, 3, ClientPacket::Hello).await;
    drop(stream);
    wait_for_friend_state(&store, Presence::Offline).await;
    shutdown.cancel();
    task.await.expect("join").expect("serve");
}

#[tokio::test]
async fn cancellation_disconnects_and_cleans_up_presence() {
    let store = social_store();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let shutdown = CancellationToken::new();
    let service = MessageService::new(store.clone(), 7, CodecLimits::default());
    let server_shutdown = shutdown.clone();
    let task = tokio::spawn(async move { service.serve(listener, server_shutdown).await });
    let mut stream = TcpStream::connect(address).await.expect("connect");
    authenticate(&mut stream).await;
    send_packet(
        &mut stream,
        7,
        2,
        ClientPacket::Status {
            status: Presence::Online,
        },
    )
    .await;
    wait_for_friend_state(&store, Presence::Online).await;
    shutdown.cancel();
    wait_for_friend_state(&store, Presence::Offline).await;
    drop(stream);
    task.await.expect("join").expect("serve");
}

#[tokio::test]
async fn two_clients_receive_presence_and_friend_chat() {
    let store = MemoryStore::default();
    store.register_user(User {
        id: 1,
        nickname: b"Alice".to_vec(),
        guild_id: None,
        guild_name: vec![],
    });
    store.register_user(User {
        id: 2,
        nickname: b"Bob".to_vec(),
        guild_id: None,
        guild_name: vec![],
    });
    store.confirm_friend(1, 2).expect("friends");
    let mut alice = MessageSession::new(store.clone());
    let mut bob = MessageSession::new(store);
    alice
        .handle(ClientPacket::CredentialDeclaration {
            user_id: 1,
            user_nickname: b"Alice".to_vec(),
        })
        .await
        .expect("alice auth");
    bob.handle(ClientPacket::CredentialDeclaration {
        user_id: 2,
        user_nickname: b"Bob".to_vec(),
    })
    .await
    .expect("bob auth");
    bob.handle(ClientPacket::Status {
        status: Presence::Online,
    })
    .await
    .expect("status");
    assert!(
        alice
            .poll()
            .await
            .expect("presence")
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Presence { user_id: 2, .. }))
    );
    alice
        .handle(ClientPacket::Chat {
            user_id: 2,
            message: b"hello".to_vec(),
        })
        .await
        .expect("chat");
    assert!(
        bob.poll().await.expect("chat delivery").iter().any(
            |packet| matches!(packet, ServerPacket::Chat { message, .. } if message == b"hello")
        )
    );
}
