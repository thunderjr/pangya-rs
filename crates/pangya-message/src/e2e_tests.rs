use super::*;

#[test]
fn two_clients_receive_presence_and_friend_chat() {
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
        .expect("alice auth");
    bob.handle(ClientPacket::CredentialDeclaration {
        user_id: 2,
        user_nickname: b"Bob".to_vec(),
    })
    .expect("bob auth");
    bob.handle(ClientPacket::Status {
        status: Presence::Online,
    })
    .expect("status");
    assert!(
        alice
            .poll()
            .expect("presence")
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Presence { user_id: 2, .. }))
    );
    alice
        .handle(ClientPacket::Chat {
            user_id: 2,
            message: b"hello".to_vec(),
        })
        .expect("chat");
    assert!(
        bob.poll().expect("chat delivery").iter().any(
            |packet| matches!(packet, ServerPacket::Chat { message, .. } if message == b"hello")
        )
    );
}
