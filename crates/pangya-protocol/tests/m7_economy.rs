//! Generated local synthetic M7 economy protocol tests; no client/source bytes.

use pangya_protocol::*;
use proptest::prelude::*;
use uuid::Uuid;

const PROFILE: CompatibilityProfile = CompatibilityProfile::US_852;
fn body<T: EncodePacket>(value: &T) -> Vec<u8> {
    let mut w = PacketWriter::new();
    value.encode(&mut w, &PROFILE).expect("encode");
    w.into_inner()
}
fn decode<T: DecodePacket>(bytes: &[u8]) -> Result<T, PacketDecodeError> {
    let mut r = PacketReader::new(
        bytes,
        Direction::ClientToServer,
        ServiceKind::Game,
        Some(T::OPCODE),
    );
    T::decode(&mut r, &PROFILE)
}
fn op(n: u8) -> Uuid {
    Uuid::from_bytes([n, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15])
}

#[test]
fn all_eleven_layouts_round_trip_exactly() {
    let offer = ShopOffer::new(100, EconomyItemKind::Consumable, 25, 99, 0, 0).expect("offer");
    let page = ShopPage::new(0, 1, vec![offer]).expect("page");
    let purchase = PurchaseRequestPacket::new(op(1), 100, 2).expect("purchase");
    let equip = EquipRequest::new(op(2), 3, 4, Some(5), Some(6)).expect("equip");
    let consume = ConsumeOneRequest::new(op(3), 7).expect("consume");
    let repair = RepairRequest::new(op(4), 8).expect("repair");
    let command = EconomyCommandResult::new(EconomyCommand::Purchase, EconomyOutcome::Success);
    let bought = PurchaseCommitted::new(op(5), 9, 100, 2, None, 950).expect("bought");
    let changed = InventoryChanged::new(op(6), 9, 100, 0, None).expect("changed");
    let equipped = EquipmentChanged::new(op(7), 4, Some(5), Some(6), 4).expect("equipped");
    let repaired = RepairCommitted::new(op(8), 5, 100, 900).expect("repaired");
    macro_rules! rt {
        ($v:expr,$t:ty) => {{
            let bytes = body(&$v);
            assert_eq!(decode::<$t>(&bytes).expect("decode"), $v);
        }};
    }
    rt!(ShopPageRequest::new(2), ShopPageRequest);
    rt!(purchase, PurchaseRequestPacket);
    rt!(equip, EquipRequest);
    rt!(consume, ConsumeOneRequest);
    rt!(repair, RepairRequest);
    rt!(page, ShopPage);
    rt!(command, EconomyCommandResult);
    rt!(bought, PurchaseCommitted);
    rt!(changed, InventoryChanged);
    rt!(equipped, EquipmentChanged);
    rt!(repaired, RepairCommitted);
}

#[test]
fn every_layout_rejects_all_truncations_and_trailing_bytes() {
    type Decoder = fn(&[u8]) -> bool;
    let values: Vec<(Vec<u8>, Decoder)> = vec![
        (body(&ShopPageRequest::new(0)), |b| {
            decode::<ShopPageRequest>(b).is_ok()
        }),
        (
            body(&PurchaseRequestPacket::new(op(1), 1, 1).expect("v")),
            |b| decode::<PurchaseRequestPacket>(b).is_ok(),
        ),
        (
            body(&EquipRequest::new(op(2), 0, 1, None, None).expect("v")),
            |b| decode::<EquipRequest>(b).is_ok(),
        ),
        (body(&ConsumeOneRequest::new(op(3), 1).expect("v")), |b| {
            decode::<ConsumeOneRequest>(b).is_ok()
        }),
        (body(&RepairRequest::new(op(4), 1).expect("v")), |b| {
            decode::<RepairRequest>(b).is_ok()
        }),
        (
            body(
                &ShopPage::new(
                    0,
                    1,
                    vec![
                        ShopOffer::new(1, EconomyItemKind::Consumable, 1, 2, 0, 0).expect("offer"),
                    ],
                )
                .expect("page"),
            ),
            |b| decode::<ShopPage>(b).is_ok(),
        ),
        (
            body(&EconomyCommandResult::new(
                EconomyCommand::ShopPage,
                EconomyOutcome::Success,
            )),
            |b| decode::<EconomyCommandResult>(b).is_ok(),
        ),
        (
            body(&PurchaseCommitted::new(op(5), 1, 2, 1, None, 0).expect("v")),
            |b| decode::<PurchaseCommitted>(b).is_ok(),
        ),
        (
            body(&InventoryChanged::new(op(6), 1, 2, 0, None).expect("v")),
            |b| decode::<InventoryChanged>(b).is_ok(),
        ),
        (
            body(&EquipmentChanged::new(op(7), 1, None, None, 0).expect("v")),
            |b| decode::<EquipmentChanged>(b).is_ok(),
        ),
        (
            body(&RepairCommitted::new(op(8), 1, 1, 0).expect("v")),
            |b| decode::<RepairCommitted>(b).is_ok(),
        ),
    ];
    for (bytes, f) in values {
        for n in 0..bytes.len() {
            assert!(!f(&bytes[..n]));
        }
        let mut tail = bytes.clone();
        tail.push(0);
        assert!(!f(&tail));
    }
}

#[test]
fn constructors_and_wire_reject_noncanonical_authority() {
    assert!(PurchaseRequestPacket::new(Uuid::nil(), 1, 1).is_err());
    assert!(PurchaseRequestPacket::new(op(1), 1, 0).is_err());
    assert!(PurchaseRequestPacket::new(op(1), 1, 100).is_err());
    assert!(EquipRequest::new(op(1), 0, 0, None, None).is_err());
    assert!(EquipRequest::new(op(1), 0, 1, Some(2), Some(2)).is_err());
    assert!(ShopOffer::new(1, EconomyItemKind::Consumable, 1, 0, 0, 0).is_err());
    assert!(ShopOffer::new(1, EconomyItemKind::ClubSet, 1, 1, 10, 0).is_err());
    assert!(PurchaseCommitted::new(op(1), 1, 2, 1, Some(0), 0).is_err());
    let mut bad = body(&PurchaseRequestPacket::new(op(1), 1, 1).expect("v"));
    bad[16..20].copy_from_slice(&0u32.to_le_bytes());
    assert!(decode::<PurchaseRequestPacket>(&bad).is_err());
}

#[test]
fn registry_is_channel_only_and_unknown_stays_unknown() {
    let registry = synthetic_m7_registry();
    for opcode in [
        SYNTHETIC_M7_C2S_SHOP_PAGE,
        SYNTHETIC_M7_C2S_PURCHASE,
        SYNTHETIC_M7_C2S_EQUIP,
        SYNTHETIC_M7_C2S_CONSUME,
        SYNTHETIC_M7_C2S_REPAIR,
    ] {
        let key = |state| RegistryKey {
            service: ServiceKind::Game,
            direction: Direction::ClientToServer,
            version: ClientVersion::Us852,
            state,
            opcode,
        };
        assert_eq!(
            registry.classify(key(ConnectionState::InChannel)),
            RegistryLookup::Accepted
        );
        assert_eq!(
            registry.classify(key(ConnectionState::InRoom)),
            RegistryLookup::InvalidState
        );
    }
    let key = RegistryKey {
        service: ServiceKind::Game,
        direction: Direction::ClientToServer,
        version: ClientVersion::Us852,
        state: ConnectionState::InChannel,
        opcode: 0x7f4f,
    };
    assert_eq!(registry.classify(key), RegistryLookup::Unknown);
}

proptest! {
 #[test]
 fn arbitrary_bodies_never_panic(data in proptest::collection::vec(any::<u8>(),0..256)){
   let _=decode::<ShopPageRequest>(&data);let _=decode::<PurchaseRequestPacket>(&data);let _=decode::<EquipRequest>(&data);
   let _=decode::<ConsumeOneRequest>(&data);let _=decode::<RepairRequest>(&data);let _=decode::<ShopPage>(&data);
   let _=decode::<EconomyCommandResult>(&data);let _=decode::<PurchaseCommitted>(&data);let _=decode::<InventoryChanged>(&data);
   let _=decode::<EquipmentChanged>(&data);let _=decode::<RepairCommitted>(&data);
 }
}
