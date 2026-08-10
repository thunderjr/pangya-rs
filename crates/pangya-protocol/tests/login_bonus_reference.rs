//! PacketDoc literal layouts for the login-bonus protocol pair.

use pangya_protocol::{
    CompatibilityProfile, DecodePacket, Direction, EncodePacket, PacketReader,
    RetailLoginBonusClaimRequest, RetailLoginBonusClaimResponse, RetailLoginBonusStatus,
    ServiceKind,
};

#[test]
fn claim_request_literal_fixture_is_a_typed_zero_payload_packet() {
    // Keep the fixture literal and independent from the encoder: this catches both opcode drift
    // and an accidental payload field in the request layout.
    let fixture = include_bytes!("fixtures/login-in-016f/fixture.bin");
    assert_eq!(fixture, &[0x6f, 0x01]);
    let mut reader = PacketReader::new(
        &fixture[2..],
        Direction::ClientToServer,
        ServiceKind::Game,
        Some(RetailLoginBonusClaimRequest::OPCODE),
    );
    assert_eq!(
        RetailLoginBonusClaimRequest::decode(&mut reader, &CompatibilityProfile::US_852)
            .expect("claim request"),
        RetailLoginBonusClaimRequest
    );
}

#[test]
fn status_uncollected_branch_is_the_reference_25_byte_union() {
    let packet = RetailLoginBonusStatus::Uncollected {
        unknown_a: [0; 4],
        current_item_id: 0x1a00_0001,
        current_item_quantity: 3,
        padding_a: [0; 8],
        current_bonus_day: 7,
    };
    let mut writer = pangya_protocol::PacketWriter::new();
    packet
        .encode(&mut writer, &CompatibilityProfile::US_852)
        .expect("encode");
    let bytes = writer.into_inner();
    assert_eq!(bytes.len(), 25);
    assert_eq!(bytes[4], 0);
    assert_eq!(
        RetailLoginBonusStatus::decode(
            &mut PacketReader::new(
                &bytes,
                Direction::ServerToClient,
                ServiceKind::Game,
                Some(0x0248)
            ),
            &CompatibilityProfile::US_852,
        )
        .expect("decode"),
        packet
    );
}

#[test]
fn claim_response_has_five_opaque_bytes_and_five_words() {
    let packet = RetailLoginBonusClaimResponse {
        unknown_a: [0; 5],
        current_item_id: 1,
        current_item_quantity: 2,
        future_item_id: 3,
        future_item_quantity: 4,
        current_bonus_day: 5,
    };
    let mut writer = pangya_protocol::PacketWriter::new();
    packet
        .encode(&mut writer, &CompatibilityProfile::US_852)
        .expect("encode");
    assert_eq!(writer.into_inner().len(), 25);
}
