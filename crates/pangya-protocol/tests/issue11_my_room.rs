//! Literal reference-derived wire tests for issue #11.
//!
//! Layouts are from the checked-out `opensource-references/pangbox--packetdoc` corpus:
//! `gameservice/server/012d.ksy`, `gameservice/client/00b9.ksy`, and
//! `gameservice/server/012e.ksy`.  These tests are intentionally added before the runtime
//! implementation so a change cannot silently choose a convenient layout.

use pangya_protocol::{
    CompatibilityProfile, EncodePacket, PacketWriter, RetailMascotMessageUpdate,
    RetailMyRoomFurniture, RetailMyRoomInventoryRequest, RetailMyRoomLayout,
    RetailUccUploadKeyRefusal, ServiceKind, decode_packet_payload, encode_packet_payload,
};

#[test]
fn packetdoc_00b7_literal_fixture_decodes_user_and_option() {
    // Literal bytes for the PacketDoc 0x00b7 body: user_id=0x11223344, unknown_a=1.
    let request = decode_packet_payload::<RetailMyRoomInventoryRequest>(
        &[0x44, 0x33, 0x22, 0x11, 0x01],
        &CompatibilityProfile::US_852,
        ServiceKind::Game,
    )
    .expect("00b7 fixture decodes");
    assert_eq!(request.user_id, 0x1122_3344);
}

#[test]
fn packetdoc_012d_literal_fixture_keeps_opaque_entry_bytes() {
    let layout = RetailMyRoomLayout::new(vec![RetailMyRoomFurniture {
        unknown_prefix: [1, 2, 3, 4],
        item_type_id: 0x0800_1234,
        unknown_suffix: [5; 19],
    }]);
    let mut writer = PacketWriter::default();
    layout
        .encode(&mut writer, &CompatibilityProfile::US_852)
        .expect("012d encodes");
    assert_eq!(
        writer.into_inner(),
        [
            1, 0, 0, 0, 1, 0, // option and one entry
            1, 2, 3, 4, // unknown_b
            0x34, 0x12, 0x00, 0x08, // Furniture.iff item id
            5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, // unknown_c
        ]
    );
}

#[test]
fn mascot_message_update_fixture_round_trips_as_0073() {
    let update = RetailMascotMessageUpdate {
        mascot_id: 0x1122_3344,
        message: b"hello".to_vec(),
    };
    let bytes = encode_packet_payload(&update, &CompatibilityProfile::US_852).expect("encode");
    assert_eq!(
        bytes.as_slice(),
        [0x44, 0x33, 0x22, 0x11, 5, 0, b'h', b'e', b'l', b'l', b'o']
    );
    let decoded = decode_packet_payload::<RetailMascotMessageUpdate>(
        &bytes,
        &CompatibilityProfile::US_852,
        ServiceKind::Game,
    )
    .expect("decode");
    assert_eq!(decoded, update);
}

#[test]
fn ucc_upload_key_refusal_is_an_explicit_0153_response() {
    let refusal = RetailUccUploadKeyRefusal::unsupported();
    let mut writer = PacketWriter::default();
    refusal
        .encode_for(
            &mut writer,
            &CompatibilityProfile::US_852,
            ServiceKind::Game,
        )
        .expect("refusal encodes");
    assert_eq!(writer.into_inner(), vec![1, 1, 0, 0x01, 0x10, 0x05]);
}
