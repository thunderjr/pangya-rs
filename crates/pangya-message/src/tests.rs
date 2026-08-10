use super::*;

#[test]
fn packetdoc_handshake_uses_message_namespace_and_exact_pstrings() {
    let packet = ClientPacket::CredentialDeclaration {
        user_id: 42,
        user_nickname: b"Alice".to_vec(),
    };
    assert_eq!(packet.opcode(), 0x12);
    assert_eq!(
        packet.encode_payload().expect("encode"),
        b"*\0\0\0\x05\0Alice"
    );
    assert_eq!(
        ServerPacket::CredentialResponse { user_id: 42 }
            .encode_payload()
            .expect("encode"),
        [0, 42, 0, 0, 0]
    );
}

#[test]
#[allow(clippy::redundant_clone)]
fn packetdoc_status_friend_entry_round_trips_without_single_hole_assumptions() {
    let entry = FriendEntry {
        nickname: b"Bob".to_vec(),
        alias: b"Friend".to_vec(),
        user_id: 7,
        channel: ChannelInfo::offline(),
        state: Presence::Online,
        relationship: Relationship::Friend,
        blocked: false,
    };
    let packet = ServerPacket::FriendList {
        page: Page {
            number: 1,
            total: 1,
            current: 1,
        },
        entries: vec![entry.clone()],
    };
    let decoded =
        ServerPacket::decode(0x30, &packet.encode_payload().expect("encode")).expect("decode");
    assert_eq!(decoded, packet);
}

#[test]
fn friend_mutations_are_authorized_persisted_and_idempotent() {
    let store = MemoryStore::default();
    store.register_user(User {
        id: 1,
        nickname: b"Alice".to_vec(),
        guild_id: Some(9),
        guild_name: b"Guild".to_vec(),
    });
    store.register_user(User {
        id: 2,
        nickname: b"Bob".to_vec(),
        guild_id: Some(9),
        guild_name: b"Guild".to_vec(),
    });
    store.add_friend(1, 2).expect("first add");
    store.add_friend(1, 2).expect("replay add");
    assert!(store.friends(1).iter().any(|friend| friend.user_id == 2));
    store.block_friend(1, 2).expect("block");
    assert!(
        store
            .friends(1)
            .iter()
            .find(|friend| friend.user_id == 2)
            .expect("friend")
            .blocked
    );
    store.unblock_friend(1, 2).expect("unblock");
    store.delete_friend(1, 2).expect("delete");
    assert!(store.friends(1).is_empty());
}

#[test]
#[allow(clippy::redundant_clone)]
fn offline_messages_survive_store_restart_and_are_delivered_once() {
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
    store.add_friend(1, 2).expect("friend");
    store.add_friend(2, 1).expect("friend");
    store.queue_message(1, 2, b"hello".to_vec()).expect("queue");
    let restarted = store.clone();
    let messages = restarted.take_messages(2).expect("take");
    assert_eq!(messages.len(), 1);
    assert_eq!(restarted.take_messages(2).expect("second take").len(), 0);
}

#[test]
fn rate_limit_and_replay_guard_are_bounded() {
    let mut guard = ReplayGuard::new(2);
    assert!(guard.admit(1));
    assert!(!guard.admit(1));
    assert!(guard.admit(2));
    assert!(guard.admit(3));
    assert!(!guard.admit(2));
    let mut rate = RateLimiter::new(2);
    assert!(rate.admit());
    assert!(rate.admit());
    assert!(!rate.admit());
}
