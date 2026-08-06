//! Strict provisional local-only synthetic M7 economy packets.
//!
//! These generated layouts are not retail PangYa protocol evidence.

use crate::{
    CompatibilityProfile, ConnectionState, DecodePacket, Direction, EncodePacket,
    PacketDecodeError, PacketEncodeError, PacketReader, PacketRegistry, PacketWriter, RegistryKey,
    ServiceKind,
};
use std::num::{NonZeroU32, NonZeroU64};
use uuid::Uuid;

/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const SYNTHETIC_M7_C2S_SHOP_PAGE: u16 = 0x7f40;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const SYNTHETIC_M7_C2S_PURCHASE: u16 = 0x7f41;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const SYNTHETIC_M7_C2S_EQUIP: u16 = 0x7f42;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const SYNTHETIC_M7_C2S_CONSUME: u16 = 0x7f43;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const SYNTHETIC_M7_C2S_REPAIR: u16 = 0x7f44;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const SYNTHETIC_M7_S2C_SHOP_PAGE: u16 = 0x7fc0;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const SYNTHETIC_M7_S2C_COMMAND_RESULT: u16 = 0x7fc1;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const SYNTHETIC_M7_S2C_PURCHASE_COMMITTED: u16 = 0x7fc2;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const SYNTHETIC_M7_S2C_INVENTORY_CHANGED: u16 = 0x7fc3;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const SYNTHETIC_M7_S2C_EQUIPMENT_CHANGED: u16 = 0x7fc4;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const SYNTHETIC_M7_S2C_REPAIR_COMMITTED: u16 = 0x7fc5;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const MAX_SHOP_PAGE_ENTRIES: usize = 50;
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub const MAX_PURCHASE_QUANTITY: u32 = 99;

fn end(r: &PacketReader<'_>) -> Result<(), PacketDecodeError> {
    if r.remaining() == 0 {
        Ok(())
    } else {
        Err(r.invalid("synthetic M7 trailing bytes"))
    }
}
fn uuid(r: &mut PacketReader<'_>) -> Result<Uuid, PacketDecodeError> {
    let v = Uuid::from_bytes(r.array::<16>()?);
    if v.is_nil() {
        Err(r.invalid("operation id must be non-nil"))
    } else {
        Ok(v)
    }
}
fn put_uuid(w: &mut PacketWriter, v: Uuid) {
    w.bytes(v.as_bytes())
}
fn nz32(r: &mut PacketReader<'_>, f: &'static str) -> Result<NonZeroU32, PacketDecodeError> {
    NonZeroU32::new(r.u32_le()?).ok_or_else(|| r.invalid(format!("{f} must be nonzero")))
}
fn nz64(r: &mut PacketReader<'_>, f: &'static str) -> Result<NonZeroU64, PacketDecodeError> {
    NonZeroU64::new(r.u64_le()?).ok_or_else(|| r.invalid(format!("{f} must be nonzero")))
}
fn enc_invalid(field: &'static str) -> PacketEncodeError {
    PacketEncodeError::Invalid { field }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub enum EconomyItemKind {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    ClubSet = 0,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    Ball = 1,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    Consumable = 2,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    CharacterPart = 3,
}
impl EconomyItemKind {
    fn decode(r: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match r.u8()? {
            0 => Ok(Self::ClubSet),
            1 => Ok(Self::Ball),
            2 => Ok(Self::Consumable),
            3 => Ok(Self::CharacterPart),
            _ => Err(r.invalid("unknown economy item kind")),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub enum EconomyCommand {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    ShopPage = 0,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    Purchase = 1,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    Equip = 2,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    Consume = 3,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    Repair = 4,
}
impl EconomyCommand {
    fn decode(r: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match r.u8()? {
            0 => Ok(Self::ShopPage),
            1 => Ok(Self::Purchase),
            2 => Ok(Self::Equip),
            3 => Ok(Self::Consume),
            4 => Ok(Self::Repair),
            _ => Err(r.invalid("unknown economy command")),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub enum EconomyOutcome {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    Success = 0,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    Disabled = 1,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    Invalid = 2,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    NotOwned = 3,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    Incompatible = 4,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    InsufficientPang = 5,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    StackFull = 6,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    VersionConflict = 7,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    IdempotencyDrift = 8,
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    Timeout = 9,
}
impl EconomyOutcome {
    fn decode(r: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match r.u8()? {
            0 => Ok(Self::Success),
            1 => Ok(Self::Disabled),
            2 => Ok(Self::Invalid),
            3 => Ok(Self::NotOwned),
            4 => Ok(Self::Incompatible),
            5 => Ok(Self::InsufficientPang),
            6 => Ok(Self::StackFull),
            7 => Ok(Self::VersionConflict),
            8 => Ok(Self::IdempotencyDrift),
            9 => Ok(Self::Timeout),
            _ => Err(r.invalid("unknown economy outcome")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub struct ShopPageRequest {
    page: u16,
}
impl ShopPageRequest {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn new(page: u16) -> Self {
        Self { page }
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn page(self) -> u16 {
        self.page
    }
}
impl DecodePacket for ShopPageRequest {
    const OPCODE: u16 = SYNTHETIC_M7_C2S_SHOP_PAGE;
    fn decode(
        r: &mut PacketReader<'_>,
        _: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let v = Self::new(r.u16_le()?);
        end(r)?;
        Ok(v)
    }
}
impl EncodePacket for ShopPageRequest {
    const OPCODE: u16 = SYNTHETIC_M7_C2S_SHOP_PAGE;
    fn encode(
        &self,
        w: &mut PacketWriter,
        _: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        w.u16_le(self.page);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub struct PurchaseRequestPacket {
    operation_id: Uuid,
    type_id: NonZeroU32,
    quantity: NonZeroU32,
}
impl PurchaseRequestPacket {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub fn new(operation_id: Uuid, type_id: u32, quantity: u32) -> Result<Self, PacketEncodeError> {
        if operation_id.is_nil() {
            return Err(enc_invalid("operation_id"));
        }
        let type_id = NonZeroU32::new(type_id).ok_or(enc_invalid("type_id"))?;
        let quantity = NonZeroU32::new(quantity)
            .filter(|v| v.get() <= MAX_PURCHASE_QUANTITY)
            .ok_or(enc_invalid("quantity"))?;
        Ok(Self {
            operation_id,
            type_id,
            quantity,
        })
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn operation_id(self) -> Uuid {
        self.operation_id
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn type_id(self) -> u32 {
        self.type_id.get()
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn quantity(self) -> u32 {
        self.quantity.get()
    }
}
impl DecodePacket for PurchaseRequestPacket {
    const OPCODE: u16 = SYNTHETIC_M7_C2S_PURCHASE;
    fn decode(
        r: &mut PacketReader<'_>,
        _: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let op = uuid(r)?;
        let ty = nz32(r, "type_id")?;
        let q = nz32(r, "quantity")?;
        if q.get() > MAX_PURCHASE_QUANTITY {
            return Err(r.invalid("quantity exceeds cap"));
        }
        end(r)?;
        Ok(Self {
            operation_id: op,
            type_id: ty,
            quantity: q,
        })
    }
}
impl EncodePacket for PurchaseRequestPacket {
    const OPCODE: u16 = SYNTHETIC_M7_C2S_PURCHASE;
    fn encode(
        &self,
        w: &mut PacketWriter,
        _: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        put_uuid(w, self.operation_id);
        w.u32_le(self.type_id.get());
        w.u32_le(self.quantity.get());
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub struct EquipRequest {
    operation_id: Uuid,
    expected_version: u32,
    character_id: NonZeroU64,
    club_id: Option<NonZeroU64>,
    ball_id: Option<NonZeroU64>,
}
impl EquipRequest {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub fn new(
        op: Uuid,
        version: u32,
        character: u64,
        club: Option<u64>,
        ball: Option<u64>,
    ) -> Result<Self, PacketEncodeError> {
        if op.is_nil() {
            return Err(enc_invalid("operation_id"));
        }
        let character_id = NonZeroU64::new(character).ok_or(enc_invalid("character_id"))?;
        let club_id = club
            .map(|v| NonZeroU64::new(v).ok_or(enc_invalid("club_id")))
            .transpose()?;
        let ball_id = ball
            .map(|v| NonZeroU64::new(v).ok_or(enc_invalid("ball_id")))
            .transpose()?;
        if club_id == ball_id && club_id.is_some() {
            return Err(enc_invalid("equipment_ids"));
        }
        Ok(Self {
            operation_id: op,
            expected_version: version,
            character_id,
            club_id,
            ball_id,
        })
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn operation_id(self) -> Uuid {
        self.operation_id
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn expected_version(self) -> u32 {
        self.expected_version
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn character_id(self) -> u64 {
        self.character_id.get()
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn club_id(self) -> Option<u64> {
        match self.club_id {
            Some(v) => Some(v.get()),
            None => None,
        }
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn ball_id(self) -> Option<u64> {
        match self.ball_id {
            Some(v) => Some(v.get()),
            None => None,
        }
    }
}
impl DecodePacket for EquipRequest {
    const OPCODE: u16 = SYNTHETIC_M7_C2S_EQUIP;
    fn decode(
        r: &mut PacketReader<'_>,
        _: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let op = uuid(r)?;
        let v = r.u32_le()?;
        let c = nz64(r, "character_id")?;
        let raw_club = r.u64_le()?;
        let raw_ball = r.u64_le()?;
        let club = NonZeroU64::new(raw_club);
        let ball = NonZeroU64::new(raw_ball);
        if club == ball && club.is_some() {
            return Err(r.invalid("club and ball ids must differ"));
        }
        end(r)?;
        Ok(Self {
            operation_id: op,
            expected_version: v,
            character_id: c,
            club_id: club,
            ball_id: ball,
        })
    }
}
impl EncodePacket for EquipRequest {
    const OPCODE: u16 = SYNTHETIC_M7_C2S_EQUIP;
    fn encode(
        &self,
        w: &mut PacketWriter,
        _: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        put_uuid(w, self.operation_id);
        w.u32_le(self.expected_version);
        w.u64_le(self.character_id.get());
        w.u64_le(self.club_id.map_or(0, NonZeroU64::get));
        w.u64_le(self.ball_id.map_or(0, NonZeroU64::get));
        Ok(())
    }
}

macro_rules! inventory_request {
    ($name:ident,$op:expr) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        /// Generated local-synthetic M7 protocol value; not a retail claim.
        pub struct $name {
            operation_id: Uuid,
            inventory_id: NonZeroU64,
        }
        impl $name {
            /// Generated local-synthetic M7 protocol value; not a retail claim.
            pub fn new(operation_id: Uuid, inventory_id: u64) -> Result<Self, PacketEncodeError> {
                if operation_id.is_nil() {
                    return Err(enc_invalid("operation_id"));
                }
                Ok(Self {
                    operation_id,
                    inventory_id: NonZeroU64::new(inventory_id)
                        .ok_or(enc_invalid("inventory_id"))?,
                })
            }
            /// Generated local-synthetic M7 protocol value; not a retail claim.
            pub const fn operation_id(self) -> Uuid {
                self.operation_id
            }
            /// Generated local-synthetic M7 protocol value; not a retail claim.
            pub const fn inventory_id(self) -> u64 {
                self.inventory_id.get()
            }
        }
        impl DecodePacket for $name {
            const OPCODE: u16 = $op;
            fn decode(
                r: &mut PacketReader<'_>,
                _: &CompatibilityProfile,
            ) -> Result<Self, PacketDecodeError> {
                let operation_id = uuid(r)?;
                let inventory_id = nz64(r, "inventory_id")?;
                end(r)?;
                Ok(Self {
                    operation_id,
                    inventory_id,
                })
            }
        }
        impl EncodePacket for $name {
            const OPCODE: u16 = $op;
            fn encode(
                &self,
                w: &mut PacketWriter,
                _: &CompatibilityProfile,
            ) -> Result<(), PacketEncodeError> {
                put_uuid(w, self.operation_id);
                w.u64_le(self.inventory_id.get());
                Ok(())
            }
        }
    };
}
inventory_request!(ConsumeOneRequest, SYNTHETIC_M7_C2S_CONSUME);
inventory_request!(RepairRequest, SYNTHETIC_M7_C2S_REPAIR);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub struct ShopOffer {
    type_id: NonZeroU32,
    kind: EconomyItemKind,
    pang_price: u64,
    max_stack: u32,
    max_durability: u32,
    repair_rate: u32,
}
impl ShopOffer {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub fn new(
        type_id: u32,
        kind: EconomyItemKind,
        pang_price: u64,
        max_stack: u32,
        max_durability: u32,
        repair_rate: u32,
    ) -> Result<Self, PacketEncodeError> {
        let type_id = NonZeroU32::new(type_id).ok_or(enc_invalid("type_id"))?;
        if pang_price == 0 {
            return Err(enc_invalid("pang_price"));
        }
        let valid = match kind {
            EconomyItemKind::Consumable => max_stack > 0 && max_durability == 0 && repair_rate == 0,
            EconomyItemKind::ClubSet => {
                (max_durability == 0 && repair_rate == 0) || (max_durability > 0 && repair_rate > 0)
            }
            EconomyItemKind::Ball | EconomyItemKind::CharacterPart => {
                max_stack == 1 && max_durability == 0 && repair_rate == 0
            }
        };
        if !valid {
            return Err(enc_invalid("offer_shape"));
        }
        Ok(Self {
            type_id,
            kind,
            pang_price,
            max_stack,
            max_durability,
            repair_rate,
        })
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn type_id(self) -> u32 {
        self.type_id.get()
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn kind(self) -> EconomyItemKind {
        self.kind
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn pang_price(self) -> u64 {
        self.pang_price
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn max_stack(self) -> u32 {
        self.max_stack
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn max_durability(self) -> u32 {
        self.max_durability
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn repair_rate(self) -> u32 {
        self.repair_rate
    }
}
fn decode_offer(r: &mut PacketReader<'_>) -> Result<ShopOffer, PacketDecodeError> {
    let ty = nz32(r, "type_id")?;
    let kind = EconomyItemKind::decode(r)?;
    let price = r.u64_le()?;
    let stack = r.u32_le()?;
    let durability = r.u32_le()?;
    let repair = r.u32_le()?;
    ShopOffer::new(ty.get(), kind, price, stack, durability, repair)
        .map_err(|_| r.invalid("invalid shop offer"))
}
fn encode_offer(v: ShopOffer, w: &mut PacketWriter) {
    w.u32_le(v.type_id.get());
    w.u8(v.kind as u8);
    w.u64_le(v.pang_price);
    w.u32_le(v.max_stack);
    w.u32_le(v.max_durability);
    w.u32_le(v.repair_rate)
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub struct ShopPage {
    page: u16,
    total_pages: u16,
    entries: Vec<ShopOffer>,
}
impl ShopPage {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub fn new(
        page: u16,
        total_pages: u16,
        entries: Vec<ShopOffer>,
    ) -> Result<Self, PacketEncodeError> {
        if total_pages == 0 || page >= total_pages || entries.len() > MAX_SHOP_PAGE_ENTRIES {
            return Err(enc_invalid("shop_page"));
        }
        Ok(Self {
            page,
            total_pages,
            entries,
        })
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn page(&self) -> u16 {
        self.page
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn total_pages(&self) -> u16 {
        self.total_pages
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub fn entries(&self) -> &[ShopOffer] {
        &self.entries
    }
}
impl DecodePacket for ShopPage {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_SHOP_PAGE;
    fn decode(
        r: &mut PacketReader<'_>,
        _: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let page = r.u16_le()?;
        let total = r.u16_le()?;
        let count = usize::from(r.u8()?);
        if count > MAX_SHOP_PAGE_ENTRIES {
            return Err(r.invalid("shop page count exceeds cap"));
        }
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            entries.push(decode_offer(r)?)
        }
        end(r)?;
        Self::new(page, total, entries).map_err(|_| r.invalid("invalid shop page"))
    }
}
impl EncodePacket for ShopPage {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_SHOP_PAGE;
    fn encode(
        &self,
        w: &mut PacketWriter,
        _: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        let checked = Self::new(self.page, self.total_pages, self.entries.clone())?;
        w.u16_le(checked.page);
        w.u16_le(checked.total_pages);
        w.u8(u8::try_from(checked.entries.len()).map_err(|_| enc_invalid("count"))?);
        for v in checked.entries {
            encode_offer(v, w)
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub struct EconomyCommandResult {
    command: EconomyCommand,
    outcome: EconomyOutcome,
}
impl EconomyCommandResult {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn new(command: EconomyCommand, outcome: EconomyOutcome) -> Self {
        Self { command, outcome }
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn command(self) -> EconomyCommand {
        self.command
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn outcome(self) -> EconomyOutcome {
        self.outcome
    }
}
impl DecodePacket for EconomyCommandResult {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_COMMAND_RESULT;
    fn decode(
        r: &mut PacketReader<'_>,
        _: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let v = Self::new(EconomyCommand::decode(r)?, EconomyOutcome::decode(r)?);
        end(r)?;
        Ok(v)
    }
}
impl EncodePacket for EconomyCommandResult {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_COMMAND_RESULT;
    fn encode(
        &self,
        w: &mut PacketWriter,
        _: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        w.u8(self.command as u8);
        w.u8(self.outcome as u8);
        Ok(())
    }
}

fn decode_durability(r: &mut PacketReader<'_>) -> Result<Option<u32>, PacketDecodeError> {
    let tag = r.u8()?;
    let value = r.u32_le()?;
    match (tag, value) {
        (0, 0) => Ok(None),
        (1, v) if v > 0 => Ok(Some(v)),
        _ => Err(r.invalid("noncanonical durability option")),
    }
}
fn encode_durability(w: &mut PacketWriter, v: Option<u32>) -> Result<(), PacketEncodeError> {
    match v {
        None => {
            w.u8(0);
            w.u32_le(0)
        }
        Some(0) => return Err(enc_invalid("durability")),
        Some(x) => {
            w.u8(1);
            w.u32_le(x)
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub struct PurchaseCommitted {
    operation_id: Uuid,
    inventory_id: NonZeroU64,
    type_id: NonZeroU32,
    quantity_after: NonZeroU32,
    durability: Option<u32>,
    pang_balance: u64,
}
impl PurchaseCommitted {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub fn new(
        op: Uuid,
        inventory: u64,
        ty: u32,
        qty: u32,
        durability: Option<u32>,
        pang: u64,
    ) -> Result<Self, PacketEncodeError> {
        if op.is_nil() {
            return Err(enc_invalid("operation_id"));
        }
        if durability == Some(0) {
            return Err(enc_invalid("durability"));
        }
        Ok(Self {
            operation_id: op,
            inventory_id: NonZeroU64::new(inventory).ok_or(enc_invalid("inventory_id"))?,
            type_id: NonZeroU32::new(ty).ok_or(enc_invalid("type_id"))?,
            quantity_after: NonZeroU32::new(qty).ok_or(enc_invalid("quantity"))?,
            durability,
            pang_balance: pang,
        })
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn operation_id(self) -> Uuid {
        self.operation_id
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn inventory_id(self) -> u64 {
        self.inventory_id.get()
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn type_id(self) -> u32 {
        self.type_id.get()
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn quantity_after(self) -> u32 {
        self.quantity_after.get()
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn durability(self) -> Option<u32> {
        self.durability
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn pang_balance(self) -> u64 {
        self.pang_balance
    }
}
impl DecodePacket for PurchaseCommitted {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_PURCHASE_COMMITTED;
    fn decode(
        r: &mut PacketReader<'_>,
        _: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let op = uuid(r)?;
        let id = nz64(r, "inventory_id")?;
        let ty = nz32(r, "type_id")?;
        let qty = nz32(r, "quantity")?;
        let d = decode_durability(r)?;
        let pang = r.u64_le()?;
        end(r)?;
        Ok(Self {
            operation_id: op,
            inventory_id: id,
            type_id: ty,
            quantity_after: qty,
            durability: d,
            pang_balance: pang,
        })
    }
}
impl EncodePacket for PurchaseCommitted {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_PURCHASE_COMMITTED;
    fn encode(
        &self,
        w: &mut PacketWriter,
        _: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        put_uuid(w, self.operation_id);
        w.u64_le(self.inventory_id.get());
        w.u32_le(self.type_id.get());
        w.u32_le(self.quantity_after.get());
        encode_durability(w, self.durability)?;
        w.u64_le(self.pang_balance);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub struct InventoryChanged {
    operation_id: Uuid,
    inventory_id: NonZeroU64,
    type_id: NonZeroU32,
    quantity_after: u32,
    durability: Option<u32>,
}
impl InventoryChanged {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub fn new(
        op: Uuid,
        id: u64,
        ty: u32,
        qty: u32,
        d: Option<u32>,
    ) -> Result<Self, PacketEncodeError> {
        if op.is_nil() {
            return Err(enc_invalid("operation_id"));
        }
        if d == Some(0) {
            return Err(enc_invalid("durability"));
        }
        Ok(Self {
            operation_id: op,
            inventory_id: NonZeroU64::new(id).ok_or(enc_invalid("inventory_id"))?,
            type_id: NonZeroU32::new(ty).ok_or(enc_invalid("type_id"))?,
            quantity_after: qty,
            durability: d,
        })
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn operation_id(self) -> Uuid {
        self.operation_id
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn inventory_id(self) -> u64 {
        self.inventory_id.get()
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn type_id(self) -> u32 {
        self.type_id.get()
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn quantity_after(self) -> u32 {
        self.quantity_after
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn durability(self) -> Option<u32> {
        self.durability
    }
}
impl DecodePacket for InventoryChanged {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_INVENTORY_CHANGED;
    fn decode(
        r: &mut PacketReader<'_>,
        _: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let op = uuid(r)?;
        let id = nz64(r, "inventory_id")?;
        let ty = nz32(r, "type_id")?;
        let qty = r.u32_le()?;
        let d = decode_durability(r)?;
        end(r)?;
        Ok(Self {
            operation_id: op,
            inventory_id: id,
            type_id: ty,
            quantity_after: qty,
            durability: d,
        })
    }
}
impl EncodePacket for InventoryChanged {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_INVENTORY_CHANGED;
    fn encode(
        &self,
        w: &mut PacketWriter,
        _: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        put_uuid(w, self.operation_id);
        w.u64_le(self.inventory_id.get());
        w.u32_le(self.type_id.get());
        w.u32_le(self.quantity_after);
        encode_durability(w, self.durability)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub struct EquipmentChanged {
    operation_id: Uuid,
    character_id: NonZeroU64,
    club_id: Option<NonZeroU64>,
    ball_id: Option<NonZeroU64>,
    version: u32,
}
impl EquipmentChanged {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub fn new(
        op: Uuid,
        character: u64,
        club: Option<u64>,
        ball: Option<u64>,
        version: u32,
    ) -> Result<Self, PacketEncodeError> {
        let req = EquipRequest::new(op, version, character, club, ball)?;
        Ok(Self {
            operation_id: req.operation_id,
            character_id: req.character_id,
            club_id: req.club_id,
            ball_id: req.ball_id,
            version,
        })
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn operation_id(self) -> Uuid {
        self.operation_id
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn character_id(self) -> u64 {
        self.character_id.get()
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn club_id(self) -> Option<u64> {
        match self.club_id {
            Some(v) => Some(v.get()),
            None => None,
        }
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn ball_id(self) -> Option<u64> {
        match self.ball_id {
            Some(v) => Some(v.get()),
            None => None,
        }
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn version(self) -> u32 {
        self.version
    }
}
impl DecodePacket for EquipmentChanged {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_EQUIPMENT_CHANGED;
    fn decode(
        r: &mut PacketReader<'_>,
        _: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let op = uuid(r)?;
        let character = nz64(r, "character_id")?;
        let club = NonZeroU64::new(r.u64_le()?);
        let ball = NonZeroU64::new(r.u64_le()?);
        let version = r.u32_le()?;
        if club == ball && club.is_some() {
            return Err(r.invalid("equipment ids must differ"));
        }
        end(r)?;
        Ok(Self {
            operation_id: op,
            character_id: character,
            club_id: club,
            ball_id: ball,
            version,
        })
    }
}
impl EncodePacket for EquipmentChanged {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_EQUIPMENT_CHANGED;
    fn encode(
        &self,
        w: &mut PacketWriter,
        _: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        put_uuid(w, self.operation_id);
        w.u64_le(self.character_id.get());
        w.u64_le(self.club_id.map_or(0, NonZeroU64::get));
        w.u64_le(self.ball_id.map_or(0, NonZeroU64::get));
        w.u32_le(self.version);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Generated local-synthetic M7 protocol value; not a retail claim.
pub struct RepairCommitted {
    operation_id: Uuid,
    inventory_id: NonZeroU64,
    durability: NonZeroU32,
    pang_balance: u64,
}
impl RepairCommitted {
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub fn new(op: Uuid, id: u64, durability: u32, pang: u64) -> Result<Self, PacketEncodeError> {
        if op.is_nil() {
            return Err(enc_invalid("operation_id"));
        }
        Ok(Self {
            operation_id: op,
            inventory_id: NonZeroU64::new(id).ok_or(enc_invalid("inventory_id"))?,
            durability: NonZeroU32::new(durability).ok_or(enc_invalid("durability"))?,
            pang_balance: pang,
        })
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn operation_id(self) -> Uuid {
        self.operation_id
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn inventory_id(self) -> u64 {
        self.inventory_id.get()
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn durability(self) -> u32 {
        self.durability.get()
    }
    /// Generated local-synthetic M7 protocol value; not a retail claim.
    pub const fn pang_balance(self) -> u64 {
        self.pang_balance
    }
}
impl DecodePacket for RepairCommitted {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_REPAIR_COMMITTED;
    fn decode(
        r: &mut PacketReader<'_>,
        _: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let op = uuid(r)?;
        let id = nz64(r, "inventory_id")?;
        let d = nz32(r, "durability")?;
        let pang = r.u64_le()?;
        end(r)?;
        Ok(Self {
            operation_id: op,
            inventory_id: id,
            durability: d,
            pang_balance: pang,
        })
    }
}
impl EncodePacket for RepairCommitted {
    const OPCODE: u16 = SYNTHETIC_M7_S2C_REPAIR_COMMITTED;
    fn encode(
        &self,
        w: &mut PacketWriter,
        _: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        put_uuid(w, self.operation_id);
        w.u64_le(self.inventory_id.get());
        w.u32_le(self.durability.get());
        w.u64_le(self.pang_balance);
        Ok(())
    }
}

/// Generated M7 inbound registry; all economy commands are channel-only.
pub fn synthetic_m7_registry() -> PacketRegistry {
    let mut r = PacketRegistry::default();
    for opcode in [
        SYNTHETIC_M7_C2S_SHOP_PAGE,
        SYNTHETIC_M7_C2S_PURCHASE,
        SYNTHETIC_M7_C2S_EQUIP,
        SYNTHETIC_M7_C2S_CONSUME,
        SYNTHETIC_M7_C2S_REPAIR,
    ] {
        r.register(RegistryKey {
            service: ServiceKind::Game,
            direction: Direction::ClientToServer,
            version: crate::ClientVersion::Us852,
            state: ConnectionState::InChannel,
            opcode,
        });
    }
    r
}
