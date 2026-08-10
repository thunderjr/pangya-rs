#![allow(missing_docs)]

use pangya_protocol::{
    CompatibilityProfile, EncodePacket, LoginMessageServerList as MessageServerList,
    MessageServerEntry, PacketWriter, RetailMessageServerList, UnknownBytes,
};

fn entry() -> MessageServerEntry {
    MessageServerEntry {
        name: b"Message".to_vec(),
        id: 9,
        max_users: 300,
        num_users: 2,
        ip_address: b"127.0.0.1".to_vec(),
        port: 30303,
        unknown2: UnknownBytes([0; 2]),
        flags: UnknownBytes([0; 2]),
        unknown3: UnknownBytes([0; 14]),
        char_icon: 0,
    }
}
fn encoded<T: EncodePacket>(packet: &T) -> Vec<u8> {
    let mut writer = PacketWriter::new();
    packet
        .encode(&mut writer, &CompatibilityProfile::US_852)
        .expect("encode");
    writer.into_inner()
}

#[test]
fn login_message_server_list_uses_one_byte_count_and_92_byte_entries() {
    assert_eq!(
        encoded(&MessageServerList {
            servers: vec![entry()]
        })
        .len(),
        1 + 92
    );
    assert_eq!(
        encoded(&MessageServerList {
            servers: vec![entry()]
        })[0],
        1
    );
}

#[test]
fn game_message_server_list_has_distinct_game_opcode() {
    assert_eq!(MessageServerList::OPCODE, 9);
    assert_eq!(RetailMessageServerList::OPCODE, 0x00fc);
    assert_eq!(
        encoded(&RetailMessageServerList {
            servers: vec![entry()]
        })[0],
        1
    );
}
