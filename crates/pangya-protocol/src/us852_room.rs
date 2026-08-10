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
    CHARACTER_BLOCK_BYTES, CHARACTER_PARTS, CompatibilityProfile, DecodePacket, EncodePacket,
    PacketDecodeError, PacketEncodeError, PacketReader, PacketWriter, RetailCharacter, ServiceKind,
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
    /// One-player course practice.
    Practice = 0x13,
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
            0x13 => Some(Self::Practice),
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

/// One requested room-setting mutation carried by client opcode `0x000a`.
///
/// # Provenance
///
/// The tagged body shapes and valid value sets come from
/// `pangbox--packetdoc` `gameservice/client/000a.ksy` (ISC). Types 11 through 13
/// are additionally documented by `alter-pangya`
/// `RoomSettingsUpdatePacketHandler.kt`; their bodies are retained as protocol
/// facts and explicitly refused by the game service until matching authoritative aggregates
/// exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetailRoomSettingChange {
    /// Change the room name.
    Name(Vec<u8>),
    /// Change the room password; an empty value makes the room public.
    Password(Vec<u8>),
    /// Change the semantic room type.
    Mode(RetailRoomType),
    /// Change the course ordinal.
    Course(u8),
    /// Change the configured hole count.
    HoleCount(u8),
    /// Change the hole sequence.
    HoleProgression(RetailHoleProgression),
    /// Change the per-shot timer, in seconds.
    ShotTimerSeconds(u16),
    /// Change the room capacity.
    PlayerCount(u8),
    /// Change the whole-game timer, in minutes.
    GameTimerMinutes(u8),
    /// Change the repeated-hole selector.
    RepeatHole(u8),
    /// Change the fixed repeated-hole selector.
    FixedRepeatHole(i32),
    /// Change the selected artifact catalog id.
    Artifact(u32),
    /// Enable or disable natural wind.
    NaturalWind(bool),
}

/// Bounded maximum number of room-setting entries in one retail frame.
pub const MAX_RETAIL_ROOM_SETTING_CHANGES: usize = 14;

/// Room-settings update, client opcode `0x000a`.
///
/// The leading `u16` is `0xffff` in the documented examples. It is retained as
/// an unknown field rather than treated as authority: the server only acts on
/// the checked tagged updates that follow it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomSettingsUpdate {
    /// Independently validated requested changes, in their wire order.
    pub changes: Vec<RetailRoomSettingChange>,
}

impl DecodePacket for RetailRoomSettingsUpdate {
    const OPCODE: u16 = 0x000a;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let _unknown = reader.u16_le()?;
        let count = usize::from(reader.u8()?);
        if count > MAX_RETAIL_ROOM_SETTING_CHANGES {
            return Err(reader.invalid("room settings carries too many changes"));
        }
        let mut changes = Vec::with_capacity(count);
        for _ in 0..count {
            let setting = match reader.u8()? {
                0 => RetailRoomSettingChange::Name(reader.pstring(ROOM_NAME_BYTES)?.to_vec()),
                1 => RetailRoomSettingChange::Password(reader.pstring(ROOM_NAME_BYTES)?.to_vec()),
                2 => {
                    let mode = RetailRoomType::from_wire(reader.u8()?)
                        .ok_or_else(|| reader.invalid("unknown room type"))?;
                    RetailRoomSettingChange::Mode(mode)
                }
                3 => {
                    let course = reader.u8()?;
                    if !is_retail_course(course) {
                        return Err(reader.invalid("unknown room course"));
                    }
                    RetailRoomSettingChange::Course(course)
                }
                4 => {
                    let hole_count = reader.u8()?;
                    if !matches!(hole_count, 1 | 3 | 6 | 9 | 18) {
                        return Err(reader.invalid("invalid room hole count"));
                    }
                    RetailRoomSettingChange::HoleCount(hole_count)
                }
                5 => {
                    let progression = match reader.u8()? {
                        0 => RetailHoleProgression::FrontStart,
                        1 => RetailHoleProgression::BackStart,
                        2 => RetailHoleProgression::RandomStart,
                        3 => RetailHoleProgression::ShuffleAll,
                        _ => return Err(reader.invalid("unknown hole progression")),
                    };
                    RetailRoomSettingChange::HoleProgression(progression)
                }
                6 => {
                    let seconds = reader.u16_le()?;
                    if !matches!(seconds, 30 | 60 | 120 | 300) {
                        return Err(reader.invalid("invalid shot timer"));
                    }
                    RetailRoomSettingChange::ShotTimerSeconds(seconds)
                }
                7 => {
                    let player_count = reader.u8()?;
                    if !matches!(player_count, 2 | 3 | 4 | 10 | 20 | 30) {
                        return Err(reader.invalid("invalid room capacity"));
                    }
                    RetailRoomSettingChange::PlayerCount(player_count)
                }
                8 => {
                    let minutes = reader.u8()?;
                    if !matches!(minutes, 15 | 20 | 25 | 30 | 35 | 40 | 45 | 50) {
                        return Err(reader.invalid("invalid game timer"));
                    }
                    RetailRoomSettingChange::GameTimerMinutes(minutes)
                }
                11 => RetailRoomSettingChange::RepeatHole(reader.u8()?),
                12 => RetailRoomSettingChange::FixedRepeatHole(reader.i32_le()?),
                13 => RetailRoomSettingChange::Artifact(reader.u32_le()?),
                14 => match reader.u32_le()? {
                    0 => RetailRoomSettingChange::NaturalWind(false),
                    1 => RetailRoomSettingChange::NaturalWind(true),
                    _ => return Err(reader.invalid("invalid natural wind flag")),
                },
                _ => return Err(reader.invalid("unknown room setting type")),
            };
            changes.push(setting);
        }
        Ok(Self { changes })
    }
}

fn is_retail_course(course: u8) -> bool {
    matches!(course, 0x00..=0x0b | 0x0d..=0x10 | 0x12..=0x14 | 0x7f)
}

/// Team selection, client opcode `0x0010`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RetailTeam {
    /// Red team.
    Red = 0,
    /// Blue team.
    Blue = 1,
}

impl RetailTeam {
    fn from_wire(reader: &PacketReader<'_>, value: u8) -> Result<Self, PacketDecodeError> {
        match value {
            0 => Ok(Self::Red),
            1 => Ok(Self::Blue),
            _ => Err(reader.invalid("unknown room team")),
        }
    }
}

/// Team change request, client opcode `0x0010`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailTeamChange {
    /// Requested team.
    pub team: RetailTeam,
}

impl DecodePacket for RetailTeamChange {
    const OPCODE: u16 = 0x0010;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let team = reader.u8()?;
        Ok(Self {
            team: RetailTeam::from_wire(reader, team)?,
        })
    }
}

/// Room resync request, client opcode `0x001c`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomResync {
    /// Reference-defined opaque request marker.
    pub marker: u8,
    /// Opaque per-player entries retained for bounded validation.
    pub entries: Vec<(u8, u32)>,
}

impl DecodePacket for RetailRoomResync {
    const OPCODE: u16 = 0x001c;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let marker = reader.u8()?;
        let count = usize::from(reader.u8()?);
        if count > MAX_ROOM_PLAYERS as usize {
            return Err(reader.invalid("room resync carries too many entries"));
        }
        let entries = (0..count)
            .map(|_| Ok((reader.u8()?, reader.u32_le()?)))
            .collect::<Result<Vec<_>, PacketDecodeError>>()?;
        Ok(Self { marker, entries })
    }
}

/// Room information request, client opcode `0x002d`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailRoomInformationRequest {
    /// Room number from the room directory.
    pub room_number: u16,
}

impl DecodePacket for RetailRoomInformationRequest {
    const OPCODE: u16 = 0x002d;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self {
            room_number: reader.u16_le()?,
        })
    }
}

/// Master kick request, client opcode `0x0026`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailRoomKick {
    /// Connection ID to remove.
    pub connection_id: u32,
}

impl DecodePacket for RetailRoomKick {
    const OPCODE: u16 = 0x0026;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self {
            connection_id: reader.u32_le()?,
        })
    }
}

/// Legacy room invite information request, client opcode `0x0029`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailRoomInviteInfo {
    /// Invited account ID.
    pub account_id: u32,
}

impl DecodePacket for RetailRoomInviteInfo {
    const OPCODE: u16 = 0x0029;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self {
            account_id: reader.u32_le()?,
        })
    }
}

/// Newer room invite request, client opcode `0x00ba`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomInvite {
    /// Invited nickname as sent by the client.
    pub nickname: Vec<u8>,
    /// Invited account ID.
    pub account_id: u32,
}

impl DecodePacket for RetailRoomInvite {
    const OPCODE: u16 = 0x00ba;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self {
            nickname: reader.pstring(ROOM_NAME_BYTES)?.to_vec(),
            account_id: reader.u32_le()?,
        })
    }
}

/// Server team-change announce, opcode `0x007d`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailTeamChangeAnnounce {
    /// Member whose team changed.
    pub connection_id: u32,
    /// New team.
    pub team: RetailTeam,
}

impl EncodePacket for RetailTeamChangeAnnounce {
    const OPCODE: u16 = 0x007d;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.connection_id);
        writer.u8(self.team as u8);
        Ok(())
    }
}

/// Server acknowledgement for legacy invites, opcode `0x0130`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailRoomInviteInfoResponse {
    /// Invited account ID.
    pub account_id: u32,
}

impl EncodePacket for RetailRoomInviteInfoResponse {
    const OPCODE: u16 = 0x0130;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.account_id);
        Ok(())
    }
}

/// Server acknowledgement for newer invites, opcode `0x012f`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomInviteResponse {
    /// Server identity (one deployment-specific value).
    pub server_id: u32,
    /// Channel identity.
    pub channel_id: u8,
    /// Room number.
    pub room_id: u16,
    /// Inviter account ID.
    pub inviter_id: u32,
    /// Inviter nickname.
    pub inviter_nickname: Vec<u8>,
    /// Invitee account ID.
    pub invitee_id: u32,
}

impl EncodePacket for RetailRoomInviteResponse {
    const OPCODE: u16 = 0x012f;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u16_le(0xffff);
        writer.u32_le(self.server_id);
        writer.u8(self.channel_id);
        writer.u16_le(self.room_id);
        writer.u32_le(self.inviter_id);
        writer.pstring(&self.inviter_nickname, ROOM_NAME_BYTES)?;
        writer.u32_le(self.invitee_id);
        Ok(())
    }
}

/// Invitation delivered to the invitee, opcode `0x0083`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomInviteNotification {
    /// Server identity (one deployment-specific value).
    pub server_id: u32,
    /// Channel identity.
    pub channel_id: u8,
    /// Room number.
    pub room_id: u16,
    /// Inviter account ID.
    pub inviter_id: u32,
    /// Inviter nickname.
    pub inviter_nickname: Vec<u8>,
    /// Invitee account ID.
    pub invitee_id: u32,
}

impl EncodePacket for RetailRoomInviteNotification {
    const OPCODE: u16 = 0x0083;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u16_le(0xffff);
        writer.u32_le(self.server_id);
        writer.u8(self.channel_id);
        writer.u16_le(self.room_id);
        writer.u32_le(self.inviter_id);
        writer.pstring(&self.inviter_nickname, ROOM_NAME_BYTES)?;
        writer.u32_le(self.invitee_id);
        Ok(())
    }
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

/// Full room-equipment submission that starts one-player Practice, client opcode `0x000c` type 7.
///
/// # Provenance
///
/// SuperSS-Dev parses type `TC_ALL = 7` as character inventory id, caddie inventory id, ClubSet
/// inventory id, and Ball catalog id in that order (`TYPE/pangya_game_st.h:2876-2906`,
/// `GAME/channel.cpp:9588-9619`). Its `TC_ALL` branch validates/equips those values and calls
/// `startGame` (`GAME/room.cpp:2601-3239`). Other `0x000c` types are deliberately not accepted by
/// this model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailPracticeStart {
    /// Equipped character inventory id.
    pub character_id: u32,
    /// Equipped caddie inventory id, or zero.
    pub caddie_id: u32,
    /// Equipped ClubSet inventory id.
    pub clubset_id: u32,
    /// Equipped Ball catalog id.
    pub ball_type_id: u32,
}

impl DecodePacket for RetailPracticeStart {
    const OPCODE: u16 = 0x000c;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let kind = reader.u8()?;
        if kind != 7 {
            return Err(reader.invalid("room equipment submission is not TC_ALL (7)"));
        }
        Ok(Self {
            character_id: reader.u32_le()?,
            caddie_id: reader.u32_le()?,
            clubset_id: reader.u32_le()?,
            ball_type_id: reader.u32_le()?,
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
    /// Semantic room type. Practice uses 19 even though its UI-family mode is 4.
    pub play_mode: u8,
    /// UI-family mode. Practice and tournament both use 4 here.
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
    /// Artifact catalog id selected for the room.
    pub artifact_id: u32,
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
        // The later semantic type is distinct from the earlier UI-family byte for Practice.
        writer.u8(self.play_mode);
        writer.u32_le(self.artifact_id);
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
/// actually is now, including the configured whole-card course shape.
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
/// `ClientEquipmentUpdate`), ISC licensed. Type names `8` (mascot) and `9` (cut-in) follow the
/// issue's named GB.852 behavioral reference; their client bodies remain the packetdoc words.
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
    /// Equipped mascot roster slot.
    Mascot,
    /// Equipped cut-in data for a character.
    CutIn,
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
            Self::Mascot => 8,
            Self::CutIn => 9,
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
            8 => Some(Self::Mascot),
            9 => Some(Self::CutIn),
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
    /// The worn part set out of the 513-byte character body.
    ///
    /// Only the fields this server can act on are lifted out; the rest of the block — cards,
    /// stats, cut-in, the two large unknown runs — is still read and dropped by the bounded
    /// decoder, because nothing persists them and inventing a model for them would be a guess.
    ///
    /// Layout from `pangbox/server` `pangya/player.go:141-159` (`PlayerCharacterData`):
    /// `CharTypeID u32 @0x000`, `ID u32 @0x004`, `HairColor u8 @0x008`, `Shirt u8 @0x009`, two
    /// unknown bytes, `PartTypeIDs [24]u32 @0x00c`, `PartIDs [24]u32 @0x06c`, then 216 unknown
    /// bytes before the aux parts. The whole body is 513 bytes, which is the width
    /// `RetailCharacter` already writes.
    CharacterParts(RetailCharacterParts),
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
    /// Owned mascot roster slot.
    Mascot(u32),
    /// Cut-in selections associated with a character.
    CutIn {
        /// Owned character id.
        character_id: u32,
        /// PacketDoc-defined opaque bytes.
        data: [u8; 16],
    },
}

/// The worn part set the client sent inside a character block.
///
/// Deliberately not the whole block. This carries what the server can honestly hold: which
/// character the outfit belongs to, its hair colour, and the 24 part slots as paired
/// `(type id, owned row id)`. A slot the client leaves at zero is an empty slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailCharacterParts {
    /// Catalog id of the character wearing these parts.
    pub character_type_id: u32,
    /// Owned character row.
    pub character_id: u32,
    /// Hair colour index, persisted on `characters` but still sent as zero in the bootstrap.
    pub hair_color: u8,
    /// Catalog id per slot, `0` for an empty slot.
    pub part_type_ids: [u32; CHARACTER_PARTS],
    /// Owned row per slot, `0` when the client named no owned row.
    pub part_uids: [u32; CHARACTER_PARTS],
}

impl RetailCharacterParts {
    /// Bytes of the character block this reads before the tail it drops.
    const PREFIX_BYTES: usize = 0x0c + CHARACTER_PARTS * 4 * 2;
    /// Total width of the block — the same constant `RetailCharacter` writes, so the two cannot
    /// drift apart.
    const BODY_BYTES: usize = CHARACTER_BLOCK_BYTES;

    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        let character_type_id = reader.u32_le()?;
        let character_id = reader.u32_le()?;
        let hair_color = reader.u8()?;
        // Shirt, then two bytes this project has never identified.
        let _shirt = reader.u8()?;
        let _unknown = reader.array::<2>()?;
        let mut part_type_ids = [0_u32; CHARACTER_PARTS];
        for value in &mut part_type_ids {
            *value = reader.u32_le()?;
        }
        let mut part_uids = [0_u32; CHARACTER_PARTS];
        for value in &mut part_uids {
            *value = reader.u32_le()?;
        }
        // Everything after the part arrays — aux parts, cut-in, stats, mastery, three card
        // groups — is consumed so the frame is fully accounted for, and dropped because nothing
        // persists it. Reading it into a model this server cannot honour would be the guess
        // this decoder exists to avoid.
        let _tail = reader
            .array::<{ RetailCharacterParts::BODY_BYTES - RetailCharacterParts::PREFIX_BYTES }>()?;
        Ok(Self {
            character_type_id,
            character_id,
            hair_color,
            part_type_ids,
            part_uids,
        })
    }
}

/// Equipment changes sent by the lobby/room opcodes `0x000b` and `0x000c`.
///
/// Both packets use the leading byte and little-endian scalar bodies. The active packetdoc
/// `gameservice/client/000b.ksy` resolves the issue-28 collision as equipment (`type 4` followed
/// by one little-endian roster-slot word); the `pangbox/server` tutorial-start alias has no body
/// parser and is not selected here. SuperSS-Dev's `requestChangePlayerItemChannel` additionally
/// handles caddie, ball, and club set (`1`–`4`); `0x000c` additionally has the four-word `7` form.
/// The channel implementation follows the broader 852-targeting reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetailRoomEquipmentUpdate {
    /// Equipped caddie roster slot.
    Caddie(u32),
    /// Equipped ball catalog id.
    Ball(u32),
    /// Equipped club-set inventory slot.
    ClubSet(u32),
    /// Equipped character roster slot.
    Character(u32),
    /// Reference-defined but unclassified combined form.
    UnknownSeven {
        /// Character roster slot.
        character: u32,
        /// Caddie roster slot.
        caddie: u32,
        /// Club-set inventory slot.
        club_set: u32,
        /// Ball catalog id.
        ball: u32,
    },
}

impl RetailRoomEquipmentUpdate {
    fn decode_body(reader: &mut PacketReader<'_>, lobby: bool) -> Result<Self, PacketDecodeError> {
        let tag = reader.u8()?;
        let value = match tag {
            1 => Self::Caddie(reader.u32_le()?),
            2 => Self::Ball(reader.u32_le()?),
            3 => Self::ClubSet(reader.u32_le()?),
            4 => Self::Character(reader.u32_le()?),
            7 if !lobby => Self::UnknownSeven {
                character: reader.u32_le()?,
                caddie: reader.u32_le()?,
                club_set: reader.u32_le()?,
                ball: reader.u32_le()?,
            },
            _ => return Err(reader.invalid("unsupported lobby/room equipment type")),
        };
        Ok(value)
    }
}

/// Equipment change in the channel/lobby, client opcode `0x000b`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailLobbyEquipmentUpdate(pub RetailRoomEquipmentUpdate);

impl DecodePacket for RetailLobbyEquipmentUpdate {
    const OPCODE: u16 = 0x000b;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self(RetailRoomEquipmentUpdate::decode_body(reader, true)?))
    }
}

/// Equipment change in a room, client opcode `0x000c`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailRoomEquipmentUpdatePacket(pub RetailRoomEquipmentUpdate);

impl DecodePacket for RetailRoomEquipmentUpdatePacket {
    const OPCODE: u16 = 0x000c;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self(RetailRoomEquipmentUpdate::decode_body(reader, false)?))
    }
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
            RetailEquipmentSlot::CharacterParts => {
                RetailEquipmentRequested::CharacterParts(RetailCharacterParts::decode(reader)?)
            }
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
            RetailEquipmentSlot::Mascot => RetailEquipmentRequested::Mascot(reader.u32_le()?),
            RetailEquipmentSlot::CutIn => {
                let character_id = reader.u32_le()?;
                let data = reader.array::<16>()?;
                RetailEquipmentRequested::CutIn { character_id, data }
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
    /// Validation/ownership failure; status 1 retains the old durable projection.
    Rejected {
        /// Slot whose prior projection remains active.
        slot: RetailEquipmentSlot,
    },
    /// Full equipped-character body returned for subtype zero.
    CharacterFull(RetailCharacter),
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
    /// Mascot response body reserved by packetdoc (62 bytes).
    Mascot {
        /// Preserved mascot response bytes.
        data: [u8; 62],
    },
    /// Cut-in response body from packetdoc.
    CutIn {
        /// Character roster slot.
        character_id: u32,
        /// Opaque subtype-9 bytes. PacketDoc does not establish skin semantics for these bytes.
        data: [u8; 16],
    },
}

impl RetailEquipmentUpdated {
    /// Status byte upstream sends alongside an applied update.
    pub const STATUS_APPLIED: u8 = 0x04;

    /// Returns the slot this reply describes.
    #[must_use]
    pub const fn slot(&self) -> RetailEquipmentSlot {
        match self {
            Self::Rejected { slot } => *slot,
            Self::CharacterFull(_) => RetailEquipmentSlot::CharacterParts,
            Self::Caddie { .. } => RetailEquipmentSlot::Caddie,
            Self::Consumables { .. } => RetailEquipmentSlot::Consumables,
            Self::BallAndClub { .. } => RetailEquipmentSlot::Ball,
            Self::Decoration { .. } => RetailEquipmentSlot::Decoration,
            Self::Character { .. } => RetailEquipmentSlot::Character,
            Self::Mascot { .. } => RetailEquipmentSlot::Mascot,
            Self::CutIn { .. } => RetailEquipmentSlot::CutIn,
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
        writer.u8(if matches!(self, Self::Rejected { .. }) {
            1
        } else {
            Self::STATUS_APPLIED
        });
        writer.u8(self.slot().tag());
        match self {
            Self::Rejected { .. } => {}

            Self::CharacterFull(character) => character.encode_body(writer),
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
            Self::Mascot { data } => writer.bytes(data),
            Self::CutIn { character_id, data } => {
                writer.u32_le(*character_id);
                writer.bytes(data);
            }
        }
        Ok(())
    }
}

/// Room-wide user equipment announcement, server opcode `0x004b`.
///
/// The four-byte zero prefix and connection id are part of packetdoc's layout. Unlike the
/// self-acknowledgement `0x006b`, this frame is addressed to every room member and carries the
/// selected user's public equipment projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetailEquipmentAnnounce {
    /// Caddie roster slot and catalog id.
    Caddie {
        /// Connection whose loadout changed.
        connection_id: u32,
        /// Roster/inventory slot.
        caddie_uid: u32,
        /// Caddie catalog id.
        caddie_type_id: u32,
    },
    /// Ball catalog id.
    Ball {
        /// Connection whose loadout changed.
        connection_id: u32,
        /// Ball catalog id.
        ball_type_id: u32,
    },
    /// Club-set inventory slot and catalog id.
    ClubSet {
        /// Connection whose loadout changed.
        connection_id: u32,
        /// Inventory slot.
        club_item_id: u32,
        /// Club-set catalog id.
        club_type_id: u32,
    },
    /// Equipped character data.
    Character {
        /// Connection whose loadout changed.
        connection_id: u32,
        /// Character catalog id.
        character_type_id: u32,
        /// Character roster slot.
        character_uid: u32,
    },
}

impl EncodePacket for RetailEquipmentAnnounce {
    const OPCODE: u16 = 0x004b;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.bytes(&[0; 4]);
        match self {
            Self::Caddie {
                connection_id,
                caddie_uid,
                caddie_type_id,
            } => {
                writer.u8(1);
                writer.u32_le(*connection_id);
                writer.u32_le(*caddie_uid);
                writer.u32_le(*caddie_type_id);
                writer.bytes(&[0; 4]);
                writer.u8(0);
                writer.u32_le(0);
                writer.bytes(&[0; 8]);
            }
            Self::Ball {
                connection_id,
                ball_type_id,
            } => {
                writer.u8(2);
                writer.u32_le(*connection_id);
                writer.u32_le(*ball_type_id);
            }
            Self::ClubSet {
                connection_id,
                club_item_id,
                club_type_id,
            } => {
                writer.u8(3);
                writer.u32_le(*connection_id);
                writer.u32_le(*club_item_id);
                writer.u32_le(*club_type_id);
                writer.bytes(&[0; 20]);
            }
            Self::Character {
                connection_id,
                character_type_id,
                character_uid,
            } => {
                writer.u8(4);
                writer.u32_le(*connection_id);
                RetailCharacter {
                    iff_id: *character_type_id,
                    uid: *character_uid,
                    hair_color: 0,
                    part_iff_ids: [0; CHARACTER_PARTS],
                    part_uids: [0; CHARACTER_PARTS],
                    stats: [0; crate::CHARACTER_STATS],
                    mastery: 0,
                }
                .encode_body(writer);
            }
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

/// Opens the Daily Quest panel, client opcode `0x0151`.
///
/// # Provenance
///
/// SuperSS-Dev registers `0x0151` as `packet151` and routes it to `requestDailyQuest`
/// (`Game Server/game_server.cpp:449`, `PACKET/packet_func_sv.cpp:3499-3512`). The request has no
/// body (`GAME/channel.cpp:6892-6906`). Daily quests are Tier D, so this server truthfully returns
/// an empty current state rather than inventing mutable quest records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailDailyQuestRequest;

impl DecodePacket for RetailDailyQuestRequest {
    const OPCODE: u16 = 0x0151;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self)
    }
}

/// Empty achievement delta sent before Daily Quest state, server opcode `0x0216`.
///
/// SuperSS-Dev writes UTC Unix time followed by a zero achievement count when there are no new
/// daily quests (`UTIL/mgr_daily_quest.cpp:110-118`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailDailyQuestDelta {
    /// UTC Unix time truncated to the client's 32-bit field.
    pub server_time: u32,
}

impl EncodePacket for RetailDailyQuestDelta {
    const OPCODE: u16 = 0x0216;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.server_time);
        writer.u32_le(0); // achievement count
        Ok(())
    }
}

/// Empty Daily Quest state, server opcode `0x0225`.
///
/// The option-zero shape is option, current date, accept date, quest count, and deleted-quest
/// count (`PACKET/packet_func_sv.cpp:5914-5939`). All are zero because Tier-D quest state is not
/// implemented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailDailyQuestState;

impl EncodePacket for RetailDailyQuestState {
    const OPCODE: u16 = 0x0225;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.bytes(&[0; 20]);
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

/// Client request to persist a mascot message, client opcode `0x0073`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailMascotMessageUpdate {
    /// Owned mascot inventory row.
    pub mascot_id: u32,
    /// New message bytes.
    pub message: Vec<u8>,
}

impl DecodePacket for RetailMascotMessageUpdate {
    const OPCODE: u16 = 0x0073;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let mascot_id = reader.u32_le()?;
        let message = reader.pstring(30)?.to_vec();
        if message.is_empty() || message.contains(&0) {
            return Err(reader.invalid("mascot message must be non-empty and NUL-free"));
        }
        Ok(Self { mascot_id, message })
    }
}

impl EncodePacket for RetailMascotMessageUpdate {
    const OPCODE: u16 = 0x0073;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        if self.message.is_empty() || self.message.len() > 30 || self.message.contains(&0) {
            return Err(PacketEncodeError::Invalid {
                field: "mascot message",
            });
        }
        writer.u32_le(self.mascot_id);
        writer.pstring(&self.message, 30)
    }
}

/// Mascot message result, server opcode `0x00e2`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailMascotMessageResult {
    /// Result status (`4` is success, `255` is refusal).
    pub status: u8,
    /// Mascot inventory row addressed by the request.
    pub mascot_id: u32,
    /// Message returned on success.
    pub message: Vec<u8>,
    /// Authoritative Pang balance.
    pub pang: u64,
}

impl EncodePacket for RetailMascotMessageResult {
    const OPCODE: u16 = 0x00e2;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u8(self.status);
        writer.u32_le(self.mascot_id);
        writer.pstring(&self.message, 30)?;
        writer.u64_le(self.pang);
        Ok(())
    }
}

/// One furniture entry in the retail `0x012d` My Room layout.
///
/// PacketDoc defines a fixed 27-byte entry: four opaque bytes, a Furniture.iff catalog id,
/// and nineteen opaque bytes. The opaque bytes are retained rather than being reinterpreted as
/// coordinates because the checked-out GB/US references do not establish those fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailMyRoomFurniture {
    /// Opaque entry prefix.
    pub unknown_prefix: [u8; 4],
    /// Furniture.iff catalog id.
    pub item_type_id: u32,
    /// Opaque entry suffix.
    pub unknown_suffix: [u8; 19],
}

impl RetailMyRoomFurniture {
    /// Constructs an entry using the reference's zero-filled opaque fields.
    #[must_use]
    pub const fn new(item_type_id: u32) -> Self {
        Self {
            unknown_prefix: [0; 4],
            item_type_id,
            unknown_suffix: [0; 19],
        }
    }

    fn encode_body(&self, writer: &mut PacketWriter) {
        writer.bytes(&self.unknown_prefix);
        writer.u32_le(self.item_type_id);
        writer.bytes(&self.unknown_suffix);
    }
}

/// My Room furniture layout, server opcode `0x012d`.
///
/// # Provenance
///
/// PacketDoc `gameservice/server/012d.ksy` defines a `u4` option (`1`), a `u2` count, then
/// repeated fixed 27-byte entries. The count is bounded here before it reaches the wire.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailMyRoomLayout {
    /// Persisted furniture entries in deterministic database order.
    pub furniture: Vec<RetailMyRoomFurniture>,
}

impl RetailMyRoomLayout {
    /// Maximum entries accepted in one client-visible layout.
    pub const MAX_FURNITURE: usize = 1024;

    /// Creates a layout; encoding enforces the bounded entry count.
    #[must_use]
    pub fn new(furniture: Vec<RetailMyRoomFurniture>) -> Self {
        Self { furniture }
    }
}

impl EncodePacket for RetailMyRoomLayout {
    const OPCODE: u16 = 0x012d;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        let count = u16::try_from(self.furniture.len()).map_err(|_| PacketEncodeError::Limit {
            field: "furniture count",
            actual: self.furniture.len(),
            maximum: usize::from(u16::MAX),
        })?;
        if self.furniture.len() > Self::MAX_FURNITURE {
            return Err(PacketEncodeError::Limit {
                field: "furniture count",
                actual: self.furniture.len(),
                maximum: Self::MAX_FURNITURE,
            });
        }
        writer.u32_le(1);
        writer.u16_le(count);
        for furniture in &self.furniture {
            furniture.encode_body(writer);
        }
        Ok(())
    }
}

/// Explicitly safe refusal for the unsupported UCC upload-key flow, server opcode `0x0153`.
///
/// SuperSS-Dev's error response is two success/status bytes followed by the bounded system
/// error `0x05100100`. Returning that packet is preferable to silently dropping `0x00c9`, while
/// no upload URL, bearer, or proprietary asset is fabricated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailUccUploadKeyRefusal;

impl RetailUccUploadKeyRefusal {
    /// Stable server-side refusal used when UCC upload infrastructure is disabled.
    #[must_use]
    pub const fn unsupported() -> Self {
        Self
    }

    /// Encodes the refusal while retaining the service argument at the call boundary used by
    /// protocol compliance tests and future multi-service routing.
    pub fn encode_for(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
        _service: ServiceKind,
    ) -> Result<(), PacketEncodeError> {
        self.encode(writer, profile)
    }
}

impl EncodePacket for RetailUccUploadKeyRefusal {
    const OPCODE: u16 = 0x0153;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u8(1);
        writer.u8(1);
        writer.u32_le(0x0510_0100);
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

/// One user entry in the room-information response, server opcode `0x0086`.
///
/// This is deliberately not [`RetailRoomPlayer`]. PacketDoc `gameservice/server/0086.ksy`
/// defines a compact 18-byte user record; the 341-byte identity record belongs only to the
/// room census (`0x0048`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomInformationUser {
    /// Per-connection identifier.
    pub connection_id: u32,
    /// User rank byte.
    pub rank: u8,
    /// Reference-defined opaque bytes.
    pub unknown_a: [u8; 5],
    /// Custom title badge catalog id.
    pub title_badge: u32,
    /// Reference-defined opaque bytes.
    pub unknown_c: [u8; 4],
}

impl RetailRoomInformationUser {
    /// Builds the reference-compatible compact record with zero opaque fields.
    #[must_use]
    pub const fn new(connection_id: u32, rank: u8, title_badge: u32) -> Self {
        Self {
            connection_id,
            rank,
            unknown_a: [0; 5],
            title_badge,
            unknown_c: [0; 4],
        }
    }

    fn encode_body(&self, writer: &mut PacketWriter) {
        writer.u32_le(self.connection_id);
        writer.u8(self.rank);
        writer.bytes(&self.unknown_a);
        writer.u32_le(self.title_badge);
        writer.bytes(&self.unknown_c);
    }
}

/// Room information response, server opcode `0x0086`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailRoomInformationResponse {
    /// Compact public user records for the requested room.
    pub players: Vec<RetailRoomInformationUser>,
}

impl EncodePacket for RetailRoomInformationResponse {
    const OPCODE: u16 = 0x0086;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        let count = u32::try_from(self.players.len()).map_err(|_| PacketEncodeError::Limit {
            field: "room players",
            actual: self.players.len(),
            maximum: u32::MAX as usize,
        })?;
        writer.u32_le(count);
        writer.bytes(&[0; 12]);
        for player in &self.players {
            player.encode_body(writer);
        }
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
            (
                RetailEquipmentUpdated::Mascot { data: [0; 62] },
                RetailEquipmentSlot::Mascot,
                2 + 62,
            ),
            (
                RetailEquipmentUpdated::CutIn {
                    character_id: 3,
                    data: [0; 16],
                },
                RetailEquipmentSlot::CutIn,
                2 + 4 + 16,
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
    fn packetdoc_000b_and_000c_equipment_bodies_are_distinct_and_little_endian() {
        for (tag, value, expected) in [
            (1_u8, 11_u32, RetailRoomEquipmentUpdate::Caddie(11)),
            (2, 22, RetailRoomEquipmentUpdate::Ball(22)),
            (3, 33, RetailRoomEquipmentUpdate::ClubSet(33)),
            (4, 44, RetailRoomEquipmentUpdate::Character(44)),
        ] {
            let mut payload = vec![tag];
            payload.extend_from_slice(&value.to_le_bytes());
            let lobby = decode_packet_payload::<RetailLobbyEquipmentUpdate>(
                &payload,
                &profile(),
                ServiceKind::Game,
            )
            .expect("decode lobby equipment");
            assert_eq!(lobby.0, expected);
        }

        let room = decode_packet_payload::<RetailRoomEquipmentUpdatePacket>(
            &[3, 0x37, 0, 0, 0],
            &profile(),
            ServiceKind::Game,
        )
        .expect("decode room equipment");
        assert_eq!(room.0, RetailRoomEquipmentUpdate::ClubSet(55));
    }

    #[test]
    fn packetdoc_004b_has_zero_prefix_type_connection_and_payload() {
        let packet = RetailEquipmentAnnounce::Character {
            connection_id: 0x1122_3344,
            character_type_id: 0x0400_000b,
            character_uid: 42,
        };
        let bytes = encode_packet_payload(&packet, &profile()).expect("encode announce");
        assert_eq!(&bytes[..9], &[0, 0, 0, 0, 4, 0x44, 0x33, 0x22, 0x11]);
        assert_eq!(bytes.len(), 9 + CHARACTER_BLOCK_BYTES);
    }

    #[test]
    fn equipment_update_decodes_mascot_and_cut_in_subtypes() {
        let mascot = decode_packet_payload::<RetailEquipmentUpdate>(
            &[8, 7, 0, 0, 0],
            &profile(),
            ServiceKind::Game,
        )
        .expect("decode mascot");
        assert_eq!(mascot.requested, RetailEquipmentRequested::Mascot(7));
        let mut cut_in = vec![9];
        for value in [42_u32, 100, 200, 300, 400] {
            cut_in.extend_from_slice(&value.to_le_bytes());
        }
        let cut_in =
            decode_packet_payload::<RetailEquipmentUpdate>(&cut_in, &profile(), ServiceKind::Game)
                .expect("decode cut-in");
        assert_eq!(
            cut_in.requested,
            RetailEquipmentRequested::CutIn {
                character_id: 42,
                data: [100, 0, 0, 0, 200, 0, 0, 0, 44, 1, 0, 0, 144, 1, 0, 0,],
            }
        );
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

    #[test]
    fn practice_start_decodes_the_reference_equipment_order() {
        let mut payload = vec![7];
        for value in [257_u32, 0, 310, 0x1400_00c9] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        let request =
            decode_packet_payload::<RetailPracticeStart>(&payload, &profile(), ServiceKind::Game)
                .expect("practice start");
        assert_eq!(request.character_id, 257);
        assert_eq!(request.caddie_id, 0);
        assert_eq!(request.clubset_id, 310);
        assert_eq!(request.ball_type_id, 0x1400_00c9);
        payload[0] = 6;
        assert!(
            decode_packet_payload::<RetailPracticeStart>(&payload, &profile(), ServiceKind::Game)
                .is_err()
        );
    }

    #[test]
    fn empty_daily_quest_sequence_matches_the_reference_widths() {
        decode_packet_payload::<RetailDailyQuestRequest>(&[], &profile(), ServiceKind::Game)
            .expect("empty request");
        let delta = encode_packet_payload(
            &RetailDailyQuestDelta {
                server_time: 0x1234_5678,
            },
            &profile(),
        )
        .expect("delta");
        assert_eq!(delta.as_slice(), [0x78, 0x56, 0x34, 0x12, 0, 0, 0, 0]);
        assert_eq!(
            encode_packet_payload(&RetailDailyQuestState, &profile())
                .expect("state")
                .as_slice(),
            [0; 20]
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
            artifact_id: 1234,
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
        assert_eq!(
            u32::from_le_bytes(payload[4 + 186..4 + 190].try_into().expect("artifact")),
            1234,
            "the reference room-list record carries ArtifactID"
        );
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

    /// `pangbox--packetdoc` `gameservice/client/000a.ksy` defines this tagged sequence;
    /// natural-wind's little-endian `u4` is intentionally not copied from alter's BE read.
    #[test]
    fn room_settings_update_decodes_every_applied_retail_setting() {
        let mut writer = PacketWriter::default();
        writer.u16_le(0xffff);
        writer.u8(9);
        writer.u8(1);
        writer
            .pstring(b"digest", ROOM_NAME_BYTES)
            .expect("password");
        writer.u8(3);
        writer.u8(7);
        writer.u8(4);
        writer.u8(9);
        writer.u8(5);
        writer.u8(RetailHoleProgression::ShuffleAll as u8);
        writer.u8(6);
        writer.u16_le(120);
        writer.u8(7);
        writer.u8(4);
        writer.u8(8);
        writer.u8(30);
        writer.u8(14);
        writer.u32_le(1);
        writer.u8(13);
        writer.u32_le(0x1234_5678);

        let update = decode_packet_payload::<RetailRoomSettingsUpdate>(
            &writer.into_inner(),
            &profile(),
            ServiceKind::Game,
        )
        .expect("room settings update");
        assert_eq!(
            update.changes,
            vec![
                RetailRoomSettingChange::Password(b"digest".to_vec()),
                RetailRoomSettingChange::Course(7),
                RetailRoomSettingChange::HoleCount(9),
                RetailRoomSettingChange::HoleProgression(RetailHoleProgression::ShuffleAll),
                RetailRoomSettingChange::ShotTimerSeconds(120),
                RetailRoomSettingChange::PlayerCount(4),
                RetailRoomSettingChange::GameTimerMinutes(30),
                RetailRoomSettingChange::NaturalWind(true),
                RetailRoomSettingChange::Artifact(0x1234_5678),
            ]
        );
    }

    /// PacketDoc `000a.ksy` has no recovery frame: a partial tagged body must be a decode error.
    #[test]
    fn room_settings_update_refuses_a_truncated_tagged_body() {
        let payload = [0xff, 0xff, 1, 6, 120];
        assert!(
            decode_packet_payload::<RetailRoomSettingsUpdate>(
                &payload,
                &profile(),
                ServiceKind::Game,
            )
            .is_err()
        );
    }

    /// PacketDoc `000a.ksy` limits the setting enum and natural-wind to `0` or `1`.
    #[test]
    fn room_settings_update_accepts_generic_one_hole_cards() {
        let payload = [0xff, 0xff, 1, 4, 1];
        let decoded = decode_packet_payload::<RetailRoomSettingsUpdate>(
            &payload,
            &profile(),
            ServiceKind::Game,
        )
        .expect("one-hole setting");
        assert_eq!(decoded.changes, vec![RetailRoomSettingChange::HoleCount(1)]);
    }

    #[test]
    fn room_settings_update_refuses_unknown_or_out_of_range_values() {
        let unknown = [0xff, 0xff, 1, 10];
        assert!(
            decode_packet_payload::<RetailRoomSettingsUpdate>(
                &unknown,
                &profile(),
                ServiceKind::Game,
            )
            .is_err()
        );
        let wind = [0xff, 0xff, 1, 14, 2, 0, 0, 0];
        assert!(
            decode_packet_payload::<RetailRoomSettingsUpdate>(&wind, &profile(), ServiceKind::Game)
                .is_err()
        );
    }

    #[test]
    fn room_information_uses_packetdoc_18_byte_user_records() {
        let payload = encode_packet_payload(
            &RetailRoomInformationResponse {
                players: vec![RetailRoomInformationUser::new(0x1122_3344, 7, 0x5566_7788)],
            },
            &profile(),
        )
        .expect("room information");
        assert_eq!(payload.len(), 16 + 18);
        assert_eq!(&payload[..4], &1_u32.to_le_bytes());
        assert_eq!(&payload[4..16], &[0; 12]);
        assert_eq!(&payload[16..20], &0x1122_3344_u32.to_le_bytes());
        assert_eq!(payload[20], 7);
        assert_eq!(&payload[26..30], &0x5566_7788_u32.to_le_bytes());
    }

    #[test]
    fn room_management_requests_follow_packetdoc_layouts() {
        let team = decode_packet_payload::<RetailTeamChange>(&[1], &profile(), ServiceKind::Game)
            .expect("team change");
        assert_eq!(team.team, RetailTeam::Blue);
        let resync = decode_packet_payload::<RetailRoomResync>(
            &[0, 1, 0, 0x78, 0x56, 0x34, 0x12],
            &profile(),
            ServiceKind::Game,
        )
        .expect("resync");
        assert_eq!(resync.entries, vec![(0, 0x1234_5678)]);
        let info = decode_packet_payload::<RetailRoomInformationRequest>(
            &[9, 0],
            &profile(),
            ServiceKind::Game,
        )
        .expect("room info");
        assert_eq!(info.room_number, 9);
        let kick =
            decode_packet_payload::<RetailRoomKick>(&[7, 0, 0, 0], &profile(), ServiceKind::Game)
                .expect("kick");
        assert_eq!(kick.connection_id, 7);
        let invite = decode_packet_payload::<RetailRoomInviteInfo>(
            &[42, 0, 0, 0],
            &profile(),
            ServiceKind::Game,
        )
        .expect("legacy invite");
        assert_eq!(invite.account_id, 42);
        let mut newer = vec![3, 0, b'B', b'o', b'b'];
        newer.extend_from_slice(&42_u32.to_le_bytes());
        let invite =
            decode_packet_payload::<RetailRoomInvite>(&newer, &profile(), ServiceKind::Game)
                .expect("new invite");
        assert_eq!(invite.nickname, b"Bob");
        assert_eq!(invite.account_id, 42);
    }

    #[test]
    fn room_management_responses_keep_reference_opcodes_and_widths() {
        let announce = encode_packet_payload(
            &RetailTeamChangeAnnounce {
                connection_id: 7,
                team: RetailTeam::Blue,
            },
            &profile(),
        )
        .expect("announce");
        assert_eq!(announce.as_slice(), [7, 0, 0, 0, 1]);
        let response =
            encode_packet_payload(&RetailRoomInviteInfoResponse { account_id: 42 }, &profile())
                .expect("legacy response");
        assert_eq!(response.as_slice(), [42, 0, 0, 0]);
        let notification = encode_packet_payload(
            &RetailRoomInviteNotification {
                server_id: 1,
                channel_id: 2,
                room_id: 3,
                inviter_id: 4,
                inviter_nickname: b"Host".to_vec(),
                invitee_id: 5,
            },
            &profile(),
        )
        .expect("target notification");
        assert_eq!(notification[0..2], [255, 255]);
        assert_eq!(RetailRoomInviteInfo::OPCODE, 0x0029);
        assert_eq!(RetailRoomInvite::OPCODE, 0x00ba);
        assert_eq!(RetailRoomInviteInfoResponse::OPCODE, 0x0130);
        assert_eq!(RetailRoomInviteResponse::OPCODE, 0x012f);
        assert_eq!(RetailRoomInviteNotification::OPCODE, 0x0083);
        let response = encode_packet_payload(
            &RetailRoomInviteResponse {
                server_id: 1,
                channel_id: 2,
                room_id: 3,
                inviter_id: 4,
                inviter_nickname: b"Host".to_vec(),
                invitee_id: 5,
            },
            &profile(),
        )
        .expect("new response");
        assert_eq!(&response[..9], &[255, 255, 1, 0, 0, 0, 2, 3, 0]);
        assert_eq!(&response[response.len() - 4..], &[5, 0, 0, 0]);
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
        assert_eq!(
            RetailRoomType::from_wire(0x13),
            Some(RetailRoomType::Practice)
        );
        assert_eq!(RetailRoomType::from_wire(0x01), None);
        assert_eq!(RetailRoomType::from_wire(0xff), None);
    }

    /// The part arrays sit at fixed offsets inside a 513-byte block, and reading them one field
    /// early or late still yields plausible-looking u32s. This pins the offsets against the
    /// reference layout rather than trusting that the decode "looked right".
    #[test]
    fn a_character_block_yields_its_part_slots_from_the_documented_offsets() {
        let mut body = vec![0_u8; 1 + CHARACTER_BLOCK_BYTES];
        body[0] = 0; // slot tag: character parts
        let block = &mut body[1..];
        block[0x000..0x004].copy_from_slice(&0x0400_0002_u32.to_le_bytes()); // CharTypeID
        block[0x004..0x008].copy_from_slice(&4242_u32.to_le_bytes()); // ID
        block[0x008] = 7; // HairColor
        block[0x009] = 3; // Shirt, read and dropped
        // PartTypeIDs @0x00c, PartIDs @0x06c
        block[0x00c..0x010].copy_from_slice(&0x0800_0400_u32.to_le_bytes());
        block[0x00c + 23 * 4..0x00c + 24 * 4].copy_from_slice(&0x0800_04ff_u32.to_le_bytes());
        block[0x06c..0x070].copy_from_slice(&909_u32.to_le_bytes());

        let update = crate::decode_packet_payload::<RetailEquipmentUpdate>(
            &body,
            &CompatibilityProfile::US_852,
            crate::ServiceKind::Game,
        )
        .expect("decode");
        let RetailEquipmentRequested::CharacterParts(parts) = update.requested else {
            panic!("expected character parts, got {:?}", update.requested);
        };
        assert_eq!(parts.character_type_id, 0x0400_0002);
        assert_eq!(parts.character_id, 4242);
        assert_eq!(parts.hair_color, 7);
        assert_eq!(parts.part_type_ids[0], 0x0800_0400);
        assert_eq!(
            parts.part_type_ids[23], 0x0800_04ff,
            "the last slot must not be clipped"
        );
        assert_eq!(parts.part_uids[0], 909);
        assert_eq!(parts.part_type_ids[1], 0, "an untouched slot stays empty");
    }

    /// A body one byte short must fail rather than decode a shifted outfit.
    #[test]
    fn a_truncated_character_block_is_refused() {
        let body = vec![0_u8; CHARACTER_BLOCK_BYTES]; // tag + 512, one short
        assert!(
            crate::decode_packet_payload::<RetailEquipmentUpdate>(
                &body,
                &CompatibilityProfile::US_852,
                crate::ServiceKind::Game,
            )
            .is_err()
        );
    }
}
