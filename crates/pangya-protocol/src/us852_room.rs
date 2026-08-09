//! Reference-derived U.S. 852 retail lobby and room packets.
//!
//! Derived from the vendored `pangbox--packetdoc` definitions and corroborated against a
//! GB.852-targeting reference server's observable protocol behavior. **None has been
//! accepted by a real client.** These supersede the synthetic `0x7f00`/`0x7f80` family.
//!
//! Where PacketDoc and the reference server disagree on how the fixed middle of a room
//! record is subdivided, the reference server wins: it is the one demonstrably accepted by
//! a client. The totals agree either way.

use crate::{
    CHARACTER_BLOCK_BYTES, CompatibilityProfile, DecodePacket, EncodePacket, PacketDecodeError,
    PacketEncodeError, PacketReader, PacketWriter, RetailCharacter,
};

/// Fixed byte width of a room name on the wire.
pub const ROOM_NAME_BYTES: usize = 64;
/// Maximum rooms one list packet may carry.
pub const MAX_ROOMS_PER_LIST: usize = 255;
/// Maximum players a room may hold.
pub const MAX_ROOM_PLAYERS: u8 = 30;
/// Exact wire width of one room record.
pub const ROOM_RECORD_BYTES: usize = 210;

fn check_decode_profile(
    profile: &CompatibilityProfile,
    reader: &PacketReader<'_>,
) -> Result<(), PacketDecodeError> {
    profile
        .require_us852()
        .map_err(|error| reader.invalid(error.to_string()))
}

fn check_encode_profile(profile: &CompatibilityProfile) -> Result<(), PacketEncodeError> {
    profile.require_us852().map_err(Into::into)
}

/// Retail room modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RetailRoomType {
    /// Standard versus play.
    Versus = 0x00,
    /// Chat-only lounge room.
    Chat = 0x02,
    /// Tournament play.
    Tournament = 0x04,
    /// Pang battle.
    Battle = 0x0a,
}

impl RetailRoomType {
    /// Maps a wire value onto a known room type.
    #[must_use]
    pub const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(Self::Versus),
            0x02 => Some(Self::Chat),
            0x04 => Some(Self::Tournament),
            0x0a => Some(Self::Battle),
            _ => None,
        }
    }
}

/// Retail hole progression modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RetailHoleProgression {
    /// Play the front holes in order.
    FrontStart = 0x00,
    /// Play the back holes in order.
    BackStart = 0x01,
    /// Start at a random hole.
    RandomStart = 0x02,
    /// Shuffle every hole.
    ShuffleAll = 0x03,
}

/// Room creation request, client opcode `0x0008`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomCreate {
    /// Per-shot timer in milliseconds; used by versus and chat rooms.
    pub shot_timer_ms: u32,
    /// Whole-game timer in milliseconds; used by tournament and battle rooms.
    pub game_timer_ms: u32,
    /// Requested player capacity.
    pub max_players: u8,
    /// Requested mode.
    pub room_type: u8,
    /// Hole count, or the fixed hole in a chat room.
    pub hole_count: u8,
    /// Course ordinal.
    pub course: u8,
    /// Display name.
    pub name: Vec<u8>,
    /// Password; empty means public.
    pub password: Vec<u8>,
}

impl DecodePacket for RetailRoomCreate {
    const OPCODE: u16 = 0x0008;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let _unknown_a = reader.u8()?;
        let shot_timer_ms = reader.u32_le()?;
        let game_timer_ms = reader.u32_le()?;
        let max_players = reader.u8()?;
        let room_type = reader.u8()?;
        let hole_count = reader.u8()?;
        let course = reader.u8()?;
        let _unknown_b = reader.array::<5>()?;
        let name = reader.pstring(ROOM_NAME_BYTES)?.to_vec();
        let password = reader.pstring(ROOM_NAME_BYTES)?.to_vec();
        Ok(Self {
            shot_timer_ms,
            game_timer_ms,
            max_players,
            room_type,
            hole_count,
            course,
            name,
            password,
        })
    }
}

/// Room join request, client opcode `0x0009`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomJoin {
    /// Target room number as advertised in the room list.
    pub room_number: u16,
    /// Password attempt; empty for a public room.
    pub password: Vec<u8>,
}

impl DecodePacket for RetailRoomJoin {
    const OPCODE: u16 = 0x0009;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self {
            room_number: reader.u16_le()?,
            password: reader.pstring(ROOM_NAME_BYTES)?.to_vec(),
        })
    }
}

/// Room state as the list advertises it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetailRoomState {
    /// Waiting in the lobby, joinable.
    Lobby,
    /// Match running but still joinable.
    InGameJoinable,
    /// Match running and closed.
    InGame,
}

/// One advertised room.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoom {
    /// Display name.
    pub name: Vec<u8>,
    /// Whether the room takes no password.
    pub public: bool,
    /// Current lifecycle state.
    pub state: RetailRoomState,
    /// Capacity.
    pub max_players: u8,
    /// Current occupancy.
    pub player_count: u8,
    /// Hole count.
    pub hole_count: u8,
    /// A second mode byte whose meaning is not established; kept at zero.
    pub play_mode: u8,
    /// Mode, as the client's own create request spelled it.
    ///
    /// Carried verbatim rather than mapped onto [`RetailRoomType`]: the client has modes this
    /// server does not model — its single-player practice room is one — and it renders its own
    /// header and gates its Start button on getting the mode it asked for back.
    pub mode: u8,
    /// Room number.
    pub id: u16,
    /// Hole progression.
    pub hole_progression: RetailHoleProgression,
    /// Course ordinal.
    pub course: u8,
    /// Per-shot timer in milliseconds.
    pub shot_timer_ms: u32,
    /// Whole-game timer in milliseconds.
    pub game_timer_ms: u32,
    /// Numeric identity of the room owner.
    pub owner_uid: u32,
    /// Whether wind varies naturally.
    pub natural_wind: bool,
}

impl RetailRoom {
    fn encode_body(&self, writer: &mut PacketWriter) -> Result<(), PacketEncodeError> {
        let start = writer.as_slice().len();
        writer.fixed_nul(&self.name, ROOM_NAME_BYTES)?;
        writer.u8(u8::from(self.public));
        writer.u8(u8::from(self.state == RetailRoomState::Lobby));
        writer.u8(u8::from(self.state == RetailRoomState::InGameJoinable));
        writer.u8(self.max_players);
        writer.u8(self.player_count);
        writer.bytes(&[0; 17]);
        writer.u8(MAX_ROOM_PLAYERS);
        writer.u8(self.hole_count);
        writer.u8(self.mode);
        writer.u16_le(self.id);
        writer.u8(self.hole_progression as u8);
        writer.u8(self.course);
        writer.u32_le(self.shot_timer_ms);
        writer.u32_le(self.game_timer_ms);
        writer.u32_le(0); // trophy catalog id
        writer.u16_le(0);
        writer.bytes(&[0; 66]); // guild info
        writer.u32_le(100);
        writer.u32_le(100);
        writer.u32_le(self.owner_uid);
        // A second mode byte, held at zero. Echoing the room's own mode into it was tried and
        // changed nothing on screen, so what it selects is still unestablished; zero is what
        // every room this server described carried before the room profile existed.
        writer.u8(self.play_mode);
        writer.u32_le(0); // artifact catalog id
        writer.u32_le(u32::from(self.natural_wind));
        for _ in 0..4 {
            writer.u32_le(0); // event info
        }
        debug_assert_eq!(writer.as_slice().len() - start, ROOM_RECORD_BYTES);
        Ok(())
    }
}

/// The settings of the room a client is sitting in, server opcode `0x004a`.
///
/// Sent when a room is joined and whenever its settings change. It is the answer to a client
/// room edit (`0x000a`): the client asks for a change and learns from this what the room
/// actually is now, which for this server is what it already was — one hole on the configured
/// course.
///
/// The two constants are pinned by captures rather than derived: the leading `u16` is `0xffff`
/// in every recorded example, and the `u16` after the capacity is `30`. `pangbox/server` writes
/// zero for both and its clients cope, but the captures are the stronger evidence.
///
/// # Provenance
///
/// Layout from `pangbox/server` (`game/packet/server.go` `ServerRoomStatus`, filled by
/// `game/room/room.go` `roomStatus`), ISC licensed, cross-checked against
/// `pangbox/packetdoc` `004a.ksy`, `Acrisio-Filho/SuperSS-Dev` `pacote04A` and
/// `hsreina/pangya-server` `TriggerGameUpdated`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomStatus {
    /// The room these settings describe.
    pub room: RetailRoom,
}

impl EncodePacket for RetailRoomStatus {
    const OPCODE: u16 = 0x004a;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u16_le(0xffff);
        writer.u8(self.room.mode);
        writer.u8(self.room.course);
        writer.u8(self.room.hole_count);
        writer.u8(self.room.hole_progression as u8);
        writer.u32_le(u32::from(self.room.natural_wind));
        writer.u8(self.room.max_players);
        writer.u16_le(30);
        writer.u32_le(self.room.shot_timer_ms);
        writer.u32_le(self.room.game_timer_ms);
        writer.u32_le(0); // flags
        // Upstream sends the same broadcast to every occupant with this byte set, so it names
        // the room's own ownership rather than the recipient's.
        writer.u8(1);
        writer.pstring(&self.room.name, ROOM_NAME_BYTES)?;
        Ok(())
    }
}

/// How a room list frame relates to what the client already holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RoomListKind {
    /// Replaces the client's list.
    Initial = 0,
    /// Adds rooms.
    Additions = 1,
    /// Removes rooms.
    Removals = 2,
    /// Updates rooms already listed.
    Modifications = 3,
}

/// Maximum line items one retail purchase may carry.
pub const MAX_RETAIL_PURCHASE_ITEMS: usize = 32;

/// One line item in a retail purchase.
///
/// The two cost fields are what the *client* believes the item costs. They are decoded because
/// they are on the wire, and then deliberately ignored: price comes from the server's catalog, so
/// a modified client cannot name its own price.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailPurchaseItem {
    /// Catalog type being bought.
    pub item_type_id: u32,
    /// Requested quantity.
    pub quantity: u32,
    /// Client-asserted pang cost. Not authoritative.
    pub claimed_cost_pang: u32,
    /// Client-asserted point cost. Not authoritative.
    pub claimed_cost_point: u32,
}

/// Shop purchase, client opcode `0x001d`.
///
/// # Provenance
///
/// Layout from `pangbox/server` (`game/packet/client.go` `ClientBuyItem`, `PurchaseItem`), ISC
/// licensed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailPurchaseRequest {
    /// Requested line items.
    pub items: Vec<RetailPurchaseItem>,
}

impl DecodePacket for RetailPurchaseRequest {
    const OPCODE: u16 = 0x001d;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let _unknown = reader.u8()?;
        let count = usize::from(reader.u16_le()?);
        if count > MAX_RETAIL_PURCHASE_ITEMS {
            return Err(reader.invalid("purchase carries too many items"));
        }
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let _unknown = reader.u32_le()?;
            let item_type_id = reader.u32_le()?;
            let _unknown2 = reader.u16_le()?;
            let _unknown3 = reader.u16_le()?;
            let quantity = reader.u32_le()?;
            let claimed_cost_pang = reader.u32_le()?;
            let claimed_cost_point = reader.u32_le()?;
            items.push(RetailPurchaseItem {
                item_type_id,
                quantity,
                claimed_cost_pang,
                claimed_cost_point,
            });
        }
        Ok(Self { items })
    }
}

/// Pang balance after a purchase, server opcode `0x00c8`.
///
/// # Provenance
///
/// Two `u8`s, from `pangbox/server` (`game/packet/server.go` `ServerPangBalanceData`), ISC
/// licensed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailPangSpent {
    /// Balance after the purchase.
    pub remaining: u64,
    /// Total spent by the purchase.
    pub spent: u64,
}

impl EncodePacket for RetailPangSpent {
    const OPCODE: u16 = 0x00c8;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u64_le(self.remaining);
        writer.u64_le(self.spent);
        Ok(())
    }
}

/// Purchase outcome, server opcode `0x0068`.
///
/// # Provenance
///
/// A `u4` status then two `u8` balances, from `pangbox/server` (`game/packet/server.go`
/// `ServerPurchaseItemResponse`), ISC licensed. Status `0` is success and `1` is the refusal
/// upstream sends when the balance cannot cover the cost.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailPurchaseResponse {
    /// Zero on success.
    pub status: u32,
    /// Pang balance after the attempt.
    pub pang: u64,
    /// Point balance after the attempt.
    pub points: u64,
}

impl RetailPurchaseResponse {
    /// Status the client reads as "purchase refused".
    pub const REFUSED: u32 = 1;
}

impl EncodePacket for RetailPurchaseResponse {
    const OPCODE: u16 = 0x0068;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.status);
        writer.u64_le(self.pang);
        writer.u64_le(self.points);
        Ok(())
    }
}

/// Consumable slots carried by an equipment update.
pub const RETAIL_CONSUMABLE_SLOTS: usize = 10;

/// Which equipment an update concerns.
///
/// # Provenance
///
/// Discriminants and bodies from `pangbox/server` (`game/packet/client.go`
/// `ClientEquipmentUpdate`), ISC licensed. Types `8` and `9` are unclassified there and here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetailEquipmentSlot {
    /// Character parts. The body is the client's character block and is not modelled.
    CharacterParts,
    /// Selected caddie.
    Caddie,
    /// The ten consumable slots.
    Consumables,
    /// Selected ball, which the client calls a comet.
    Ball,
    /// Profile decoration: background, frame, sticker, slot, cut-in and title.
    Decoration,
    /// Selected character.
    Character,
    /// Unclassified update carrying one `u4`.
    UnknownEight,
    /// Unclassified update carrying a character id and four `u4`s.
    UnknownNine,
}

impl RetailEquipmentSlot {
    /// Returns the wire discriminant.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::CharacterParts => 0,
            Self::Caddie => 1,
            Self::Consumables => 2,
            Self::Ball => 3,
            Self::Decoration => 4,
            Self::Character => 5,
            Self::UnknownEight => 8,
            Self::UnknownNine => 9,
        }
    }

    /// Returns the slot for a wire discriminant.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::CharacterParts),
            1 => Some(Self::Caddie),
            2 => Some(Self::Consumables),
            3 => Some(Self::Ball),
            4 => Some(Self::Decoration),
            5 => Some(Self::Character),
            8 => Some(Self::UnknownEight),
            9 => Some(Self::UnknownNine),
            _ => None,
        }
    }
}

/// Requested value carried by an equipment update.
///
/// # Provenance
///
/// `pangbox/server` `game/packet/client.go:296-357` documents every tagged body. Character parts
/// remain opaque until their complete mutable model exists; the bounded packet decoder still owns
/// and drops that tail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetailEquipmentRequested {
    /// Full 513-byte character/part/card body, not yet mutable in this server.
    CharacterParts,
    /// Owned caddie id.
    Caddie(u32),
    /// Ten consumable catalog ids.
    Consumables([u32; RETAIL_CONSUMABLE_SLOTS]),
    /// Ball catalog id and club-set inventory id, updated together by retail type `3`.
    ///
    /// SuperSS-Dev `GAME/channel.cpp:5233-5299` reads both request words; the older Pangbox
    /// model names only the first and is incomplete for U.S. 852.
    BallAndClub {
        /// Ball/comet catalog id.
        ball_type_id: u32,
        /// Owned club-set row.
        club_item_id: u32,
    },
    /// Six profile-decoration catalog ids.
    Decoration([u32; 6]),
    /// Owned character id.
    Character(u32),
    /// Unclassified four-byte body.
    UnknownEight(u32),
    /// Unclassified character id plus four words.
    UnknownNine {
        /// Owned character id.
        character_id: u32,
        /// Unclassified body words.
        values: [u32; 4],
    },
}

/// Equipment change, client opcode `0x0020`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailEquipmentUpdate {
    /// Which equipment the client is changing.
    pub slot: RetailEquipmentSlot,
    /// Requested value for that tagged slot.
    pub requested: RetailEquipmentRequested,
}

impl DecodePacket for RetailEquipmentUpdate {
    const OPCODE: u16 = 0x0020;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let tag = reader.u8()?;
        let slot = RetailEquipmentSlot::from_tag(tag)
            .ok_or_else(|| reader.invalid("unknown equipment slot"))?;
        let requested = match slot {
            RetailEquipmentSlot::CharacterParts => RetailEquipmentRequested::CharacterParts,
            RetailEquipmentSlot::Caddie => RetailEquipmentRequested::Caddie(reader.u32_le()?),
            RetailEquipmentSlot::Consumables => {
                let mut values = [0_u32; RETAIL_CONSUMABLE_SLOTS];
                for value in &mut values {
                    *value = reader.u32_le()?;
                }
                RetailEquipmentRequested::Consumables(values)
            }
            RetailEquipmentSlot::Ball => RetailEquipmentRequested::BallAndClub {
                ball_type_id: reader.u32_le()?,
                club_item_id: reader.u32_le()?,
            },
            RetailEquipmentSlot::Decoration => {
                let mut values = [0_u32; 6];
                for value in &mut values {
                    *value = reader.u32_le()?;
                }
                RetailEquipmentRequested::Decoration(values)
            }
            RetailEquipmentSlot::Character => RetailEquipmentRequested::Character(reader.u32_le()?),
            RetailEquipmentSlot::UnknownEight => {
                RetailEquipmentRequested::UnknownEight(reader.u32_le()?)
            }
            RetailEquipmentSlot::UnknownNine => {
                let character_id = reader.u32_le()?;
                let mut values = [0_u32; 4];
                for value in &mut values {
                    *value = reader.u32_le()?;
                }
                RetailEquipmentRequested::UnknownNine {
                    character_id,
                    values,
                }
            }
        };
        Ok(Self { slot, requested })
    }
}

/// The equipment this server holds for one slot, server opcode `0x006b`.
///
/// # Provenance
///
/// Layout from `pangbox/server` (`game/packet/server.go` `ServerPlayerEquipmentUpdated` and its
/// per-slot bodies), ISC licensed, including the status value it sends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetailEquipmentUpdated {
    /// The caddie in use; zero when none is.
    Caddie {
        /// Caddie identifier.
        caddie_id: u32,
    },
    /// The ten consumable slots.
    Consumables {
        /// Catalog type in each slot; zero when empty.
        item_type_ids: [u32; RETAIL_CONSUMABLE_SLOTS],
    },
    /// The ball and club set in use; retail updates and acknowledges them together.
    BallAndClub {
        /// Ball/comet catalog type.
        ball_type_id: u32,
        /// Owned club-set row.
        club_item_id: u32,
    },
    /// Profile decoration; zeroes when none is set.
    Decoration {
        /// Background, frame, sticker, slot, cut-in and title types.
        type_ids: [u32; 6],
    },
    /// The selected character.
    Character {
        /// Owned character row.
        character_id: u32,
    },
}

impl RetailEquipmentUpdated {
    /// Status byte upstream sends alongside an applied update.
    pub const STATUS_APPLIED: u8 = 0x04;

    /// Returns the slot this reply describes.
    #[must_use]
    pub const fn slot(&self) -> RetailEquipmentSlot {
        match self {
            Self::Caddie { .. } => RetailEquipmentSlot::Caddie,
            Self::Consumables { .. } => RetailEquipmentSlot::Consumables,
            Self::BallAndClub { .. } => RetailEquipmentSlot::Ball,
            Self::Decoration { .. } => RetailEquipmentSlot::Decoration,
            Self::Character { .. } => RetailEquipmentSlot::Character,
        }
    }
}

impl EncodePacket for RetailEquipmentUpdated {
    const OPCODE: u16 = 0x006b;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u8(Self::STATUS_APPLIED);
        writer.u8(self.slot().tag());
        match self {
            Self::Caddie { caddie_id } => writer.u32_le(*caddie_id),
            Self::Consumables { item_type_ids } => {
                for type_id in item_type_ids {
                    writer.u32_le(*type_id);
                }
            }
            Self::BallAndClub {
                ball_type_id,
                club_item_id,
            } => {
                writer.u32_le(*ball_type_id);
                writer.u32_le(*club_item_id);
            }
            Self::Decoration { type_ids } => {
                for type_id in type_ids {
                    writer.u32_le(*type_id);
                }
            }
            Self::Character { character_id } => writer.u32_le(*character_id),
        }
        Ok(())
    }
}

/// Shop entry, client opcode `0x0140`.
///
/// # Provenance
///
/// No payload. Opcode from `pangbox/server` (`game/packet/client.go` `ClientShopJoin`), ISC
/// licensed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailShopJoin;

impl DecodePacket for RetailShopJoin {
    const OPCODE: u16 = 0x0140;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self)
    }
}

/// Shop entry acknowledgement, server opcode `0x020e`.
///
/// # Provenance
///
/// Eight zero bytes, from `pangbox/server` (`game/packet/server.go` `Server020E`), ISC licensed,
/// which sends it with its unknown block left zeroed. The field meanings are not established.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailShopJoined;

impl EncodePacket for RetailShopJoined {
    const OPCODE: u16 = 0x020e;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.bytes(&[0; 8]);
        Ok(())
    }
}

/// My Room entry, client opcode `0x00b5`.
///
/// # Provenance
///
/// Two `u4`s, from `pangbox/server` (`game/packet/client.go` `ClientEnterMyRoom`), ISC licensed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailMyRoomEnter {
    /// The requesting player.
    pub user_id: u32,
    /// Whose room is being entered.
    pub room_user_id: u32,
}

impl DecodePacket for RetailMyRoomEnter {
    const OPCODE: u16 = 0x00b5;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self {
            user_id: reader.u32_le()?,
            room_user_id: reader.u32_le()?,
        })
    }
}

/// My Room entry acknowledgement, server opcode `0x012b`.
///
/// # Provenance
///
/// Three `u4`s then 99 unclassified bytes, from `pangbox/server` (`game/packet/server.go`
/// `ServerMyRoomEntered`), ISC licensed. The two constants are the values it sends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailMyRoomEntered {
    /// The player whose room was entered.
    pub user_id: u32,
}

impl EncodePacket for RetailMyRoomEntered {
    const OPCODE: u16 = 0x012b;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(1);
        writer.u32_le(self.user_id);
        writer.u32_le(1);
        writer.bytes(&[0; 99]);
        Ok(())
    }
}

/// My Room furniture request, client opcode `0x00b7`.
///
/// # Provenance
///
/// A `u4` then a `u1`, from `pangbox/server` (`game/packet/client.go` `ClientRequestInventory`),
/// ISC licensed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailMyRoomInventoryRequest {
    /// The requesting player.
    pub user_id: u32,
}

impl DecodePacket for RetailMyRoomInventoryRequest {
    const OPCODE: u16 = 0x00b7;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let user_id = reader.u32_le()?;
        let _unknown = reader.u8()?;
        Ok(Self { user_id })
    }
}

/// My Room furniture layout, server opcode `0x012d`.
///
/// This server has no furniture, so the layout is empty. Upstream sends the same empty layout.
///
/// # Provenance
///
/// A `u4` then a `u2` count, from `pangbox/server` (`game/packet/server.go` `ServerMyRoomLayout`),
/// ISC licensed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailMyRoomLayout;

impl EncodePacket for RetailMyRoomLayout {
    const OPCODE: u16 = 0x012d;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(1);
        writer.u16_le(0);
        Ok(())
    }
}

/// Player record for My Room, server opcode `0x0168`.
///
/// # Provenance
///
/// Carries the same room-player record as the room census, from `pangbox/server`
/// (`game/packet/server.go` `ServerPlayerInfo`), ISC licensed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailPlayerInfo {
    /// The player being described.
    pub player: RetailRoomPlayer,
}

impl EncodePacket for RetailPlayerInfo {
    const OPCODE: u16 = 0x0168;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        self.player.encode_body(writer)
    }
}

/// Locker inventory request, client opcode `0x00d3`.
///
/// # Provenance
///
/// Opcode from `pangbox/server` (`game/packet/client.go` `ClientLockerInventoryRequest`), ISC
/// licensed. The payload is not modelled: upstream answers with a fixed status without reading it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailLockerInventoryRequest;

impl DecodePacket for RetailLockerInventoryRequest {
    const OPCODE: u16 = 0x00d3;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self)
    }
}

/// Locker inventory response, server opcode `0x0170`.
///
/// The locker is part of the combination/password system, which this server does not implement;
/// status `76` is what upstream answers with, telling the client the locker is unavailable rather
/// than presenting an empty but usable one.
///
/// # Provenance
///
/// Two `u4`s and the status value from `pangbox/server` (`game/packet/server.go`
/// `ServerLockerInventoryResponse`), ISC licensed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailLockerInventoryResponse;

impl EncodePacket for RetailLockerInventoryResponse {
    const OPCODE: u16 = 0x0170;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(0);
        writer.u32_le(76);
        Ok(())
    }
}

/// Locker combination attempt, client opcode `0x00cc`.
///
/// # Provenance
///
/// Opcode from `pangbox/server` (`game/packet/client.go` `ClientLockerCombinationAttempt`), ISC
/// licensed. The payload carries the attempted combination and is deliberately not modelled or
/// logged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailLockerCombinationAttempt;

impl DecodePacket for RetailLockerCombinationAttempt {
    const OPCODE: u16 = 0x00cc;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self)
    }
}

/// Locker combination response, server opcode `0x016c`.
///
/// # Provenance
///
/// A `u4` status, zero, from `pangbox/server` (`game/packet/server.go`
/// `ServerLockerCombinationResponse`), ISC licensed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailLockerCombinationResponse;

impl EncodePacket for RetailLockerCombinationResponse {
    const OPCODE: u16 = 0x016c;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(0);
        Ok(())
    }
}

/// Multiplayer-mode enter acknowledgement, server opcode `0x00f5`.
///
/// # Provenance
///
/// Empty body, from `pangbox/server` (`game/packet/server.go` `ServerMultiplayerJoined`), ISC
/// licensed; the vendored PacketDoc `gameservice/server/00f5.ksy` documents the same empty
/// response to the client's `0x0081`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailMultiplayerJoined;

impl EncodePacket for RetailMultiplayerJoined {
    const OPCODE: u16 = 0x00f5;

    fn encode(
        &self,
        _writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)
    }
}

/// Multiplayer-mode leave acknowledgement, server opcode `0x00f6`.
///
/// # Provenance
///
/// Empty body, from `pangbox/server` (`game/packet/server.go` `ServerMultiplayerLeft`), ISC
/// licensed, matching PacketDoc `gameservice/server/00f6.ksy`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailMultiplayerLeft;

impl EncodePacket for RetailMultiplayerLeft {
    const OPCODE: u16 = 0x00f6;

    fn encode(
        &self,
        _writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)
    }
}

/// Room list, server opcode `0x0047`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomList {
    /// How the client should apply this frame.
    pub kind: RoomListKind,
    /// Advertised rooms.
    pub rooms: Vec<RetailRoom>,
}

impl EncodePacket for RetailRoomList {
    const OPCODE: u16 = 0x0047;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        let count = u8::try_from(self.rooms.len()).map_err(|_| PacketEncodeError::Limit {
            field: "rooms",
            actual: self.rooms.len(),
            maximum: MAX_ROOMS_PER_LIST,
        })?;
        writer.u8(count);
        writer.u8(self.kind as u8);
        writer.u16_le(0xffff);
        for room in &self.rooms {
            room.encode_body(writer)?;
        }
        Ok(())
    }
}

/// Why a join attempt failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RoomJoinRejection {
    /// The match has already begun.
    AlreadyStarted = 8,
    /// The room could not be created or entered.
    CannotCreate = 18,
}

/// Join outcome, server opcode `0x0049`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetailRoomJoinResult {
    /// Entry accepted; carries the room the client is now in.
    Accepted(Box<RetailRoom>),
    /// Entry refused.
    Rejected(RoomJoinRejection),
}

impl EncodePacket for RetailRoomJoinResult {
    const OPCODE: u16 = 0x0049;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        match self {
            // Success carries a u16 status; rejection carries a single byte. The widths
            // genuinely differ, so this is not an oversight.
            Self::Accepted(room) => {
                writer.u16_le(0);
                room.encode_body(writer)?;
            }
            Self::Rejected(reason) => writer.u8(*reason as u8),
        }
        Ok(())
    }
}

/// Leave acknowledgement, server opcode `0x004c`.
///
/// The payload is the room the client now occupies; `0xffff` means the lobby.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailRoomLeave {
    /// Room the client is now in, or `0xffff` for the lobby.
    pub new_room_id: u16,
}

impl RetailRoomLeave {
    /// Acknowledges a return to the lobby.
    #[must_use]
    pub const fn to_lobby() -> Self {
        Self {
            new_room_id: 0xffff,
        }
    }
}

impl EncodePacket for RetailRoomLeave {
    const OPCODE: u16 = 0x004c;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u16_le(self.new_room_id);
        Ok(())
    }
}

/// How a room census frame relates to what the client already holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RoomCensusKind {
    /// Replaces the client's roster.
    List = 0,
    /// Adds one player.
    Add = 1,
    /// Removes one player.
    Remove = 2,
    /// Updates one player.
    Update = 3,
}

/// Exact wire width of one room census player record.
///
/// The identity half is 341 bytes and the equipped-character block that closes it is another
/// 513. Both halves are mandatory: a client reading a record short by the character block finds
/// the next record 513 bytes early, which it renders as a roster of garbage names. That stays
/// invisible with a single member — there is no second record to land wrong — so it only
/// surfaces once a room holds two, which is every room that can start a versus hole.
pub const ROOM_PLAYER_RECORD_BYTES: usize = 341 + CHARACTER_BLOCK_BYTES;

/// Room status bits the client renders next to each player.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RoomPlayerFlags(u16);

impl RoomPlayerFlags {
    /// Room owner.
    pub const MASTER: u16 = 1 << 3;
    /// Marked away.
    pub const AWAY: u16 = 1 << 2;
    /// Marked ready.
    pub const READY: u16 = 1 << 9;

    /// Builds flags from owner and ready state.
    #[must_use]
    pub const fn new(is_owner: bool, is_ready: bool) -> Self {
        let mut bits = 0;
        if is_owner {
            bits |= Self::MASTER;
        }
        if is_ready {
            bits |= Self::READY;
        }
        Self(bits)
    }

    /// Returns the packed bits.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }
}

/// One player as the room census describes them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomPlayer {
    /// Per-connection identifier the client uses to address this player.
    pub connection_id: u32,
    /// Display nickname.
    pub nickname: Vec<u8>,
    /// Zero-based seat within the room.
    pub slot: u8,
    /// Equipped character's **catalog** id, from the client's own `Character.iff`.
    ///
    /// Not the inventory id: this is what the client looks the character model up by, and an
    /// id it cannot resolve leaves it holding a null it dereferences when the hole loads.
    pub character_iff_id: u32,
    /// Room status bits.
    pub flags: RoomPlayerFlags,
    /// Player level.
    pub level: u8,
    /// Durable numeric account identifier.
    pub user_id: u32,
    /// Equipped character, which closes the record and which the client renders in the slot.
    pub character: RetailCharacter,
}

/// Exact wire width of the identity half of a room census record, without the character block.
///
/// A modification frame carries only this half: both reference descriptions of `0x0048` give
/// the modification case a bare user record and no trailing character data.
pub const ROOM_PLAYER_IDENTITY_BYTES: usize = 341;

impl RetailRoomPlayer {
    fn encode_body(&self, writer: &mut PacketWriter) -> Result<(), PacketEncodeError> {
        self.encode_identity(writer)?;
        self.character.encode_body(writer);
        Ok(())
    }

    fn encode_identity(&self, writer: &mut PacketWriter) -> Result<(), PacketEncodeError> {
        let start = writer.as_slice().len();
        writer.u32_le(self.connection_id);
        writer.fixed_nul(&self.nickname, 22)?;
        writer.fixed_nul(&[], 17)?; // guild name
        writer.u8(self.slot);
        writer.u32_le(0);
        writer.u32_le(0); // title
        writer.u32_le(self.character_iff_id);
        for _ in 0..4 {
            writer.u32_le(0); // skin background, frame, sticker, slot
        }
        writer.u32_le(0);
        writer.u32_le(0); // duplicated title
        writer.u16_le(self.flags.bits());
        writer.u8(self.level);
        // Constant the reference server always sends here; meaning unestablished.
        writer.u16_le(0x2560);
        writer.u32_le(0); // guild id
        writer.fixed_nul(&[], 12)?; // guild mark
        writer.u32_le(self.user_id);
        // Lounge pose and position.
        writer.u32_le(0);
        writer.u16_le(0);
        writer.u32_le(0);
        writer.f32_le(0.0);
        writer.f32_le(0.0);
        writer.f32_le(0.0);
        // Lounge shop.
        writer.u32_le(0);
        writer.fixed_nul(&[], 64)?;
        writer.u32_le(0); // mascot
        writer.u16_le(0); // item boost
        writer.u32_le(0);
        writer.fixed_nul(&[], 22)?;
        writer.bytes(&[0; 105]);
        writer.u8(0); // invited
        writer.f32_le(0.0); // average score
        writer.f32_le(0.0);
        debug_assert_eq!(writer.as_slice().len() - start, ROOM_PLAYER_IDENTITY_BYTES);
        Ok(())
    }
}

/// Room member census, server opcode `0x0048`.
///
/// This is what populates the room's player list. Note the frame shapes differ per kind:
/// `Remove` carries only a connection id, and `Update` writes that id twice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetailRoomCensus {
    /// Full roster replacing whatever the client holds.
    List(Vec<RetailRoomPlayer>),
    /// One player joined.
    Add(Box<RetailRoomPlayer>),
    /// One player left, addressed by connection id.
    Remove(u32),
    /// One player changed.
    Update(Box<RetailRoomPlayer>),
}

impl EncodePacket for RetailRoomCensus {
    const OPCODE: u16 = 0x0048;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        match self {
            Self::List(players) => {
                let count = u8::try_from(players.len()).map_err(|_| PacketEncodeError::Limit {
                    field: "room players",
                    actual: players.len(),
                    maximum: usize::from(MAX_ROOM_PLAYERS),
                })?;
                writer.u8(RoomCensusKind::List as u8);
                writer.u16_le(0xffff);
                writer.u8(count);
                for player in players {
                    player.encode_body(writer)?;
                }
                writer.u8(0);
            }
            Self::Add(player) => {
                writer.u8(RoomCensusKind::Add as u8);
                writer.u16_le(0xffff);
                writer.u8(1);
                player.encode_body(writer)?;
            }
            Self::Remove(connection_id) => {
                writer.u8(RoomCensusKind::Remove as u8);
                writer.u16_le(0xffff);
                writer.u32_le(*connection_id);
            }
            Self::Update(player) => {
                writer.u8(RoomCensusKind::Update as u8);
                // The same `-1` every other census kind carries between the kind byte and its
                // body; a modification frame is not an exception to it.
                writer.u16_le(0xffff);
                // The connection id is intentionally repeated: once here and again as the
                // first field of the record itself.
                writer.u32_le(player.connection_id);
                // Identity only. A modification frame carries no character block and no
                // terminator after the record.
                player.encode_identity(writer)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// The reply is a tagged union, so the slot byte and the body must agree; a mismatch would
    /// make the client read the next field at the wrong offset.
    #[test]
    fn equipment_reply_tags_match_their_bodies() {
        let cases = [
            (
                RetailEquipmentUpdated::Caddie { caddie_id: 7 },
                RetailEquipmentSlot::Caddie,
                2 + 4,
            ),
            (
                RetailEquipmentUpdated::Consumables {
                    item_type_ids: [0; RETAIL_CONSUMABLE_SLOTS],
                },
                RetailEquipmentSlot::Consumables,
                2 + 4 * RETAIL_CONSUMABLE_SLOTS,
            ),
            (
                RetailEquipmentUpdated::BallAndClub {
                    ball_type_id: 1,
                    club_item_id: 2,
                },
                RetailEquipmentSlot::Ball,
                2 + 8,
            ),
            (
                RetailEquipmentUpdated::Decoration { type_ids: [0; 6] },
                RetailEquipmentSlot::Decoration,
                2 + 24,
            ),
            (
                RetailEquipmentUpdated::Character { character_id: 3 },
                RetailEquipmentSlot::Character,
                2 + 4,
            ),
        ];
        for (reply, slot, expected_len) in cases {
            let mut writer = PacketWriter::new();
            reply
                .encode(&mut writer, &CompatibilityProfile::US_852)
                .expect("encode");
            let bytes = writer.into_inner();
            assert_eq!(bytes.len(), expected_len, "{slot:?}");
            assert_eq!(bytes[0], RetailEquipmentUpdated::STATUS_APPLIED);
            assert_eq!(bytes[1], slot.tag(), "{slot:?}");
        }
    }

    /// Every slot the client can send must decode, and nothing else may.
    #[test]
    fn equipment_update_accepts_only_known_slots() {
        for tag in [0_u8, 1, 2, 3, 4, 5, 8, 9] {
            let slot = RetailEquipmentSlot::from_tag(tag).expect("known slot");
            assert_eq!(slot.tag(), tag);
        }
        for tag in [6_u8, 7, 10, 255] {
            assert!(RetailEquipmentSlot::from_tag(tag).is_none(), "{tag}");
        }
    }

    use super::*;
    use crate::{ServiceKind, decode_packet_payload, encode_packet_payload};

    fn profile() -> CompatibilityProfile {
        CompatibilityProfile::US_852
    }

    #[test]
    fn equipment_update_decodes_requested_ball_type() {
        let mut payload = vec![RetailEquipmentSlot::Ball.tag()];
        payload.extend_from_slice(&0x1400_00c9_u32.to_le_bytes());
        payload.extend_from_slice(&310_u32.to_le_bytes());
        let decoded =
            decode_packet_payload::<RetailEquipmentUpdate>(&payload, &profile(), ServiceKind::Game)
                .expect("decode ball update");
        assert_eq!(decoded.slot, RetailEquipmentSlot::Ball);
        assert_eq!(
            decoded.requested,
            RetailEquipmentRequested::BallAndClub {
                ball_type_id: 0x1400_00c9,
                club_item_id: 310,
            }
        );
    }

    fn sample_room() -> RetailRoom {
        RetailRoom {
            name: b"Test Room".to_vec(),
            public: true,
            state: RetailRoomState::Lobby,
            max_players: 4,
            player_count: 1,
            hole_count: 3,
            mode: RetailRoomType::Versus as u8,
            play_mode: 0,
            id: 7,
            hole_progression: RetailHoleProgression::FrontStart,
            course: 0,
            shot_timer_ms: 30_000,
            game_timer_ms: 600_000,
            owner_uid: 42,
            natural_wind: false,
        }
    }

    #[test]
    fn room_record_is_exactly_two_hundred_ten_bytes() {
        let list = RetailRoomList {
            kind: RoomListKind::Initial,
            rooms: vec![sample_room()],
        };
        let payload = encode_packet_payload(&list, &profile()).expect("encode");
        // count, kind, and the always-0xffff marker precede the records.
        assert_eq!(payload.len(), 4 + ROOM_RECORD_BYTES);
        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], RoomListKind::Initial as u8);
        assert_eq!(u16::from_le_bytes([payload[2], payload[3]]), 0xffff);
        assert_eq!(&payload[4..13], b"Test Room");
    }

    #[test]
    fn room_state_flags_are_mutually_exclusive() {
        for (state, lobby, joinable) in [
            (RetailRoomState::Lobby, 1, 0),
            (RetailRoomState::InGameJoinable, 0, 1),
            (RetailRoomState::InGame, 0, 0),
        ] {
            let list = RetailRoomList {
                kind: RoomListKind::Initial,
                rooms: vec![RetailRoom {
                    state,
                    ..sample_room()
                }],
            };
            let payload = encode_packet_payload(&list, &profile()).expect("encode");
            // Name, then the public flag, then the two state flags.
            assert_eq!(payload[4 + ROOM_NAME_BYTES + 1], lobby);
            assert_eq!(payload[4 + ROOM_NAME_BYTES + 2], joinable);
        }
    }

    #[test]
    fn room_create_decodes_the_retail_layout() {
        let mut writer = PacketWriter::default();
        writer.u8(0);
        writer.u32_le(30_000);
        writer.u32_le(600_000);
        writer.u8(4);
        writer.u8(RetailRoomType::Versus as u8);
        writer.u8(3);
        writer.u8(1);
        writer.bytes(&[0; 5]);
        writer.pstring(b"My Room", ROOM_NAME_BYTES).expect("name");
        writer.pstring(b"", ROOM_NAME_BYTES).expect("password");
        let payload = writer.into_inner();
        let decoded =
            decode_packet_payload::<RetailRoomCreate>(&payload, &profile(), ServiceKind::Game)
                .expect("decode");
        assert_eq!(decoded.shot_timer_ms, 30_000);
        assert_eq!(decoded.game_timer_ms, 600_000);
        assert_eq!(decoded.max_players, 4);
        assert_eq!(decoded.hole_count, 3);
        assert_eq!(decoded.course, 1);
        assert_eq!(decoded.name, b"My Room");
        assert!(decoded.password.is_empty());
    }

    #[test]
    fn room_join_decodes_number_and_password() {
        let mut writer = PacketWriter::default();
        writer.u16_le(9);
        writer.pstring(b"hunter2", ROOM_NAME_BYTES).expect("pw");
        let payload = writer.into_inner();
        let decoded =
            decode_packet_payload::<RetailRoomJoin>(&payload, &profile(), ServiceKind::Game)
                .expect("decode");
        assert_eq!(decoded.room_number, 9);
        assert_eq!(decoded.password, b"hunter2");
    }

    #[test]
    fn join_result_widths_differ_between_accept_and_reject() {
        let accepted = encode_packet_payload(
            &RetailRoomJoinResult::Accepted(Box::new(sample_room())),
            &profile(),
        )
        .expect("accept");
        assert_eq!(accepted.len(), 2 + ROOM_RECORD_BYTES);
        let rejected = encode_packet_payload(
            &RetailRoomJoinResult::Rejected(RoomJoinRejection::AlreadyStarted),
            &profile(),
        )
        .expect("reject");
        assert_eq!(rejected.as_slice(), &[8]);
    }

    #[test]
    fn leaving_returns_the_client_to_the_lobby_sentinel() {
        let payload =
            encode_packet_payload(&RetailRoomLeave::to_lobby(), &profile()).expect("encode");
        assert_eq!(payload.as_slice(), &[0xff, 0xff]);
    }

    fn sample_player() -> RetailRoomPlayer {
        RetailRoomPlayer {
            connection_id: 11,
            nickname: b"Nick".to_vec(),
            slot: 0,
            character_iff_id: 5,
            flags: RoomPlayerFlags::new(true, false),
            level: 3,
            user_id: 42,
            character: RetailCharacter {
                iff_id: 0x0100_0000,
                uid: 5,
                hair_color: 0,
                part_iff_ids: [0; crate::CHARACTER_PARTS],
                part_uids: [0; crate::CHARACTER_PARTS],
                stats: [0; crate::CHARACTER_STATS],
                mastery: 0,
            },
        }
    }

    #[test]
    fn census_player_record_carries_its_character_block() {
        let payload = encode_packet_payload(
            &RetailRoomCensus::Add(Box::new(sample_player())),
            &profile(),
        )
        .expect("encode");
        // kind, the 0xffff marker, the count, then one record.
        assert_eq!(payload.len(), 4 + ROOM_PLAYER_RECORD_BYTES);
        assert_eq!(ROOM_PLAYER_RECORD_BYTES, 854);
        assert_eq!(payload[0], RoomCensusKind::Add as u8);
        assert_eq!(payload[3], 1);
        // The character block closes the record, so its catalog id sits exactly 513 bytes from
        // the end. A record that lost the block would put the next one 513 bytes early.
        let character = payload.len() - CHARACTER_BLOCK_BYTES;
        assert_eq!(
            u32::from_le_bytes([
                payload[character],
                payload[character + 1],
                payload[character + 2],
                payload[character + 3],
            ]),
            0x0100_0000
        );
    }

    #[test]
    fn census_frame_shapes_differ_per_kind() {
        let list = encode_packet_payload(
            &RetailRoomCensus::List(vec![sample_player(), sample_player()]),
            &profile(),
        )
        .expect("list");
        // List carries a trailing terminator byte that Add does not.
        assert_eq!(list.len(), 4 + 2 * ROOM_PLAYER_RECORD_BYTES + 1);

        let remove =
            encode_packet_payload(&RetailRoomCensus::Remove(11), &profile()).expect("remove");
        assert_eq!(remove.as_slice(), &[2, 0xff, 0xff, 11, 0, 0, 0]);

        let update = encode_packet_payload(
            &RetailRoomCensus::Update(Box::new(sample_player())),
            &profile(),
        )
        .expect("update");
        // Update carries the same 0xffff every other kind does, then the connection id,
        // then the identity half of the record — which repeats that id — and nothing else.
        // It is the only kind whose record has no character block and no terminator.
        assert_eq!(update.len(), 1 + 2 + 4 + ROOM_PLAYER_IDENTITY_BYTES);
        assert_eq!(u16::from_le_bytes([update[1], update[2]]), 0xffff);
        assert_eq!(
            u32::from_le_bytes([update[3], update[4], update[5], update[6]]),
            11
        );
        assert_eq!(
            u32::from_le_bytes([update[7], update[8], update[9], update[10]]),
            11
        );
    }

    #[test]
    fn owner_and_ready_flags_pack_into_distinct_bits() {
        assert_eq!(RoomPlayerFlags::new(false, false).bits(), 0);
        assert_eq!(RoomPlayerFlags::new(true, false).bits(), 1 << 3);
        assert_eq!(RoomPlayerFlags::new(false, true).bits(), 1 << 9);
        assert_eq!(RoomPlayerFlags::new(true, true).bits(), (1 << 3) | (1 << 9));
    }

    /// The two constants are the whole reason this packet is easy to get subtly wrong: both
    /// are values the captures carry and the layout does not explain.
    #[test]
    fn room_status_carries_the_settings_and_the_two_captured_constants() {
        let room = sample_room();
        let payload = encode_packet_payload(&RetailRoomStatus { room: room.clone() }, &profile())
            .expect("status");
        assert_eq!(&payload[0..2], &[0xff, 0xff]);
        assert_eq!(payload[2], room.mode);
        assert_eq!(payload[3], room.course);
        assert_eq!(payload[4], room.hole_count);
        assert_eq!(payload[5], room.hole_progression as u8);
        assert_eq!(payload[10], room.max_players);
        assert_eq!(&payload[11..13], &30_u16.to_le_bytes());
        assert_eq!(&payload[13..17], &room.shot_timer_ms.to_le_bytes());
        assert_eq!(&payload[17..21], &room.game_timer_ms.to_le_bytes());
        assert_eq!(&payload[21..25], &[0, 0, 0, 0]);
        assert_eq!(payload[25], 1);
        assert_eq!(payload.len(), 26 + 2 + room.name.len());
    }

    #[test]
    fn unknown_room_types_are_rejected_rather_than_guessed() {
        assert_eq!(
            RetailRoomType::from_wire(0x00),
            Some(RetailRoomType::Versus)
        );
        assert_eq!(
            RetailRoomType::from_wire(0x0a),
            Some(RetailRoomType::Battle)
        );
        assert_eq!(RetailRoomType::from_wire(0x01), None);
        assert_eq!(RetailRoomType::from_wire(0xff), None);
    }
}
