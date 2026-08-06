#![no_main]

use libfuzzer_sys::fuzz_target;
use pangya_protocol::{
    BalanceUpdate, CheckNickname, CompatibilityProfile, ConsumeOneRequest, DecodePacket,
    Direction, EconomyCommandResult, EquipRequest, EquipmentChanged, FinishHole, HoleResult,
    InventoryChanged, LoadingComplete, LoginRequest, MatchAborted, MatchPhase, MatchStarted,
    PacketReader, PurchaseCommitted, PurchaseRequestPacket, RepairCommitted, RepairRequest,
    RoomChatRequest, RoomCreateRequest, RoomJoinRequest, RoomKickRequest,
    RoomLeaveRequest, RoomListRequest, RoomReadyRequest, RoomSettingsRequest, RoomStateRequest,
    SYNTHETIC_M4_C2S_CHAT, SYNTHETIC_M4_C2S_CREATE, SYNTHETIC_M4_C2S_JOIN, SYNTHETIC_M4_C2S_KICK,
    SYNTHETIC_M4_C2S_LEAVE, SYNTHETIC_M4_C2S_LIST, SYNTHETIC_M4_C2S_READY,
    SYNTHETIC_M4_C2S_SETTINGS, SYNTHETIC_M4_C2S_STATE, SYNTHETIC_M5_C2S_FINISH_HOLE,
    SYNTHETIC_M5_C2S_LOADING_COMPLETE, SYNTHETIC_M5_C2S_SHOT_ACTION, SYNTHETIC_M5_C2S_SHOT_RESULT,
    SYNTHETIC_M5_C2S_START_SOLO, SYNTHETIC_M5_S2C_BALANCE_UPDATE, SYNTHETIC_M5_S2C_COMMAND_RESULT,
    SYNTHETIC_M5_S2C_HOLE_RESULT, SYNTHETIC_M5_S2C_MATCH_ABORTED, SYNTHETIC_M5_S2C_MATCH_PHASE,
    SYNTHETIC_M5_S2C_MATCH_STARTED, SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY,
    SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY, SYNTHETIC_M6_C2S_GIVE_UP,
    SYNTHETIC_M6_C2S_LOADING_COMPLETE, SYNTHETIC_M6_C2S_SHOT_ACTION,
    SYNTHETIC_M6_C2S_SHOT_RESULT, SYNTHETIC_M6_C2S_START_STROKE_TWO,
    SYNTHETIC_M6_S2C_ACTION_RELAY, SYNTHETIC_M6_S2C_BALANCE_UPDATE,
    SYNTHETIC_M6_S2C_COMMAND_RESULT, SYNTHETIC_M6_S2C_MATCH_ABORTED,
    SYNTHETIC_M6_S2C_MATCH_STARTED, SYNTHETIC_M6_S2C_PHASE, SYNTHETIC_M6_S2C_RESULT_RELAY,
    SYNTHETIC_M6_S2C_STANDINGS, SYNTHETIC_M6_S2C_TURN_STARTED, SYNTHETIC_M7_C2S_CONSUME,
    SYNTHETIC_M7_C2S_EQUIP, SYNTHETIC_M7_C2S_PURCHASE, SYNTHETIC_M7_C2S_REPAIR,
    SYNTHETIC_M7_C2S_SHOP_PAGE, SYNTHETIC_M7_S2C_COMMAND_RESULT,
    SYNTHETIC_M7_S2C_EQUIPMENT_CHANGED, SYNTHETIC_M7_S2C_INVENTORY_CHANGED,
    SYNTHETIC_M7_S2C_PURCHASE_COMMITTED, SYNTHETIC_M7_S2C_REPAIR_COMMITTED,
    SYNTHETIC_M7_S2C_SHOP_PAGE, SelectCharacter, SelectServer, ServiceKind, SetNickname,
    ShopPage, ShopPageRequest, ShotAction, ShotActionRelay, ShotResult, ShotResultRelay,
    SoloCommandResult, StartSolo, StartStrokeTwo, StrokeActionRelay, StrokeBalanceUpdate,
    StrokeCommandResult, StrokeGiveUp, StrokeLoadingComplete, StrokeMatchAborted,
    StrokeMatchStarted, StrokePhase, StrokeResultRelay, StrokeShotAction, StrokeShotResult,
    StrokeStandings, StrokeTurnStarted,
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
    let is_m6_c2s =
        (SYNTHETIC_M6_C2S_START_STROKE_TWO..=SYNTHETIC_M6_C2S_GIVE_UP).contains(&opcode);
    let is_m6_s2c =
        (SYNTHETIC_M6_S2C_MATCH_STARTED..=SYNTHETIC_M6_S2C_BALANCE_UPDATE).contains(&opcode);
    let is_m7_c2s = (SYNTHETIC_M7_C2S_SHOP_PAGE..=SYNTHETIC_M7_C2S_REPAIR).contains(&opcode);
    let is_m7_s2c =
        (SYNTHETIC_M7_S2C_SHOP_PAGE..=SYNTHETIC_M7_S2C_REPAIR_COMMITTED).contains(&opcode);
    let service = if is_m4 || is_m5_c2s || is_m5_s2c || is_m6_c2s || is_m6_s2c || is_m7_c2s || is_m7_s2c {
        ServiceKind::Game
    } else {
        ServiceKind::Login
    };
    let direction = if is_m5_s2c || is_m6_s2c || is_m7_s2c {
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
        SYNTHETIC_M6_C2S_START_STROKE_TWO => {
            let _ = StartStrokeTwo::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_C2S_LOADING_COMPLETE => {
            let _ = StrokeLoadingComplete::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_C2S_SHOT_ACTION => {
            let _ = StrokeShotAction::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_C2S_SHOT_RESULT => {
            let _ = StrokeShotResult::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_C2S_GIVE_UP => {
            let _ = StrokeGiveUp::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_S2C_MATCH_STARTED => {
            let _ = StrokeMatchStarted::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_S2C_PHASE => {
            let _ = StrokePhase::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_S2C_TURN_STARTED => {
            let _ = StrokeTurnStarted::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_S2C_ACTION_RELAY => {
            let _ = StrokeActionRelay::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_S2C_RESULT_RELAY => {
            let _ = StrokeResultRelay::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_S2C_STANDINGS => {
            let _ = StrokeStandings::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_S2C_COMMAND_RESULT => {
            let _ = StrokeCommandResult::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_S2C_MATCH_ABORTED => {
            let _ = StrokeMatchAborted::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M6_S2C_BALANCE_UPDATE => {
            let _ = StrokeBalanceUpdate::decode(&mut reader, &CompatibilityProfile::US_852);
        }
        SYNTHETIC_M7_C2S_SHOP_PAGE => { let _ = ShopPageRequest::decode(&mut reader, &CompatibilityProfile::US_852); }
        SYNTHETIC_M7_C2S_PURCHASE => { let _ = PurchaseRequestPacket::decode(&mut reader, &CompatibilityProfile::US_852); }
        SYNTHETIC_M7_C2S_EQUIP => { let _ = EquipRequest::decode(&mut reader, &CompatibilityProfile::US_852); }
        SYNTHETIC_M7_C2S_CONSUME => { let _ = ConsumeOneRequest::decode(&mut reader, &CompatibilityProfile::US_852); }
        SYNTHETIC_M7_C2S_REPAIR => { let _ = RepairRequest::decode(&mut reader, &CompatibilityProfile::US_852); }
        SYNTHETIC_M7_S2C_SHOP_PAGE => { let _ = ShopPage::decode(&mut reader, &CompatibilityProfile::US_852); }
        SYNTHETIC_M7_S2C_COMMAND_RESULT => { let _ = EconomyCommandResult::decode(&mut reader, &CompatibilityProfile::US_852); }
        SYNTHETIC_M7_S2C_PURCHASE_COMMITTED => { let _ = PurchaseCommitted::decode(&mut reader, &CompatibilityProfile::US_852); }
        SYNTHETIC_M7_S2C_INVENTORY_CHANGED => { let _ = InventoryChanged::decode(&mut reader, &CompatibilityProfile::US_852); }
        SYNTHETIC_M7_S2C_EQUIPMENT_CHANGED => { let _ = EquipmentChanged::decode(&mut reader, &CompatibilityProfile::US_852); }
        SYNTHETIC_M7_S2C_REPAIR_COMMITTED => { let _ = RepairCommitted::decode(&mut reader, &CompatibilityProfile::US_852); }
        _ => {
            let _ = reader.pstring(4096);
        }
    }
});
