//! Generated local synthetic M5 solo protocol tests; no source/client bytes.

use pangya_protocol::{
    BalanceUpdate, CompatibilityProfile, ConnectionState, DecodePacket, Direction, EncodePacket,
    FinishHole, HoleResult, Lie, LoadingComplete, MatchAbortReason, MatchAborted, MatchPhase,
    MatchStarted, PacketDecodeError, PacketReader, PacketWriter, RegistryKey, RegistryLookup,
    SYNTHETIC_M5_C2S_FINISH_HOLE, SYNTHETIC_M5_C2S_LOADING_COMPLETE, SYNTHETIC_M5_C2S_SHOT_ACTION,
    SYNTHETIC_M5_C2S_SHOT_RESULT, SYNTHETIC_M5_C2S_START_SOLO, SYNTHETIC_M5_S2C_BALANCE_UPDATE,
    SYNTHETIC_M5_S2C_COMMAND_RESULT, SYNTHETIC_M5_S2C_HOLE_RESULT, SYNTHETIC_M5_S2C_MATCH_ABORTED,
    SYNTHETIC_M5_S2C_MATCH_PHASE, SYNTHETIC_M5_S2C_MATCH_STARTED,
    SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY, SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY, ServiceKind,
    ShotAction, ShotActionRelay, ShotResult, ShotResultRelay, SoloCommand, SoloCommandOutcome,
    SoloCommandResult, SoloPhase, StartSolo, Weather, Wind, synthetic_m5_registry,
};
use proptest::prelude::*;
use uuid::Uuid;

const PROFILE: CompatibilityProfile = CompatibilityProfile::US_852;
const MATCH_BYTES: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];
const RESULT_BYTES: [u8; 16] = [
    0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00,
];

fn packet_reader(bytes: &[u8], direction: Direction, opcode: u16) -> PacketReader<'_> {
    PacketReader::new(bytes, direction, ServiceKind::Game, Some(opcode))
}

fn encoded<T: EncodePacket>(packet: &T) -> Result<Vec<u8>, pangya_protocol::PacketEncodeError> {
    let mut writer = PacketWriter::new();
    writer.u16_le(T::OPCODE);
    packet.encode(&mut writer, &PROFILE)?;
    Ok(writer.into_inner())
}

fn body<T: EncodePacket>(packet: &T) -> Vec<u8> {
    encoded(packet).expect("valid synthetic packet")[2..].to_vec()
}

fn action() -> ShotAction {
    ShotAction::new(42, 3, 250.5, -45.0, 0.25, -0.5).expect("valid action")
}

fn result() -> ShotResult {
    ShotResult::new(42, 123.25, -5.5, 99_999.0, Lie::Green, true).expect("valid result")
}

fn match_started() -> MatchStarted {
    MatchStarted::new(
        Uuid::from_bytes(MATCH_BYTES),
        7,
        4,
        core::array::from_fn(|index| index as u8),
        Weather::Rain,
        Wind::new(7.5, 270.25).expect("valid wind"),
        30_000,
    )
    .expect("valid match")
}

fn decode(opcode: u16, direction: Direction, bytes: &[u8]) -> Result<(), PacketDecodeError> {
    let mut reader = packet_reader(bytes, direction, opcode);
    match opcode {
        SYNTHETIC_M5_C2S_START_SOLO => StartSolo::decode(&mut reader, &PROFILE).map(|_| ()),
        SYNTHETIC_M5_C2S_LOADING_COMPLETE => {
            LoadingComplete::decode(&mut reader, &PROFILE).map(|_| ())
        }
        SYNTHETIC_M5_C2S_SHOT_ACTION => ShotAction::decode(&mut reader, &PROFILE).map(|_| ()),
        SYNTHETIC_M5_C2S_SHOT_RESULT => ShotResult::decode(&mut reader, &PROFILE).map(|_| ()),
        SYNTHETIC_M5_C2S_FINISH_HOLE => FinishHole::decode(&mut reader, &PROFILE).map(|_| ()),
        SYNTHETIC_M5_S2C_MATCH_STARTED => MatchStarted::decode(&mut reader, &PROFILE).map(|_| ()),
        SYNTHETIC_M5_S2C_MATCH_PHASE => MatchPhase::decode(&mut reader, &PROFILE).map(|_| ()),
        SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY => {
            ShotActionRelay::decode(&mut reader, &PROFILE).map(|_| ())
        }
        SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY => {
            ShotResultRelay::decode(&mut reader, &PROFILE).map(|_| ())
        }
        SYNTHETIC_M5_S2C_HOLE_RESULT => HoleResult::decode(&mut reader, &PROFILE).map(|_| ()),
        SYNTHETIC_M5_S2C_BALANCE_UPDATE => BalanceUpdate::decode(&mut reader, &PROFILE).map(|_| ()),
        SYNTHETIC_M5_S2C_COMMAND_RESULT => {
            SoloCommandResult::decode(&mut reader, &PROFILE).map(|_| ())
        }
        SYNTHETIC_M5_S2C_MATCH_ABORTED => MatchAborted::decode(&mut reader, &PROFILE).map(|_| ()),
        _ => unreachable!("test dispatch only receives known opcodes"),
    }
}

#[test]
fn generated_start_fixture_is_exact() {
    let fixture = include_bytes!("fixtures/m5-in-start-synthetic/fixture.bin");
    assert_eq!(fixture.as_slice(), &[0x20, 0x7f]);
    let mut reader = packet_reader(
        &fixture[2..],
        Direction::ClientToServer,
        SYNTHETIC_M5_C2S_START_SOLO,
    );
    let packet = StartSolo::decode(&mut reader, &PROFILE).expect("start fixture");
    assert_eq!(encoded(&packet).expect("encode start"), fixture);
}

#[test]
fn generated_match_started_fixture_is_exact_and_uuid_is_network_order() {
    let fixture = include_bytes!("fixtures/m5-out-match-started-synthetic/fixture.bin");
    assert_eq!(&fixture[2..18], &MATCH_BYTES);
    let mut reader = packet_reader(
        &fixture[2..],
        Direction::ServerToClient,
        SYNTHETIC_M5_S2C_MATCH_STARTED,
    );
    let packet = MatchStarted::decode(&mut reader, &PROFILE).expect("match-start fixture");
    assert_eq!(packet.match_id(), Uuid::from_bytes(MATCH_BYTES));
    assert_eq!(packet.course_id(), 7);
    assert_eq!(packet.hole(), 1);
    assert_eq!(packet.par(), 4);
    assert_eq!(packet.seed(), &core::array::from_fn(|index| index as u8));
    assert_eq!(packet.weather(), Weather::Rain);
    assert_eq!(packet.wind(), Wind::new(7.5, 270.25).expect("wind"));
    assert_eq!(packet.load_timeout_ms(), 30_000);
    assert_eq!(encoded(&packet).expect("encode match-start"), fixture);
}

#[test]
fn generated_action_result_and_hole_result_fixtures_are_exact() {
    let action_fixture = include_bytes!("fixtures/m5-in-action-synthetic/fixture.bin");
    let mut reader = packet_reader(
        &action_fixture[2..],
        Direction::ClientToServer,
        SYNTHETIC_M5_C2S_SHOT_ACTION,
    );
    let decoded_action = ShotAction::decode(&mut reader, &PROFILE).expect("action fixture");
    assert_eq!(decoded_action, action());
    assert_eq!(
        encoded(&decoded_action).expect("encode action"),
        action_fixture
    );

    let result_fixture = include_bytes!("fixtures/m5-in-result-synthetic/fixture.bin");
    let mut reader = packet_reader(
        &result_fixture[2..],
        Direction::ClientToServer,
        SYNTHETIC_M5_C2S_SHOT_RESULT,
    );
    let decoded_result = ShotResult::decode(&mut reader, &PROFILE).expect("result fixture");
    assert_eq!(decoded_result, result());
    assert_eq!(
        encoded(&decoded_result).expect("encode result"),
        result_fixture
    );

    let hole_fixture = include_bytes!("fixtures/m5-out-hole-result-synthetic/fixture.bin");
    assert_eq!(&hole_fixture[2..18], &MATCH_BYTES);
    assert_eq!(&hole_fixture[39..55], &RESULT_BYTES);
    let mut reader = packet_reader(
        &hole_fixture[2..],
        Direction::ServerToClient,
        SYNTHETIC_M5_S2C_HOLE_RESULT,
    );
    let hole = HoleResult::decode(&mut reader, &PROFILE).expect("hole fixture");
    assert_eq!(hole.match_id(), Uuid::from_bytes(MATCH_BYTES));
    assert_eq!(hole.hole(), 1);
    assert_eq!(hole.strokes(), 4);
    assert_eq!(hole.score(), -1);
    assert_eq!(hole.pang(), 1_234);
    assert_eq!(hole.experience(), 567);
    assert_eq!(hole.result_id(), Uuid::from_bytes(RESULT_BYTES));
    assert_eq!(encoded(&hole).expect("encode hole"), hole_fixture);
}

#[test]
fn constructors_accept_exact_bounds_and_reject_every_outside_class() {
    assert!(LoadingComplete::new(99).is_err());
    assert!(LoadingComplete::new(100).is_ok());
    assert!(LoadingComplete::new(101).is_err());

    assert!(ShotAction::new(0, 0, 0.0, 0.0, 0.0, 0.0).is_err());
    for club in [0, 13] {
        assert!(ShotAction::new(1, club, 0.0, -360.0, -1.0, -1.0).is_ok());
        assert!(ShotAction::new(1, club, 500.0, 360.0, 1.0, 1.0).is_ok());
    }
    assert!(ShotAction::new(1, 14, 1.0, 0.0, 0.0, 0.0).is_err());
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(ShotAction::new(1, 0, invalid, 0.0, 0.0, 0.0).is_err());
        assert!(ShotAction::new(1, 0, 1.0, invalid, 0.0, 0.0).is_err());
        assert!(ShotAction::new(1, 0, 1.0, 0.0, invalid, 0.0).is_err());
        assert!(ShotAction::new(1, 0, 1.0, 0.0, 0.0, invalid).is_err());
    }
    assert!(ShotAction::new(1, 0, -f32::EPSILON, 0.0, 0.0, 0.0).is_err());
    assert!(ShotAction::new(1, 0, 500.000_1, 0.0, 0.0, 0.0).is_err());
    assert!(ShotAction::new(1, 0, 1.0, -360.000_1, 0.0, 0.0).is_err());
    assert!(ShotAction::new(1, 0, 1.0, 360.000_1, 0.0, 0.0).is_err());
    assert!(ShotAction::new(1, 0, 1.0, 0.0, -1.000_1, 0.0).is_err());
    assert!(ShotAction::new(1, 0, 1.0, 0.0, 0.0, 1.000_1).is_err());

    assert!(ShotResult::new(0, 0.0, 0.0, 0.0, Lie::Tee, false).is_err());
    assert!(ShotResult::new(1, -100_000.0, 100_000.0, 0.0, Lie::Tee, false).is_ok());
    for invalid in [f32::NAN, f32::INFINITY, -100_000.1, 100_000.1] {
        assert!(ShotResult::new(1, invalid, 0.0, 0.0, Lie::Fairway, false).is_err());
        assert!(ShotResult::new(1, 0.0, invalid, 0.0, Lie::Fairway, false).is_err());
        assert!(ShotResult::new(1, 0.0, 0.0, invalid, Lie::Fairway, false).is_err());
    }

    assert!(Wind::new(0.0, 0.0).is_ok());
    assert!(Wind::new(15.0, 359.999_97).is_ok());
    assert!(Wind::new(-f32::EPSILON, 0.0).is_err());
    assert!(Wind::new(15.000_1, 0.0).is_err());
    assert!(Wind::new(1.0, -f32::EPSILON).is_err());
    assert!(Wind::new(1.0, 360.0).is_err());
    assert!(Wind::new(f32::NAN, 0.0).is_err());
    assert!(Wind::new(1.0, f32::INFINITY).is_err());

    let id = Uuid::nil();
    let wind = Wind::new(1.0, 2.0).expect("wind");
    assert!(MatchStarted::new(id, 0, 4, [0; 32], Weather::Clear, wind, 1).is_err());
    assert!(MatchStarted::new(id, 1, 0, [0; 32], Weather::Clear, wind, 1).is_err());
    assert!(MatchStarted::new(id, 1, 11, [0; 32], Weather::Clear, wind, 1).is_err());
    assert!(MatchStarted::new(id, 1, 1, [0; 32], Weather::Clear, wind, 0).is_err());
    assert!(MatchStarted::new(id, u32::MAX, 10, [0; 32], Weather::Clear, wind, u32::MAX).is_ok());
    assert!(ShotActionRelay::new(0, action()).is_err());
    assert!(ShotResultRelay::new(0, result()).is_err());
    assert!(HoleResult::new(id, 0, 0, 0, 0, id).is_err());
    assert!(HoleResult::new(id, u16::MAX, i16::MIN, u64::MAX, u64::MAX, id).is_ok());
}

#[test]
fn wire_rejects_nonfinite_out_of_range_noncanonical_and_unknown_values() {
    let mut action_body = body(&action());
    action_body[0..4].copy_from_slice(&0_u32.to_le_bytes());
    assert!(
        decode(
            SYNTHETIC_M5_C2S_SHOT_ACTION,
            Direction::ClientToServer,
            &action_body
        )
        .is_err()
    );
    let original_action = body(&action());
    for (offset, finite_outside) in [(5, -0.1_f32), (9, 360.1), (13, 1.1), (17, -1.1)] {
        for bits in [
            f32::NAN.to_bits(),
            f32::INFINITY.to_bits(),
            finite_outside.to_bits(),
        ] {
            let mut invalid = original_action.clone();
            invalid[offset..offset + 4].copy_from_slice(&bits.to_le_bytes());
            assert!(
                decode(
                    SYNTHETIC_M5_C2S_SHOT_ACTION,
                    Direction::ClientToServer,
                    &invalid
                )
                .is_err()
            );
        }
    }
    let mut invalid = original_action;
    invalid[4] = 14;
    assert!(
        decode(
            SYNTHETIC_M5_C2S_SHOT_ACTION,
            Direction::ClientToServer,
            &invalid
        )
        .is_err()
    );

    let original_result = body(&result());
    let mut invalid_sequence = original_result.clone();
    invalid_sequence[0..4].copy_from_slice(&0_u32.to_le_bytes());
    assert!(
        decode(
            SYNTHETIC_M5_C2S_SHOT_RESULT,
            Direction::ClientToServer,
            &invalid_sequence
        )
        .is_err()
    );
    for (offset, value) in [(4, f32::NEG_INFINITY), (8, -100_000.1), (12, 100_000.1)] {
        let mut invalid = original_result.clone();
        invalid[offset..offset + 4].copy_from_slice(&value.to_bits().to_le_bytes());
        assert!(
            decode(
                SYNTHETIC_M5_C2S_SHOT_RESULT,
                Direction::ClientToServer,
                &invalid
            )
            .is_err()
        );
    }
    let mut invalid = original_result.clone();
    invalid[16] = 6;
    assert!(
        decode(
            SYNTHETIC_M5_C2S_SHOT_RESULT,
            Direction::ClientToServer,
            &invalid
        )
        .is_err()
    );
    let mut invalid = original_result;
    invalid[17] = 2;
    assert!(
        decode(
            SYNTHETIC_M5_C2S_SHOT_RESULT,
            Direction::ClientToServer,
            &invalid
        )
        .is_err()
    );

    for progress in [0, 99, 101, 255] {
        assert!(
            decode(
                SYNTHETIC_M5_C2S_LOADING_COMPLETE,
                Direction::ClientToServer,
                &[progress]
            )
            .is_err()
        );
    }

    let original_match = body(&match_started());
    for (offset, bytes) in [
        (16, 0_u32.to_le_bytes().to_vec()),
        (20, vec![2]),
        (21, vec![0]),
        (21, vec![11]),
        (54, vec![3]),
        (55, f32::NAN.to_bits().to_le_bytes().to_vec()),
        (55, 15.1_f32.to_bits().to_le_bytes().to_vec()),
        (59, (-f32::EPSILON).to_bits().to_le_bytes().to_vec()),
        (59, 360.0_f32.to_bits().to_le_bytes().to_vec()),
        (63, 0_u32.to_le_bytes().to_vec()),
    ] {
        let mut invalid = original_match.clone();
        invalid[offset..offset + bytes.len()].copy_from_slice(&bytes);
        assert!(
            decode(
                SYNTHETIC_M5_S2C_MATCH_STARTED,
                Direction::ServerToClient,
                &invalid
            )
            .is_err()
        );
    }

    let id = Uuid::from_bytes(MATCH_BYTES);
    let mut phase = body(&MatchPhase::new(id, SoloPhase::Playing));
    phase[16] = 4;
    assert!(
        decode(
            SYNTHETIC_M5_S2C_MATCH_PHASE,
            Direction::ServerToClient,
            &phase
        )
        .is_err()
    );

    let mut command = body(&SoloCommandResult::new(
        SoloCommand::ShotAction,
        SoloCommandOutcome::Success,
    ));
    command[0] = 5;
    assert!(
        decode(
            SYNTHETIC_M5_S2C_COMMAND_RESULT,
            Direction::ServerToClient,
            &command
        )
        .is_err()
    );
    command[0] = 0;
    command[1] = 5;
    assert!(
        decode(
            SYNTHETIC_M5_S2C_COMMAND_RESULT,
            Direction::ServerToClient,
            &command
        )
        .is_err()
    );

    let mut abort = body(&MatchAborted::new(id, MatchAbortReason::LoadingTimeout));
    abort[16] = 4;
    assert!(
        decode(
            SYNTHETIC_M5_S2C_MATCH_ABORTED,
            Direction::ServerToClient,
            &abort
        )
        .is_err()
    );

    let mut relay = body(&ShotActionRelay::new(1, action()).expect("relay"));
    relay[0..8].copy_from_slice(&0_u64.to_le_bytes());
    assert!(
        decode(
            SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY,
            Direction::ServerToClient,
            &relay
        )
        .is_err()
    );

    let hole = HoleResult::new(id, 4, -1, 1, 2, Uuid::from_bytes(RESULT_BYTES)).expect("hole");
    let original_hole = body(&hole);
    for (offset, bytes) in [(16, vec![2]), (17, 0_u16.to_le_bytes().to_vec())] {
        let mut invalid = original_hole.clone();
        invalid[offset..offset + bytes.len()].copy_from_slice(&bytes);
        assert!(
            decode(
                SYNTHETIC_M5_S2C_HOLE_RESULT,
                Direction::ServerToClient,
                &invalid
            )
            .is_err()
        );
    }
}

fn all_valid_bodies() -> Vec<(u16, Direction, Vec<u8>)> {
    let id = Uuid::from_bytes(MATCH_BYTES);
    vec![
        (
            SYNTHETIC_M5_C2S_START_SOLO,
            Direction::ClientToServer,
            body(&StartSolo::new()),
        ),
        (
            SYNTHETIC_M5_C2S_LOADING_COMPLETE,
            Direction::ClientToServer,
            body(&LoadingComplete::new(100).expect("loading")),
        ),
        (
            SYNTHETIC_M5_C2S_SHOT_ACTION,
            Direction::ClientToServer,
            body(&action()),
        ),
        (
            SYNTHETIC_M5_C2S_SHOT_RESULT,
            Direction::ClientToServer,
            body(&result()),
        ),
        (
            SYNTHETIC_M5_C2S_FINISH_HOLE,
            Direction::ClientToServer,
            body(&FinishHole::new()),
        ),
        (
            SYNTHETIC_M5_S2C_MATCH_STARTED,
            Direction::ServerToClient,
            body(&match_started()),
        ),
        (
            SYNTHETIC_M5_S2C_MATCH_PHASE,
            Direction::ServerToClient,
            body(&MatchPhase::new(id, SoloPhase::Playing)),
        ),
        (
            SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY,
            Direction::ServerToClient,
            body(&ShotActionRelay::new(99, action()).expect("relay")),
        ),
        (
            SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY,
            Direction::ServerToClient,
            body(&ShotResultRelay::new(99, result()).expect("relay")),
        ),
        (
            SYNTHETIC_M5_S2C_HOLE_RESULT,
            Direction::ServerToClient,
            body(
                &HoleResult::new(id, 4, -1, 1_234, 567, Uuid::from_bytes(RESULT_BYTES))
                    .expect("hole"),
            ),
        ),
        (
            SYNTHETIC_M5_S2C_BALANCE_UPDATE,
            Direction::ServerToClient,
            body(&BalanceUpdate::new(u64::MAX, 7)),
        ),
        (
            SYNTHETIC_M5_S2C_COMMAND_RESULT,
            Direction::ServerToClient,
            body(&SoloCommandResult::new(
                SoloCommand::FinishHole,
                SoloCommandOutcome::Timeout,
            )),
        ),
        (
            SYNTHETIC_M5_S2C_MATCH_ABORTED,
            Direction::ServerToClient,
            body(&MatchAborted::new(id, MatchAbortReason::ServerShutdown)),
        ),
    ]
}

#[test]
fn every_layout_rejects_all_truncations_and_trailing_bytes() {
    for (opcode, direction, valid) in all_valid_bodies() {
        assert!(
            decode(opcode, direction, &valid).is_ok(),
            "valid {opcode:#06x}"
        );
        for length in 0..valid.len() {
            assert!(
                decode(opcode, direction, &valid[..length]).is_err(),
                "truncation {opcode:#06x} at {length}"
            );
        }
        let mut trailing = valid;
        trailing.push(0);
        assert!(
            decode(opcode, direction, &trailing).is_err(),
            "trailing {opcode:#06x}"
        );
    }
}

#[test]
fn every_closed_discriminator_round_trips() {
    let id = Uuid::nil();
    for (wire, phase) in [
        SoloPhase::Loading,
        SoloPhase::Playing,
        SoloPhase::HoleComplete,
        SoloPhase::Finished,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            encoded(&MatchPhase::new(id, phase)).expect("phase")[18],
            wire as u8
        );
    }
    for (wire, lie) in [
        Lie::Tee,
        Lie::Fairway,
        Lie::Rough,
        Lie::Bunker,
        Lie::Green,
        Lie::Fringe,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            encoded(&ShotResult::new(1, 0.0, 0.0, 0.0, lie, false).expect("result"))
                .expect("encode")[18],
            wire as u8
        );
    }
    for (wire, weather) in [Weather::Clear, Weather::Cloudy, Weather::Rain]
        .into_iter()
        .enumerate()
    {
        let packet = MatchStarted::new(
            id,
            1,
            1,
            [0; 32],
            weather,
            Wind::new(0.0, 0.0).expect("wind"),
            1,
        )
        .expect("match");
        assert_eq!(encoded(&packet).expect("encode")[56], wire as u8);
    }
    for (wire, command) in [
        SoloCommand::StartSolo,
        SoloCommand::LoadingComplete,
        SoloCommand::ShotAction,
        SoloCommand::ShotResult,
        SoloCommand::FinishHole,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            encoded(&SoloCommandResult::new(
                command,
                SoloCommandOutcome::Success
            ))
            .expect("command")[2],
            wire as u8
        );
    }
    for (wire, outcome) in [
        SoloCommandOutcome::Success,
        SoloCommandOutcome::InvalidSequence,
        SoloCommandOutcome::InvalidAction,
        SoloCommandOutcome::InvalidPhase,
        SoloCommandOutcome::Timeout,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            encoded(&SoloCommandResult::new(SoloCommand::StartSolo, outcome)).expect("outcome")[3],
            wire as u8
        );
    }
    for (wire, reason) in [
        MatchAbortReason::PlayerDisconnected,
        MatchAbortReason::LoadingTimeout,
        MatchAbortReason::ProtocolViolation,
        MatchAbortReason::ServerShutdown,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            encoded(&MatchAborted::new(id, reason)).expect("abort")[18],
            wire as u8
        );
    }
}

#[test]
fn seed_debug_is_redacted_without_raw_seed_fragments() {
    let packet = match_started();
    let debug = format!("{packet:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("[0, 1, 2"));
    assert!(!debug.contains("31]"));
}

#[test]
fn registry_enforces_precise_m5_states_and_preserves_unknown_classification() {
    let registry = synthetic_m5_registry(PROFILE.version());
    assert_eq!(registry.len(), 5);
    let base = RegistryKey {
        service: ServiceKind::Game,
        direction: Direction::ClientToServer,
        version: PROFILE.version(),
        state: ConnectionState::InRoom,
        opcode: SYNTHETIC_M5_C2S_START_SOLO,
    };
    let expected = [
        (SYNTHETIC_M5_C2S_START_SOLO, ConnectionState::InRoom),
        (
            SYNTHETIC_M5_C2S_LOADING_COMPLETE,
            ConnectionState::InMatchLoading,
        ),
        (SYNTHETIC_M5_C2S_SHOT_ACTION, ConnectionState::InMatch),
        (SYNTHETIC_M5_C2S_SHOT_RESULT, ConnectionState::InMatch),
        (SYNTHETIC_M5_C2S_FINISH_HOLE, ConnectionState::InMatch),
    ];
    for (opcode, accepted_state) in expected {
        assert_eq!(
            registry.classify(RegistryKey {
                opcode,
                state: accepted_state,
                ..base
            }),
            RegistryLookup::Accepted
        );
        for state in [
            ConnectionState::InChannel,
            ConnectionState::InRoom,
            ConnectionState::InMatchLoading,
            ConnectionState::InMatch,
        ] {
            if state != accepted_state {
                assert_eq!(
                    registry.classify(RegistryKey {
                        opcode,
                        state,
                        ..base
                    }),
                    RegistryLookup::InvalidState
                );
            }
        }
    }
    assert_eq!(
        registry.classify(RegistryKey {
            opcode: 0x7f25,
            ..base
        }),
        RegistryLookup::Unknown
    );
    assert_eq!(
        registry.classify(RegistryKey {
            direction: Direction::ServerToClient,
            opcode: SYNTHETIC_M5_C2S_START_SOLO,
            ..base
        }),
        RegistryLookup::Unknown
    );
}

proptest! {
    #[test]
    fn arbitrary_m5_bodies_never_panic_or_overread(
        opcode_index in 0_usize..13,
        data in prop::collection::vec(any::<u8>(), 0..256),
    ) {
        let opcodes = [
            SYNTHETIC_M5_C2S_START_SOLO,
            SYNTHETIC_M5_C2S_LOADING_COMPLETE,
            SYNTHETIC_M5_C2S_SHOT_ACTION,
            SYNTHETIC_M5_C2S_SHOT_RESULT,
            SYNTHETIC_M5_C2S_FINISH_HOLE,
            SYNTHETIC_M5_S2C_MATCH_STARTED,
            SYNTHETIC_M5_S2C_MATCH_PHASE,
            SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY,
            SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY,
            SYNTHETIC_M5_S2C_HOLE_RESULT,
            SYNTHETIC_M5_S2C_BALANCE_UPDATE,
            SYNTHETIC_M5_S2C_COMMAND_RESULT,
            SYNTHETIC_M5_S2C_MATCH_ABORTED,
        ];
        let opcode = opcodes[opcode_index];
        let direction = if opcode < SYNTHETIC_M5_S2C_MATCH_STARTED { Direction::ClientToServer } else { Direction::ServerToClient };
        let mut reader = packet_reader(&data, direction, opcode);
        let _ = match opcode {
            SYNTHETIC_M5_C2S_START_SOLO => StartSolo::decode(&mut reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M5_C2S_LOADING_COMPLETE => LoadingComplete::decode(&mut reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M5_C2S_SHOT_ACTION => ShotAction::decode(&mut reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M5_C2S_SHOT_RESULT => ShotResult::decode(&mut reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M5_C2S_FINISH_HOLE => FinishHole::decode(&mut reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M5_S2C_MATCH_STARTED => MatchStarted::decode(&mut reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M5_S2C_MATCH_PHASE => MatchPhase::decode(&mut reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY => ShotActionRelay::decode(&mut reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY => ShotResultRelay::decode(&mut reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M5_S2C_HOLE_RESULT => HoleResult::decode(&mut reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M5_S2C_BALANCE_UPDATE => BalanceUpdate::decode(&mut reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M5_S2C_COMMAND_RESULT => SoloCommandResult::decode(&mut reader, &PROFILE).map(|_| ()),
            _ => MatchAborted::decode(&mut reader, &PROFILE).map(|_| ()),
        };
        prop_assert!(reader.offset() <= data.len());
        prop_assert_eq!(reader.offset() + reader.remaining(), data.len());
    }
}
