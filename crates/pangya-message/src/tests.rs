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
fn packetdoc_literal_status_sub_layouts_match_reference() {
    // PacketDoc 0x0115 is uid + unknown[11] + server:u32 + unknown:u8 + unknown[64].
    let mut fixture = vec![0x15, 0x01, 7, 0, 0, 0, 0x7f];
    fixture.extend([0; 10]);
    fixture.extend(9_u32.to_le_bytes());
    fixture.push(0xff);
    fixture.extend([0; 64]);
    assert_eq!(
        ServerPacket::decode(0x30, &fixture),
        Ok(ServerPacket::Presence {
            user_id: 7,
            unknown_f: [0x7f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            server_id: 9,
            unknown_g: 0xff,
            unknown_h: vec![],
        })
    );

    let mut lookup = vec![0x17, 0x01, 0, 0, 0, 0, 3, 0, b'B', b'o', b'b'];
    lookup.extend(9_u32.to_le_bytes());
    assert_eq!(
        ServerPacket::decode(0x30, &lookup),
        Ok(ServerPacket::LookupResponse {
            status: 0,
            nickname: b"Bob".to_vec(),
            user_id: Some(9),
        })
    );
}

#[test]
fn packetdoc_friend_request_sub_layout_has_one_result_word() {
    let mut fixture = vec![0x04, 0x01, 0, 0, 0, 0];
    fixture.extend(b"Bob\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0".iter());
    fixture.extend(b"Friend\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0".iter());
    fixture.extend(7_u32.to_le_bytes());
    fixture.extend([0; 99 + 2 + 1 + 2]);
    assert_eq!(fixture.len(), 2 + 4 + FriendEntry::WIRE_LEN);
    let packet = ServerPacket::decode(0x30, &fixture).expect("literal 0x104");
    assert!(matches!(
        packet,
        ServerPacket::FriendRequest {
            status: 0,
            entry: FriendEntry {
                user_id: 7,
                state: Presence::Offline,
                ..
            }
        }
    ));
}

#[test]
fn packetdoc_literal_friend_list_header_and_entry_are_bounded() {
    let mut fixture = vec![0x02, 0x01, 0, 1, 0, 1, 0, 0, 0];
    fixture.extend([0x42; FriendEntry::WIRE_LEN]);
    let packet = ServerPacket::decode(0x30, &fixture).expect("literal 0x102");
    assert!(
        matches!(packet, ServerPacket::FriendList { entries, .. } if entries[0].user_id == 0x42424242)
    );
}

#[test]
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
        entries: vec![entry],
    };
    let encoded = packet.encode_payload().expect("encode");
    assert_eq!(encoded.len(), 2 + 1 + 2 + 4 + FriendEntry::WIRE_LEN);
    let decoded = ServerPacket::decode(0x30, &encoded).expect("decode");
    assert!(matches!(
        decoded,
        ServerPacket::FriendList { page, entries }
            if page == Page { number: 1, total: 1, current: 1 }
                && entries.len() == 1 && entries[0].user_id == 7
    ));
}

#[tokio::test]
async fn duplicate_message_sessions_fence_and_disconnect_old_generation() {
    let store = MemoryStore::default();
    store.register_user(User {
        id: 1,
        nickname: b"Alice".to_vec(),
        guild_id: None,
        guild_name: vec![],
    });
    let registry = Arc::new(SessionRegistry::default());
    let mut first = MessageSession::new(store.clone()).with_registry(registry.clone());
    first
        .handle(ClientPacket::CredentialDeclaration {
            user_id: 1,
            user_nickname: b"Alice".to_vec(),
        })
        .await
        .expect("first auth");
    let mut replacement = MessageSession::new(store).with_registry(registry);
    replacement
        .handle(ClientPacket::CredentialDeclaration {
            user_id: 1,
            user_nickname: b"Alice".to_vec(),
        })
        .await
        .expect("replacement auth");
    assert_eq!(first.poll().await, Err(MessageError::Rejected));
    first.disconnect().await.expect("old disconnect");
    replacement
        .disconnect()
        .await
        .expect("replacement disconnect");
}

#[tokio::test]
async fn lookup_and_actions_require_authenticated_account() {
    let store = MemoryStore::default();
    store.register_user(User {
        id: 1,
        nickname: b"Alice".to_vec(),
        guild_id: None,
        guild_name: vec![],
    });
    let mut session = MessageSession::new(store);
    assert_eq!(
        session
            .handle(ClientPacket::Lookup {
                nickname: b"Alice".to_vec(),
            })
            .await,
        Err(MessageError::Unauthorized)
    );
    assert_eq!(
        session
            .handle(ClientPacket::BlockFriend { user_id: 1 })
            .await,
        Err(MessageError::Unauthorized)
    );
}

#[test]
fn reference_chat_reply_round_trips_without_invented_subtype() {
    let packet = ServerPacket::Chat {
        user_id: 7,
        nickname: b"Alice".to_vec(),
        message: b"hello".to_vec(),
        guild: false,
    };
    let payload = packet.encode_payload().expect("encode");
    assert_eq!(
        ServerPacket::decode(
            0x30,
            &[
                0x13, 0x01, 7, 0, 0, 0, 5, 0, b'A', b'l', b'i', b'c', b'e', 5, 0, b'h', b'e', b'l',
                b'l', b'o', 0,
            ]
        )
        .expect("decode"),
        packet
    );
    assert_eq!(payload[0..2], [0x13, 0x01]);
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
