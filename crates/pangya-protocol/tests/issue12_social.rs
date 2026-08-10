use pangya_protocol::{
    decode_packet_payload, encode_packet_payload, CompatibilityProfile,
    GameChat, GameChatResponse, LoungeAction, LoungeActionResponse, MacroUpdate, TypingIndicator,
    UserInfoRequest, UserInfoResponse, Whisper, WhisperResponse,
};

const PROFILE: CompatibilityProfile = CompatibilityProfile::US_852;

#[test]
fn retail_social_request_layouts_match_packetdoc() {
    let chat = GameChat::new(b"Pangya".to_vec(), b"hello lobby".to_vec());
    let encoded = encode_packet_payload(&chat, &PROFILE).expect("chat encoding");
    assert_eq!(encoded.get(..4), Some(&[0, 0, 0, 0][..]));
    assert_eq!(decode_packet_payload::<GameChat>(&encoded, &PROFILE, pangya_protocol::ServiceKind::Game).expect("chat decoding"), chat);

    let whisper = Whisper::new(b"friend".to_vec(), b"hello".to_vec());
    let encoded = encode_packet_payload(&whisper, &PROFILE).expect("whisper encoding");
    assert_eq!(decode_packet_payload::<Whisper>(&encoded, &PROFILE, pangya_protocol::ServiceKind::Game).expect("whisper decoding"), whisper);

    let typing = TypingIndicator { typing: true };
    assert_eq!(encode_packet_payload(&typing, &PROFILE).expect("typing encoding").as_slice(), [1, 0]);
    assert_eq!(encode_packet_payload(&TypingIndicator { typing: false }, &PROFILE).expect("typing stop encoding").as_slice(), [255, 255]);
}

#[test]
fn lounge_actions_preserve_packetdoc_subbyte_and_payload() {
    let action = LoungeAction::emote(b"chat_dance".to_vec());
    let encoded = encode_packet_payload(&action, &PROFILE).expect("action encoding");
    assert_eq!(encoded[0], 7);
    assert_eq!(decode_packet_payload::<LoungeAction>(&encoded, &PROFILE, pangya_protocol::ServiceKind::Game).expect("action decoding"), action);

    let response = LoungeActionResponse::new(42, encoded);
    assert_eq!(decode_packet_payload::<LoungeActionResponse>(&encode_packet_payload(&response, &PROFILE).expect("response encoding"), &PROFILE, pangya_protocol::ServiceKind::Game).expect("response decoding"), response);
}

#[test]
fn macros_are_nine_fixed_64_byte_slots_and_user_info_is_typed() {
    let macros = MacroUpdate::new(std::array::from_fn(|i| format!("macro-{i}").into_bytes()));
    let encoded = encode_packet_payload(&macros, &PROFILE).expect("macro encoding");
    assert_eq!(encoded.len(), 9 * 64);
    assert_eq!(&encoded[7 * 64..7 * 64 + 7], b"macro-7");
    assert_eq!(decode_packet_payload::<MacroUpdate>(&encoded, &PROFILE, pangya_protocol::ServiceKind::Game).expect("macro decoding"), macros);

    let request = UserInfoRequest { user_id: 77, request_type: 5 };
    let encoded = encode_packet_payload(&request, &PROFILE).expect("info encoding");
    assert_eq!(encoded.as_slice(), [77, 0, 0, 0, 5]);
    assert_eq!(decode_packet_payload::<UserInfoRequest>(&encoded, &PROFILE, pangya_protocol::ServiceKind::Game).expect("info decoding"), request);

    let response = UserInfoResponse { status: 1, request_type: 5, user_id: 77 };
    assert_eq!(encode_packet_payload(&response, &PROFILE).expect("info response encoding").as_slice(), [1, 0, 0, 0, 5, 77, 0, 0, 0]);
}

#[test]
fn response_opcodes_are_retail() {
    assert_eq!(GameChatResponse::OPCODE, 0x0040);
    assert_eq!(WhisperResponse::OPCODE, 0x0084);
    assert_eq!(LoungeActionResponse::OPCODE, 0x00c4);
}
