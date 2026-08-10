#![allow(missing_docs)]

//! MessageService protocol and social state boundary.
//!
//! MessageService has an independent opcode namespace. These IDs intentionally overlap game
//! packets and must never be registered in the GameService table.

use sqlx::{PgPool, Row};
use std::{
    collections::{HashMap, VecDeque},
    net::IpAddr,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use thiserror::Error;

mod runtime;
pub use runtime::{MessageRuntimeError, MessageService};

/// MessageService client opcodes (little-endian wire IDs).
pub mod client_opcode {
    pub const LOGIN: u16 = 0x12;
    pub const UNKNOWN: u16 = 0x13;
    pub const HELLO: u16 = 0x14;
    pub const GOODBYE: u16 = 0x16;
    pub const LOOKUP: u16 = 0x17;
    pub const ADD_FRIEND: u16 = 0x18;
    pub const CONFIRM_FRIEND: u16 = 0x19;
    pub const BLOCK_FRIEND: u16 = 0x1a;
    pub const UNBLOCK_FRIEND: u16 = 0x1b;
    pub const DELETE_FRIEND: u16 = 0x1c;
    pub const STATUS: u16 = 0x1d;
    pub const CHAT: u16 = 0x1e;
    pub const ALIAS: u16 = 0x1f;
    pub const SERVER: u16 = 0x23;
    pub const ROOM_INVITE: u16 = 0x24;
    pub const GUILD_CHAT: u16 = 0x25;
    pub const GUILD_BATTLE_INVITE: u16 = 0x28;
    pub const GIFT: u16 = 0x29;
    pub const GUILD_ACCEPT: u16 = 0x2a;
    pub const GUILD_KICK: u16 = 0x2b;
    pub const GUILD_IMAGE: u16 = 0x2c;
    pub const GUILD_NAME: u16 = 0x2d;
}
/// MessageService server opcodes.
pub mod server_opcode {
    pub const CREDENTIAL_RESPONSE: u16 = 0x2f;
    pub const STATUS: u16 = 0x30;
}
/// Maximum user-controlled MessageService text.
pub const MAX_TEXT_BYTES: usize = 512;
/// Maximum nickname bytes accepted by MessageService.
pub const MAX_NICKNAME_BYTES: usize = 22;
/// Maximum friend rows emitted in one page.
pub const FRIEND_PAGE_SIZE: usize = 30;
/// Maximum friend rows materialized for one hello/list response.
pub const MAX_FRIEND_ROWS: usize = FRIEND_PAGE_SIZE * 10;
/// Maximum queued messages claimed or presence events emitted in one pass.
pub const MAX_DELIVERY_BATCH: usize = 30;
/// Maximum pending/live/in-flight chat rows retained for one recipient in memory.
pub const MAX_QUEUED_MESSAGES: usize = 1024;
/// Maximum replay nonces retained by a session.
pub const MAX_REPLAY_NONCES: usize = 256;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Page {
    pub number: u8,
    pub total: u16,
    pub current: u16,
}
impl Page {
    fn encode(&self, out: &mut Vec<u8>, entry_count: usize) {
        // PacketDoc 0x0102: unknown:u8, unknown:u16, entry_count:u32.
        out.push(self.number);
        out.extend(self.total.to_le_bytes());
        out.extend((entry_count as u32).to_le_bytes());
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ChannelInfo {
    pub room_number: i16,
    pub room_type: i32,
    pub server_id: u32,
    pub channel_id: i8,
    pub channel_name: Vec<u8>,
}
impl ChannelInfo {
    pub fn offline() -> Self {
        Self {
            room_number: -1,
            room_type: -1,
            server_id: u32::MAX,
            channel_id: -1,
            channel_name: Vec::new(),
        }
    }
}
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Presence {
    Playing = 0,
    Idle = 1,
    Busy = 3,
    Online = 4,
    Offline = 5,
}
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Relationship {
    Friend = 1,
    GuildMember = 2,
    FriendAndGuild = 3,
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FriendEntry {
    pub nickname: Vec<u8>,
    pub alias: Vec<u8>,
    pub user_id: u32,
    pub channel: ChannelInfo,
    pub state: Presence,
    pub relationship: Relationship,
    pub blocked: bool,
}
impl FriendEntry {
    const WIRE_LEN: usize = 22 + 25 + 4 + 99 + 2 + 1 + 2;

    fn encode(&self, out: &mut Vec<u8>) {
        fixed(out, &self.nickname, 22);
        // PacketDoc 0x0102 entry: nickname[22], alias[25], uid, then 104
        // bytes whose meanings are not established by the corpus. Presence is
        // sent separately as subtype 0x0115; do not invent fields here.
        fixed(out, &self.alias, 25);
        out.extend(self.user_id.to_le_bytes());
        // PacketDoc deliberately leaves the complete 104-byte tail opaque. SuperSS's
        // ChannelPlayerInfo projection is useful behavioural evidence for 0x0115, but it is
        // not authority to reinterpret this 0x0102/0x0104 field. Keep it byte-exact and opaque.
        out.extend([0; 104]);
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ClientPacket {
    CredentialDeclaration {
        user_id: u32,
        user_nickname: Vec<u8>,
    },
    Unknown,
    Hello,
    Goodbye,
    Lookup {
        nickname: Vec<u8>,
    },
    AddFriend {
        user_id: u32,
        nickname: Vec<u8>,
    },
    ConfirmFriend {
        user_id: u32,
    },
    BlockFriend {
        user_id: u32,
    },
    UnblockFriend {
        user_id: u32,
    },
    DeleteFriend {
        user_id: u32,
        nickname: Vec<u8>,
    },
    Status {
        status: Presence,
    },
    Chat {
        user_id: u32,
        message: Vec<u8>,
    },
    Alias {
        user_id: u32,
        alias: Vec<u8>,
    },
    Server {
        unknown_a: [u8; 2],
        unknown_b: [u8; 4],
        server_id: u32,
        channel_id: u8,
        channel_name: Vec<u8>,
    },
    RoomInvite {
        user_id: u32,
    },
    GuildChat {
        message: Vec<u8>,
    },
    GuildBattleInvite {
        server_id: u32,
        channel_id: u8,
        room: u16,
        inviter_id: u32,
        inviter_nickname: Vec<u8>,
        invited_id: u32,
    },
    Gift {
        sender_id: u32,
        recipient_id: u32,
    },
    GuildAccept {
        user_id: u32,
    },
    GuildKick {
        user_id: u32,
    },
    GuildImage {
        guild_id: u32,
    },
    GuildName {
        guild_id: u32,
        name: Vec<u8>,
    },
}
impl ClientPacket {
    #[must_use]
    pub fn opcode(&self) -> u16 {
        match self {
            Self::CredentialDeclaration { .. } => 0x12,
            Self::Unknown => 0x13,
            Self::Hello => 0x14,
            Self::Goodbye => 0x16,
            Self::Lookup { .. } => 0x17,
            Self::AddFriend { .. } => 0x18,
            Self::ConfirmFriend { .. } => 0x19,
            Self::BlockFriend { .. } => 0x1a,
            Self::UnblockFriend { .. } => 0x1b,
            Self::DeleteFriend { .. } => 0x1c,
            Self::Status { .. } => 0x1d,
            Self::Chat { .. } => 0x1e,
            Self::Alias { .. } => 0x1f,
            Self::Server { .. } => 0x23,
            Self::RoomInvite { .. } => 0x24,
            Self::GuildChat { .. } => 0x25,
            Self::GuildBattleInvite { .. } => 0x28,
            Self::Gift { .. } => 0x29,
            Self::GuildAccept { .. } => 0x2a,
            Self::GuildKick { .. } => 0x2b,
            Self::GuildImage { .. } => 0x2c,
            Self::GuildName { .. } => 0x2d,
        }
    }
    pub fn decode(opcode: u16, bytes: &[u8]) -> Result<Self, MessageError> {
        let mut r = Reader::new(bytes);
        let p = match opcode {
            0x12 => Self::CredentialDeclaration {
                user_id: r.u32()?,
                user_nickname: r.string(MAX_NICKNAME_BYTES)?,
            },
            0x13 => Self::Unknown,
            0x14 => Self::Hello,
            0x16 => Self::Goodbye,
            0x17 => Self::Lookup {
                nickname: r.string(MAX_NICKNAME_BYTES)?,
            },
            0x18 => Self::AddFriend {
                user_id: r.u32()?,
                nickname: r.string(MAX_NICKNAME_BYTES)?,
            },
            0x19 => Self::ConfirmFriend { user_id: r.u32()? },
            0x1a => Self::BlockFriend { user_id: r.u32()? },
            0x1b => Self::UnblockFriend { user_id: r.u32()? },
            0x1c => Self::DeleteFriend {
                user_id: r.u32()?,
                nickname: r.string(MAX_NICKNAME_BYTES)?,
            },
            0x1d => Self::Status {
                status: Presence::try_from(r.u8()?)?,
            },
            0x1e => Self::Chat {
                user_id: r.u32()?,
                message: r.string(MAX_TEXT_BYTES)?,
            },
            0x1f => Self::Alias {
                user_id: r.u32()?,
                alias: r.string(10)?,
            },
            0x23 => Self::Server {
                unknown_a: r.array()?,
                unknown_b: r.array()?,
                server_id: r.u32()?,
                channel_id: r.u8()?,
                channel_name: r.fixed(64)?,
            },
            0x24 => Self::RoomInvite { user_id: r.u32()? },
            0x25 => Self::GuildChat {
                message: r.string(MAX_TEXT_BYTES)?,
            },
            0x28 => Self::GuildBattleInvite {
                server_id: r.u32()?,
                channel_id: r.u8()?,
                room: r.u16()?,
                inviter_id: r.u32()?,
                inviter_nickname: r.string(MAX_NICKNAME_BYTES)?,
                invited_id: r.u32()?,
            },
            0x29 => Self::Gift {
                sender_id: r.u32()?,
                recipient_id: r.u32()?,
            },
            0x2a => Self::GuildAccept { user_id: r.u32()? },
            0x2b => Self::GuildKick { user_id: r.u32()? },
            0x2c => Self::GuildImage { guild_id: r.u32()? },
            0x2d => Self::GuildName {
                guild_id: r.u32()?,
                name: r.string(64)?,
            },
            _ => return Err(MessageError::UnknownOpcode(opcode)),
        };
        r.end()?;
        Ok(p)
    }
    pub fn encode_payload(&self) -> Result<Vec<u8>, MessageError> {
        let mut o = Vec::new();
        match self {
            Self::CredentialDeclaration {
                user_id,
                user_nickname,
            } => {
                o.extend(user_id.to_le_bytes());
                string(&mut o, user_nickname, MAX_NICKNAME_BYTES)?;
            }
            Self::Lookup { nickname } => string(&mut o, nickname, MAX_NICKNAME_BYTES)?,
            Self::AddFriend { user_id, nickname } | Self::DeleteFriend { user_id, nickname } => {
                o.extend(user_id.to_le_bytes());
                string(&mut o, nickname, MAX_NICKNAME_BYTES)?;
            }
            Self::ConfirmFriend { user_id }
            | Self::BlockFriend { user_id }
            | Self::UnblockFriend { user_id }
            | Self::RoomInvite { user_id }
            | Self::GuildAccept { user_id }
            | Self::GuildKick { user_id } => o.extend(user_id.to_le_bytes()),
            Self::Status { status } => o.push(*status as u8),
            Self::Chat { user_id, message } => {
                o.extend(user_id.to_le_bytes());
                string(&mut o, message, MAX_TEXT_BYTES)?;
            }
            Self::Alias { user_id, alias } => {
                o.extend(user_id.to_le_bytes());
                string(&mut o, alias, 10)?;
            }
            Self::Server {
                unknown_a,
                unknown_b,
                server_id,
                channel_id,
                channel_name,
            } => {
                o.extend(unknown_a);
                o.extend(unknown_b);
                o.extend(server_id.to_le_bytes());
                o.push(*channel_id);
                fixed(&mut o, channel_name, 64);
            }
            Self::GuildChat { message } => string(&mut o, message, MAX_TEXT_BYTES)?,
            Self::GuildBattleInvite {
                server_id,
                channel_id,
                room,
                inviter_id,
                inviter_nickname,
                invited_id,
            } => {
                o.extend(server_id.to_le_bytes());
                o.push(*channel_id);
                o.extend(room.to_le_bytes());
                o.extend(inviter_id.to_le_bytes());
                string(&mut o, inviter_nickname, MAX_NICKNAME_BYTES)?;
                o.extend(invited_id.to_le_bytes());
            }
            Self::Gift {
                sender_id,
                recipient_id,
            } => {
                o.extend(sender_id.to_le_bytes());
                o.extend(recipient_id.to_le_bytes());
            }
            Self::GuildImage { guild_id } => o.extend(guild_id.to_le_bytes()),
            Self::GuildName { guild_id, name } => {
                o.extend(guild_id.to_le_bytes());
                string(&mut o, name, 64)?;
            }
            Self::Hello | Self::Goodbye | Self::Unknown => {}
        }
        Ok(o)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ServerPacket {
    CredentialResponse {
        user_id: u32,
    },
    FriendList {
        page: Page,
        entries: Vec<FriendEntry>,
    },
    LookupResponse {
        status: u32,
        nickname: Vec<u8>,
        user_id: Option<u32>,
    },
    FriendRequest {
        status: u32,
        entry: FriendEntry,
    },
    /// SuperSS/K4T-established confirmation result (0x0109).
    ConfirmResult {
        status: u32,
        user_id: u32,
    },
    /// Reference-described 0x0115 status packet. Its eleven-byte, one-byte, and 64-byte
    /// regions are intentionally opaque: PacketDoc does not assign them presence semantics.
    Presence {
        user_id: u32,
        unknown_f: [u8; 11],
        server_id: u32,
        unknown_g: u8,
        unknown_h: Vec<u8>,
    },
    /// SuperSS 0x010b delete-friend result.
    DeleteResult {
        status: u32,
        user_id: u32,
    },
    /// SuperSS 0x010c block-friend result.
    BlockResult {
        status: u32,
        user_id: u32,
    },
    /// SuperSS 0x010d unblock-friend result.
    UnblockResult {
        status: u32,
        user_id: u32,
    },
    /// Reference-described 0x010f logout notification.
    Logout {
        user_id: u32,
    },
    /// SuperSS 0x0119 alias result.
    AliasResult {
        status: u32,
        user_id: u32,
        alias: Vec<u8>,
    },
    /// Reference-described 0x0113 friend/guild chat delivery.
    Chat {
        user_id: u32,
        nickname: Vec<u8>,
        message: Vec<u8>,
        guild: bool,
    },
}
impl ServerPacket {
    pub fn encode_payload(&self) -> Result<Vec<u8>, MessageError> {
        let mut o = Vec::new();
        match self {
            Self::CredentialResponse { user_id } => {
                o.push(0);
                o.extend(user_id.to_le_bytes());
            }
            Self::FriendList { page, entries } => {
                if entries.len() > FRIEND_PAGE_SIZE {
                    return Err(MessageError::Limit);
                }
                o.extend(0x102_u16.to_le_bytes());
                page.encode(&mut o, entries.len());
                for e in entries {
                    e.encode(&mut o);
                }
            }
            Self::LookupResponse {
                status,
                nickname,
                user_id,
            } => {
                o.extend(0x117_u16.to_le_bytes());
                o.extend(status.to_le_bytes());
                string(&mut o, nickname, MAX_NICKNAME_BYTES)?;
                // PacketDoc makes this a mandatory u32. Zero is the explicit
                // not-found value used by this service.
                o.extend(user_id.unwrap_or_default().to_le_bytes());
            }
            Self::FriendRequest { status, entry } => {
                o.extend(0x104_u16.to_le_bytes());
                // 0x104 starts with one u32 result, followed by the same PacketDoc entry.
                o.extend(status.to_le_bytes());
                entry.encode(&mut o);
            }
            Self::ConfirmResult { status, user_id } => {
                o.extend(0x109_u16.to_le_bytes());
                o.extend(status.to_le_bytes());
                o.extend(user_id.to_le_bytes());
            }
            Self::Presence {
                user_id,
                unknown_f,
                server_id,
                unknown_g,
                unknown_h,
            } => {
                o.extend(0x115_u16.to_le_bytes());
                o.extend(user_id.to_le_bytes());
                o.extend(unknown_f);
                o.extend(server_id.to_le_bytes());
                o.push(*unknown_g);
                fixed(&mut o, unknown_h, 64);
            }
            Self::DeleteResult { status, user_id } => {
                o.extend(0x10b_u16.to_le_bytes());
                o.extend(status.to_le_bytes());
                o.extend(user_id.to_le_bytes());
            }
            Self::BlockResult { status, user_id } => {
                o.extend(0x10c_u16.to_le_bytes());
                o.extend(status.to_le_bytes());
                o.extend(user_id.to_le_bytes());
            }
            Self::UnblockResult { status, user_id } => {
                o.extend(0x10d_u16.to_le_bytes());
                o.extend(status.to_le_bytes());
                o.extend(user_id.to_le_bytes());
            }
            Self::Logout { user_id } => {
                o.extend(0x10f_u16.to_le_bytes());
                o.extend(user_id.to_le_bytes());
            }
            Self::AliasResult {
                status,
                user_id,
                alias,
            } => {
                o.extend(0x119_u16.to_le_bytes());
                o.extend(status.to_le_bytes());
                o.extend(user_id.to_le_bytes());
                string(&mut o, alias, 10)?;
            }
            Self::Chat {
                user_id,
                nickname,
                message,
                guild,
            } => {
                o.extend(0x113_u16.to_le_bytes());
                o.extend(user_id.to_le_bytes());
                string(&mut o, nickname, MAX_NICKNAME_BYTES)?;
                string(&mut o, message, MAX_TEXT_BYTES)?;
                o.push(u8::from(*guild));
            }
        }
        Ok(o)
    }
    pub fn decode(opcode: u16, bytes: &[u8]) -> Result<Self, MessageError> {
        if opcode == 0x2f {
            let mut r = Reader::new(bytes);
            let _ = r.u8()?;
            let user_id = r.u32()?;
            r.end()?;
            return Ok(Self::CredentialResponse { user_id });
        }
        if opcode != 0x30 {
            return Err(MessageError::UnknownOpcode(opcode));
        }
        let mut r = Reader::new(bytes);
        let subtype = r.u16()?;
        let p = match subtype {
            0x102 => {
                let number = r.u8()?;
                let total = r.u16()?;
                let count = r.u32()? as usize;
                let page = Page {
                    number,
                    total,
                    current: u16::try_from(count).map_err(|_| MessageError::Limit)?,
                };
                if count > FRIEND_PAGE_SIZE || r.remaining() != count * FriendEntry::WIRE_LEN {
                    return Err(MessageError::Limit);
                }
                let entries = (0..count)
                    .map(|_| FriendEntry::decode(&mut r))
                    .collect::<Result<Vec<_>, _>>()?;
                Self::FriendList { page, entries }
            }
            0x104 => {
                let status = r.u32()?;
                let entry = FriendEntry::decode(&mut r)?;
                Self::FriendRequest { status, entry }
            }
            0x109 => Self::ConfirmResult {
                status: r.u32()?,
                user_id: r.u32()?,
            },
            0x115 => Self::Presence {
                user_id: r.u32()?,
                unknown_f: r.array()?,
                server_id: r.u32()?,
                unknown_g: r.u8()?,
                unknown_h: r.fixed(64)?,
            },
            0x10b => Self::DeleteResult {
                status: r.u32()?,
                user_id: r.u32()?,
            },
            0x10c => Self::BlockResult {
                status: r.u32()?,
                user_id: r.u32()?,
            },
            0x10d => Self::UnblockResult {
                status: r.u32()?,
                user_id: r.u32()?,
            },
            0x10f => Self::Logout { user_id: r.u32()? },
            0x113 => Self::Chat {
                user_id: r.u32()?,
                nickname: r.string(MAX_NICKNAME_BYTES)?,
                message: r.string(MAX_TEXT_BYTES)?,
                guild: r.u8()? != 0,
            },
            0x119 => Self::AliasResult {
                status: r.u32()?,
                user_id: r.u32()?,
                alias: r.string(10)?,
            },
            0x117 => {
                let status = r.u32()?;
                let nickname = r.string(MAX_NICKNAME_BYTES)?;
                let id = r.u32()?;
                Self::LookupResponse {
                    status,
                    nickname,
                    user_id: (id != 0).then_some(id),
                }
            }
            _ => return Err(MessageError::UnknownSubtype(subtype)),
        };
        r.end()?;
        Ok(p)
    }
}
impl FriendEntry {
    fn decode(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let nickname = r.fixed(22)?;
        let alias = r.fixed(25)?;
        let user_id = r.u32()?;
        // The 104-byte region is intentionally not decoded: PacketDoc assigns no semantics to
        // any of it, so do not create a hybrid PacketDoc/SuperSS interpretation.
        r.take(104)?;
        Ok(Self {
            nickname,
            alias,
            user_id,
            channel: ChannelInfo::offline(),
            state: Presence::Offline,
            relationship: Relationship::Friend,
            blocked: false,
        })
    }
}
#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum MessageError {
    #[error("message packet is truncated")]
    Truncated,
    #[error("message packet has trailing bytes")]
    Trailing,
    #[error("invalid client status {0}")]
    InvalidPresence(u8),
    #[error("message packet exceeds a bounded field")]
    Limit,
    #[error("unknown MessageService opcode {0:#x}")]
    UnknownOpcode(u16),
    #[error("unknown MessageService subtype {0:#x}")]
    UnknownSubtype(u16),
    #[error("session is not authenticated")]
    Unauthorized,
    #[error("social operation rejected")]
    Rejected,
}
impl TryFrom<u8> for Presence {
    type Error = MessageError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        // PacketDoc 0x001d establishes only playing, idle, and online client declarations.
        match v {
            0 => Ok(Self::Playing),
            1 => Ok(Self::Idle),
            4 => Ok(Self::Online),
            x => Err(MessageError::InvalidPresence(x)),
        }
    }
}

impl Presence {
    fn from_stored(v: i16) -> Result<Self, MessageError> {
        match v {
            0 => Ok(Self::Playing),
            1 => Ok(Self::Idle),
            3 => Ok(Self::Busy),
            4 => Ok(Self::Online),
            5 => Ok(Self::Offline),
            _ => Err(MessageError::Rejected),
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, p: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], MessageError> {
        let e = self.p.checked_add(n).ok_or(MessageError::Limit)?;
        let x = self.b.get(self.p..e).ok_or(MessageError::Truncated)?;
        self.p = e;
        Ok(x)
    }
    fn u8(&mut self) -> Result<u8, MessageError> {
        Ok(*self.take(1)?.first().ok_or(MessageError::Truncated)?)
    }
    fn u16(&mut self) -> Result<u16, MessageError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| MessageError::Truncated)?,
        ))
    }
    fn u32(&mut self) -> Result<u32, MessageError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| MessageError::Truncated)?,
        ))
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], MessageError> {
        self.take(N)?
            .try_into()
            .map_err(|_| MessageError::Truncated)
    }
    fn fixed(&mut self, n: usize) -> Result<Vec<u8>, MessageError> {
        let x = self.take(n)?;
        Ok(x.split(|b| *b == 0).next().unwrap_or_default().to_vec())
    }
    fn string(&mut self, max: usize) -> Result<Vec<u8>, MessageError> {
        let n = usize::from(self.u16()?);
        if n > max {
            return Err(MessageError::Limit);
        };
        Ok(self.take(n)?.to_vec())
    }
    fn remaining(&self) -> usize {
        self.b.len() - self.p
    }
    fn end(&self) -> Result<(), MessageError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(MessageError::Trailing)
        }
    }
}
fn string(out: &mut Vec<u8>, value: &[u8], max: usize) -> Result<(), MessageError> {
    if value.len() > max || value.contains(&0) {
        return Err(MessageError::Limit);
    };
    out.extend((value.len() as u16).to_le_bytes());
    out.extend(value);
    Ok(())
}
fn fixed(out: &mut Vec<u8>, value: &[u8], width: usize) {
    let n = value.len().min(width);
    out.extend(&value[..n]);
    out.resize(out.len() + width - n, 0);
}

/// Builds the only reference-established status notification (0x0115). PacketDoc marks all
/// fields other than the user and server IDs opaque; never place our local presence enum into
/// those bytes.
fn status_packet(user_id: u32, status: Presence, channel: &ChannelInfo) -> ServerPacket {
    let mut unknown_f = [0; 11];
    // SuperSS writes state:u32, result:u8, room.number:i16, room.type:i32 before the
    // separately named server/channel/name fields. Preserve that established byte order while
    // keeping the packet decoder's opaque representation.
    unknown_f[..4].copy_from_slice(&(status as u32).to_le_bytes());
    unknown_f[4] = 1;
    unknown_f[5..7].copy_from_slice(&channel.room_number.to_le_bytes());
    unknown_f[7..11].copy_from_slice(&channel.room_type.to_le_bytes());
    ServerPacket::Presence {
        user_id,
        unknown_f,
        server_id: channel.server_id,
        unknown_g: channel.channel_id as u8,
        unknown_h: channel.channel_name.clone(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct User {
    pub id: u32,
    pub nickname: Vec<u8>,
    pub guild_id: Option<u32>,
    pub guild_name: Vec<u8>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfflineMessage {
    pub sender_id: u32,
    pub recipient_id: u32,
    pub nickname: Vec<u8>,
    pub body: Vec<u8>,
    delivery_id: i64,
    lease_token: [u8; 16],
}
#[derive(Default, Clone)]
pub struct MemoryStore(Arc<Mutex<StoreData>>);
#[derive(Default)]
struct StoreData {
    users: HashMap<u32, User>,
    friends: HashMap<(u32, u32), FriendState>,
    messages: HashMap<u32, VecDeque<OfflineMessage>>,
    live_messages: HashMap<u32, VecDeque<OfflineMessage>>,
    inflight_messages: HashMap<u32, VecDeque<OfflineMessage>>,
    online: HashMap<u32, (Presence, ChannelInfo)>,
    presence_expiry: HashMap<u32, Instant>,
    presence_events: HashMap<u32, VecDeque<(u32, Presence, ChannelInfo)>>,
}
#[async_trait::async_trait]
pub trait MessageStore: Send + Sync {
    async fn authenticate(
        &self,
        user_id: u32,
        nickname: &[u8],
        peer_ip: IpAddr,
    ) -> Result<bool, MessageError>;
    async fn friends(&self, id: u32) -> Result<Vec<FriendEntry>, MessageError>;
    async fn lookup(&self, nickname: &[u8]) -> Result<Option<u32>, MessageError>;
    async fn add_friend(&self, a: u32, b: u32) -> Result<(), MessageError>;
    async fn confirm_friend(&self, a: u32, b: u32) -> Result<(), MessageError>;
    async fn has_pending_friend_request(
        &self,
        owner: u32,
        friend: u32,
    ) -> Result<bool, MessageError>;
    async fn block_friend(&self, a: u32, b: u32) -> Result<(), MessageError>;
    async fn unblock_friend(&self, a: u32, b: u32) -> Result<(), MessageError>;
    async fn delete_friend(&self, a: u32, b: u32) -> Result<(), MessageError>;
    async fn set_online(
        &self,
        id: u32,
        status: Presence,
        channel: ChannelInfo,
    ) -> Result<(), MessageError>;
    async fn set_offline(&self, id: u32) -> Result<(), MessageError>;
    async fn heartbeat(&self, id: u32) -> Result<(), MessageError>;
    async fn take_presence_events(
        &self,
        id: u32,
    ) -> Result<Vec<(u32, Presence, ChannelInfo)>, MessageError>;
    async fn queue_message(
        &self,
        sender: u32,
        recipient: u32,
        body: Vec<u8>,
    ) -> Result<(), MessageError>;
    async fn queue_guild_message(
        &self,
        sender: u32,
        recipient: u32,
        body: Vec<u8>,
    ) -> Result<(), MessageError>;
    async fn take_live_messages(&self, id: u32) -> Result<Vec<OfflineMessage>, MessageError>;
    async fn take_messages(&self, id: u32) -> Result<Vec<OfflineMessage>, MessageError>;
    async fn ack_messages(&self, messages: &[OfflineMessage]) -> Result<(), MessageError>;
    async fn set_alias(&self, owner: u32, friend: u32, alias: Vec<u8>) -> Result<(), MessageError>;
    async fn guild_members(&self, id: u32) -> Result<Vec<u32>, MessageError>;
}

#[async_trait::async_trait]
impl MessageStore for MemoryStore {
    async fn authenticate(
        &self,
        user_id: u32,
        nickname: &[u8],
        _peer_ip: IpAddr,
    ) -> Result<bool, MessageError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| MessageError::Rejected)?
            .users
            .get(&user_id)
            .is_some_and(|u| u.nickname == nickname))
    }
    async fn friends(&self, id: u32) -> Result<Vec<FriendEntry>, MessageError> {
        Ok(MemoryStore::friends(self, id))
    }
    async fn lookup(&self, nickname: &[u8]) -> Result<Option<u32>, MessageError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| MessageError::Rejected)?
            .users
            .values()
            .find(|u| u.nickname.eq_ignore_ascii_case(nickname))
            .map(|u| u.id))
    }
    async fn add_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        MemoryStore::add_friend(self, a, b)
    }
    async fn confirm_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        MemoryStore::confirm_friend(self, a, b)
    }
    async fn has_pending_friend_request(
        &self,
        owner: u32,
        friend: u32,
    ) -> Result<bool, MessageError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| MessageError::Rejected)?
            .friends
            .get(&(owner, friend))
            .is_some_and(|f| f.pending && f.requested_by == Some(friend)))
    }
    async fn block_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        MemoryStore::block_friend(self, a, b)
    }
    async fn unblock_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        MemoryStore::unblock_friend(self, a, b)
    }
    async fn delete_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        MemoryStore::delete_friend(self, a, b)
    }
    async fn set_online(
        &self,
        id: u32,
        status: Presence,
        channel: ChannelInfo,
    ) -> Result<(), MessageError> {
        MemoryStore::set_online(self, id, status, channel);
        Ok(())
    }
    async fn set_offline(&self, id: u32) -> Result<(), MessageError> {
        MemoryStore::set_offline(self, id);
        Ok(())
    }
    async fn heartbeat(&self, id: u32) -> Result<(), MessageError> {
        MemoryStore::heartbeat(self, id)
    }
    async fn take_presence_events(
        &self,
        id: u32,
    ) -> Result<Vec<(u32, Presence, ChannelInfo)>, MessageError> {
        Ok(MemoryStore::take_presence_events(self, id))
    }
    async fn queue_message(
        &self,
        sender: u32,
        recipient: u32,
        body: Vec<u8>,
    ) -> Result<(), MessageError> {
        MemoryStore::queue_message(self, sender, recipient, body)
    }
    async fn queue_guild_message(
        &self,
        sender: u32,
        recipient: u32,
        body: Vec<u8>,
    ) -> Result<(), MessageError> {
        MemoryStore::queue_guild_message(self, sender, recipient, body)
    }
    async fn take_live_messages(&self, id: u32) -> Result<Vec<OfflineMessage>, MessageError> {
        MemoryStore::take_live_messages(self, id)
    }
    async fn take_messages(&self, id: u32) -> Result<Vec<OfflineMessage>, MessageError> {
        MemoryStore::take_messages(self, id)
    }
    async fn ack_messages(&self, messages: &[OfflineMessage]) -> Result<(), MessageError> {
        MemoryStore::ack_messages(self, messages)
    }
    async fn set_alias(&self, owner: u32, friend: u32, alias: Vec<u8>) -> Result<(), MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let f = d
            .friends
            .get_mut(&(owner, friend))
            .ok_or(MessageError::Rejected)?;
        f.alias = alias;
        Ok(())
    }
    async fn guild_members(&self, id: u32) -> Result<Vec<u32>, MessageError> {
        let d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let guild = d.users.get(&id).and_then(|u| u.guild_id);
        Ok(guild.map_or_else(Vec::new, |guild| {
            d.users
                .values()
                .filter(|u| u.guild_id == Some(guild))
                .map(|u| u.id)
                .collect()
        }))
    }
}

/// PostgreSQL-backed MessageService store. All social state and queued messages survive process restart.
#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}
impl PostgresStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    fn id(id: u32) -> i64 {
        i64::from(id)
    }
    fn presence(value: i16) -> Result<Presence, MessageError> {
        Presence::from_stored(value)
    }
}
#[async_trait::async_trait]
impl MessageStore for PostgresStore {
    async fn authenticate(
        &self,
        user_id: u32,
        nickname: &[u8],
        peer_ip: IpAddr,
    ) -> Result<bool, MessageError> {
        let nickname = String::from_utf8(nickname.to_vec()).map_err(|_| MessageError::Rejected)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| MessageError::Rejected)?;
        let row = sqlx::query("SELECT e.expires_at, a.status FROM message_login_eligibility e JOIN accounts a ON a.id = e.account_id JOIN profiles p ON p.account_id = e.account_id AND p.nickname_display = e.nickname WHERE e.account_id = $1 AND e.nickname = $2 AND e.peer_ip = $3::inet FOR UPDATE")
            .bind(Self::id(user_id))
            .bind(&nickname)
            .bind(peer_ip.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| MessageError::Rejected)?;
        let Some(row) = row else {
            return Ok(false);
        };
        let expires_at: chrono::DateTime<chrono::Utc> = row
            .try_get("expires_at")
            .map_err(|_| MessageError::Rejected)?;
        let status: String = row.try_get("status").map_err(|_| MessageError::Rejected)?;
        if status != "active" || chrono::Utc::now() >= expires_at {
            sqlx::query("DELETE FROM message_login_eligibility WHERE account_id = $1 AND nickname = $2 AND peer_ip = $3::inet")
                .bind(Self::id(user_id)).bind(&nickname).bind(peer_ip.to_string())
                .execute(&mut *tx).await.map_err(|_| MessageError::Rejected)?;
            tx.commit().await.map_err(|_| MessageError::Rejected)?;
            return Ok(false);
        }
        sqlx::query("DELETE FROM message_login_eligibility WHERE account_id = $1 AND nickname = $2 AND peer_ip = $3::inet")
            .bind(Self::id(user_id)).bind(&nickname).bind(peer_ip.to_string())
            .execute(&mut *tx).await.map_err(|_| MessageError::Rejected)?;
        tx.commit().await.map_err(|_| MessageError::Rejected)?;
        Ok(true)
    }
    async fn friends(&self, id: u32) -> Result<Vec<FriendEntry>, MessageError> {
        let rows = sqlx::query("SELECT f.friend_account_id, f.alias, f.blocked, p.nickname_display, pr.status, pr.room_number, pr.room_type, pr.server_id, pr.channel_id, pr.channel_name FROM message_friends f JOIN profiles p ON p.account_id=f.friend_account_id LEFT JOIN message_presence pr ON pr.account_id=f.friend_account_id AND pr.expires_at > now() WHERE f.owner_account_id=$1 ORDER BY f.friend_account_id LIMIT $2")
            .bind(Self::id(id))
            .bind(i64::try_from(MAX_FRIEND_ROWS).map_err(|_| MessageError::Limit)?)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| MessageError::Rejected)?;
        rows.into_iter()
            .map(|row| {
                let status = row
                    .try_get::<Option<i16>, _>("status")
                    .map_err(|_| MessageError::Rejected)?
                    .map_or(Ok(Presence::Offline), Self::presence)?;
                let channel = row
                    .try_get::<Option<i16>, _>("room_number")
                    .map_err(|_| MessageError::Rejected)?
                    .map(|room_number| ChannelInfo {
                        room_number,
                        room_type: row.try_get("room_type").unwrap_or_default(),
                        server_id: row
                            .try_get::<i64, _>("server_id")
                            .unwrap_or(i64::from(u32::MAX))
                            as u32,
                        channel_id: row.try_get::<i16, _>("channel_id").unwrap_or(-1) as i8,
                        channel_name: row
                            .try_get::<Vec<u8>, _>("channel_name")
                            .unwrap_or_default(),
                    })
                    .unwrap_or_else(ChannelInfo::offline);
                Ok(FriendEntry {
                    nickname: row
                        .try_get::<String, _>("nickname_display")
                        .map_err(|_| MessageError::Rejected)?
                        .into_bytes(),
                    alias: row
                        .try_get::<String, _>("alias")
                        .map_err(|_| MessageError::Rejected)?
                        .into_bytes(),
                    user_id: row
                        .try_get::<i64, _>("friend_account_id")
                        .map_err(|_| MessageError::Rejected)? as u32,
                    channel,
                    state: status,
                    relationship: Relationship::Friend,
                    blocked: row.try_get("blocked").unwrap_or(false),
                })
            })
            .collect()
    }
    async fn lookup(&self, nickname: &[u8]) -> Result<Option<u32>, MessageError> {
        let nickname = String::from_utf8(nickname.to_vec()).map_err(|_| MessageError::Rejected)?;
        Ok(
            sqlx::query("SELECT account_id FROM profiles WHERE nickname_normalized=lower($1)")
                .bind(nickname)
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| MessageError::Rejected)?
                .map(|r| r.try_get::<i64, _>("account_id").unwrap_or_default() as u32),
        )
    }
    async fn add_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| MessageError::Rejected)?;
        sqlx::query("INSERT INTO message_friends(owner_account_id,friend_account_id,pending,requested_by_account_id) VALUES($1,$2,true,$3) ON CONFLICT DO NOTHING")
            .bind(Self::id(a)).bind(Self::id(b)).bind(Self::id(a)).execute(&mut *tx).await.map_err(|_| MessageError::Rejected)?;
        sqlx::query("INSERT INTO message_friends(owner_account_id,friend_account_id,pending,requested_by_account_id) VALUES($1,$2,true,$3) ON CONFLICT DO NOTHING")
            .bind(Self::id(b)).bind(Self::id(a)).bind(Self::id(a)).execute(&mut *tx).await.map_err(|_| MessageError::Rejected)?;
        tx.commit().await.map_err(|_| MessageError::Rejected)
    }
    async fn confirm_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| MessageError::Rejected)?;
        let first = sqlx::query("UPDATE message_friends SET pending=false, requested_by_account_id=NULL WHERE owner_account_id=$1 AND friend_account_id=$2 AND pending AND requested_by_account_id=$2")
            .bind(Self::id(a)).bind(Self::id(b)).execute(&mut *tx).await.map_err(|_| MessageError::Rejected)?;
        let second = sqlx::query("UPDATE message_friends SET pending=false, requested_by_account_id=NULL WHERE owner_account_id=$1 AND friend_account_id=$2 AND pending AND requested_by_account_id=$3")
            .bind(Self::id(b)).bind(Self::id(a)).bind(Self::id(b)).execute(&mut *tx).await.map_err(|_| MessageError::Rejected)?;
        if first.rows_affected() != 1 || second.rows_affected() != 1 {
            return Err(MessageError::Rejected);
        }
        tx.commit().await.map_err(|_| MessageError::Rejected)
    }
    async fn has_pending_friend_request(
        &self,
        owner: u32,
        friend: u32,
    ) -> Result<bool, MessageError> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM message_friends WHERE owner_account_id=$1 AND friend_account_id=$2 AND pending AND requested_by_account_id=$2)")
            .bind(Self::id(owner)).bind(Self::id(friend)).fetch_one(&self.pool).await.map_err(|_| MessageError::Rejected)
    }
    async fn block_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        let result = sqlx::query("UPDATE message_friends SET blocked=true WHERE owner_account_id=$1 AND friend_account_id=$2").bind(Self::id(a)).bind(Self::id(b)).execute(&self.pool).await.map_err(|_| MessageError::Rejected)?;
        (result.rows_affected() != 0)
            .then_some(())
            .ok_or(MessageError::Rejected)
    }
    async fn unblock_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        let result = sqlx::query("UPDATE message_friends SET blocked=false WHERE owner_account_id=$1 AND friend_account_id=$2").bind(Self::id(a)).bind(Self::id(b)).execute(&self.pool).await.map_err(|_| MessageError::Rejected)?;
        (result.rows_affected() != 0)
            .then_some(())
            .ok_or(MessageError::Rejected)
    }
    async fn delete_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        sqlx::query("DELETE FROM message_friends WHERE (owner_account_id=$1 AND friend_account_id=$2) OR (owner_account_id=$2 AND friend_account_id=$1)").bind(Self::id(a)).bind(Self::id(b)).execute(&self.pool).await.map_err(|_| MessageError::Rejected)?;
        Ok(())
    }
    async fn set_online(
        &self,
        id: u32,
        status: Presence,
        channel: ChannelInfo,
    ) -> Result<(), MessageError> {
        let channel_name = channel.channel_name.clone();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| MessageError::Rejected)?;
        sqlx::query("INSERT INTO message_presence(account_id,status,room_number,room_type,server_id,channel_id,channel_name) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(account_id) DO UPDATE SET status=excluded.status,room_number=excluded.room_number,room_type=excluded.room_type,server_id=excluded.server_id,channel_id=excluded.channel_id,channel_name=excluded.channel_name").bind(Self::id(id)).bind(status as i16).bind(channel.room_number).bind(channel.room_type).bind(i64::from(channel.server_id)).bind(i16::from(channel.channel_id)).bind(&channel_name).execute(&mut *tx).await.map_err(|_| MessageError::Rejected)?;
        sqlx::query("INSERT INTO message_presence_events(recipient_account_id,sender_account_id,status,room_number,room_type,server_id,channel_id,channel_name) SELECT f.owner_account_id,$1,$2,$3,$4,$5,$6,$7 FROM message_friends f WHERE f.friend_account_id=$1 AND f.pending=false AND NOT f.blocked ORDER BY f.owner_account_id LIMIT $8").bind(Self::id(id)).bind(status as i16).bind(channel.room_number).bind(channel.room_type).bind(i64::from(channel.server_id)).bind(i16::from(channel.channel_id)).bind(channel.channel_name).bind(i64::try_from(MAX_DELIVERY_BATCH).map_err(|_| MessageError::Limit)?).execute(&mut *tx).await.map_err(|_| MessageError::Rejected)?;
        tx.commit().await.map_err(|_| MessageError::Rejected)
    }
    async fn set_offline(&self, id: u32) -> Result<(), MessageError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| MessageError::Rejected)?;
        sqlx::query("DELETE FROM message_presence WHERE account_id=$1")
            .bind(Self::id(id))
            .execute(&mut *tx)
            .await
            .map_err(|_| MessageError::Rejected)?;
        sqlx::query("INSERT INTO message_presence_events(recipient_account_id,sender_account_id,status,room_number,room_type,server_id,channel_id,channel_name) SELECT f.owner_account_id,$1,5,-1,-1,-1,-1,''::bytea FROM message_friends f WHERE f.friend_account_id=$1 AND f.pending=false AND NOT f.blocked ORDER BY f.owner_account_id LIMIT $2").bind(Self::id(id)).bind(i64::try_from(MAX_DELIVERY_BATCH).map_err(|_| MessageError::Limit)?).execute(&mut *tx).await.map_err(|_| MessageError::Rejected)?;
        tx.commit().await.map_err(|_| MessageError::Rejected)
    }
    async fn heartbeat(&self, id: u32) -> Result<(), MessageError> {
        sqlx::query("UPDATE message_presence SET expires_at=now() + interval '90 seconds' WHERE account_id=$1")
            .bind(Self::id(id)).execute(&self.pool).await.map_err(|_| MessageError::Rejected)?;
        Ok(())
    }
    async fn take_presence_events(
        &self,
        id: u32,
    ) -> Result<Vec<(u32, Presence, ChannelInfo)>, MessageError> {
        sqlx::query("DELETE FROM message_presence_events WHERE id IN (SELECT e.id FROM message_presence_events e WHERE e.recipient_account_id=$1 AND e.status <> 5 AND NOT EXISTS (SELECT 1 FROM message_presence p WHERE p.account_id=e.sender_account_id AND p.expires_at > now()) ORDER BY e.id LIMIT $2)")
            .bind(Self::id(id))
            .bind(i64::try_from(MAX_DELIVERY_BATCH).map_err(|_| MessageError::Limit)?)
            .execute(&self.pool)
            .await
            .map_err(|_| MessageError::Rejected)?;
        let rows=sqlx::query("DELETE FROM message_presence_events WHERE id IN (SELECT id FROM message_presence_events WHERE recipient_account_id=$1 ORDER BY id LIMIT $2) RETURNING sender_account_id,status,room_number,room_type,server_id,channel_id,channel_name")
            .bind(Self::id(id))
            .bind(i64::try_from(MAX_DELIVERY_BATCH).map_err(|_| MessageError::Limit)?)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| MessageError::Rejected)?;
        rows.into_iter()
            .map(|r| {
                Ok((
                    r.try_get::<i64, _>("sender_account_id")
                        .map_err(|_| MessageError::Rejected)? as u32,
                    Self::presence(r.try_get("status").map_err(|_| MessageError::Rejected)?)?,
                    ChannelInfo {
                        room_number: r.try_get("room_number").unwrap_or(-1),
                        room_type: r.try_get("room_type").unwrap_or(-1),
                        server_id: r.try_get::<i64, _>("server_id").unwrap_or(-1) as u32,
                        channel_id: r.try_get::<i16, _>("channel_id").unwrap_or(-1) as i8,
                        channel_name: r.try_get("channel_name").unwrap_or_default(),
                    },
                ))
            })
            .collect()
    }
    async fn queue_message(
        &self,
        sender: u32,
        recipient: u32,
        body: Vec<u8>,
    ) -> Result<(), MessageError> {
        if body.len() > MAX_TEXT_BYTES {
            return Err(MessageError::Limit);
        }
        let allowed=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM message_friends a JOIN message_friends b ON b.owner_account_id=$2 AND b.friend_account_id=$1 WHERE a.owner_account_id=$1 AND a.friend_account_id=$2 AND NOT a.blocked AND NOT b.blocked AND NOT a.pending AND NOT b.pending)").bind(Self::id(sender)).bind(Self::id(recipient)).fetch_one(&self.pool).await.map_err(|_| MessageError::Rejected)?;
        if !allowed {
            return Err(MessageError::Rejected);
        }
        let inserted = sqlx::query("INSERT INTO message_offline_messages(sender_account_id,recipient_account_id,body) SELECT $1,$2,$3 WHERE (SELECT count(*) FROM message_offline_messages WHERE recipient_account_id=$2 AND delivered_at IS NULL) < $4")
            .bind(Self::id(sender))
            .bind(Self::id(recipient))
            .bind(body)
            .bind(i64::try_from(MAX_QUEUED_MESSAGES).map_err(|_| MessageError::Limit)?)
            .execute(&self.pool)
            .await
            .map_err(|_| MessageError::Rejected)?;
        if inserted.rows_affected() != 1 {
            return Err(MessageError::Limit);
        }
        Ok(())
    }
    async fn queue_guild_message(
        &self,
        _sender: u32,
        _recipient: u32,
        _body: Vec<u8>,
    ) -> Result<(), MessageError> {
        // No authoritative guild-membership table exists in the current schema.
        Ok(())
    }
    async fn take_live_messages(&self, id: u32) -> Result<Vec<OfflineMessage>, MessageError> {
        self.take_messages(id).await
    }
    async fn take_messages(&self, id: u32) -> Result<Vec<OfflineMessage>, MessageError> {
        let token = rand::random::<[u8; 16]>();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| MessageError::Rejected)?;
        let rows=sqlx::query("SELECT m.id,m.sender_account_id,m.recipient_account_id,m.body,p.nickname_display FROM message_offline_messages m JOIN profiles p ON p.account_id=m.sender_account_id WHERE m.recipient_account_id=$1 AND m.delivered_at IS NULL AND (m.delivery_lease_until IS NULL OR m.delivery_lease_until <= now()) ORDER BY m.id LIMIT $2 FOR UPDATE OF m SKIP LOCKED")
            .bind(Self::id(id))
            .bind(i64::try_from(MAX_DELIVERY_BATCH).map_err(|_| MessageError::Limit)?)
            .fetch_all(&mut *tx).await.map_err(|_| MessageError::Rejected)?;
        for row in &rows {
            let message_id: i64 = row.try_get("id").map_err(|_| MessageError::Rejected)?;
            sqlx::query("UPDATE message_offline_messages SET delivery_lease_until=now() + interval '30 seconds', delivery_lease_token=$2 WHERE id=$1")
                .bind(message_id).bind(token.as_slice()).execute(&mut *tx).await.map_err(|_| MessageError::Rejected)?;
        }
        tx.commit().await.map_err(|_| MessageError::Rejected)?;
        rows.into_iter()
            .map(|r| {
                Ok(OfflineMessage {
                    sender_id: r
                        .try_get::<i64, _>("sender_account_id")
                        .map_err(|_| MessageError::Rejected)? as u32,
                    recipient_id: r
                        .try_get::<i64, _>("recipient_account_id")
                        .map_err(|_| MessageError::Rejected)?
                        as u32,
                    body: r.try_get("body").map_err(|_| MessageError::Rejected)?,
                    delivery_id: r.try_get("id").map_err(|_| MessageError::Rejected)?,
                    lease_token: token,
                    nickname: r
                        .try_get::<String, _>("nickname_display")
                        .map_err(|_| MessageError::Rejected)?
                        .into_bytes(),
                })
            })
            .collect()
    }
    async fn ack_messages(&self, messages: &[OfflineMessage]) -> Result<(), MessageError> {
        for message in messages {
            sqlx::query("UPDATE message_offline_messages SET delivered_at=now(), delivery_lease_until=NULL, delivery_lease_token=NULL WHERE id=$1 AND delivery_lease_token=$2 AND delivered_at IS NULL")
                .bind(message.delivery_id).bind(message.lease_token.as_slice()).execute(&self.pool).await.map_err(|_| MessageError::Rejected)?;
        }
        Ok(())
    }
    async fn set_alias(&self, owner: u32, friend: u32, alias: Vec<u8>) -> Result<(), MessageError> {
        let alias = String::from_utf8(alias).map_err(|_| MessageError::Rejected)?;
        let result = sqlx::query("UPDATE message_friends SET alias=$3 WHERE owner_account_id=$1 AND friend_account_id=$2").bind(Self::id(owner)).bind(Self::id(friend)).bind(alias).execute(&self.pool).await.map_err(|_| MessageError::Rejected)?;
        (result.rows_affected() != 0)
            .then_some(())
            .ok_or(MessageError::Rejected)
    }
    async fn guild_members(&self, _id: u32) -> Result<Vec<u32>, MessageError> {
        // Guild membership is not part of the account foundation schema yet; avoid a guessed
        // query against an absent column. Guild packets remain a safe no-op until its durable
        // schema is introduced.
        Ok(Vec::new())
    }
}

#[derive(Default)]
struct SessionRegistry {
    active: Mutex<HashMap<u32, u64>>,
    next: AtomicU64,
}
impl SessionRegistry {
    fn claim(&self, user_id: u32) -> Result<u64, MessageError> {
        let mut active = self.active.lock().map_err(|_| MessageError::Rejected)?;
        let lease = self.next.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        // Replacement is intentional: the old connection becomes fenced immediately and its
        // next packet/poll is rejected, allowing the runtime to disconnect it without a race.
        active.insert(user_id, lease);
        Ok(lease)
    }
    fn is_current(&self, user_id: u32, lease: u64) -> Result<bool, MessageError> {
        Ok(self
            .active
            .lock()
            .map_err(|_| MessageError::Rejected)?
            .get(&user_id)
            .copied()
            == Some(lease))
    }
    fn release(&self, user_id: u32, lease: u64) -> Result<bool, MessageError> {
        let mut active = self.active.lock().map_err(|_| MessageError::Rejected)?;
        if active.get(&user_id).copied() == Some(lease) {
            active.remove(&user_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[derive(Clone, Debug)]
struct FriendState {
    alias: Vec<u8>,
    blocked: bool,
    pending: bool,
    requested_by: Option<u32>,
}
impl MemoryStore {
    pub fn register_user(&self, user: User) {
        if let Ok(mut d) = self.0.lock() {
            d.users.insert(user.id, user);
        }
    }
    pub fn friends(&self, id: u32) -> Vec<FriendEntry> {
        self.0.lock().map_or_else(
            |_| Vec::new(),
            |mut d| {
                let now = Instant::now();
                let expired: Vec<u32> = d
                    .presence_expiry
                    .iter()
                    .filter_map(|(&user, &until)| (until <= now).then_some(user))
                    .collect();
                for user in expired {
                    d.online.remove(&user);
                    d.presence_expiry.remove(&user);
                }
                d.friends
                    .iter()
                    .filter_map(|(&(owner, target), f)| {
                        if owner != id {
                            return None;
                        }
                        let u = d.users.get(&target)?;
                        Some(FriendEntry {
                            nickname: u.nickname.clone(),
                            alias: f.alias.clone(),
                            user_id: target,
                            channel: d
                                .online
                                .get(&target)
                                .map_or_else(ChannelInfo::offline, |(_, c)| c.clone()),
                            state: d.online.get(&target).map_or(Presence::Offline, |(s, _)| *s),
                            relationship: Relationship::Friend,
                            blocked: f.blocked,
                        })
                    })
                    .take(MAX_FRIEND_ROWS)
                    .collect()
            },
        )
    }
    pub fn add_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        if !d.users.contains_key(&a) || !d.users.contains_key(&b) || a == b {
            return Err(MessageError::Rejected);
        };
        let reciprocal_was_pending = d
            .friends
            .get(&(b, a))
            .is_some_and(|f| f.pending && f.requested_by == Some(b));
        d.friends.entry((a, b)).or_insert(FriendState {
            alias: b"Friend".to_vec(),
            blocked: false,
            pending: true,
            requested_by: Some(a),
        });
        d.friends.entry((b, a)).or_insert(FriendState {
            alias: b"Friend".to_vec(),
            blocked: false,
            pending: true,
            requested_by: Some(a),
        });
        if reciprocal_was_pending {
            for key in [(a, b), (b, a)] {
                if let Some(f) = d.friends.get_mut(&key) {
                    f.pending = false;
                    f.requested_by = None;
                }
            }
        }
        Ok(())
    }
    pub fn confirm_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let Some(incoming) = d.friends.get(&(a, b)) else {
            return Err(MessageError::Rejected);
        };
        if !incoming.pending || incoming.requested_by != Some(b) {
            return Err(MessageError::Rejected);
        }
        let Some(reverse) = d.friends.get(&(b, a)) else {
            return Err(MessageError::Rejected);
        };
        if !reverse.pending || reverse.requested_by != Some(b) {
            return Err(MessageError::Rejected);
        }
        for key in [(a, b), (b, a)] {
            if let Some(f) = d.friends.get_mut(&key) {
                f.pending = false;
                f.requested_by = None;
            }
        }
        Ok(())
    }
    pub fn block_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let f = d.friends.get_mut(&(a, b)).ok_or(MessageError::Rejected)?;
        f.blocked = true;
        Ok(())
    }
    pub fn unblock_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let f = d.friends.get_mut(&(a, b)).ok_or(MessageError::Rejected)?;
        f.blocked = false;
        Ok(())
    }
    pub fn delete_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        d.friends.remove(&(a, b));
        d.friends.remove(&(b, a));
        Ok(())
    }
    pub fn set_online(&self, id: u32, status: Presence, channel: ChannelInfo) {
        if let Ok(mut d) = self.0.lock() {
            d.online.insert(id, (status, channel.clone()));
            d.presence_expiry
                .insert(id, Instant::now() + Duration::from_secs(90));
            let recipients: Vec<u32> = d
                .friends
                .iter()
                .filter_map(|(&(owner, friend), state)| {
                    (friend == id && owner != id && !state.blocked && !state.pending)
                        .then_some(owner)
                })
                .collect();
            for recipient in recipients {
                let events = d.presence_events.entry(recipient).or_default();
                if events.len() >= MAX_DELIVERY_BATCH {
                    events.pop_front();
                }
                events.push_back((id, status, channel.clone()));
            }
        }
    }
    pub fn heartbeat(&self, id: u32) -> Result<(), MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        if d.online.contains_key(&id) {
            d.presence_expiry
                .insert(id, Instant::now() + Duration::from_secs(90));
            Ok(())
        } else {
            Err(MessageError::Rejected)
        }
    }
    pub fn set_offline(&self, id: u32) {
        if let Ok(mut d) = self.0.lock() {
            if let Some(messages) = d.live_messages.remove(&id) {
                d.messages.entry(id).or_default().extend(messages);
            }
            if let Some(messages) = d.inflight_messages.remove(&id) {
                d.messages.entry(id).or_default().extend(messages);
            }
            d.online.remove(&id);
            d.presence_expiry.remove(&id);
            let recipients: Vec<u32> = d
                .friends
                .iter()
                .filter_map(|(&(owner, friend), state)| {
                    (friend == id && owner != id && !state.blocked && !state.pending)
                        .then_some(owner)
                })
                .collect();
            for recipient in recipients {
                let events = d.presence_events.entry(recipient).or_default();
                if events.len() >= MAX_DELIVERY_BATCH {
                    events.pop_front();
                }
                events.push_back((id, Presence::Offline, ChannelInfo::offline()));
            }
        }
    }
    pub fn take_presence_events(&self, id: u32) -> Vec<(u32, Presence, ChannelInfo)> {
        self.0.lock().map_or_else(
            |_| Vec::new(),
            |mut d| {
                let now = Instant::now();
                d.presence_events
                    .remove(&id)
                    .map_or_else(Vec::new, |events| {
                        events
                            .into_iter()
                            .filter(|(sender, status, _)| {
                                *status == Presence::Offline
                                    || d.presence_expiry
                                        .get(sender)
                                        .is_some_and(|until| *until > now)
                            })
                            .take(MAX_DELIVERY_BATCH)
                            .collect()
                    })
            },
        )
    }
    pub fn queue_message(
        &self,
        sender: u32,
        recipient: u32,
        body: Vec<u8>,
    ) -> Result<(), MessageError> {
        if body.len() > MAX_TEXT_BYTES {
            return Err(MessageError::Limit);
        }
        let d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let allowed = d
            .friends
            .get(&(sender, recipient))
            .is_some_and(|f| !f.blocked && !f.pending)
            && d.friends
                .get(&(recipient, sender))
                .is_some_and(|f| !f.blocked && !f.pending);
        drop(d);
        if !allowed {
            return Err(MessageError::Rejected);
        };
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let queued = d.messages.get(&recipient).map_or(0, VecDeque::len)
            + d.live_messages.get(&recipient).map_or(0, VecDeque::len)
            + d.inflight_messages.get(&recipient).map_or(0, VecDeque::len);
        if queued >= MAX_QUEUED_MESSAGES {
            return Err(MessageError::Limit);
        }
        let nickname = d
            .users
            .get(&sender)
            .map_or_else(Vec::new, |u| u.nickname.clone());
        let message = OfflineMessage {
            sender_id: sender,
            recipient_id: recipient,
            nickname,
            body,
            delivery_id: 0,
            lease_token: [0; 16],
        };
        if d.online.contains_key(&recipient) {
            d.live_messages
                .entry(recipient)
                .or_default()
                .push_back(message);
        } else {
            d.messages.entry(recipient).or_default().push_back(message);
        }
        Ok(())
    }
    pub fn queue_guild_message(
        &self,
        sender: u32,
        recipient: u32,
        body: Vec<u8>,
    ) -> Result<(), MessageError> {
        if body.len() > MAX_TEXT_BYTES {
            return Err(MessageError::Limit);
        }
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let Some(sender_user) = d.users.get(&sender).cloned() else {
            return Err(MessageError::Rejected);
        };
        let Some(recipient_user) = d.users.get(&recipient) else {
            return Err(MessageError::Rejected);
        };
        if sender_user.guild_id.is_none() || sender_user.guild_id != recipient_user.guild_id {
            return Err(MessageError::Rejected);
        }
        let queued = d.messages.get(&recipient).map_or(0, VecDeque::len)
            + d.live_messages.get(&recipient).map_or(0, VecDeque::len)
            + d.inflight_messages.get(&recipient).map_or(0, VecDeque::len);
        if queued >= MAX_QUEUED_MESSAGES {
            return Err(MessageError::Limit);
        }
        let message = OfflineMessage {
            sender_id: sender,
            recipient_id: recipient,
            nickname: sender_user.nickname,
            body,
            delivery_id: 0,
            lease_token: [0; 16],
        };
        if d.online.contains_key(&recipient) {
            d.live_messages
                .entry(recipient)
                .or_default()
                .push_back(message);
        } else {
            d.messages.entry(recipient).or_default().push_back(message);
        }
        Ok(())
    }
    pub fn take_live_messages(&self, id: u32) -> Result<Vec<OfflineMessage>, MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let messages: Vec<_> = d.live_messages.get_mut(&id).map_or_else(Vec::new, |q| {
            q.drain(..q.len().min(MAX_DELIVERY_BATCH)).collect()
        });
        if d.live_messages.get(&id).is_some_and(VecDeque::is_empty) {
            d.live_messages.remove(&id);
        }
        d.inflight_messages
            .entry(id)
            .or_default()
            .extend(messages.iter().cloned());
        Ok(messages)
    }
    pub fn take_messages(&self, id: u32) -> Result<Vec<OfflineMessage>, MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let messages: Vec<_> = d.messages.get_mut(&id).map_or_else(Vec::new, |q| {
            q.drain(..q.len().min(MAX_DELIVERY_BATCH)).collect()
        });
        if d.messages.get(&id).is_some_and(VecDeque::is_empty) {
            d.messages.remove(&id);
        }
        d.inflight_messages
            .entry(id)
            .or_default()
            .extend(messages.iter().cloned());
        Ok(messages)
    }
    pub fn ack_messages(&self, messages: &[OfflineMessage]) -> Result<(), MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        for message in messages {
            if let Some(queue) = d.inflight_messages.get_mut(&message.recipient_id) {
                queue.retain(|candidate| {
                    candidate.sender_id != message.sender_id || candidate.body != message.body
                });
            }
        }
        d.inflight_messages.retain(|_, q| !q.is_empty());
        Ok(())
    }
}

#[derive(Default)]
struct ReplayGuardData {
    entries: VecDeque<u64>,
}
pub struct ReplayGuard {
    capacity: usize,
    data: ReplayGuardData,
}
impl ReplayGuard {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.min(MAX_REPLAY_NONCES),
            data: ReplayGuardData::default(),
        }
    }
    pub fn admit(&mut self, value: u64) -> bool {
        if self.data.entries.contains(&value) {
            return false;
        }
        if self.capacity == 0 {
            return false;
        }
        if self.data.entries.len() == self.capacity {
            self.data.entries.pop_front();
        }
        self.data.entries.push_back(value);
        true
    }
}
pub struct RateLimiter {
    limit: u32,
    window: Duration,
    start: Instant,
    count: u32,
}
impl RateLimiter {
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            window: Duration::from_secs(60),
            start: Instant::now(),
            count: 0,
        }
    }
    pub fn admit(&mut self) -> bool {
        if self.start.elapsed() >= self.window {
            self.start = Instant::now();
            self.count = 0;
        }
        if self.count >= self.limit {
            return false;
        }
        self.count += 1;
        true
    }
}

/// A MessageService state machine over either the in-memory test store or a durable backend.
/// Production composition uses [`PostgresStore`]; all mutations complete before packets return.
pub struct MessageSession {
    store: Arc<dyn MessageStore>,
    user_id: Option<u32>,
    nickname: Vec<u8>,
    peer_ip: IpAddr,
    status: Presence,
    channel: ChannelInfo,
    rate: RateLimiter,
    replay: ReplayGuard,
    registry: Option<Arc<SessionRegistry>>,
    lease: Option<u64>,
    pending_messages: Vec<OfflineMessage>,
}
impl MessageSession {
    /// Drains presence and live-chat notifications generated for this session.
    pub async fn poll(&mut self) -> Result<Vec<ServerPacket>, MessageError> {
        let id = self.user_id.ok_or(MessageError::Unauthorized)?;
        if let (Some(registry), Some(lease)) = (&self.registry, self.lease)
            && !registry.is_current(id, lease)?
        {
            return Err(MessageError::Rejected);
        }
        let _ = self.store.heartbeat(id).await;
        let mut responses = self
            .store
            .take_presence_events(id)
            .await?
            .into_iter()
            .map(|(user_id, status, channel)| status_packet(user_id, status, &channel))
            .collect::<Vec<_>>();
        let live = if self.pending_messages.is_empty() {
            self.store.take_live_messages(id).await?
        } else {
            Vec::new()
        };
        self.pending_messages.extend(live.iter().cloned());
        responses.extend(live.into_iter().map(|message| ServerPacket::Chat {
            user_id: message.sender_id,
            nickname: message.nickname,
            message: message.body,
            guild: false,
        }));
        Ok(responses)
    }
    pub(crate) async fn ack_pending(&mut self) -> Result<(), MessageError> {
        if !self.pending_messages.is_empty() {
            self.store.ack_messages(&self.pending_messages).await?;
            self.pending_messages.clear();
        }
        Ok(())
    }
    async fn friend_pages(&self, id: u32) -> Result<Vec<ServerPacket>, MessageError> {
        let entries = self.store.friends(id).await?;
        let total = u16::try_from(entries.len()).map_err(|_| MessageError::Limit)?;
        Ok(entries
            .chunks(FRIEND_PAGE_SIZE)
            .enumerate()
            .map(|(index, chunk)| ServerPacket::FriendList {
                page: Page {
                    number: u8::try_from(index + 1).unwrap_or(u8::MAX),
                    total,
                    current: u16::try_from(chunk.len()).unwrap_or(u16::MAX),
                },
                entries: chunk.to_vec(),
            })
            .collect())
    }
    #[must_use]
    pub fn new(store: MemoryStore) -> Self {
        Self::with_store(Arc::new(store))
    }
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.user_id.is_some()
    }

    #[must_use]
    pub fn with_store(store: Arc<dyn MessageStore>) -> Self {
        Self {
            store,
            user_id: None,
            nickname: Vec::new(),
            peer_ip: "127.0.0.1".parse().expect("literal peer"),
            status: Presence::Offline,
            channel: ChannelInfo::offline(),
            rate: RateLimiter::new(60),
            replay: ReplayGuard::new(256),
            registry: None,
            lease: None,
            pending_messages: Vec::new(),
        }
    }
    pub(crate) fn with_registry(mut self, registry: Arc<SessionRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }
    pub(crate) fn with_peer_ip(mut self, peer_ip: IpAddr) -> Self {
        self.peer_ip = peer_ip;
        self
    }
    pub async fn disconnect(&mut self) -> Result<(), MessageError> {
        if let Some(id) = self.user_id.take() {
            let owns_generation = match (&self.registry, self.lease) {
                (Some(registry), Some(lease)) => registry.release(id, lease)?,
                _ => true,
            };
            if owns_generation {
                self.store.set_offline(id).await?;
            }
            self.lease = None;
        }
        Ok(())
    }
    pub fn admit_nonce(&mut self, nonce: u64) -> Result<(), MessageError> {
        self.replay
            .admit(nonce)
            .then_some(())
            .ok_or(MessageError::Rejected)
    }
    pub async fn handle(
        &mut self,
        packet: ClientPacket,
    ) -> Result<Vec<ServerPacket>, MessageError> {
        if let (Some(id), Some(registry), Some(lease)) = (self.user_id, &self.registry, self.lease)
            && !registry.is_current(id, lease)?
        {
            return Err(MessageError::Rejected);
        }
        if !self.rate.admit() {
            return Err(MessageError::Rejected);
        };
        // Every operation other than the credential declaration is bound to the authenticated
        // account. In particular, lookup is not a public nickname oracle.
        if self.user_id.is_none() && !matches!(packet, ClientPacket::CredentialDeclaration { .. }) {
            return Err(MessageError::Unauthorized);
        }
        match packet {
            ClientPacket::CredentialDeclaration {
                user_id,
                user_nickname,
            } => {
                if self.user_id.is_some()
                    || !self
                        .store
                        .authenticate(user_id, &user_nickname, self.peer_ip)
                        .await?
                {
                    return Err(MessageError::Rejected);
                }
                let lease = self
                    .registry
                    .as_ref()
                    .map(|registry| registry.claim(user_id))
                    .transpose()?;
                self.user_id = Some(user_id);
                self.lease = lease;
                self.nickname = user_nickname;
                Ok(vec![ServerPacket::CredentialResponse { user_id }])
            }
            ClientPacket::Hello => {
                let id = self.user_id.ok_or(MessageError::Unauthorized)?;
                let mut out = vec![status_packet(id, self.status, &self.channel)];
                out.extend(self.friend_pages(id).await?);
                let offline = if self.pending_messages.is_empty() {
                    self.store.take_messages(id).await?
                } else {
                    Vec::new()
                };
                self.pending_messages.extend(offline.iter().cloned());
                out.extend(offline.into_iter().map(|m| ServerPacket::Chat {
                    user_id: m.sender_id,
                    nickname: m.nickname,
                    message: m.body,
                    guild: false,
                }));
                Ok(out)
            }
            ClientPacket::Goodbye => {
                self.disconnect().await?;
                Ok(Vec::new())
            }
            ClientPacket::Lookup { nickname } => {
                self.user_id.ok_or(MessageError::Unauthorized)?;
                let found = self.store.lookup(&nickname).await?;
                Ok(vec![ServerPacket::LookupResponse {
                    status: u32::from(found.is_none()),
                    nickname,
                    user_id: found,
                }])
            }
            ClientPacket::AddFriend { user_id, nickname } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                if self.store.lookup(&nickname).await? != Some(user_id) {
                    return Err(MessageError::Rejected);
                }
                self.store.add_friend(me, user_id).await?;
                Ok(vec![ServerPacket::FriendRequest {
                    status: 0,
                    entry: self
                        .store
                        .friends(me)
                        .await?
                        .into_iter()
                        .find(|f| f.user_id == user_id)
                        .ok_or(MessageError::Rejected)?,
                }])
            }
            ClientPacket::ConfirmFriend { user_id } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                if !self.store.has_pending_friend_request(me, user_id).await? {
                    return Err(MessageError::Rejected);
                }
                self.store.confirm_friend(me, user_id).await?;
                // SuperSS's established acceptor result is 0x0109 (status + user ID).
                Ok(vec![ServerPacket::ConfirmResult { status: 0, user_id }])
            }
            ClientPacket::BlockFriend { user_id } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.store.block_friend(me, user_id).await?;
                Ok(vec![ServerPacket::BlockResult { status: 0, user_id }])
            }
            ClientPacket::UnblockFriend { user_id } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.store.unblock_friend(me, user_id).await?;
                Ok(vec![ServerPacket::UnblockResult { status: 0, user_id }])
            }
            ClientPacket::DeleteFriend { user_id, .. } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.store.delete_friend(me, user_id).await?;
                Ok(vec![ServerPacket::DeleteResult { status: 0, user_id }])
            }
            ClientPacket::Status { status } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.status = status;
                if status == Presence::Offline {
                    self.store.set_offline(me).await?;
                } else {
                    self.store
                        .set_online(me, status, self.channel.clone())
                        .await?;
                }
                let mut responses = vec![status_packet(me, status, &self.channel)];
                responses.extend(
                    self.store
                        .take_presence_events(me)
                        .await?
                        .into_iter()
                        .map(|(user_id, status, channel)| status_packet(user_id, status, &channel)),
                );
                Ok(responses)
            }
            ClientPacket::Server {
                server_id,
                channel_id,
                channel_name,
                ..
            } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.channel = ChannelInfo {
                    room_number: -1,
                    room_type: -1,
                    server_id,
                    channel_id: channel_id as i8,
                    channel_name,
                };
                self.store
                    .set_online(me, self.status, self.channel.clone())
                    .await?;
                self.friend_pages(me).await
            }
            ClientPacket::Chat { user_id, message } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.store.queue_message(me, user_id, message).await?;
                Ok(Vec::new())
            }
            ClientPacket::Alias { user_id, alias } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.store.set_alias(me, user_id, alias.clone()).await?;
                Ok(vec![ServerPacket::AliasResult {
                    status: 0,
                    user_id,
                    alias,
                }])
            }
            ClientPacket::GuildChat { message } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                let ids = self.store.guild_members(me).await?;
                if ids.is_empty() {
                    return Ok(Vec::new());
                }
                for id in ids {
                    if id != me {
                        let _ = self
                            .store
                            .queue_guild_message(me, id, message.clone())
                            .await;
                    }
                }
                Ok(vec![ServerPacket::Chat {
                    user_id: me,
                    nickname: self.nickname.clone(),
                    message,
                    guild: true,
                }])
            }
            ClientPacket::RoomInvite { .. }
            | ClientPacket::GuildBattleInvite { .. }
            | ClientPacket::Gift { .. }
            | ClientPacket::GuildAccept { .. }
            | ClientPacket::GuildKick { .. }
            | ClientPacket::GuildImage { .. }
            | ClientPacket::GuildName { .. }
            | ClientPacket::Unknown => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
mod e2e_tests;
#[cfg(test)]
mod tests;
