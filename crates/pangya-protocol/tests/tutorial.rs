#![allow(missing_docs)]

use pangya_protocol::{
    CompatibilityProfile, RetailLobbyEquipmentUpdate, RetailRoomEquipmentUpdate, ServiceKind,
    TutorialMission, TutorialStatusCompletion, TutorialStatusLogin, decode_packet_payload,
    encode_packet_payload,
};

#[test]
fn mission_00ae_is_exact_u16_then_u32_and_rejects_tail() {
    let packet = decode_packet_payload::<TutorialMission>(
        &[0x00, 0x01, 0x00, 0x01, 0x00, 0x00],
        &CompatibilityProfile::US_852,
        ServiceKind::Game,
    )
    .expect("tutorial mission");
    assert_eq!(
        packet,
        TutorialMission {
            code: 0x100,
            mission_id: 0x100
        }
    );
    assert!(
        decode_packet_payload::<TutorialMission>(
            &[0, 0, 0, 0, 0, 0, 0],
            &CompatibilityProfile::US_852,
            ServiceKind::Game,
        )
        .is_err()
    );
}

#[test]
fn status_packets_are_exact_reference_bodies() {
    let login = encode_packet_payload(
        &TutorialStatusLogin {
            code: 3,
            mission_id: 0xff,
        },
        &CompatibilityProfile::US_852,
    )
    .expect("login status");
    assert_eq!(
        login.as_slice(),
        [3, 0, 0xff, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    let completion = encode_packet_payload(
        &TutorialStatusCompletion {
            code: 1,
            mission_id: 0xff,
        },
        &CompatibilityProfile::US_852,
    )
    .expect("completion status");
    assert_eq!(completion.as_slice(), [1, 1, 0xff, 0, 0, 0]);
}

#[test]
fn opcode_000b_is_stateful_equipment_not_tutorial() {
    let payload = [4, 0x0b, 0x00, 0x00, 0x04];
    let equipment = decode_packet_payload::<RetailLobbyEquipmentUpdate>(
        &payload,
        &CompatibilityProfile::US_852,
        ServiceKind::Game,
    )
    .expect("packetdoc 000b equipment");
    assert_eq!(
        equipment.0,
        RetailRoomEquipmentUpdate::Character(0x0400_000b)
    );
    assert!(
        decode_packet_payload::<TutorialMission>(
            &payload,
            &CompatibilityProfile::US_852,
            ServiceKind::Game,
        )
        .is_err()
    );
}
