//! Reference-derived regression tests for LoginService ghost/reconnect/refusal behavior.
use pangya_protocol::{
    CompatibilityProfile, DecodePacket, Direction, EncodePacket, GhostLogin, LoginResult,
    PacketReader, PacketWriter, ReconnectRequest, ServiceKind,
    LOGIN_ERROR_ALREADY_LOGGED_IN, LOGIN_ERROR_DUPLICATE_CONNECTION,
    LOGIN_ERROR_INVALID_CREDENTIALS, LOGIN_ERROR_INVALID_RECONNECT_TOKEN,
};

const PROFILE: CompatibilityProfile = CompatibilityProfile::US_852;

fn reader(bytes: &[u8], opcode: u16) -> PacketReader<'_> {
    PacketReader::new(bytes, Direction::ClientToServer, ServiceKind::Login, Some(opcode))
}

#[test]
fn ghost_is_the_empty_reference_packet() {
    let mut reader = reader(&[], 0x0004);
    assert_eq!(GhostLogin::decode(&mut reader, &PROFILE).expect("ghost"), GhostLogin);
    assert_eq!(GhostLogin::OPCODE, 0x0004);
}

#[test]
fn reconnect_decodes_username_uid_and_secret_without_exposing_secret() {
    let mut writer = PacketWriter::new();
    writer.pstring(b"Alice", 64).expect("username");
    writer.u32_le(42);
    writer.pstring(b"session-token", 128).expect("token");
    let packet = ReconnectRequest::decode(&mut reader(&writer.into_inner(), 0x000b), &PROFILE)
        .expect("reconnect");
    assert_eq!(packet.username, b"Alice");
    assert_eq!(packet.user_id, 42);
    assert_eq!(packet.session_token, b"session-token");
    assert!(!format!("{packet:?}").contains("session-token"));
}

#[test]
fn reconnect_trailing_bytes_and_oversized_token_are_rejected() {
    let mut bytes = vec![0; 2];
    bytes.extend_from_slice(&42_u32.to_le_bytes());
    bytes.extend_from_slice(&129_u16.to_le_bytes());
    bytes.extend_from_slice(&[b'x'; 129]);
    assert!(ReconnectRequest::decode(&mut reader(&bytes, 0x000b), &PROFILE).is_err());
    assert!(GhostLogin::decode(&mut reader(&[1], 0x0004), &PROFILE).is_err());
}

#[test]
fn refusal_codes_follow_reference_error_enum() {
    assert_eq!(LOGIN_ERROR_INVALID_CREDENTIALS, 5_100_143);
    assert_eq!(LOGIN_ERROR_ALREADY_LOGGED_IN, 5_100_019);
    assert_eq!(LOGIN_ERROR_DUPLICATE_CONNECTION, 5_100_107);
    assert_eq!(LOGIN_ERROR_INVALID_RECONNECT_TOKEN, 5_157_002);
    let mut writer = PacketWriter::new();
    LoginResult::Error(LOGIN_ERROR_INVALID_RECONNECT_TOKEN)
        .encode(&mut writer, &PROFILE)
        .expect("error packet");
    assert_eq!(writer.into_inner(), [0xe3, 0x4a, 0x9e, 0x4e, 0x00]);
}
