//! Generated local synthetic M4 room protocol tests; no source/client bytes.

use pangya_domain::{
    AccountId, ChatText, MemberSnapshot, PlayerConnectionId, RoomId, RoomName, RoomPassword,
    RoomSettings, RoomSnapshot, RoomSummary,
};
use pangya_protocol::{
    CompatibilityProfile, ConnectionState, DecodePacket, Direction, EncodePacket, ErrorClass,
    MAX_ROOM_SUMMARIES, PacketReader, PacketWriter, RegistryKey, RegistryLookup, RoomChatEvent,
    RoomChatRequest, RoomCommand, RoomCommandResult, RoomCommandResultResponse, RoomCreateRequest,
    RoomJoinRequest, RoomKickRequest, RoomLeaveRequest, RoomListRequest, RoomListResponse,
    RoomMembershipEvent, RoomMembershipKind, RoomReadyRequest, RoomSettingsRequest,
    RoomStateRequest, RoomStateResponse, SYNTHETIC_M4_C2S_CHAT, SYNTHETIC_M4_C2S_CREATE,
    SYNTHETIC_M4_C2S_JOIN, SYNTHETIC_M4_C2S_KICK, SYNTHETIC_M4_C2S_LEAVE, SYNTHETIC_M4_C2S_LIST,
    SYNTHETIC_M4_C2S_READY, SYNTHETIC_M4_C2S_SETTINGS, SYNTHETIC_M4_C2S_STATE,
    SYNTHETIC_M4_S2C_CHAT, SYNTHETIC_M4_S2C_COMMAND_RESULT, SYNTHETIC_M4_S2C_LIST,
    SYNTHETIC_M4_S2C_MEMBERSHIP_EVENT, SYNTHETIC_M4_S2C_STATE, ServiceKind, synthetic_m4_registry,
};
use proptest::prelude::*;

const PROFILE: CompatibilityProfile = CompatibilityProfile::US_852;

fn reader(bytes: &[u8], direction: Direction, opcode: u16) -> PacketReader<'_> {
    PacketReader::new(bytes, direction, ServiceKind::Game, Some(opcode))
}

fn encoded<T: EncodePacket>(packet: &T) -> Result<Vec<u8>, pangya_protocol::PacketEncodeError> {
    let mut writer = PacketWriter::new();
    writer.u16_le(T::OPCODE);
    packet.encode(&mut writer, &PROFILE)?;
    Ok(writer.into_inner())
}

fn room_id(value: u32) -> RoomId {
    RoomId::new(value).expect("positive synthetic room ID")
}

fn connection_id(value: u64) -> PlayerConnectionId {
    PlayerConnectionId::new(value).expect("positive synthetic connection ID")
}

fn account_id(value: i64) -> AccountId {
    AccountId::new(value).expect("positive synthetic account ID")
}

fn member(
    connection: u64,
    account: i64,
    nickname: &str,
    owner: bool,
    ready: bool,
) -> MemberSnapshot {
    MemberSnapshot::new(
        connection_id(connection),
        account_id(account),
        nickname.to_owned(),
        owner,
        ready,
    )
}

fn summary(
    id: u32,
    name: &str,
    owner: &str,
    members: u8,
    maximum: u8,
    protected: bool,
) -> RoomSummary {
    RoomSummary::new(
        room_id(id),
        RoomName::parse(name).expect("valid synthetic room name"),
        owner.to_owned(),
        members,
        maximum,
        protected,
    )
}

#[test]
fn generated_create_fixture_decodes_and_reencodes_exactly() {
    let fixture = include_bytes!("fixtures/m4-in-create-synthetic/fixture.bin");
    assert_eq!(
        u16::from_le_bytes([fixture[0], fixture[1]]),
        SYNTHETIC_M4_C2S_CREATE
    );
    let mut input = reader(
        &fixture[2..],
        Direction::ClientToServer,
        SYNTHETIC_M4_C2S_CREATE,
    );
    let packet = RoomCreateRequest::decode(&mut input, &PROFILE).expect("synthetic create");
    assert_eq!(packet.name.as_str(), "Moon Room");
    assert_eq!(
        packet.password.as_ref().map(RoomPassword::expose_bytes),
        Some(b"secret".as_slice())
    );
    assert_eq!(packet.settings.max_members(), 4);
    assert_eq!(input.remaining(), 0);
    assert_eq!(encoded(&packet).expect("encode create"), fixture);
    assert!(!format!("{packet:?}").contains("secret"));
}

#[test]
fn generated_list_fixture_decodes_and_reencodes_exactly() {
    let fixture = include_bytes!("fixtures/m4-out-list-synthetic/fixture.bin");
    let mut input = reader(
        &fixture[2..],
        Direction::ServerToClient,
        SYNTHETIC_M4_S2C_LIST,
    );
    let packet = RoomListResponse::decode(&mut input, &PROFILE).expect("synthetic room list");
    assert_eq!(packet.rooms.len(), 2);
    assert_eq!(packet.rooms[0].name().as_str(), "Moon Room");
    assert!(packet.rooms[0].password_protected());
    assert_eq!(packet.rooms[1].owner_nickname(), "Bob");
    assert_eq!(encoded(&packet).expect("encode list"), fixture);
}

#[test]
fn generated_state_and_chat_fixtures_are_exact() {
    let state_fixture = include_bytes!("fixtures/m4-out-state-synthetic/fixture.bin");
    let mut input = reader(
        &state_fixture[2..],
        Direction::ServerToClient,
        SYNTHETIC_M4_S2C_STATE,
    );
    let state = RoomStateResponse::decode(&mut input, &PROFILE).expect("synthetic state");
    assert_eq!(state.room.summary().id().get(), 7);
    assert_eq!(state.room.members().len(), 2);
    assert!(state.room.members()[0].is_owner());
    assert!(state.room.members()[1].is_ready());
    assert_eq!(encoded(&state).expect("encode state"), state_fixture);

    let chat_fixture = include_bytes!("fixtures/m4-out-chat-synthetic/fixture.bin");
    let mut input = reader(
        &chat_fixture[2..],
        Direction::ServerToClient,
        SYNTHETIC_M4_S2C_CHAT,
    );
    let event = RoomChatEvent::decode(&mut input, &PROFILE).expect("synthetic chat event");
    assert_eq!(event.sender.connection_id().get(), 1001);
    assert_eq!(event.sender.nickname(), "Alice");
    assert_eq!(event.text.as_str(), "Hello Pangya!");
    assert_eq!(encoded(&event).expect("encode chat event"), chat_fixture);
}

#[test]
fn inbound_requests_have_no_client_supplied_sender_identity() {
    let create = RoomCreateRequest {
        name: RoomName::parse("Local").expect("name"),
        password: None,
        settings: RoomSettings::new(4).expect("settings"),
    };
    let join = RoomJoinRequest {
        room_id: room_id(7),
        password: Some(RoomPassword::parse("secret").expect("password")),
    };
    let chat = RoomChatRequest {
        text: ChatText::parse("hello").expect("chat"),
    };
    assert_eq!(encoded(&create).expect("create")[2], 5);
    assert_eq!(&encoded(&join).expect("join")[2..6], &7_u32.to_le_bytes());
    assert_eq!(&encoded(&chat).expect("chat")[2..4], &5_u16.to_le_bytes());
}

#[test]
fn lengths_utf8_and_domain_boundaries_are_strict() {
    let maximum_name = [vec![32, 0], vec![b'a'; 32], vec![0, 30]].concat();
    let mut input = reader(
        &maximum_name,
        Direction::ClientToServer,
        SYNTHETIC_M4_C2S_CREATE,
    );
    let decoded = RoomCreateRequest::decode(&mut input, &PROFILE).expect("32-byte room name");
    assert_eq!(decoded.name.as_str().len(), 32);
    assert_eq!(decoded.settings.max_members(), 30);

    for body in [
        [vec![33, 0], vec![b'a'; 33], vec![0, 4]].concat(),
        vec![2, 0, 0xc3, 0x28, 0, 4],
        [vec![1, 0, b'a', 1, 17, 0], vec![b'p'; 17], vec![4]].concat(),
    ] {
        let mut input = reader(&body, Direction::ClientToServer, SYNTHETIC_M4_C2S_CREATE);
        assert!(RoomCreateRequest::decode(&mut input, &PROFILE).is_err());
        assert!(input.offset() <= body.len());
    }

    let maximum_chat = [vec![128, 0], vec![b'x'; 128]].concat();
    let mut input = reader(
        &maximum_chat,
        Direction::ClientToServer,
        SYNTHETIC_M4_C2S_CHAT,
    );
    assert_eq!(
        RoomChatRequest::decode(&mut input, &PROFILE)
            .expect("128-byte chat")
            .text
            .as_str()
            .len(),
        128
    );
    let oversized_chat = [vec![129, 0], vec![b'x'; 129]].concat();
    let mut input = reader(
        &oversized_chat,
        Direction::ClientToServer,
        SYNTHETIC_M4_C2S_CHAT,
    );
    assert!(matches!(
        RoomChatRequest::decode(&mut input, &PROFILE)
            .expect_err("129-byte chat")
            .context(),
        Some((_, _, _, 0, ErrorClass::Limit))
    ));
}

#[test]
fn counts_are_capped_before_allocation_and_encoding() {
    let count = u16::try_from(MAX_ROOM_SUMMARIES + 1).expect("wire count");
    let count_bytes = count.to_le_bytes();
    let mut input = reader(
        &count_bytes,
        Direction::ServerToClient,
        SYNTHETIC_M4_S2C_LIST,
    );
    let error = RoomListResponse::decode(&mut input, &PROFILE).expect_err("over list cap");
    assert_eq!(input.offset(), 2);
    assert!(matches!(
        error.context(),
        Some((_, _, _, 0, ErrorClass::Limit))
    ));

    let room = summary(7, "Room", "Alice", 0, 30, false);
    let mut prefix = PacketWriter::new();
    RoomListResponse {
        rooms: vec![room.clone()],
    }
    .encode(&mut prefix, &PROFILE)
    .expect("summary body");
    let summary_body = &prefix.as_slice()[2..];
    let mut state_body = summary_body.to_vec();
    state_body.extend_from_slice(&31_u16.to_le_bytes());
    let mut input = reader(
        &state_body,
        Direction::ServerToClient,
        SYNTHETIC_M4_S2C_STATE,
    );
    let error = RoomStateResponse::decode(&mut input, &PROFILE).expect_err("over member cap");
    assert!(matches!(
        error.context(),
        Some((_, _, _, _, ErrorClass::Limit))
    ));

    let too_many = RoomListResponse {
        rooms: vec![room; MAX_ROOM_SUMMARIES + 1],
    };
    assert!(encoded(&too_many).is_err());
}

#[test]
fn booleans_discriminants_and_trailing_bytes_are_strict() {
    let mut input = reader(
        &[1, 0, b'a', 2, 4],
        Direction::ClientToServer,
        SYNTHETIC_M4_C2S_CREATE,
    );
    assert!(RoomCreateRequest::decode(&mut input, &PROFILE).is_err());
    let mut input = reader(&[2], Direction::ClientToServer, SYNTHETIC_M4_C2S_READY);
    assert!(RoomReadyRequest::decode(&mut input, &PROFILE).is_err());
    let mut input = reader(
        &[0xff, 0],
        Direction::ServerToClient,
        SYNTHETIC_M4_S2C_COMMAND_RESULT,
    );
    assert!(RoomCommandResultResponse::decode(&mut input, &PROFILE).is_err());
    let mut input = reader(
        &[0, 0xff],
        Direction::ServerToClient,
        SYNTHETIC_M4_S2C_COMMAND_RESULT,
    );
    assert!(RoomCommandResultResponse::decode(&mut input, &PROFILE).is_err());

    let membership = [7_u32.to_le_bytes().as_slice(), &[0xff]].concat();
    let mut input = reader(
        &membership,
        Direction::ServerToClient,
        SYNTHETIC_M4_S2C_MEMBERSHIP_EVENT,
    );
    assert!(RoomMembershipEvent::decode(&mut input, &PROFILE).is_err());

    assert_all_inbound_reject_trailing_bytes();
    for (opcode, fixture) in [
        (
            SYNTHETIC_M4_S2C_LIST,
            include_bytes!("fixtures/m4-out-list-synthetic/fixture.bin").as_slice(),
        ),
        (
            SYNTHETIC_M4_S2C_STATE,
            include_bytes!("fixtures/m4-out-state-synthetic/fixture.bin").as_slice(),
        ),
        (
            SYNTHETIC_M4_S2C_CHAT,
            include_bytes!("fixtures/m4-out-chat-synthetic/fixture.bin").as_slice(),
        ),
    ] {
        let mut body = fixture[2..].to_vec();
        body.push(0);
        let mut input = reader(&body, Direction::ServerToClient, opcode);
        let result = match opcode {
            value if value == SYNTHETIC_M4_S2C_LIST => {
                RoomListResponse::decode(&mut input, &PROFILE).map(|_| ())
            }
            value if value == SYNTHETIC_M4_S2C_STATE => {
                RoomStateResponse::decode(&mut input, &PROFILE).map(|_| ())
            }
            _ => RoomChatEvent::decode(&mut input, &PROFILE).map(|_| ()),
        };
        assert!(result.is_err());
    }
}

fn assert_all_inbound_reject_trailing_bytes() {
    let cases: &[(u16, &[u8])] = &[
        (SYNTHETIC_M4_C2S_LIST, &[0]),
        (SYNTHETIC_M4_C2S_CREATE, &[1, 0, b'a', 0, 4, 0]),
        (SYNTHETIC_M4_C2S_JOIN, &[1, 0, 0, 0, 0, 0]),
        (SYNTHETIC_M4_C2S_LEAVE, &[0]),
        (SYNTHETIC_M4_C2S_SETTINGS, &[4, 0]),
        (SYNTHETIC_M4_C2S_READY, &[1, 0]),
        (SYNTHETIC_M4_C2S_CHAT, &[1, 0, b'x', 0]),
        (SYNTHETIC_M4_C2S_KICK, &[1, 0, 0, 0, 0, 0, 0, 0, 0]),
        (SYNTHETIC_M4_C2S_STATE, &[0]),
    ];
    for (opcode, body) in cases {
        let mut input = reader(body, Direction::ClientToServer, *opcode);
        let result = match *opcode {
            SYNTHETIC_M4_C2S_LIST => RoomListRequest::decode(&mut input, &PROFILE).map(|_| ()),
            SYNTHETIC_M4_C2S_CREATE => RoomCreateRequest::decode(&mut input, &PROFILE).map(|_| ()),
            SYNTHETIC_M4_C2S_JOIN => RoomJoinRequest::decode(&mut input, &PROFILE).map(|_| ()),
            SYNTHETIC_M4_C2S_LEAVE => RoomLeaveRequest::decode(&mut input, &PROFILE).map(|_| ()),
            SYNTHETIC_M4_C2S_SETTINGS => {
                RoomSettingsRequest::decode(&mut input, &PROFILE).map(|_| ())
            }
            SYNTHETIC_M4_C2S_READY => RoomReadyRequest::decode(&mut input, &PROFILE).map(|_| ()),
            SYNTHETIC_M4_C2S_CHAT => RoomChatRequest::decode(&mut input, &PROFILE).map(|_| ()),
            SYNTHETIC_M4_C2S_KICK => RoomKickRequest::decode(&mut input, &PROFILE).map(|_| ()),
            _ => RoomStateRequest::decode(&mut input, &PROFILE).map(|_| ()),
        };
        assert!(
            result.is_err(),
            "opcode {opcode:#06x} accepted a trailing byte"
        );
    }
}

#[test]
fn command_result_and_membership_discriminants_are_fixed() {
    let commands = [
        RoomCommand::List,
        RoomCommand::Create,
        RoomCommand::Join,
        RoomCommand::Leave,
        RoomCommand::Settings,
        RoomCommand::Ready,
        RoomCommand::Chat,
        RoomCommand::Kick,
        RoomCommand::State,
    ];
    for (wire, command) in (0_u8..=8).zip(commands) {
        let packet = RoomCommandResultResponse {
            command,
            result: RoomCommandResult::Success,
        };
        assert_eq!(
            encoded(&packet).expect("command result"),
            [0x82, 0x7f, wire, 0]
        );
    }

    let results = [
        RoomCommandResult::Success,
        RoomCommandResult::QueueFull,
        RoomCommandResult::Closed,
        RoomCommandResult::AlreadyMember,
        RoomCommandResult::Full,
        RoomCommandResult::InvalidPassword,
        RoomCommandResult::NotMember,
        RoomCommandResult::NotOwner,
        RoomCommandResult::CannotKickSelf,
        RoomCommandResult::MemberNotFound,
        RoomCommandResult::CapacityBelowOccupancy,
        RoomCommandResult::MaxRooms,
        RoomCommandResult::RoomNotFound,
        RoomCommandResult::IdExhausted,
        RoomCommandResult::Timeout,
    ];
    for (wire, result) in (0_u8..=14).zip(results) {
        let body = [RoomCommand::Create as u8, wire];
        let mut input = reader(
            &body,
            Direction::ServerToClient,
            SYNTHETIC_M4_S2C_COMMAND_RESULT,
        );
        assert_eq!(
            RoomCommandResultResponse::decode(&mut input, &PROFILE)
                .expect("fixed command result")
                .result,
            result
        );
    }

    for (wire, kind) in [
        RoomMembershipKind::Joined,
        RoomMembershipKind::Left,
        RoomMembershipKind::Kicked,
        RoomMembershipKind::OwnerChanged,
    ]
    .into_iter()
    .enumerate()
    {
        let event = RoomMembershipEvent {
            room_id: room_id(7),
            kind,
            member: member(1, 1, "Alice", true, false),
        };
        assert_eq!(encoded(&event).expect("membership event")[6], wire as u8);
    }
}

#[test]
fn registry_distinguishes_wrong_state_from_unknown_opcode() {
    let registry = synthetic_m4_registry(PROFILE.version());
    assert_eq!(registry.len(), 9);
    let list = RegistryKey {
        service: ServiceKind::Game,
        direction: Direction::ClientToServer,
        version: PROFILE.version(),
        state: ConnectionState::InChannel,
        opcode: SYNTHETIC_M4_C2S_LIST,
    };
    for opcode in [
        SYNTHETIC_M4_C2S_LIST,
        SYNTHETIC_M4_C2S_CREATE,
        SYNTHETIC_M4_C2S_JOIN,
    ] {
        let key = RegistryKey { opcode, ..list };
        assert_eq!(registry.classify(key), RegistryLookup::Accepted);
        assert_eq!(
            registry.classify(RegistryKey {
                state: ConnectionState::InRoom,
                ..key
            }),
            RegistryLookup::InvalidState
        );
    }
    assert_eq!(
        registry.classify(RegistryKey {
            opcode: 0x7f7f,
            ..list
        }),
        RegistryLookup::Unknown
    );
    for opcode in [
        SYNTHETIC_M4_C2S_LEAVE,
        SYNTHETIC_M4_C2S_SETTINGS,
        SYNTHETIC_M4_C2S_READY,
        SYNTHETIC_M4_C2S_CHAT,
        SYNTHETIC_M4_C2S_KICK,
        SYNTHETIC_M4_C2S_STATE,
    ] {
        let key = RegistryKey {
            state: ConnectionState::InRoom,
            opcode,
            ..list
        };
        assert_eq!(registry.classify(key), RegistryLookup::Accepted);
        assert_eq!(
            registry.classify(RegistryKey {
                state: ConnectionState::InChannel,
                ..key
            }),
            RegistryLookup::InvalidState
        );
    }
}

#[test]
fn state_encoder_rejects_inconsistent_public_snapshot() {
    let inconsistent = RoomStateResponse {
        room: RoomSnapshot::new(
            summary(7, "Room", "Alice", 2, 4, false),
            vec![member(1, 1, "Alice", true, false)],
        ),
    };
    assert!(encoded(&inconsistent).is_err());
}

proptest! {
    #[test]
    fn arbitrary_m4_inbound_body_never_panics_or_overreads(
        opcode_index in 0_usize..9,
        data in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        let opcodes = [
            SYNTHETIC_M4_C2S_LIST,
            SYNTHETIC_M4_C2S_CREATE,
            SYNTHETIC_M4_C2S_JOIN,
            SYNTHETIC_M4_C2S_LEAVE,
            SYNTHETIC_M4_C2S_SETTINGS,
            SYNTHETIC_M4_C2S_READY,
            SYNTHETIC_M4_C2S_CHAT,
            SYNTHETIC_M4_C2S_KICK,
            SYNTHETIC_M4_C2S_STATE,
        ];
        let opcode = opcodes[opcode_index];
        let mut input = reader(&data, Direction::ClientToServer, opcode);
        match opcode {
            SYNTHETIC_M4_C2S_LIST => { let _ = RoomListRequest::decode(&mut input, &PROFILE); }
            SYNTHETIC_M4_C2S_CREATE => { let _ = RoomCreateRequest::decode(&mut input, &PROFILE); }
            SYNTHETIC_M4_C2S_JOIN => { let _ = RoomJoinRequest::decode(&mut input, &PROFILE); }
            SYNTHETIC_M4_C2S_LEAVE => { let _ = RoomLeaveRequest::decode(&mut input, &PROFILE); }
            SYNTHETIC_M4_C2S_SETTINGS => { let _ = RoomSettingsRequest::decode(&mut input, &PROFILE); }
            SYNTHETIC_M4_C2S_READY => { let _ = RoomReadyRequest::decode(&mut input, &PROFILE); }
            SYNTHETIC_M4_C2S_CHAT => { let _ = RoomChatRequest::decode(&mut input, &PROFILE); }
            SYNTHETIC_M4_C2S_KICK => { let _ = RoomKickRequest::decode(&mut input, &PROFILE); }
            _ => { let _ = RoomStateRequest::decode(&mut input, &PROFILE); }
        }
        prop_assert!(input.offset() <= data.len());
        prop_assert_eq!(input.offset() + input.remaining(), data.len());
    }
}
