//! Provisional, local-only synthetic M4 room packet models.
//!
//! These opcodes and layouts are generated for local integration. They are not
//! attributed to, or claimed compatible with, any retail client protocol.

use crate::{
    ClientVersion, CompatibilityProfile, ConnectionState, DecodePacket, Direction, EncodePacket,
    PacketDecodeError, PacketEncodeError, PacketReader, PacketRegistry, PacketWriter, RegistryKey,
    ServiceKind,
};
use pangya_domain::{
    AccountId, ChatText, MemberSnapshot, Nickname, PlayerConnectionId, RoomId, RoomName,
    RoomPassword, RoomSettings, RoomSnapshot, RoomSummary,
};

/// Provisional C->S room-list opcode.
pub const SYNTHETIC_M4_C2S_LIST: u16 = 0x7f00;
/// Provisional C->S room-create opcode.
pub const SYNTHETIC_M4_C2S_CREATE: u16 = 0x7f01;
/// Provisional C->S room-join opcode.
pub const SYNTHETIC_M4_C2S_JOIN: u16 = 0x7f02;
/// Provisional C->S room-leave opcode.
pub const SYNTHETIC_M4_C2S_LEAVE: u16 = 0x7f03;
/// Provisional C->S room-settings opcode.
pub const SYNTHETIC_M4_C2S_SETTINGS: u16 = 0x7f04;
/// Provisional C->S room-ready opcode.
pub const SYNTHETIC_M4_C2S_READY: u16 = 0x7f05;
/// Provisional C->S room-chat opcode.
pub const SYNTHETIC_M4_C2S_CHAT: u16 = 0x7f06;
/// Provisional C->S room-kick opcode.
pub const SYNTHETIC_M4_C2S_KICK: u16 = 0x7f07;
/// Provisional C->S room-state opcode.
pub const SYNTHETIC_M4_C2S_STATE: u16 = 0x7f08;

/// Provisional S->C room-list opcode.
pub const SYNTHETIC_M4_S2C_LIST: u16 = 0x7f80;
/// Provisional S->C room-state opcode.
pub const SYNTHETIC_M4_S2C_STATE: u16 = 0x7f81;
/// Provisional S->C room-command-result opcode.
pub const SYNTHETIC_M4_S2C_COMMAND_RESULT: u16 = 0x7f82;
/// Provisional S->C room-membership-event opcode.
pub const SYNTHETIC_M4_S2C_MEMBERSHIP_EVENT: u16 = 0x7f83;
/// Provisional S->C room-chat-event opcode.
pub const SYNTHETIC_M4_S2C_CHAT: u16 = 0x7f84;

/// Maximum room summaries decoded before allocation.
pub const MAX_ROOM_SUMMARIES: usize = 4096;
/// Maximum room members decoded before allocation.
pub const MAX_ROOM_MEMBERS: usize = 30;
const MAX_ROOM_NAME_BYTES: usize = 32;
const MAX_ROOM_PASSWORD_BYTES: usize = 16;
const MAX_CHAT_BYTES: usize = 128;
const MAX_NICKNAME_BYTES: usize = 64;

fn require_end(reader: &PacketReader<'_>) -> Result<(), PacketDecodeError> {
    if reader.remaining() == 0 {
        Ok(())
    } else {
        Err(reader.invalid("synthetic M4 packet has trailing bytes"))
    }
}

fn decode_bool(
    reader: &mut PacketReader<'_>,
    field: &'static str,
) -> Result<bool, PacketDecodeError> {
    match reader.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(reader.invalid(format!("{field} is not a canonical boolean"))),
    }
}

fn decode_utf8<'a>(
    reader: &mut PacketReader<'a>,
    maximum: usize,
    field: &'static str,
) -> Result<&'a str, PacketDecodeError> {
    std::str::from_utf8(reader.pstring(maximum)?)
        .map_err(|_| reader.invalid(format!("{field} is not UTF-8")))
}

fn decode_room_name(reader: &mut PacketReader<'_>) -> Result<RoomName, PacketDecodeError> {
    RoomName::parse(decode_utf8(reader, MAX_ROOM_NAME_BYTES, "room name")?)
        .map_err(|_| reader.invalid("room name violates domain policy"))
}

fn decode_password(reader: &mut PacketReader<'_>) -> Result<RoomPassword, PacketDecodeError> {
    RoomPassword::parse(decode_utf8(
        reader,
        MAX_ROOM_PASSWORD_BYTES,
        "room password",
    )?)
    .map_err(|_| reader.invalid("room password violates domain policy"))
}

fn decode_chat(reader: &mut PacketReader<'_>) -> Result<ChatText, PacketDecodeError> {
    ChatText::parse(decode_utf8(reader, MAX_CHAT_BYTES, "chat text")?)
        .map_err(|_| reader.invalid("chat text violates domain policy"))
}

fn decode_nickname(reader: &mut PacketReader<'_>) -> Result<String, PacketDecodeError> {
    let wire = decode_utf8(reader, MAX_NICKNAME_BYTES, "nickname")?;
    let nickname =
        Nickname::parse(wire).map_err(|_| reader.invalid("nickname violates domain policy"))?;
    if nickname.display() != wire {
        return Err(reader.invalid("nickname is not in canonical display form"));
    }
    Ok(nickname.display().to_owned())
}

fn encode_nickname(writer: &mut PacketWriter, value: &str) -> Result<(), PacketEncodeError> {
    let nickname =
        Nickname::parse(value).map_err(|_| PacketEncodeError::Invalid { field: "nickname" })?;
    if nickname.display() != value {
        return Err(PacketEncodeError::Invalid { field: "nickname" });
    }
    writer.pstring(value.as_bytes(), MAX_NICKNAME_BYTES)
}

fn decode_room_id(reader: &mut PacketReader<'_>) -> Result<RoomId, PacketDecodeError> {
    RoomId::new(reader.u32_le()?).map_err(|_| reader.invalid("room ID must be nonzero"))
}

fn decode_connection_id(
    reader: &mut PacketReader<'_>,
) -> Result<PlayerConnectionId, PacketDecodeError> {
    PlayerConnectionId::new(reader.u64_le()?)
        .map_err(|_| reader.invalid("connection ID must be nonzero"))
}

fn decode_account_id(reader: &mut PacketReader<'_>) -> Result<AccountId, PacketDecodeError> {
    AccountId::try_from(reader.i64_le()?).map_err(|_| reader.invalid("account ID must be positive"))
}

fn decode_settings(reader: &mut PacketReader<'_>) -> Result<RoomSettings, PacketDecodeError> {
    RoomSettings::new(reader.u8()?)
        .map_err(|_| reader.invalid("room capacity violates domain policy"))
}

fn encode_summary(
    writer: &mut PacketWriter,
    summary: &RoomSummary,
) -> Result<(), PacketEncodeError> {
    RoomSettings::new(summary.max_members()).map_err(|_| PacketEncodeError::Invalid {
        field: "room capacity",
    })?;
    if summary.members() > summary.max_members() {
        return Err(PacketEncodeError::Invalid {
            field: "room occupancy",
        });
    }
    writer.u32_le(summary.id().get());
    writer.pstring(summary.name().as_str().as_bytes(), MAX_ROOM_NAME_BYTES)?;
    encode_nickname(writer, summary.owner_nickname())?;
    writer.u8(summary.members());
    writer.u8(summary.max_members());
    writer.u8(u8::from(summary.password_protected()));
    Ok(())
}

fn decode_summary(reader: &mut PacketReader<'_>) -> Result<RoomSummary, PacketDecodeError> {
    let id = decode_room_id(reader)?;
    let name = decode_room_name(reader)?;
    let owner_nickname = decode_nickname(reader)?;
    let members = reader.u8()?;
    let settings = decode_settings(reader)?;
    if members > settings.max_members() {
        return Err(reader.invalid("room occupancy exceeds capacity"));
    }
    let password_protected = decode_bool(reader, "password-protected flag")?;
    Ok(RoomSummary::new(
        id,
        name,
        owner_nickname,
        members,
        settings.max_members(),
        password_protected,
        // The synthetic family carries a capacity and nothing else of the room's shape, so a
        // summary decoded from it describes the default game rather than inventing one.
        settings.profile(),
    ))
}

fn encode_member(
    writer: &mut PacketWriter,
    member: &MemberSnapshot,
) -> Result<(), PacketEncodeError> {
    writer.u64_le(member.connection_id().get());
    writer.i64_le(member.account_id().get());
    encode_nickname(writer, member.nickname())?;
    writer.u8(u8::from(member.is_owner()));
    writer.u8(u8::from(member.is_ready()));
    Ok(())
}

fn decode_member(reader: &mut PacketReader<'_>) -> Result<MemberSnapshot, PacketDecodeError> {
    Ok(MemberSnapshot::new(
        decode_connection_id(reader)?,
        decode_account_id(reader)?,
        decode_nickname(reader)?,
        decode_bool(reader, "owner flag")?,
        decode_bool(reader, "ready flag")?,
        // The synthetic roster frame predates the character fields and carries neither.
        None,
        None,
    ))
}

fn encode_snapshot(
    writer: &mut PacketWriter,
    snapshot: &RoomSnapshot,
) -> Result<(), PacketEncodeError> {
    if snapshot.members().len() != usize::from(snapshot.summary().members()) {
        return Err(PacketEncodeError::Invalid {
            field: "room snapshot occupancy",
        });
    }
    encode_summary(writer, snapshot.summary())?;
    writer.count_u16(snapshot.members().len(), MAX_ROOM_MEMBERS)?;
    for member in snapshot.members() {
        encode_member(writer, member)?;
    }
    Ok(())
}

fn decode_snapshot(reader: &mut PacketReader<'_>) -> Result<RoomSnapshot, PacketDecodeError> {
    let summary = decode_summary(reader)?;
    let count = reader.count_u16(MAX_ROOM_MEMBERS)?;
    if count != usize::from(summary.members()) {
        return Err(reader.invalid("room snapshot occupancy does not match member count"));
    }
    let members = reader.vector(count, MAX_ROOM_MEMBERS, decode_member)?;
    Ok(RoomSnapshot::new(summary, members))
}

/// Empty request for the current channel's bounded room list.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RoomListRequest;

impl DecodePacket for RoomListRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_LIST;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        require_end(reader)?;
        Ok(Self)
    }
}

impl EncodePacket for RoomListRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_LIST;

    fn encode(
        &self,
        _writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        Ok(())
    }
}

/// Request to create a room. Sender identity comes from the connection context.
#[derive(Debug, Eq, PartialEq)]
pub struct RoomCreateRequest {
    /// Validated public room name.
    pub name: RoomName,
    /// Optional ephemeral password input.
    pub password: Option<RoomPassword>,
    /// Initial validated room settings.
    pub settings: RoomSettings,
}

impl DecodePacket for RoomCreateRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_CREATE;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let name = decode_room_name(reader)?;
        let password = if decode_bool(reader, "password presence")? {
            Some(decode_password(reader)?)
        } else {
            None
        };
        let settings = decode_settings(reader)?;
        require_end(reader)?;
        Ok(Self {
            name,
            password,
            settings,
        })
    }
}

impl EncodePacket for RoomCreateRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_CREATE;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.pstring(self.name.as_str().as_bytes(), MAX_ROOM_NAME_BYTES)?;
        writer.u8(u8::from(self.password.is_some()));
        if let Some(password) = &self.password {
            writer.pstring(password.expose_bytes(), MAX_ROOM_PASSWORD_BYTES)?;
        }
        writer.u8(self.settings.max_members());
        Ok(())
    }
}

/// Request to join one room. Sender identity comes from the connection context.
#[derive(Debug, Eq, PartialEq)]
pub struct RoomJoinRequest {
    /// Target room.
    pub room_id: RoomId,
    /// Optional ephemeral password input, zeroized by its domain type.
    pub password: Option<RoomPassword>,
}

impl DecodePacket for RoomJoinRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_JOIN;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let room_id = decode_room_id(reader)?;
        let password = if decode_bool(reader, "password presence")? {
            Some(decode_password(reader)?)
        } else {
            None
        };
        require_end(reader)?;
        Ok(Self { room_id, password })
    }
}

impl EncodePacket for RoomJoinRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_JOIN;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u32_le(self.room_id.get());
        writer.u8(u8::from(self.password.is_some()));
        if let Some(password) = &self.password {
            writer.pstring(password.expose_bytes(), MAX_ROOM_PASSWORD_BYTES)?;
        }
        Ok(())
    }
}

/// Empty room-leave request.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RoomLeaveRequest;

impl DecodePacket for RoomLeaveRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_LEAVE;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        require_end(reader)?;
        Ok(Self)
    }
}

impl EncodePacket for RoomLeaveRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_LEAVE;

    fn encode(
        &self,
        _writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        Ok(())
    }
}

/// Request to replace mutable room settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomSettingsRequest {
    /// New validated settings.
    pub settings: RoomSettings,
}

impl DecodePacket for RoomSettingsRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_SETTINGS;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let settings = decode_settings(reader)?;
        require_end(reader)?;
        Ok(Self { settings })
    }
}

impl EncodePacket for RoomSettingsRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_SETTINGS;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u8(self.settings.max_members());
        Ok(())
    }
}

/// Request to set the connection-derived member's ready state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomReadyRequest {
    /// Requested ready state.
    pub ready: bool,
}

impl DecodePacket for RoomReadyRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_READY;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let ready = decode_bool(reader, "ready flag")?;
        require_end(reader)?;
        Ok(Self { ready })
    }
}

impl EncodePacket for RoomReadyRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_READY;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u8(u8::from(self.ready));
        Ok(())
    }
}

/// Chat request without a client-supplied sender identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomChatRequest {
    /// Validated chat text.
    pub text: ChatText,
}

impl DecodePacket for RoomChatRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_CHAT;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let text = decode_chat(reader)?;
        require_end(reader)?;
        Ok(Self { text })
    }
}

impl EncodePacket for RoomChatRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_CHAT;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.pstring(self.text.as_str().as_bytes(), MAX_CHAT_BYTES)
    }
}

/// Owner kick request; only the target identity is client supplied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomKickRequest {
    /// Target member connection ID.
    pub target: PlayerConnectionId,
}

impl DecodePacket for RoomKickRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_KICK;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let target = decode_connection_id(reader)?;
        require_end(reader)?;
        Ok(Self { target })
    }
}

impl EncodePacket for RoomKickRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_KICK;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u64_le(self.target.get());
        Ok(())
    }
}

/// Empty request for the connection's current room state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RoomStateRequest;

impl DecodePacket for RoomStateRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_STATE;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        require_end(reader)?;
        Ok(Self)
    }
}

impl EncodePacket for RoomStateRequest {
    const OPCODE: u16 = SYNTHETIC_M4_C2S_STATE;

    fn encode(
        &self,
        _writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        Ok(())
    }
}

/// Bounded public room-list response. Passwords and digests are absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomListResponse {
    /// Public summaries.
    pub rooms: Vec<RoomSummary>,
}

impl DecodePacket for RoomListResponse {
    const OPCODE: u16 = SYNTHETIC_M4_S2C_LIST;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let count = reader.count_u16(MAX_ROOM_SUMMARIES)?;
        let rooms = reader.vector(count, MAX_ROOM_SUMMARIES, decode_summary)?;
        require_end(reader)?;
        Ok(Self { rooms })
    }
}

impl EncodePacket for RoomListResponse {
    const OPCODE: u16 = SYNTHETIC_M4_S2C_LIST;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.count_u16(self.rooms.len(), MAX_ROOM_SUMMARIES)?;
        for room in &self.rooms {
            encode_summary(writer, room)?;
        }
        Ok(())
    }
}

/// Full public room-state response. Passwords and digests are absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomStateResponse {
    /// Public snapshot.
    pub room: RoomSnapshot,
}

impl DecodePacket for RoomStateResponse {
    const OPCODE: u16 = SYNTHETIC_M4_S2C_STATE;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let room = decode_snapshot(reader)?;
        require_end(reader)?;
        Ok(Self { room })
    }
}

impl EncodePacket for RoomStateResponse {
    const OPCODE: u16 = SYNTHETIC_M4_S2C_STATE;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        encode_snapshot(writer, &self.room)
    }
}

/// Fixed command discriminator echoed in a command result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RoomCommand {
    /// List rooms.
    List = 0,
    /// Create a room.
    Create = 1,
    /// Join a room.
    Join = 2,
    /// Leave a room.
    Leave = 3,
    /// Change settings.
    Settings = 4,
    /// Change ready state.
    Ready = 5,
    /// Send chat.
    Chat = 6,
    /// Kick a member.
    Kick = 7,
    /// Fetch room state.
    State = 8,
}

impl RoomCommand {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::List),
            1 => Ok(Self::Create),
            2 => Ok(Self::Join),
            3 => Ok(Self::Leave),
            4 => Ok(Self::Settings),
            5 => Ok(Self::Ready),
            6 => Ok(Self::Chat),
            7 => Ok(Self::Kick),
            8 => Ok(Self::State),
            _ => Err(reader.invalid("unknown room command discriminator")),
        }
    }
}

/// Fixed, public command outcome discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RoomCommandResult {
    /// Command completed.
    Success = 0,
    /// Bounded command queue is full.
    QueueFull = 1,
    /// Room actor is closed.
    Closed = 2,
    /// Connection already belongs to a room.
    AlreadyMember = 3,
    /// Room is full.
    Full = 4,
    /// Password authentication failed.
    InvalidPassword = 5,
    /// Connection is not a member.
    NotMember = 6,
    /// Operation requires ownership.
    NotOwner = 7,
    /// Owner attempted to kick itself.
    CannotKickSelf = 8,
    /// Target member was not found.
    MemberNotFound = 9,
    /// Capacity is below occupancy.
    CapacityBelowOccupancy = 10,
    /// Lobby room limit was reached.
    MaxRooms = 11,
    /// Room was not found.
    RoomNotFound = 12,
    /// Room identifier space is exhausted.
    IdExhausted = 13,
    /// Operation timed out.
    Timeout = 14,
}

impl RoomCommandResult {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::Success),
            1 => Ok(Self::QueueFull),
            2 => Ok(Self::Closed),
            3 => Ok(Self::AlreadyMember),
            4 => Ok(Self::Full),
            5 => Ok(Self::InvalidPassword),
            6 => Ok(Self::NotMember),
            7 => Ok(Self::NotOwner),
            8 => Ok(Self::CannotKickSelf),
            9 => Ok(Self::MemberNotFound),
            10 => Ok(Self::CapacityBelowOccupancy),
            11 => Ok(Self::MaxRooms),
            12 => Ok(Self::RoomNotFound),
            13 => Ok(Self::IdExhausted),
            14 => Ok(Self::Timeout),
            _ => Err(reader.invalid("unknown room result discriminator")),
        }
    }
}

/// Result for one synthetic M4 command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomCommandResultResponse {
    /// Command being acknowledged.
    pub command: RoomCommand,
    /// Stable public result.
    pub result: RoomCommandResult,
}

impl DecodePacket for RoomCommandResultResponse {
    const OPCODE: u16 = SYNTHETIC_M4_S2C_COMMAND_RESULT;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let command = RoomCommand::decode(reader)?;
        let result = RoomCommandResult::decode(reader)?;
        require_end(reader)?;
        Ok(Self { command, result })
    }
}

impl EncodePacket for RoomCommandResultResponse {
    const OPCODE: u16 = SYNTHETIC_M4_S2C_COMMAND_RESULT;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u8(self.command as u8);
        writer.u8(self.result as u8);
        Ok(())
    }
}

/// Fixed membership-event discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RoomMembershipKind {
    /// Member joined.
    Joined = 0,
    /// Member left voluntarily.
    Left = 1,
    /// Member was kicked.
    Kicked = 2,
    /// Member became owner.
    OwnerChanged = 3,
}

impl RoomMembershipKind {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::Joined),
            1 => Ok(Self::Left),
            2 => Ok(Self::Kicked),
            3 => Ok(Self::OwnerChanged),
            _ => Err(reader.invalid("unknown membership-event discriminator")),
        }
    }
}

/// Public room membership event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomMembershipEvent {
    /// Room producing the event.
    pub room_id: RoomId,
    /// Fixed event kind.
    pub kind: RoomMembershipKind,
    /// Server-derived public member projection.
    pub member: MemberSnapshot,
}

impl DecodePacket for RoomMembershipEvent {
    const OPCODE: u16 = SYNTHETIC_M4_S2C_MEMBERSHIP_EVENT;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let room_id = decode_room_id(reader)?;
        let kind = RoomMembershipKind::decode(reader)?;
        let member = decode_member(reader)?;
        require_end(reader)?;
        Ok(Self {
            room_id,
            kind,
            member,
        })
    }
}

impl EncodePacket for RoomMembershipEvent {
    const OPCODE: u16 = SYNTHETIC_M4_S2C_MEMBERSHIP_EVENT;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u32_le(self.room_id.get());
        writer.u8(self.kind as u8);
        encode_member(writer, &self.member)
    }
}

/// Server chat event with a server-derived member sender projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomChatEvent {
    /// Room producing the event.
    pub room_id: RoomId,
    /// Sender derived from authenticated room membership, never the request body.
    pub sender: MemberSnapshot,
    /// Validated chat text.
    pub text: ChatText,
}

impl DecodePacket for RoomChatEvent {
    const OPCODE: u16 = SYNTHETIC_M4_S2C_CHAT;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let room_id = decode_room_id(reader)?;
        let sender = decode_member(reader)?;
        let text = decode_chat(reader)?;
        require_end(reader)?;
        Ok(Self {
            room_id,
            sender,
            text,
        })
    }
}

impl EncodePacket for RoomChatEvent {
    const OPCODE: u16 = SYNTHETIC_M4_S2C_CHAT;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u32_le(self.room_id.get());
        encode_member(writer, &self.sender)?;
        writer.pstring(self.text.as_str().as_bytes(), MAX_CHAT_BYTES)
    }
}

/// Builds the local synthetic M4 inbound registry for one selected client version.
#[must_use]
pub fn synthetic_m4_registry(version: ClientVersion) -> PacketRegistry {
    let mut registry = PacketRegistry::new();
    for opcode in [
        SYNTHETIC_M4_C2S_LIST,
        SYNTHETIC_M4_C2S_CREATE,
        SYNTHETIC_M4_C2S_JOIN,
    ] {
        registry.register(RegistryKey {
            service: ServiceKind::Game,
            direction: Direction::ClientToServer,
            version,
            state: ConnectionState::InChannel,
            opcode,
        });
    }
    for opcode in [
        SYNTHETIC_M4_C2S_LEAVE,
        SYNTHETIC_M4_C2S_SETTINGS,
        SYNTHETIC_M4_C2S_READY,
        SYNTHETIC_M4_C2S_CHAT,
        SYNTHETIC_M4_C2S_KICK,
        SYNTHETIC_M4_C2S_STATE,
    ] {
        registry.register(RegistryKey {
            service: ServiceKind::Game,
            direction: Direction::ClientToServer,
            version,
            state: ConnectionState::InRoom,
            opcode,
        });
    }
    registry
}
