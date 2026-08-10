use super::*;

fn social_store_for_tests() -> MemoryStore {
    let store = MemoryStore::default();
    for (id, nickname) in [(1, b"Alice".to_vec()), (2, b"Bob".to_vec())] {
        store.register_user(User {
            id,
            nickname,
            guild_id: None,
            guild_name: vec![],
        });
    }
    store.add_friend(1, 2).expect("friend request");
    store.confirm_friend(2, 1).expect("friend confirmation");
    store
}

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

#[test]
fn packetdoc_friend_entry_tail_stays_opaque_against_super_ss_projection() {
    let entry = FriendEntry {
        nickname: b"Bob".to_vec(),
        alias: b"Friend".to_vec(),
        user_id: 7,
        channel: ChannelInfo {
            room_number: 12,
            room_type: 3,
            server_id: 99,
            channel_id: 4,
            channel_name: b"Channel".to_vec(),
        },
        state: Presence::Busy,
        relationship: Relationship::FriendAndGuild,
        blocked: true,
    };
    let encoded = ServerPacket::FriendList {
        page: Page {
            number: 1,
            total: 1,
            current: 1,
        },
        entries: vec![entry],
    }
    .encode_payload()
    .expect("encode");
    // PacketDoc 0x0102 is nickname[22], alias[25], uid:u32, opaque[104].
    assert_eq!(&encoded[2 + 1 + 2 + 4 + 22 + 25 + 4..], vec![0; 104]);
}

#[test]
fn superss_k4t_presence_fixture_uses_only_established_0115_fields() {
    let packet = status_packet(
        7,
        Presence::Online,
        &ChannelInfo {
            room_number: -1,
            room_type: -1,
            server_id: 9,
            channel_id: 3,
            channel_name: b"Lobby".to_vec(),
        },
    );
    let payload = packet.encode_payload().expect("encode");
    let mut expected = vec![0x15, 0x01, 7, 0, 0, 0];
    expected.extend(4_u32.to_le_bytes());
    expected.push(1);
    expected.extend((-1_i16).to_le_bytes());
    expected.extend((-1_i32).to_le_bytes());
    expected.extend(9_u32.to_le_bytes());
    expected.push(3);
    expected.extend(b"Lobby");
    expected.extend([0; 59]);
    assert_eq!(payload, expected);
}

#[test]
fn superss_confirm_result_fixture_is_status_and_user_id() {
    assert_eq!(
        ServerPacket::ConfirmResult {
            status: 0,
            user_id: 2,
        }
        .encode_payload()
        .expect("SuperSS 0x0109"),
        [0x09, 0x01, 0, 0, 0, 0, 2, 0, 0, 0]
    );
}

#[tokio::test]
async fn authenticated_hello_publishes_online_and_explicit_status_still_changes_presence() {
    let store = social_store_for_tests();
    let mut session = MessageSession::new(store.clone());
    session
        .handle(ClientPacket::CredentialDeclaration {
            user_id: 1,
            user_nickname: b"Alice".to_vec(),
        })
        .await
        .expect("auth");

    let responses = session.handle(ClientPacket::Hello).await.expect("hello");
    assert!(matches!(
        responses.first(),
        Some(ServerPacket::Presence {
            user_id: 1,
            unknown_f,
            ..
        }) if u32::from_le_bytes(unknown_f[..4].try_into().expect("state")) == Presence::Online as u32
    ));
    assert_eq!(store.friends(2)[0].state, Presence::Online);

    let responses = session
        .handle(ClientPacket::Status {
            status: Presence::Idle,
        })
        .await
        .expect("explicit status");
    assert!(matches!(
        responses.first(),
        Some(ServerPacket::Presence {
            user_id: 1,
            unknown_f,
            ..
        }) if u32::from_le_bytes(unknown_f[..4].try_into().expect("state")) == Presence::Idle as u32
    ));
    assert_eq!(store.friends(2)[0].state, Presence::Idle);
}

#[tokio::test]
async fn hello_with_no_friends_emits_empty_friend_list_page() {
    let store = MemoryStore::default();
    store.register_user(User {
        id: 1,
        nickname: b"Alice".to_vec(),
        guild_id: None,
        guild_name: vec![],
    });
    let mut session = MessageSession::new(store);
    session
        .handle(ClientPacket::CredentialDeclaration {
            user_id: 1,
            user_nickname: b"Alice".to_vec(),
        })
        .await
        .expect("auth");

    let responses = session.handle(ClientPacket::Hello).await.expect("hello");
    assert_eq!(responses.len(), 2, "Hello returns status plus 0x0102 page");
    assert!(matches!(
        responses[0],
        ServerPacket::Presence { user_id: 1, .. }
    ));
    assert!(matches!(
        responses[1],
        ServerPacket::FriendList {
            page: Page {
                number: 1,
                total: 0,
                current: 0,
            },
            ref entries,
        } if entries.is_empty()
    ));
    assert_eq!(
        responses[1].encode_payload().expect("empty 0x0102"),
        [0x02, 0x01, 1, 0, 0, 0, 0, 0, 0]
    );
}

#[tokio::test]
async fn confirm_requires_pending_incoming_request() {
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
    let mut session = MessageSession::new(store.clone());
    session
        .handle(ClientPacket::CredentialDeclaration {
            user_id: 1,
            user_nickname: b"Alice".to_vec(),
        })
        .await
        .expect("auth");
    assert_eq!(
        session
            .handle(ClientPacket::ConfirmFriend { user_id: 2 })
            .await,
        Err(MessageError::Rejected)
    );
    store.add_friend(2, 1).expect("incoming request");
    assert_eq!(
        session
            .handle(ClientPacket::ConfirmFriend { user_id: 2 })
            .await,
        Ok(vec![ServerPacket::ConfirmResult {
            status: 0,
            user_id: 2
        }])
    );

    let mut requester = MessageSession::new(store.clone());
    requester
        .handle(ClientPacket::CredentialDeclaration {
            user_id: 2,
            user_nickname: b"Bob".to_vec(),
        })
        .await
        .expect("auth requester");
    assert_eq!(
        requester
            .handle(ClientPacket::ConfirmFriend { user_id: 1 })
            .await,
        Err(MessageError::Rejected),
        "the requester cannot confirm its own outgoing request"
    );
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
fn memory_lease_expiry_requeues_unacknowledged_delivery() {
    let store = MemoryStore::default();
    for (id, nickname) in [(1, b"Alice".to_vec()), (2, b"Bob".to_vec())] {
        store.register_user(User {
            id,
            nickname,
            guild_id: None,
            guild_name: vec![],
        });
    }
    store.add_friend(1, 2).expect("friend request");
    store.add_friend(2, 1).expect("friend confirmation");
    store
        .queue_message(1, 2, b"leased".to_vec())
        .expect("queue");
    assert_eq!(store.take_messages(2).expect("claim").len(), 1);
    store
        .0
        .lock()
        .expect("store lock")
        .inflight_until
        .insert(2, Instant::now() - Duration::from_secs(1));
    assert_eq!(store.take_messages(2).expect("expired claim").len(), 1);
}

#[tokio::test]
async fn memory_live_lease_expiry_requeues_to_live_poll_path() {
    let store = MemoryStore::default();
    for (id, nickname) in [(1, b"Alice".to_vec()), (2, b"Bob".to_vec())] {
        store.register_user(User {
            id,
            nickname,
            guild_id: None,
            guild_name: vec![],
        });
    }
    store.add_friend(1, 2).expect("friend request");
    store.add_friend(2, 1).expect("friend confirmation");
    store.set_online(2, Presence::Online, ChannelInfo::offline());
    store
        .queue_message(1, 2, b"live lease".to_vec())
        .expect("queue");
    assert_eq!(store.take_live_messages(2).expect("claim").len(), 1);
    store
        .0
        .lock()
        .expect("store lock")
        .inflight_until
        .insert(2, Instant::now() - Duration::from_secs(1));
    assert_eq!(
        store
            .take_live_messages(2)
            .expect("expired live claim")
            .len(),
        1
    );
}

#[test]
fn memory_presence_expiry_fanout_and_reconnect_generation_are_safe() {
    let store = MemoryStore::default();
    for id in 1..=36 {
        store.register_user(User {
            id,
            nickname: id.to_string().into_bytes(),
            guild_id: None,
            guild_name: vec![],
        });
    }
    for id in 2..=36 {
        store.add_friend(1, id).expect("friend request");
        store.confirm_friend(id, 1).expect("friend confirmation");
    }
    store.set_online(1, Presence::Online, ChannelInfo::offline());
    for id in 2..=36 {
        assert_eq!(store.take_presence_events(id).len(), 1);
    }
    store.set_offline(1);
    store.set_online(1, Presence::Online, ChannelInfo::offline());
    assert!(
        store
            .take_presence_events(2)
            .iter()
            .all(|(_, status, _)| *status != Presence::Offline)
    );
    store
        .0
        .lock()
        .expect("store lock")
        .presence_expiry
        .insert(1, Instant::now() - Duration::from_secs(1));
    assert!(
        store
            .take_presence_events(2)
            .iter()
            .any(|(_, status, _)| *status == Presence::Offline)
    );
}

#[tokio::test]
async fn memory_guild_operations_are_explicit_safe_noops() {
    let store = MemoryStore::default();
    for id in 1..=3 {
        store.register_user(User {
            id,
            nickname: id.to_string().into_bytes(),
            guild_id: Some(7),
            guild_name: b"Guild".to_vec(),
        });
    }
    store
        .queue_guild_message(1, 2, b"guild".to_vec())
        .expect("deferred guild operation");
    assert!(
        store
            .guild_members(1)
            .await
            .expect("deferred membership")
            .is_empty()
    );
    assert!(store.take_messages(2).expect("member 2").is_empty());
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
