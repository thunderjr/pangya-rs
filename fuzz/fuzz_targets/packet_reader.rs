#![no_main]

use libfuzzer_sys::fuzz_target;
use pangya_protocol::{
    BalanceUpdate, CheckNickname, CompatibilityProfile, DecodePacket, Direction, FinishHole,
    HoleResult, LoadingComplete, LoginRequest, MatchAborted, MatchPhase, MatchStarted,
    PacketReader, RoomChatRequest, RoomCreateRequest, RoomJoinRequest, RoomKickRequest,
    RoomLeaveRequest, RoomListRequest, RoomReadyRequest, RoomSettingsRequest, RoomStateRequest,
    SYNTHETIC_M4_C2S_CHAT, SYNTHETIC_M4_C2S_CREATE, SYNTHETIC_M4_C2S_JOIN, SYNTHETIC_M4_C2S_KICK,
    SYNTHETIC_M4_C2S_LEAVE, SYNTHETIC_M4_C2S_LIST, SYNTHETIC_M4_C2S_READY,
    SYNTHETIC_M4_C2S_SETTINGS, SYNTHETIC_M4_C2S_STATE, SYNTHETIC_M5_C2S_FINISH_HOLE,
    SYNTHETIC_M5_C2S_LOADING_COMPLETE, SYNTHETIC_M5_C2S_SHOT_ACTION, SYNTHETIC_M5_C2S_SHOT_RESULT,
    SYNTHETIC_M5_C2S_START_SOLO, SYNTHETIC_M5_S2C_BALANCE_UPDATE, SYNTHETIC_M5_S2C_COMMAND_RESULT,
    SYNTHETIC_M5_S2C_HOLE_RESULT, SYNTHETIC_M5_S2C_MATCH_ABORTED, SYNTHETIC_M5_S2C_MATCH_PHASE,
    SYNTHETIC_M5_S2C_MATCH_STARTED, SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY,
    SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY, SelectCharacter, SelectServer, ServiceKind, SetNickname,
    ShotAction, ShotActionRelay, ShotResult, ShotResultRelay, SoloCommandResult, StartSolo,
};

fuzz_target!(|data: &[u8]| {
    let Some((&low, rest)) = data.split_first() else {
        return;
    };
    let Some((&high, payload)) = rest.split_first() else {
        return;
    };
    let opcode = u16::from_le_bytes([low, high]);
    let is_m4 = (SYNTHETIC_M4_C2S_LIST..=SYNTHETIC_M4_C2S_STATE).contains(&opcode);
    let is_m5_c2s = (SYNTHETIC_M5_C2S_START_SOLO..=SYNTHETIC_M5_C2S_FINISH_HOLE).contains(&opcode);
    let is_m5_s2c =
        (SYNTHETIC_M5_S2C_MATCH_STARTED..=SYNTHETIC_M5_S2C_MATCH_ABORTED).contains(&opcode);
    let service = if is_m4 || is_m5_c2s || is_m5_s2c {
        ServiceKind::Game
    } else {
        ServiceKind::Login
    };
    let direction = if is_m5_s2c {
        Direction::ServerToClient
    } else {
        Direction::ClientToServer
    };
    let mut reader = PacketReader::new(payload, direction, service, Some(opcode));
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
        SYNTHETIC_M5_C2S_START_SOLO => {
            let _ = StartSolo::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_C2S_LOADING_COMPLETE => {
            let _ = LoadingComplete::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_C2S_SHOT_ACTION => {
            let _ = ShotAction::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_C2S_SHOT_RESULT => {
            let _ = ShotResult::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_C2S_FINISH_HOLE => {
            let _ = FinishHole::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_S2C_MATCH_STARTED => {
            let _ = MatchStarted::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_S2C_MATCH_PHASE => {
            let _ = MatchPhase::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY => {
            let _ = ShotActionRelay::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY => {
            let _ = ShotResultRelay::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_S2C_HOLE_RESULT => {
            let _ = HoleResult::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_S2C_BALANCE_UPDATE => {
            let _ = BalanceUpdate::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_S2C_COMMAND_RESULT => {
            let _ = SoloCommandResult::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M5_S2C_MATCH_ABORTED => {
            let _ = MatchAborted::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        _ => {
            let _ = reader.pstring(4096);
        }
    }
});
