//! Fixture-driven LoginService model layout tests.

use pangya_protocol::{
    ChatMacros, CheckNickname, CompatibilityProfile, DecodePacket, Direction,
    EmptyMessageServerList, EncodePacket, GameServerEntry, GameServerList,
    LOGIN_ERROR_DUPLICATE_CONNECTION, LOGIN_ERROR_INVALID_CREDENTIALS, LOGIN_STATUS_SET_CHARACTER,
    LOGIN_STATUS_SET_NICKNAME, LoginKey, LoginRequest, LoginResult, LoginSuccess,
    NicknameCheckResult, PacketReader, PacketWriter, SelectCharacter, SelectServer, ServiceKind,
    SessionKey, SetNickname, UnknownBytes,
};

const PROFILE: CompatibilityProfile = CompatibilityProfile::US_852;

fn reader(bytes: &[u8], opcode: u16) -> PacketReader<'_> {
    PacketReader::new(
        bytes,
        Direction::ClientToServer,
        ServiceKind::Login,
        Some(opcode),
    )
}

fn encoded<T: EncodePacket>(packet: &T) -> Vec<u8> {
    let mut writer = PacketWriter::new();
    writer.u16_le(T::OPCODE);
    packet
        .encode(&mut writer, &PROFILE)
        .expect("fixture encodes");
    writer.into_inner()
}

fn without_nul(field: &[u8]) -> Vec<u8> {
    field[..field
        .iter()
        .position(|byte| *byte == 0)
        .expect("fixture NUL")]
        .to_vec()
}

#[test]
fn inbound_login_layout_fixtures_decode_exactly() {
    let login = include_bytes!("fixtures/login-in-0001/fixture.bin");
    let mut r = reader(&login[2..], 1);
    let packet = LoginRequest::decode(&mut r, &PROFILE).expect("login");
    assert_eq!(packet.username, b"john");
    assert_eq!(packet.password, b"098F6BCD4621D373CADE4E832627B4F6");
    assert_eq!(packet.unknown_tail, vec![0; 17]);

    let select = include_bytes!("fixtures/login-in-0003/fixture.bin");
    let mut r = reader(&select[2..], 3);
    assert_eq!(
        SelectServer::decode(&mut r, &PROFILE).expect("select"),
        SelectServer {
            server_id: 0x4eea,
            unknown: UnknownBytes([0, 0]),
        }
    );

    let set = include_bytes!("fixtures/login-in-0006/fixture.bin");
    let mut r = reader(&set[2..], 6);
    assert_eq!(
        SetNickname::decode(&mut r, &PROFILE).expect("set").nickname,
        b"pangbox"
    );
    let check = include_bytes!("fixtures/login-in-0007/fixture.bin");
    let mut r = reader(&check[2..], 7);
    assert_eq!(
        CheckNickname::decode(&mut r, &PROFILE)
            .expect("check")
            .nickname,
        b"pangbox"
    );

    let character = include_bytes!("fixtures/login-in-0008/fixture.bin");
    let mut r = reader(&character[2..], 8);
    assert_eq!(
        SelectCharacter::decode(&mut r, &PROFILE).expect("character"),
        SelectCharacter {
            character_id: 0x0400_0009,
            hair_color: 2,
        }
    );
}

#[test]
fn outbound_login_result_variant_fixtures_encode_exactly() {
    let success_fixture = include_bytes!("fixtures/login-out-0001-success/fixture.bin");
    let success = LoginResult::Success(LoginSuccess {
        username: b"pangbox".to_vec(),
        user_id: u32::from_le_bytes(success_fixture[12..16].try_into().expect("u32")),
        unknown: UnknownBytes(success_fixture[16..30].try_into().expect("unknown")),
        nickname: Vec::new(),
    });
    assert_eq!(encoded(&success), success_fixture);
    // These two fixtures are TH captures, and TH numbers both setup statuses one higher than
    // U.S. 852 does. The captures are still the layout evidence — status byte, and for the
    // nickname status a trailing `0xffff_ffff` — so assert the layout against them while encoding
    // the U.S. status byte. See `LOGIN_STATUS_SET_NICKNAME` for the divergence and its evidence.
    let th_nickname = include_bytes!("fixtures/login-out-0001-need-nickname/fixture.bin");
    let th_character = include_bytes!("fixtures/login-out-0001-need-character/fixture.bin");
    assert_eq!(th_nickname[2], LOGIN_STATUS_SET_NICKNAME + 1);
    assert_eq!(th_character[2], LOGIN_STATUS_SET_CHARACTER + 1);
    let mut us_nickname = th_nickname.to_vec();
    us_nickname[2] = LOGIN_STATUS_SET_NICKNAME;
    let mut us_character = th_character.to_vec();
    us_character[2] = LOGIN_STATUS_SET_CHARACTER;
    assert_eq!(encoded(&LoginResult::NeedSetNickname), us_nickname);
    assert_eq!(encoded(&LoginResult::NeedSelectCharacter), us_character);
    assert_eq!(
        encoded(&LoginResult::Error(LOGIN_ERROR_INVALID_CREDENTIALS)),
        include_bytes!("fixtures/login-out-0001-error/fixture.bin")
    );
    assert_eq!(
        encoded(&LoginResult::Error(LOGIN_ERROR_DUPLICATE_CONNECTION)),
        include_bytes!("fixtures/login-out-0001-duplicate/fixture.bin")
    );
}

#[test]
fn outbound_server_list_captured_fixture_encodes_exactly() {
    let fixture = include_bytes!("fixtures/login-out-0002/fixture.bin");
    let count = usize::from(fixture[2]);
    let mut servers = Vec::with_capacity(count);
    for field in fixture[3..].chunks_exact(92) {
        servers.push(GameServerEntry {
            name: without_nul(&field[0..40]),
            id: u32::from_le_bytes(field[40..44].try_into().expect("id")),
            max_users: u32::from_le_bytes(field[44..48].try_into().expect("max")),
            num_users: u32::from_le_bytes(field[48..52].try_into().expect("users")),
            ip_address: without_nul(&field[52..70]),
            port: u16::from_le_bytes(field[70..72].try_into().expect("port")),
            unknown2: UnknownBytes(field[72..74].try_into().expect("unknown2")),
            flags: UnknownBytes(field[74..76].try_into().expect("flags")),
            unknown3: UnknownBytes(field[76..82].try_into().expect("unknown3")),
            boosts: u16::from_le_bytes(field[82..84].try_into().expect("boosts")),
            unknown4: UnknownBytes(field[84..90].try_into().expect("unknown4")),
            char_icon: u16::from_le_bytes(field[90..92].try_into().expect("icon")),
            channels: Vec::new(),
        });
    }
    assert_eq!(servers.len(), count);
    // The TH capture's entries are exactly 92 bytes and stop at `char_icon`. U.S. 852 appends a
    // channel-count byte per entry, so an entry with no channels adds a single zero. Splicing
    // those zeroes into the capture keeps it as the layout evidence for everything before them.
    // See `GameServerEntry::channels` for why the U.S. client needs the trailer.
    assert_eq!(fixture.len() - 3, count * 92);
    let encoded = encoded(&GameServerList { servers });
    let mut normalized_fixture = Vec::with_capacity(fixture.len() + count);
    normalized_fixture.extend_from_slice(&fixture[..3]);
    for field in fixture[3..].chunks_exact(92) {
        normalized_fixture.extend_from_slice(field);
        normalized_fixture.push(0);
    }
    for field in normalized_fixture[3..].chunks_exact_mut(93) {
        for range in [0..40, 52..70] {
            let nul = field[range.clone()]
                .iter()
                .position(|byte| *byte == 0)
                .expect("fixture NUL");
            field[range.start + nul..range.end].fill(0);
        }
    }
    // Captured fixed-width strings contain ignored bytes after their first NUL;
    // the encoder intentionally canonicalizes that padding to zero.
    assert_eq!(encoded, normalized_fixture);
}

#[test]
fn synthetic_layout_derivative_fixtures_encode_exactly() {
    assert_eq!(
        encoded(&NicknameCheckResult {
            unknown_result: 0,
            nickname: b"synthetic".to_vec(),
        }),
        include_bytes!("fixtures/login-out-000e/fixture.bin")
    );
    assert_eq!(
        encoded(&SessionKey {
            unknown: UnknownBytes([0x11, 0x22, 0x33, 0x44]),
            session_key: b"synthetic-session".to_vec(),
        }),
        include_bytes!("fixtures/login-out-0003/fixture.bin")
    );
    assert_eq!(
        encoded(&SessionKey {
            unknown: UnknownBytes([0; 4]),
            session_key: [
                b"00000000-0000-4000-8000-000000000000.".as_slice(),
                &[b'A'; 43],
            ]
            .concat(),
        }),
        include_bytes!("fixtures/login-out-0003-handover/fixture.bin")
    );
    assert_eq!(
        encoded(&ChatMacros {
            values: std::array::from_fn(|index| format!("macro-{index}").into_bytes()),
        }),
        include_bytes!("fixtures/login-out-0006/fixture.bin")
    );
    assert_eq!(
        encoded(&EmptyMessageServerList),
        include_bytes!("fixtures/login-out-0009/fixture.bin")
    );
    assert_eq!(
        encoded(&LoginKey {
            login_key: b"synthetic-login-key".to_vec(),
        }),
        include_bytes!("fixtures/login-out-0010/fixture.bin")
    );
}

#[test]
fn login_models_reject_truncation_and_wire_overflow() {
    let mut r = reader(&[5, 0, b'a'], 1);
    assert!(LoginRequest::decode(&mut r, &PROFILE).is_err());
    let mut r = reader(&[1, 0, 0], 3);
    assert!(SelectServer::decode(&mut r, &PROFILE).is_err());
    let mut r = reader(&[2, 0, b'a'], 6);
    assert!(SetNickname::decode(&mut r, &PROFILE).is_err());
    let mut r = reader(&[2, 0, b'a'], 7);
    assert!(CheckNickname::decode(&mut r, &PROFILE).is_err());
    let mut r = reader(&[0; 5], 8);
    assert!(SelectCharacter::decode(&mut r, &PROFILE).is_err());

    assert!(
        encoded_result(&SessionKey {
            unknown: UnknownBytes([0; 4]),
            session_key: vec![0; 129],
        })
        .is_err()
    );
    assert!(
        encoded_result(&LoginKey {
            login_key: vec![0; 129]
        })
        .is_err()
    );
    assert!(
        encoded_result(&GameServerList {
            servers: vec![sample_server(); 256],
        })
        .is_err()
    );
    assert!(
        encoded_result(&ChatMacros {
            values: std::array::from_fn(|_| vec![b'x'; 64]),
        })
        .is_err()
    );
}

fn encoded_result<T: EncodePacket>(
    packet: &T,
) -> Result<Vec<u8>, pangya_protocol::PacketEncodeError> {
    let mut writer = PacketWriter::new();
    writer.u16_le(T::OPCODE);
    packet.encode(&mut writer, &PROFILE)?;
    Ok(writer.into_inner())
}

fn sample_server() -> GameServerEntry {
    GameServerEntry {
        name: b"server".to_vec(),
        id: 1,
        max_users: 100,
        num_users: 1,
        ip_address: b"127.0.0.1".to_vec(),
        port: 10_103,
        unknown2: UnknownBytes([0; 2]),
        flags: UnknownBytes([0; 2]),
        unknown3: UnknownBytes([0; 6]),
        boosts: 0,
        unknown4: UnknownBytes([0; 6]),
        char_icon: 0,
        channels: Vec::new(),
    }
}
