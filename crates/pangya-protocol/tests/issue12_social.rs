#![allow(missing_docs)]

use pangya_protocol::{
    CompatibilityProfile, EncodePacket, GameChat, GameChatResponse, LoungeAction,
    LoungeActionResponse, MacroUpdate, MessageServerList, MessageServerListRequest, NoteSend,
    RetailPangBalance, TypingIndicator, UserCharacterInfoResponse, UserCourseRecordsInfoResponse,
    UserEquipmentInfoResponse, UserGrandPrixTrophiesInfoResponse, UserGuildInfoResponse,
    UserInfoRequest, UserInfoResponse, UserNameInfoResponse, UserRelatedInfoResponse,
    UserSpecialTrophiesInfoResponse, UserStatisticsInfoResponse, UserStatusRequest,
    UserStatusResponse, UserTrophiesInfoResponse, Whisper, WhisperRefusalResponse, WhisperResponse,
    decode_packet_payload, encode_packet_payload,
};

const PROFILE: CompatibilityProfile = CompatibilityProfile::US_852;

#[test]
fn retail_social_request_layouts_match_packetdoc() {
    let chat = GameChat::new(b"Pangya".to_vec(), b"hello lobby".to_vec());
    let encoded = encode_packet_payload(&chat, &PROFILE).expect("chat encoding");
    assert_eq!(encoded.get(..4), Some(&[0, 0, 0, 0][..]));
    assert_eq!(
        decode_packet_payload::<GameChat>(&encoded, &PROFILE, pangya_protocol::ServiceKind::Game)
            .expect("chat decoding"),
        chat
    );

    let whisper = Whisper::new(b"friend".to_vec(), b"hello".to_vec());
    let encoded = encode_packet_payload(&whisper, &PROFILE).expect("whisper encoding");
    assert_eq!(
        decode_packet_payload::<Whisper>(&encoded, &PROFILE, pangya_protocol::ServiceKind::Game)
            .expect("whisper decoding"),
        whisper
    );

    let typing = TypingIndicator { typing: true };
    assert_eq!(
        encode_packet_payload(&typing, &PROFILE)
            .expect("typing encoding")
            .as_slice(),
        [1, 0]
    );
    assert_eq!(
        encode_packet_payload(&TypingIndicator { typing: false }, &PROFILE)
            .expect("typing stop encoding")
            .as_slice(),
        [255, 255]
    );
}

#[test]
fn lounge_actions_preserve_packetdoc_subbyte_and_payload() {
    let action = LoungeAction::emote(b"chat_dance".to_vec());
    let encoded = encode_packet_payload(&action, &PROFILE).expect("action encoding");
    assert_eq!(encoded[0], 7);
    assert_eq!(
        decode_packet_payload::<LoungeAction>(
            &encoded,
            &PROFILE,
            pangya_protocol::ServiceKind::Game
        )
        .expect("action decoding"),
        action
    );

    let response = LoungeActionResponse::new(42, encoded.to_vec());
    assert_eq!(
        decode_packet_payload::<LoungeActionResponse>(
            &encode_packet_payload(&response, &PROFILE).expect("response encoding"),
            &PROFILE,
            pangya_protocol::ServiceKind::Game
        )
        .expect("response decoding"),
        response
    );
}

#[test]
fn macros_are_nine_fixed_64_byte_slots_and_user_info_is_typed() {
    let macros = MacroUpdate::new(std::array::from_fn(|i| format!("macro-{i}").into_bytes()));
    let encoded = encode_packet_payload(&macros, &PROFILE).expect("macro encoding");
    assert_eq!(encoded.len(), 9 * 64);
    assert_eq!(&encoded[7 * 64..7 * 64 + 7], b"macro-7");
    assert_eq!(
        decode_packet_payload::<MacroUpdate>(
            &encoded,
            &PROFILE,
            pangya_protocol::ServiceKind::Game
        )
        .expect("macro decoding"),
        macros
    );

    let request = UserInfoRequest {
        user_id: 77,
        request_type: 5,
    };
    let encoded = encode_packet_payload(&request, &PROFILE).expect("info encoding");
    assert_eq!(encoded.as_slice(), [77, 0, 0, 0, 5]);
    assert_eq!(
        decode_packet_payload::<UserInfoRequest>(
            &encoded,
            &PROFILE,
            pangya_protocol::ServiceKind::Game
        )
        .expect("info decoding"),
        request
    );

    let response = UserInfoResponse {
        status: 1,
        request_type: 5,
        user_id: 77,
    };
    assert_eq!(
        encode_packet_payload(&response, &PROFILE)
            .expect("info response encoding")
            .as_slice(),
        [1, 0, 0, 0, 5, 77, 0, 0, 0]
    );
    let zero_type = UserInfoResponse {
        request_type: 0,
        ..response
    };
    assert_eq!(
        encode_packet_payload(&zero_type, &PROFILE)
            .expect("zero request type")
            .as_slice(),
        [1, 0, 0, 0, 0, 77, 0, 0, 0]
    );
}

#[test]
fn whisper_response_uses_literal_packetdoc_body_and_opcode() {
    let response = WhisperResponse {
        status: 0,
        nickname: b"Alice".to_vec(),
        message: b"hello".to_vec(),
    };
    assert_eq!(WhisperResponse::OPCODE, 0x0084);
    assert_eq!(
        encode_packet_payload(&response, &PROFILE)
            .expect("whisper response")
            .as_slice(),
        [
            0, 5, 0, b'A', b'l', b'i', b'c', b'e', 5, 0, b'h', b'e', b'l', b'l', b'o'
        ]
    );
}

#[test]
fn whisper_refusal_statuses_are_encoded_without_transport_failure() {
    for status in [4, 5] {
        let packet = WhisperRefusalResponse {
            status,
            nickname: b"target".to_vec(),
        };
        let encoded = encode_packet_payload(&packet, &PROFILE).expect("refusal encoding");
        assert_eq!(encoded[0], status);
        assert_eq!(WhisperRefusalResponse::OPCODE, 0x0040);
    }
    assert!(
        encode_packet_payload(
            &WhisperResponse {
                status: 2,
                nickname: Vec::new(),
                message: Vec::new(),
            },
            &PROFILE,
        )
        .is_err()
    );
}

#[test]
fn all_user_info_fanout_packets_have_reference_bodies() {
    let course = UserCourseRecordsInfoResponse {
        request_type: 0x33,
        user_id: 7,
    };
    assert_eq!(
        encode_packet_payload(&course, &PROFILE)
            .expect("course")
            .len(),
        13
    );
    assert_eq!(UserGuildInfoResponse::OPCODE, 0x015d);
    assert_eq!(
        encode_packet_payload(&UserGuildInfoResponse { user_id: 7 }, &PROFILE)
            .expect("guild")
            .len(),
        301
    );
    assert_eq!(
        encode_packet_payload(
            &UserRelatedInfoResponse {
                request_type: 5,
                user_id: 7
            },
            &PROFILE
        )
        .expect("related")
        .len(),
        7
    );
    assert_eq!(
        encode_packet_payload(
            &UserSpecialTrophiesInfoResponse {
                request_type: 5,
                user_id: 7
            },
            &PROFILE
        )
        .expect("special trophies")
        .len(),
        7
    );
    assert_eq!(
        encode_packet_payload(
            &UserTrophiesInfoResponse {
                request_type: 5,
                user_id: 7
            },
            &PROFILE
        )
        .expect("trophies")
        .len(),
        1 + 4 + 13 * 6
    );
    assert_eq!(
        encode_packet_payload(
            &UserGrandPrixTrophiesInfoResponse {
                request_type: 5,
                user_id: 7
            },
            &PROFILE
        )
        .expect("gp trophies")
        .len(),
        7
    );
}

#[test]
fn note_and_message_server_fixtures_are_exact() {
    let note = NoteSend {
        subtype: 0x0111,
        user_id: 0x0102_0304,
        message: b"hello".to_vec(),
        unknown: 0,
    };
    let note_bytes = encode_packet_payload(&note, &PROFILE).expect("note encoding");
    assert_eq!(&note_bytes[..6], &[0x11, 0x01, 4, 3, 2, 1]);
    assert_eq!(&note_bytes[6..], &[5, 0, b'h', b'e', b'l', b'l', b'o', 0]);
    let request = decode_packet_payload::<MessageServerListRequest>(
        &[],
        &PROFILE,
        pangya_protocol::ServiceKind::Game,
    )
    .expect("message server request");
    assert_eq!(request, MessageServerListRequest);
    let list = encode_packet_payload(&MessageServerList, &PROFILE).expect("message server list");
    assert_eq!(list.as_slice(), [0]);
    let balance = encode_packet_payload(&RetailPangBalance { pang: 0x1122_3344 }, &PROFILE)
        .expect("money update");
    assert_eq!(
        balance.as_slice(),
        [0x11, 0x01, 0, 0, 0, 0, 0x44, 0x33, 0x22, 0x11, 0, 0, 0, 0]
    );
}

#[test]
fn social_requests_refuse_trailing_or_invalid_bodies() {
    assert!(
        decode_packet_payload::<MessageServerListRequest>(
            &[0],
            &PROFILE,
            pangya_protocol::ServiceKind::Game,
        )
        .is_err()
    );
    assert!(
        decode_packet_payload::<NoteSend>(
            &[0x11, 1, 1, 0, 0, 0, 0],
            &PROFILE,
            pangya_protocol::ServiceKind::Game,
        )
        .is_err()
    );
}

#[test]
fn user_status_and_message_server_replies_match_reference_golden_bytes() {
    let request = [1, 5, 0, b'A', b'l', b'i', b'c', b'e'];
    assert_eq!(
        decode_packet_payload::<UserStatusRequest>(
            &request,
            &PROFILE,
            pangya_protocol::ServiceKind::Game,
        )
        .expect("status request"),
        UserStatusRequest {
            unknown: 1,
            username: b"Alice".to_vec(),
        }
    );
    assert_eq!(UserStatusResponse::OPCODE, 0x00a1);
    assert_eq!(
        encode_packet_payload(&UserStatusResponse, &PROFILE)
            .expect("status response")
            .as_slice(),
        [2]
    );
    assert_eq!(MessageServerList::OPCODE, 0x00fc);
    assert_eq!(
        encode_packet_payload(&MessageServerList, &PROFILE)
            .expect("empty message list")
            .as_slice(),
        [0]
    );
}

#[test]
fn response_opcodes_are_retail() {
    assert_eq!(GameChatResponse::OPCODE, 0x0040);
    assert_eq!(WhisperResponse::OPCODE, 0x0084);
    assert_eq!(LoungeActionResponse::OPCODE, 0x00c4);
}

fn stats_request() -> UserStatisticsInfoResponse {
    UserStatisticsInfoResponse {
        request_type: 5,
        user_id: 7,
        experience: 123,
        pang: 456,
    }
}

#[test]
fn user_info_fanout_has_reference_widths_and_real_projection_fields() {
    let name = encode_packet_payload(
        &UserNameInfoResponse {
            request_type: 5,
            user_id: 7,
            username: b"account".to_vec(),
            nickname: b"Player".to_vec(),
        },
        &PROFILE,
    )
    .expect("name");
    assert_eq!(
        name.len(),
        1 + 4 + 2 + 22 + 22 + 21 + 24 + 4 + 12 + 4 + 4 + 2 + 6 + 16 + 128 + 4 + 4
    );
    let stats = encode_packet_payload(
        &UserStatisticsInfoResponse {
            request_type: 5,
            user_id: 7,
            experience: 123,
            pang: 456,
        },
        &PROFILE,
    )
    .expect("stats");
    assert_eq!(
        stats.len(),
        1 + 4 + pangya_protocol::PLAYER_STATISTICS_BYTES
    );
    assert_eq!(&stats[1 + 4 + 74..1 + 4 + 78], &123_u32.to_le_bytes());
    assert_eq!(&stats[1 + 4 + 79..1 + 4 + 83], &456_u32.to_le_bytes());
    assert_eq!(&stats[1 + 4 + 91..1 + 4 + 96], &[0x7f; 5]);
    assert!(stats[1 + 4 + 96..].iter().all(|byte| *byte == 0));
    assert!(
        encode_packet_payload(
            &UserStatisticsInfoResponse {
                pang: u64::from(u32::MAX) + 1,
                ..stats_request()
            },
            &PROFILE,
        )
        .is_err()
    );
    let equipment = encode_packet_payload(
        &UserEquipmentInfoResponse {
            request_type: 5,
            user_id: 7,
            character_uid: 8,
            comet_iff_id: 9,
        },
        &PROFILE,
    )
    .expect("equipment");
    assert_eq!(equipment.len(), 1 + 4 + 29 * 4);
    let character = encode_packet_payload(
        &UserCharacterInfoResponse {
            user_id: 7,
            character_iff_id: 8,
            character_uid: 9,
        },
        &PROFILE,
    )
    .expect("character");
    assert_eq!(character.len(), 4 + pangya_protocol::CHARACTER_BLOCK_BYTES);
}
