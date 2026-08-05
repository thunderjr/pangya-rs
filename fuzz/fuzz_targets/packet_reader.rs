#![no_main]

use libfuzzer_sys::fuzz_target;
use pangya_protocol::{
    CheckNickname, CompatibilityProfile, DecodePacket, Direction, LoginRequest, PacketReader,
    RoomChatRequest, RoomCreateRequest, RoomJoinRequest, RoomKickRequest, RoomLeaveRequest,
    RoomListRequest, RoomReadyRequest, RoomSettingsRequest, RoomStateRequest,
    SYNTHETIC_M4_C2S_CHAT, SYNTHETIC_M4_C2S_CREATE, SYNTHETIC_M4_C2S_JOIN,
    SYNTHETIC_M4_C2S_KICK, SYNTHETIC_M4_C2S_LEAVE, SYNTHETIC_M4_C2S_LIST,
    SYNTHETIC_M4_C2S_READY, SYNTHETIC_M4_C2S_SETTINGS, SYNTHETIC_M4_C2S_STATE,
    SelectCharacter, SelectServer, ServiceKind, SetNickname,
};

fuzz_target!(|data: &[u8]| {
    let Some((&low, rest)) = data.split_first() else {
        return;
    };
    let Some((&high, payload)) = rest.split_first() else {
        return;
    };
    let opcode = u16::from_le_bytes([low, high]);
    let service = if (SYNTHETIC_M4_C2S_LIST..=SYNTHETIC_M4_C2S_STATE).contains(&opcode) {
        ServiceKind::Game
    } else {
        ServiceKind::Login
    };
    let mut reader = PacketReader::new(payload, Direction::ClientToServer, service, Some(opcode));
    match opcode {
        LoginRequest::OPCODE => {
            let _ = LoginRequest::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SelectServer::OPCODE => {
            let _ = SelectServer::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SetNickname::OPCODE => {
            let _ = SetNickname::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        CheckNickname::OPCODE => {
            let _ = CheckNickname::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SelectCharacter::OPCODE => {
            let _ = SelectCharacter::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M4_C2S_LIST => {
            let _ = RoomListRequest::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M4_C2S_CREATE => {
            let _ = RoomCreateRequest::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M4_C2S_JOIN => {
            let _ = RoomJoinRequest::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M4_C2S_LEAVE => {
            let _ = RoomLeaveRequest::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M4_C2S_SETTINGS => {
            let _ = RoomSettingsRequest::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M4_C2S_READY => {
            let _ = RoomReadyRequest::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M4_C2S_CHAT => {
            let _ = RoomChatRequest::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M4_C2S_KICK => {
            let _ = RoomKickRequest::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M4_C2S_STATE => {
            let _ = RoomStateRequest::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        _ => {
            let _ = reader.pstring(4096);
        }
    }
});
