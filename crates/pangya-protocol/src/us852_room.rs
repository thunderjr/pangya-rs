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
    CompatibilityProfile, DecodePacket, EncodePacket, PacketDecodeError, PacketEncodeError,
    PacketReader, PacketWriter,
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
    /// Mode.
    pub room_type: RetailRoomType,
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
        writer.u8(self.room_type as u8);
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
        writer.u8(self.room_type as u8);
        writer.u32_le(0); // artifact catalog id
        writer.u32_le(u32::from(self.natural_wind));
        for _ in 0..4 {
            writer.u32_le(0); // event info
        }
        debug_assert_eq!(writer.as_slice().len() - start, ROOM_RECORD_BYTES);
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
pub const ROOM_PLAYER_RECORD_BYTES: usize = 341;

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
    /// Equipped character inventory id.
    pub character_uid: u32,
    /// Room status bits.
    pub flags: RoomPlayerFlags,
    /// Player level.
    pub level: u8,
    /// Durable numeric account identifier.
    pub user_id: u32,
}

impl RetailRoomPlayer {
    fn encode_body(&self, writer: &mut PacketWriter) -> Result<(), PacketEncodeError> {
        let start = writer.as_slice().len();
        writer.u32_le(self.connection_id);
        writer.fixed_nul(&self.nickname, 22)?;
        writer.fixed_nul(&[], 17)?; // guild name
        writer.u8(self.slot);
        writer.u32_le(0);
        writer.u32_le(0); // title
        writer.u32_le(self.character_uid);
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
        debug_assert_eq!(writer.as_slice().len() - start, ROOM_PLAYER_RECORD_BYTES);
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
                // The connection id is intentionally repeated: once here and again as the
                // first field of the record itself.
                writer.u32_le(player.connection_id);
                player.encode_body(writer)?;
                writer.u8(0);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ServiceKind, decode_packet_payload, encode_packet_payload};

    fn profile() -> CompatibilityProfile {
        CompatibilityProfile::US_852
    }

    fn sample_room() -> RetailRoom {
        RetailRoom {
            name: b"Test Room".to_vec(),
            public: true,
            state: RetailRoomState::Lobby,
            max_players: 4,
            player_count: 1,
            hole_count: 3,
            room_type: RetailRoomType::Versus,
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
            character_uid: 5,
            flags: RoomPlayerFlags::new(true, false),
            level: 3,
            user_id: 42,
        }
    }

    #[test]
    fn census_player_record_is_exactly_three_hundred_forty_one_bytes() {
        let payload = encode_packet_payload(
            &RetailRoomCensus::Add(Box::new(sample_player())),
            &profile(),
        )
        .expect("encode");
        // kind, the 0xffff marker, the count, then one record.
        assert_eq!(payload.len(), 4 + ROOM_PLAYER_RECORD_BYTES);
        assert_eq!(payload[0], RoomCensusKind::Add as u8);
        assert_eq!(payload[3], 1);
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
        // Update writes the connection id, then the record which repeats it.
        assert_eq!(update.len(), 1 + 4 + ROOM_PLAYER_RECORD_BYTES + 1);
        assert_eq!(
            u32::from_le_bytes([update[1], update[2], update[3], update[4]]),
            11
        );
        assert_eq!(
            u32::from_le_bytes([update[5], update[6], update[7], update[8]]),
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
