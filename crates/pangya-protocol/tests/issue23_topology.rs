#![allow(missing_docs)]

use pangya_protocol::{
    CompatibilityProfile, EncodePacket, RetailNewSessionKey, RetailServerEntry, RetailServerList,
    RetailServerListRequest, RetailServerTime, RetailSubServerConnect, RetailSubServerEntry,
    ServiceKind, UnknownBytes, decode_packet_payload, encode_packet_payload,
    is_retail_accepted_session_opcode,
};

const PROFILE: CompatibilityProfile = CompatibilityProfile::US_852;

#[test]
fn unresolved_booster_and_keepalive_are_explicitly_inert_not_unknown() {
    assert!(is_retail_accepted_session_opcode(0x0065));
    assert!(is_retail_accepted_session_opcode(0x0088));
    assert!(is_retail_accepted_session_opcode(0x00f4));
}

#[test]
fn topology_requests_use_reference_opcodes_and_empty_bodies() {
    let request =
        decode_packet_payload::<RetailServerListRequest>(&[], &PROFILE, ServiceKind::Game)
            .expect("server list request");
    assert_eq!(request, RetailServerListRequest);
    let connect =
        decode_packet_payload::<RetailSubServerConnect>(&[7], &PROFILE, ServiceKind::Game)
            .expect("sub-server connect");
    assert_eq!(connect.sub_server_id, 7);
    assert!(
        decode_packet_payload::<RetailSubServerConnect>(&[7, 8], &PROFILE, ServiceKind::Game)
            .is_err()
    );
}

#[test]
fn server_list_response_is_the_009f_layout() {
    let payload = encode_packet_payload(
        &RetailServerList {
            servers: vec![],
            sub_servers: vec![],
        },
        &PROFILE,
    )
    .expect("server list");
    assert_eq!(RetailServerList::OPCODE, 0x009f);
    assert_eq!(payload.as_slice(), [0, 0]);
    let populated = encode_packet_payload(
        &RetailServerList {
            servers: vec![RetailServerEntry {
                name: b"game".to_vec(),
                id: 1,
                user_max: 200,
                user_count: 3,
                ip: b"127.0.0.1".to_vec(),
                port: 20201,
                unknown_c: UnknownBytes([0; 2]),
                flags: UnknownBytes([0; 2]),
                unknown_d: UnknownBytes([0; 14]),
                icon: 1,
            }],
            sub_servers: vec![RetailSubServerEntry {
                name: b"channel".to_vec(),
                unknown_a: UnknownBytes([0; 47]),
                id: 1,
                unknown_b: UnknownBytes([0; 8]),
            }],
        },
        &PROFILE,
    )
    .expect("populated server list");
    assert_eq!(populated.len(), 2 + 92 + 77);
}

#[test]
fn every_issue23_session_opcode_is_explicitly_accepted() {
    for opcode in [
        0x0043, 0x0047, 0x005c, 0x0065, 0x0083, 0x0088, 0x008b, 0x009c, 0x00a1, 0x00a2,
        0x00f4, 0x00fb, 0x00fe, 0x0119,
    ] {
        assert!(
            is_retail_accepted_session_opcode(opcode),
            "issue #23 opcode {opcode:#06x} must not hit unknown-opcode policy"
        );
    }
}

#[test]
fn recent_player_slots_follow_reference_field_order() {
    let payload = encode_packet_payload(
        &pangya_protocol::RetailPlayerHistoryEntries {
            entries: vec![pangya_protocol::RetailRecentPlayerSlot {
                account_id: 0x1122_3344,
                nickname: b"Nick".to_vec(),
                secondary_name: b"User".to_vec(),
                unknown: 0xaabb_ccdd,
            }],
        },
        &PROFILE,
    )
    .expect("history");
    assert_eq!(&payload[0..4], &[0xdd, 0xcc, 0xbb, 0xaa], "unknown field comes first");
    assert_eq!(&payload[4..8], b"Nick");
    assert_eq!(&payload[26..30], b"User");
    assert_eq!(&payload[48..52], &[0x44, 0x33, 0x22, 0x11]);
    assert_eq!(payload.len(), 260);
}

#[test]
fn new_session_key_is_01d4_and_redacted() {
    let packet = RetailNewSessionKey {
        unknown: UnknownBytes([1, 2, 3, 4]),
        session_key: b"handover-token".to_vec().into(),
    };
    assert_eq!(RetailNewSessionKey::OPCODE, 0x01d4);
    assert_eq!(
        encode_packet_payload(&packet, &PROFILE)
            .expect("key")
            .as_slice(),
        [
            1, 2, 3, 4, 14, 0, b'h', b'a', b'n', b'd', b'o', b'v', b'e', b'r', b'-', b't', b'o',
            b'k', b'e', b'n'
        ]
    );
    assert!(!format!("{packet:?}").contains("handover-token"));
}

#[test]
fn server_time_is_ba_with_eight_u16_fields() {
    let payload = encode_packet_payload(
        &RetailServerTime {
            year: 2026,
            month: 8,
            weekday: 6,
            day: 8,
            hour: 12,
            minute: 34,
            second: 56,
            millisecond: 789,
        },
        &PROFILE,
    )
    .expect("time");
    assert_eq!(RetailServerTime::OPCODE, 0x00ba);
    assert_eq!(payload.len(), 16);
    assert_eq!(&payload[..4], &[0xea, 0x07, 8, 0]);
}
