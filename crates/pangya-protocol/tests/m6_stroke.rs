//! Generated local synthetic M6 stroke protocol tests; no source/client bytes.

use pangya_protocol::{
    CompatibilityProfile, ConnectionState, DecodePacket, Direction, EncodePacket, Lie,
    PacketDecodeError, PacketReader, PacketWriter, RegistryKey, RegistryLookup,
    SYNTHETIC_M6_C2S_GIVE_UP, SYNTHETIC_M6_C2S_LOADING_COMPLETE, SYNTHETIC_M6_C2S_SHOT_ACTION,
    SYNTHETIC_M6_C2S_SHOT_RESULT, SYNTHETIC_M6_C2S_START_STROKE_TWO, SYNTHETIC_M6_S2C_ACTION_RELAY,
    SYNTHETIC_M6_S2C_BALANCE_UPDATE, SYNTHETIC_M6_S2C_COMMAND_RESULT,
    SYNTHETIC_M6_S2C_MATCH_ABORTED, SYNTHETIC_M6_S2C_MATCH_STARTED, SYNTHETIC_M6_S2C_PHASE,
    SYNTHETIC_M6_S2C_RESULT_RELAY, SYNTHETIC_M6_S2C_STANDINGS, SYNTHETIC_M6_S2C_TURN_STARTED,
    ServiceKind, StartStrokeTwo, StrokeAbortReason, StrokeActionRelay, StrokeBalanceUpdate,
    StrokeCommand, StrokeCommandOutcome, StrokeCommandResult, StrokeCompletion, StrokeGiveUp,
    StrokeLoadingComplete, StrokeMatchAborted, StrokeMatchStarted, StrokePhase, StrokePhaseKind,
    StrokeResultRelay, StrokeShotAction, StrokeShotResult, StrokeStandingEntry, StrokeStandings,
    StrokeTurnStarted, Weather, Wind, synthetic_m6_registry,
};
use proptest::prelude::*;
use uuid::Uuid;

const PROFILE: CompatibilityProfile = CompatibilityProfile::US_852;
const MATCH_BYTES: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];
const RESULT_1_BYTES: [u8; 16] = [
    0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00,
];
const RESULT_2_BYTES: [u8; 16] = [
    0x10, 0x21, 0x32, 0x43, 0x54, 0x65, 0x76, 0x87, 0x98, 0xa9, 0xba, 0xcb, 0xdc, 0xed, 0xfe, 0x0f,
];

fn reader(bytes: &[u8], direction: Direction, opcode: u16) -> PacketReader<'_> {
    PacketReader::new(bytes, direction, ServiceKind::Game, Some(opcode))
}

fn encoded<T: EncodePacket>(packet: &T) -> Vec<u8> {
    let mut writer = PacketWriter::new();
    writer.u16_le(T::OPCODE);
    packet.encode(&mut writer, &PROFILE).expect("valid packet");
    writer.into_inner()
}

fn body<T: EncodePacket>(packet: &T) -> Vec<u8> {
    encoded(packet)[2..].to_vec()
}

fn action() -> StrokeShotAction {
    StrokeShotAction::new(42, 3, 250.5, -45.0, 0.25, -0.5).expect("action")
}

fn result() -> StrokeShotResult {
    StrokeShotResult::new(42, 123.25, -5.5, 99_999.0, Lie::Green, true).expect("result")
}

fn started() -> StrokeMatchStarted {
    StrokeMatchStarted::new(
        Uuid::from_bytes(MATCH_BYTES),
        7,
        4,
        core::array::from_fn(|index| index as u8),
        Weather::Rain,
        Wind::new(7.5, 270.25).expect("wind"),
        30_000,
        31_000,
        600_000,
        11,
        22,
    )
    .expect("started")
}

fn standings() -> StrokeStandings {
    StrokeStandings::new(
        Uuid::from_bytes(MATCH_BYTES),
        [
            StrokeStandingEntry::new(
                11,
                1,
                StrokeCompletion::Holed,
                4,
                Some(0),
                12,
                5,
                Uuid::from_bytes(RESULT_1_BYTES),
            )
            .expect("first"),
            StrokeStandingEntry::new(
                22,
                2,
                StrokeCompletion::GameTimeout,
                0,
                None,
                0,
                0,
                Uuid::from_bytes(RESULT_2_BYTES),
            )
            .expect("second"),
        ],
    )
    .expect("standings")
}

fn decode_and_reencode(
    opcode: u16,
    direction: Direction,
    bytes: &[u8],
) -> Result<Vec<u8>, PacketDecodeError> {
    let mut packet_reader = reader(bytes, direction, opcode);
    macro_rules! packet {
        ($ty:ty) => {{
            let value = <$ty>::decode(&mut packet_reader, &PROFILE)?;
            Ok(encoded(&value))
        }};
    }
    match opcode {
        SYNTHETIC_M6_C2S_START_STROKE_TWO => packet!(StartStrokeTwo),
        SYNTHETIC_M6_C2S_LOADING_COMPLETE => packet!(StrokeLoadingComplete),
        SYNTHETIC_M6_C2S_SHOT_ACTION => packet!(StrokeShotAction),
        SYNTHETIC_M6_C2S_SHOT_RESULT => packet!(StrokeShotResult),
        SYNTHETIC_M6_C2S_GIVE_UP => packet!(StrokeGiveUp),
        SYNTHETIC_M6_S2C_MATCH_STARTED => packet!(StrokeMatchStarted),
        SYNTHETIC_M6_S2C_PHASE => packet!(StrokePhase),
        SYNTHETIC_M6_S2C_TURN_STARTED => packet!(StrokeTurnStarted),
        SYNTHETIC_M6_S2C_ACTION_RELAY => packet!(StrokeActionRelay),
        SYNTHETIC_M6_S2C_RESULT_RELAY => packet!(StrokeResultRelay),
        SYNTHETIC_M6_S2C_STANDINGS => packet!(StrokeStandings),
        SYNTHETIC_M6_S2C_COMMAND_RESULT => packet!(StrokeCommandResult),
        SYNTHETIC_M6_S2C_MATCH_ABORTED => packet!(StrokeMatchAborted),
        SYNTHETIC_M6_S2C_BALANCE_UPDATE => packet!(StrokeBalanceUpdate),
        _ => unreachable!("known test opcode"),
    }
}

fn decode_only(opcode: u16, direction: Direction, bytes: &[u8]) -> Result<(), PacketDecodeError> {
    decode_and_reencode(opcode, direction, bytes).map(|_| ())
}

const FIXTURES: [(&[u8], u16, Direction); 14] = [
    (
        include_bytes!("fixtures/m6-in-start-stroke-two-synthetic/fixture.bin"),
        SYNTHETIC_M6_C2S_START_STROKE_TWO,
        Direction::ClientToServer,
    ),
    (
        include_bytes!("fixtures/m6-in-loading-complete-synthetic/fixture.bin"),
        SYNTHETIC_M6_C2S_LOADING_COMPLETE,
        Direction::ClientToServer,
    ),
    (
        include_bytes!("fixtures/m6-in-action-synthetic/fixture.bin"),
        SYNTHETIC_M6_C2S_SHOT_ACTION,
        Direction::ClientToServer,
    ),
    (
        include_bytes!("fixtures/m6-in-result-synthetic/fixture.bin"),
        SYNTHETIC_M6_C2S_SHOT_RESULT,
        Direction::ClientToServer,
    ),
    (
        include_bytes!("fixtures/m6-in-give-up-synthetic/fixture.bin"),
        SYNTHETIC_M6_C2S_GIVE_UP,
        Direction::ClientToServer,
    ),
    (
        include_bytes!("fixtures/m6-out-match-started-synthetic/fixture.bin"),
        SYNTHETIC_M6_S2C_MATCH_STARTED,
        Direction::ServerToClient,
    ),
    (
        include_bytes!("fixtures/m6-out-phase-synthetic/fixture.bin"),
        SYNTHETIC_M6_S2C_PHASE,
        Direction::ServerToClient,
    ),
    (
        include_bytes!("fixtures/m6-out-turn-started-synthetic/fixture.bin"),
        SYNTHETIC_M6_S2C_TURN_STARTED,
        Direction::ServerToClient,
    ),
    (
        include_bytes!("fixtures/m6-out-action-relay-synthetic/fixture.bin"),
        SYNTHETIC_M6_S2C_ACTION_RELAY,
        Direction::ServerToClient,
    ),
    (
        include_bytes!("fixtures/m6-out-result-relay-synthetic/fixture.bin"),
        SYNTHETIC_M6_S2C_RESULT_RELAY,
        Direction::ServerToClient,
    ),
    (
        include_bytes!("fixtures/m6-out-standings-synthetic/fixture.bin"),
        SYNTHETIC_M6_S2C_STANDINGS,
        Direction::ServerToClient,
    ),
    (
        include_bytes!("fixtures/m6-out-command-result-synthetic/fixture.bin"),
        SYNTHETIC_M6_S2C_COMMAND_RESULT,
        Direction::ServerToClient,
    ),
    (
        include_bytes!("fixtures/m6-out-match-aborted-synthetic/fixture.bin"),
        SYNTHETIC_M6_S2C_MATCH_ABORTED,
        Direction::ServerToClient,
    ),
    (
        include_bytes!("fixtures/m6-out-balance-update-synthetic/fixture.bin"),
        SYNTHETIC_M6_S2C_BALANCE_UPDATE,
        Direction::ServerToClient,
    ),
];

#[test]
fn every_generated_fixture_is_an_exact_golden_round_trip() {
    for (fixture, opcode, direction) in FIXTURES {
        assert_eq!(&fixture[..2], &opcode.to_le_bytes(), "opcode {opcode:#06x}");
        assert_eq!(
            decode_and_reencode(opcode, direction, &fixture[2..]).expect("fixture decode"),
            fixture,
            "fixture {opcode:#06x}",
        );
    }
}

#[test]
fn generated_started_and_standings_fixture_semantics_are_exact() {
    let started_fixture = FIXTURES[5].0;
    assert_eq!(&started_fixture[2..18], &MATCH_BYTES);
    let mut started_reader = reader(
        &started_fixture[2..],
        Direction::ServerToClient,
        SYNTHETIC_M6_S2C_MATCH_STARTED,
    );
    let packet =
        StrokeMatchStarted::decode(&mut started_reader, &PROFILE).expect("started fixture");
    assert_eq!(packet.match_id(), Uuid::from_bytes(MATCH_BYTES));
    assert_eq!(packet.course_id(), 7);
    assert_eq!(packet.hole(), 1);
    assert_eq!(packet.par(), 4);
    assert_eq!(packet.seed(), &core::array::from_fn(|index| index as u8));
    assert_eq!(packet.weather(), Weather::Rain);
    assert_eq!(packet.wind(), Wind::new(7.5, 270.25).expect("wind"));
    assert_eq!(
        (
            packet.load_timeout_ms(),
            packet.turn_timeout_ms(),
            packet.game_timeout_ms()
        ),
        (30_000, 31_000, 600_000)
    );
    assert_eq!(packet.participant_connection_ids(), [11, 22]);

    let standings_fixture = FIXTURES[10].0;
    assert_eq!(&standings_fixture[2..18], &MATCH_BYTES);
    let mut standings_reader = reader(
        &standings_fixture[2..],
        Direction::ServerToClient,
        SYNTHETIC_M6_S2C_STANDINGS,
    );
    let packet =
        StrokeStandings::decode(&mut standings_reader, &PROFILE).expect("standings fixture");
    let [first, second] = packet.entries();
    assert_eq!(
        (
            first.connection_id(),
            first.place(),
            first.completion(),
            first.strokes(),
            first.score(),
            first.pang(),
            first.experience(),
            first.player_result_id()
        ),
        (
            11,
            1,
            StrokeCompletion::Holed,
            4,
            Some(0),
            12,
            5,
            Uuid::from_bytes(RESULT_1_BYTES)
        )
    );
    assert_eq!(
        (
            second.connection_id(),
            second.place(),
            second.completion(),
            second.strokes(),
            second.score(),
            second.pang(),
            second.experience(),
            second.player_result_id()
        ),
        (
            22,
            2,
            StrokeCompletion::GameTimeout,
            0,
            None,
            0,
            0,
            Uuid::from_bytes(RESULT_2_BYTES)
        )
    );
}

#[test]
fn m6_action_and_result_wire_fields_exactly_match_m5_layouts() {
    let m6_action = &FIXTURES[2].0[2..];
    let m5_action = &include_bytes!("fixtures/m5-in-action-synthetic/fixture.bin")[2..];
    assert_eq!(m6_action, m5_action);
    let m6_result = &FIXTURES[3].0[2..];
    let m5_result = &include_bytes!("fixtures/m5-in-result-synthetic/fixture.bin")[2..];
    assert_eq!(m6_result, m5_result);
}

#[test]
fn every_layout_rejects_every_truncation_and_trailing_byte() {
    for (fixture, opcode, direction) in FIXTURES {
        let valid = &fixture[2..];
        assert!(decode_only(opcode, direction, valid).is_ok());
        for length in 0..valid.len() {
            assert!(
                decode_only(opcode, direction, &valid[..length]).is_err(),
                "truncation {opcode:#06x} at {length}"
            );
        }
        let mut trailing = valid.to_vec();
        trailing.push(0);
        assert!(
            decode_only(opcode, direction, &trailing).is_err(),
            "trailing {opcode:#06x}"
        );
    }
}

#[test]
fn constructors_enforce_all_finite_bounds_and_authority_invariants() {
    assert!(StrokeLoadingComplete::new(99).is_err());
    assert!(StrokeLoadingComplete::new(100).is_ok());
    assert!(StrokeLoadingComplete::new(101).is_err());
    for impossible in [0, 1, 99, 101, u8::MAX] {
        assert!(
            decode_only(
                SYNTHETIC_M6_C2S_LOADING_COMPLETE,
                Direction::ClientToServer,
                &[impossible],
            )
            .is_err(),
            "impossible loading progress {impossible}"
        );
    }

    assert!(StrokeShotAction::new(0, 0, 0.0, 0.0, 0.0, 0.0).is_err());
    assert!(StrokeShotAction::new(1, 0, 0.0, -360.0, -1.0, -1.0).is_ok());
    assert!(StrokeShotAction::new(u32::MAX, 13, 500.0, 360.0, 1.0, 1.0).is_ok());
    assert!(StrokeShotAction::new(1, 14, 1.0, 0.0, 0.0, 0.0).is_err());
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(StrokeShotAction::new(1, 0, invalid, 0.0, 0.0, 0.0).is_err());
        assert!(StrokeShotAction::new(1, 0, 1.0, invalid, 0.0, 0.0).is_err());
        assert!(StrokeShotAction::new(1, 0, 1.0, 0.0, invalid, 0.0).is_err());
        assert!(StrokeShotAction::new(1, 0, 1.0, 0.0, 0.0, invalid).is_err());
    }
    for invalid in [-100_000.1, 100_000.1, f32::NAN, f32::INFINITY] {
        assert!(StrokeShotResult::new(1, invalid, 0.0, 0.0, Lie::Tee, false).is_err());
        assert!(StrokeShotResult::new(1, 0.0, invalid, 0.0, Lie::Tee, false).is_err());
        assert!(StrokeShotResult::new(1, 0.0, 0.0, invalid, Lie::Tee, false).is_err());
    }
    assert!(StrokeShotResult::new(0, 0.0, 0.0, 0.0, Lie::Tee, false).is_err());
    assert!(StrokeShotResult::new(1, -100_000.0, 100_000.0, 0.0, Lie::Fringe, false).is_ok());

    let id = Uuid::from_bytes(MATCH_BYTES);
    let wind = Wind::new(1.0, 2.0).expect("wind");
    let start = |course, par, load, turn, game, first, second| {
        StrokeMatchStarted::new(
            id,
            course,
            par,
            [0; 32],
            Weather::Clear,
            wind,
            load,
            turn,
            game,
            first,
            second,
        )
    };
    assert!(start(0, 4, 1, 1, 1, 1, 2).is_err());
    assert!(start(1, 0, 1, 1, 1, 1, 2).is_err());
    assert!(start(1, 11, 1, 1, 1, 1, 2).is_err());
    assert!(start(1, 4, 0, 1, 1, 1, 2).is_err());
    assert!(start(1, 4, 1, 0, 1, 1, 2).is_err());
    assert!(start(1, 4, 1, 1, 0, 1, 2).is_err());
    assert!(start(1, 4, 1, 1, 1, 0, 2).is_err());
    assert!(start(1, 4, 1, 1, 1, 1, 0).is_err());
    assert!(start(1, 4, 1, 1, 1, 1, 1).is_err());
    assert!(start(u32::MAX, 10, u32::MAX, u32::MAX, u32::MAX, 1, u64::MAX).is_ok());
    assert!(StrokeTurnStarted::new(id, 0, 1, 1, 1).is_err());
    assert!(StrokeTurnStarted::new(id, 1, 0, 1, 1).is_err());
    assert!(StrokeTurnStarted::new(id, 1, 1, 0, 1).is_err());
    assert!(StrokeTurnStarted::new(id, 1, 1, 1, 0).is_err());
    assert!(StrokeActionRelay::new(0, action()).is_err());
    assert!(StrokeResultRelay::new(0, result()).is_err());

    let result_id = Uuid::from_bytes(RESULT_1_BYTES);
    assert!(
        StrokeStandingEntry::new(0, 1, StrokeCompletion::Holed, 1, Some(0), 0, 0, result_id)
            .is_err()
    );
    assert!(
        StrokeStandingEntry::new(1, 0, StrokeCompletion::Holed, 1, Some(0), 0, 0, result_id)
            .is_err()
    );
    assert!(
        StrokeStandingEntry::new(1, 3, StrokeCompletion::Holed, 1, Some(0), 0, 0, result_id)
            .is_err()
    );
    assert!(
        StrokeStandingEntry::new(1, 1, StrokeCompletion::Holed, 0, Some(0), 0, 0, result_id)
            .is_err()
    );
    assert!(
        StrokeStandingEntry::new(1, 1, StrokeCompletion::Holed, 1, None, 0, 0, result_id).is_err()
    );
    assert!(
        StrokeStandingEntry::new(1, 1, StrokeCompletion::GiveUp, 0, Some(0), 0, 0, result_id)
            .is_err()
    );
    assert!(
        StrokeStandingEntry::new(1, 1, StrokeCompletion::GiveUp, 0, None, 1, 0, result_id).is_err()
    );
    assert!(
        StrokeStandingEntry::new(1, 1, StrokeCompletion::GiveUp, 0, None, 0, 0, Uuid::nil())
            .is_err()
    );

    let [first, second] = standings().entries();
    assert!(StrokeStandings::new(id, [second, first]).is_err());
    let duplicate_connection = StrokeStandingEntry::new(
        11,
        2,
        StrokeCompletion::GiveUp,
        0,
        None,
        0,
        0,
        Uuid::from_bytes(RESULT_2_BYTES),
    )
    .expect("duplicate connection entry");
    assert!(StrokeStandings::new(id, [first, duplicate_connection]).is_err());
    let duplicate_result = StrokeStandingEntry::new(
        22,
        2,
        StrokeCompletion::GiveUp,
        0,
        None,
        0,
        0,
        Uuid::from_bytes(RESULT_1_BYTES),
    )
    .expect("duplicate result entry");
    assert!(StrokeStandings::new(id, [first, duplicate_result]).is_err());
}

#[test]
fn standings_require_exact_winner_by_forfeit_pair_on_construct_and_decode() {
    let id = Uuid::from_bytes(MATCH_BYTES);
    let entry = |connection_id, place, completion, result_id| {
        let scored = matches!(
            completion,
            StrokeCompletion::Holed | StrokeCompletion::StrokeCap
        );
        StrokeStandingEntry::new(
            connection_id,
            place,
            completion,
            u16::from(scored),
            scored.then_some(0),
            u64::from(completion == StrokeCompletion::WinnerByForfeit) * 10,
            u64::from(completion == StrokeCompletion::WinnerByForfeit) * 5,
            result_id,
        )
        .expect("entry")
    };
    for direct in [
        StrokeCompletion::GiveUp,
        StrokeCompletion::Disconnect,
        StrokeCompletion::TurnTimeout,
    ] {
        assert!(
            StrokeStandings::new(
                id,
                [
                    entry(
                        1,
                        1,
                        StrokeCompletion::WinnerByForfeit,
                        Uuid::from_bytes(RESULT_1_BYTES)
                    ),
                    entry(2, 2, direct, Uuid::from_bytes(RESULT_2_BYTES)),
                ],
            )
            .is_ok()
        );
    }
    for malformed in [
        [
            entry(
                1,
                1,
                StrokeCompletion::WinnerByForfeit,
                Uuid::from_bytes(RESULT_1_BYTES),
            ),
            entry(
                2,
                2,
                StrokeCompletion::WinnerByForfeit,
                Uuid::from_bytes(RESULT_2_BYTES),
            ),
        ],
        [
            entry(
                1,
                1,
                StrokeCompletion::GiveUp,
                Uuid::from_bytes(RESULT_1_BYTES),
            ),
            entry(
                2,
                2,
                StrokeCompletion::WinnerByForfeit,
                Uuid::from_bytes(RESULT_2_BYTES),
            ),
        ],
        [
            entry(
                1,
                1,
                StrokeCompletion::WinnerByForfeit,
                Uuid::from_bytes(RESULT_1_BYTES),
            ),
            entry(
                2,
                2,
                StrokeCompletion::GameTimeout,
                Uuid::from_bytes(RESULT_2_BYTES),
            ),
        ],
        [
            entry(
                1,
                1,
                StrokeCompletion::GameTimeout,
                Uuid::from_bytes(RESULT_1_BYTES),
            ),
            entry(
                2,
                2,
                StrokeCompletion::WinnerByForfeit,
                Uuid::from_bytes(RESULT_2_BYTES),
            ),
        ],
        [
            entry(
                1,
                1,
                StrokeCompletion::Holed,
                Uuid::from_bytes(RESULT_1_BYTES),
            ),
            entry(
                2,
                2,
                StrokeCompletion::GiveUp,
                Uuid::from_bytes(RESULT_2_BYTES),
            ),
        ],
    ] {
        assert!(StrokeStandings::new(id, malformed).is_err());
    }
    assert!(
        StrokeStandings::new(
            id,
            [
                entry(
                    1,
                    1,
                    StrokeCompletion::GameTimeout,
                    Uuid::from_bytes(RESULT_1_BYTES)
                ),
                entry(
                    2,
                    2,
                    StrokeCompletion::GameTimeout,
                    Uuid::from_bytes(RESULT_2_BYTES)
                ),
            ],
        )
        .is_ok()
    );

    let valid = StrokeStandings::new(
        id,
        [
            entry(
                1,
                1,
                StrokeCompletion::WinnerByForfeit,
                Uuid::from_bytes(RESULT_1_BYTES),
            ),
            entry(
                2,
                2,
                StrokeCompletion::GiveUp,
                Uuid::from_bytes(RESULT_2_BYTES),
            ),
        ],
    )
    .expect("valid pair");
    let mut invalid_wire = body(&valid);
    invalid_wire[73] = StrokeCompletion::GameTimeout as u8;
    assert!(
        decode_only(
            SYNTHETIC_M6_S2C_STANDINGS,
            Direction::ServerToClient,
            &invalid_wire,
        )
        .is_err()
    );
}

#[test]
fn wire_rejects_noncanonical_values_and_cross_entry_duplicates() {
    let mut invalid = body(&action());
    invalid[0..4].copy_from_slice(&0_u32.to_le_bytes());
    assert!(
        decode_only(
            SYNTHETIC_M6_C2S_SHOT_ACTION,
            Direction::ClientToServer,
            &invalid
        )
        .is_err()
    );
    let valid_action = body(&action());
    for (offset, value) in [(5, -0.1_f32), (9, 360.1), (13, 1.1), (17, -1.1)] {
        for bits in [value.to_bits(), f32::NAN.to_bits(), f32::INFINITY.to_bits()] {
            let mut invalid = valid_action.clone();
            invalid[offset..offset + 4].copy_from_slice(&bits.to_le_bytes());
            assert!(
                decode_only(
                    SYNTHETIC_M6_C2S_SHOT_ACTION,
                    Direction::ClientToServer,
                    &invalid
                )
                .is_err()
            );
        }
    }
    let mut invalid = valid_action;
    invalid[4] = 14;
    assert!(
        decode_only(
            SYNTHETIC_M6_C2S_SHOT_ACTION,
            Direction::ClientToServer,
            &invalid
        )
        .is_err()
    );

    let valid_result = body(&result());
    let mut invalid = valid_result.clone();
    invalid[16] = 6;
    assert!(
        decode_only(
            SYNTHETIC_M6_C2S_SHOT_RESULT,
            Direction::ClientToServer,
            &invalid
        )
        .is_err()
    );
    let mut invalid = valid_result;
    invalid[17] = 2;
    assert!(
        decode_only(
            SYNTHETIC_M6_C2S_SHOT_RESULT,
            Direction::ClientToServer,
            &invalid
        )
        .is_err()
    );

    let valid_started = body(&started());
    for (offset, bytes) in [
        (16, 0_u32.to_le_bytes().to_vec()),
        (20, vec![2]),
        (21, vec![0]),
        (21, vec![11]),
        (54, vec![3]),
        (55, f32::NAN.to_bits().to_le_bytes().to_vec()),
        (59, 360.0_f32.to_bits().to_le_bytes().to_vec()),
        (63, 0_u32.to_le_bytes().to_vec()),
        (67, 0_u32.to_le_bytes().to_vec()),
        (71, 0_u32.to_le_bytes().to_vec()),
        (75, vec![1]),
        (76, 0_u64.to_le_bytes().to_vec()),
        (84, 11_u64.to_le_bytes().to_vec()),
    ] {
        let mut invalid = valid_started.clone();
        invalid[offset..offset + bytes.len()].copy_from_slice(&bytes);
        assert!(
            decode_only(
                SYNTHETIC_M6_S2C_MATCH_STARTED,
                Direction::ServerToClient,
                &invalid
            )
            .is_err(),
            "started offset {offset}"
        );
    }

    let valid = body(&standings());
    for (offset, bytes) in [
        (16, vec![1]),
        (25, vec![2]),
        (26, vec![6]),
        (29, vec![2]),
        (29, vec![0]),
        (77, 1_i16.to_le_bytes().to_vec()),
        (79, 1_u64.to_le_bytes().to_vec()),
        (48, [0_u8; 16].to_vec()),
        (64, 11_u64.to_le_bytes().to_vec()),
        (72, vec![1]),
        (111 - 16, RESULT_1_BYTES.to_vec()),
    ] {
        let mut invalid = valid.clone();
        invalid[offset..offset + bytes.len()].copy_from_slice(&bytes);
        assert!(
            decode_only(
                SYNTHETIC_M6_S2C_STANDINGS,
                Direction::ServerToClient,
                &invalid
            )
            .is_err(),
            "standings offset {offset}"
        );
    }
}

#[test]
fn every_closed_discriminator_round_trips_and_unknowns_reject() {
    let id = Uuid::from_bytes(MATCH_BYTES);
    for (wire, phase) in [
        StrokePhaseKind::Loading,
        StrokePhaseKind::Playing,
        StrokePhaseKind::ResultsPending,
        StrokePhaseKind::Finished,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(encoded(&StrokePhase::new(id, phase))[18], wire as u8);
    }
    for (wire, command) in [
        StrokeCommand::Start,
        StrokeCommand::Load,
        StrokeCommand::Action,
        StrokeCommand::Result,
        StrokeCommand::GiveUp,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            encoded(&StrokeCommandResult::new(
                command,
                StrokeCommandOutcome::Success
            ))[2],
            wire as u8
        );
    }
    for (wire, outcome) in [
        StrokeCommandOutcome::Success,
        StrokeCommandOutcome::InvalidSequence,
        StrokeCommandOutcome::InvalidAction,
        StrokeCommandOutcome::InvalidPhase,
        StrokeCommandOutcome::InvalidTurn,
        StrokeCommandOutcome::Timeout,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            encoded(&StrokeCommandResult::new(StrokeCommand::Start, outcome))[3],
            wire as u8
        );
    }
    for (wire, reason) in [
        StrokeAbortReason::LoadingTimeout,
        StrokeAbortReason::LoadingDisconnect,
        StrokeAbortReason::ServerShutdown,
        StrokeAbortReason::PersistenceFailure,
        StrokeAbortReason::StartupRecovery,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            encoded(&StrokeMatchAborted::new(id, reason))[18],
            wire as u8
        );
    }
    for (wire, completion) in [
        StrokeCompletion::Holed,
        StrokeCompletion::StrokeCap,
        StrokeCompletion::GiveUp,
        StrokeCompletion::Disconnect,
        StrokeCompletion::TurnTimeout,
        StrokeCompletion::GameTimeout,
        StrokeCompletion::WinnerByForfeit,
    ]
    .into_iter()
    .enumerate()
    {
        let direct_forfeit = matches!(
            completion,
            StrokeCompletion::GiveUp | StrokeCompletion::Disconnect | StrokeCompletion::TurnTimeout
        );
        let target_place = if direct_forfeit { 2 } else { 1 };
        let score = matches!(
            completion,
            StrokeCompletion::Holed | StrokeCompletion::StrokeCap
        )
        .then_some(0);
        let target = StrokeStandingEntry::new(
            if direct_forfeit { 2 } else { 1 },
            target_place,
            completion,
            u16::from(score.is_some()),
            score,
            u64::from(completion == StrokeCompletion::WinnerByForfeit) * 10,
            u64::from(completion == StrokeCompletion::WinnerByForfeit) * 5,
            Uuid::from_bytes(if direct_forfeit {
                RESULT_2_BYTES
            } else {
                RESULT_1_BYTES
            }),
        )
        .expect("target entry");
        let counterpart = if direct_forfeit {
            StrokeStandingEntry::new(
                1,
                1,
                StrokeCompletion::WinnerByForfeit,
                0,
                None,
                10,
                5,
                Uuid::from_bytes(RESULT_1_BYTES),
            )
        } else {
            StrokeStandingEntry::new(
                2,
                2,
                if completion == StrokeCompletion::WinnerByForfeit {
                    StrokeCompletion::GiveUp
                } else {
                    StrokeCompletion::GameTimeout
                },
                0,
                None,
                0,
                0,
                Uuid::from_bytes(RESULT_2_BYTES),
            )
        }
        .expect("counterpart entry");
        let entries = if direct_forfeit {
            [counterpart, target]
        } else {
            [target, counterpart]
        };
        let completion_offset = if direct_forfeit { 75 } else { 28 };
        assert_eq!(
            encoded(&StrokeStandings::new(id, entries).expect("standings"))[completion_offset],
            wire as u8
        );
    }

    let unknown_cases = [
        (
            SYNTHETIC_M6_S2C_PHASE,
            body(&StrokePhase::new(id, StrokePhaseKind::Loading)),
            16,
            4,
        ),
        (
            SYNTHETIC_M6_S2C_COMMAND_RESULT,
            body(&StrokeCommandResult::new(
                StrokeCommand::Start,
                StrokeCommandOutcome::Success,
            )),
            0,
            5,
        ),
        (
            SYNTHETIC_M6_S2C_COMMAND_RESULT,
            body(&StrokeCommandResult::new(
                StrokeCommand::Start,
                StrokeCommandOutcome::Success,
            )),
            1,
            6,
        ),
        (
            SYNTHETIC_M6_S2C_MATCH_ABORTED,
            body(&StrokeMatchAborted::new(
                id,
                StrokeAbortReason::LoadingTimeout,
            )),
            16,
            5,
        ),
    ];
    for (opcode, mut packet, offset, value) in unknown_cases {
        packet[offset] = value;
        assert!(decode_only(opcode, Direction::ServerToClient, &packet).is_err());
    }
}

#[test]
fn seed_debug_is_redacted_without_seed_fragments() {
    let debug = format!("{:?}", started());
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("[0, 1, 2"));
    assert!(!debug.contains("30, 31]"));
}

#[test]
fn registry_accepts_only_the_exact_m6_states_and_known_wrong_state_is_invalid() {
    let registry = synthetic_m6_registry(PROFILE.version());
    assert_eq!(registry.len(), 5);
    let base = RegistryKey {
        service: ServiceKind::Game,
        direction: Direction::ClientToServer,
        version: PROFILE.version(),
        state: ConnectionState::InRoom,
        opcode: SYNTHETIC_M6_C2S_START_STROKE_TWO,
    };
    for (opcode, accepted_state) in [
        (SYNTHETIC_M6_C2S_START_STROKE_TWO, ConnectionState::InRoom),
        (
            SYNTHETIC_M6_C2S_LOADING_COMPLETE,
            ConnectionState::InMatchLoading,
        ),
        (SYNTHETIC_M6_C2S_SHOT_ACTION, ConnectionState::InMatch),
        (SYNTHETIC_M6_C2S_SHOT_RESULT, ConnectionState::InMatch),
        (SYNTHETIC_M6_C2S_GIVE_UP, ConnectionState::InMatch),
    ] {
        for state in [
            ConnectionState::InChannel,
            ConnectionState::InRoom,
            ConnectionState::InMatchLoading,
            ConnectionState::InMatch,
        ] {
            assert_eq!(
                registry.classify(RegistryKey {
                    opcode,
                    state,
                    ..base
                }),
                if state == accepted_state {
                    RegistryLookup::Accepted
                } else {
                    RegistryLookup::InvalidState
                }
            );
        }
    }
    assert_eq!(
        registry.classify(RegistryKey {
            opcode: 0x7f35,
            ..base
        }),
        RegistryLookup::Unknown
    );
    assert_eq!(
        registry.classify(RegistryKey {
            direction: Direction::ServerToClient,
            opcode: SYNTHETIC_M6_C2S_START_STROKE_TWO,
            ..base
        }),
        RegistryLookup::Unknown
    );
}

proptest! {
    #[test]
    fn arbitrary_m6_bodies_never_panic_or_overread(
        opcode_index in 0_usize..14,
        data in prop::collection::vec(any::<u8>(), 0..256),
    ) {
        let (_, opcode, direction) = FIXTURES[opcode_index];
        let mut packet_reader = reader(&data, direction, opcode);
        let _ = match opcode {
            SYNTHETIC_M6_C2S_START_STROKE_TWO => StartStrokeTwo::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_C2S_LOADING_COMPLETE => StrokeLoadingComplete::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_C2S_SHOT_ACTION => StrokeShotAction::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_C2S_SHOT_RESULT => StrokeShotResult::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_C2S_GIVE_UP => StrokeGiveUp::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_S2C_MATCH_STARTED => StrokeMatchStarted::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_S2C_PHASE => StrokePhase::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_S2C_TURN_STARTED => StrokeTurnStarted::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_S2C_ACTION_RELAY => StrokeActionRelay::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_S2C_RESULT_RELAY => StrokeResultRelay::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_S2C_STANDINGS => StrokeStandings::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_S2C_COMMAND_RESULT => StrokeCommandResult::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            SYNTHETIC_M6_S2C_MATCH_ABORTED => StrokeMatchAborted::decode(&mut packet_reader, &PROFILE).map(|_| ()),
            _ => StrokeBalanceUpdate::decode(&mut packet_reader, &PROFILE).map(|_| ()),
        };
        prop_assert!(packet_reader.offset() <= data.len());
        prop_assert_eq!(packet_reader.offset() + packet_reader.remaining(), data.len());
    }
}
