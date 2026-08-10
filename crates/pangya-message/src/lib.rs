#![allow(missing_docs)]

//! MessageService protocol and social state boundary.
//!
//! MessageService has an independent opcode namespace. These IDs intentionally overlap game
//! packets and must never be registered in the GameService table.

use std::{
    collections::{HashMap, VecDeque},
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
pub const FRIEND_PAGE_SIZE: usize = 50;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Page {
    pub number: u8,
    pub total: u16,
    pub current: u16,
}
impl Page {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.number);
        out.extend(self.total.to_le_bytes());
        out.extend(self.current.to_le_bytes());
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
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend(self.room_number.to_le_bytes());
        out.extend(self.room_type.to_le_bytes());
        out.extend(self.server_id.to_le_bytes());
        out.push(self.channel_id as u8);
        fixed(out, &self.channel_name, 64);
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
    fn encode(&self, out: &mut Vec<u8>) {
        fixed(out, &self.nickname, 22);
        fixed(out, &self.alias, 25);
        out.extend(self.user_id.to_le_bytes());
        out.extend([0; 99]);
        out.extend([0; 2]);
        out.push(0);
        out.extend([0; 2]);
        self.channel.encode(out);
        out.push(self.state as u8);
        out.push(0xff);
        out.push(0);
        out.push(self.state as u8);
        out.push(self.relationship as u8);
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
    Presence {
        user_id: u32,
        status: Presence,
        channel: ChannelInfo,
    },
    Logout {
        user_id: u32,
    },
    Chat {
        user_id: u32,
        nickname: Vec<u8>,
        message: Vec<u8>,
        guild: bool,
    },
    Mutation {
        subtype: u16,
        status: u32,
        user_id: u32,
        text: Vec<u8>,
    },
    GuildNotice {
        subtype: u16,
        user_id: u32,
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
                o.extend(0x102_u16.to_le_bytes());
                page.encode(&mut o);
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
                if let Some(id) = user_id {
                    o.extend(id.to_le_bytes());
                }
            }
            Self::FriendRequest { status, entry } => {
                o.extend(0x104_u16.to_le_bytes());
                o.extend([0; 4]);
                o.extend(status.to_le_bytes());
                entry.encode(&mut o);
            }
            Self::Presence {
                user_id,
                status,
                channel,
            } => {
                o.extend(0x115_u16.to_le_bytes());
                o.extend(user_id.to_le_bytes());
                o.extend((*status as u32).to_le_bytes());
                o.push(1);
                channel.encode(&mut o);
            }
            Self::Logout { user_id } => {
                o.extend(0x10f_u16.to_le_bytes());
                o.extend(user_id.to_le_bytes());
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
            Self::Mutation {
                subtype,
                status,
                user_id,
                text,
            } => {
                o.extend(subtype.to_le_bytes());
                o.extend(status.to_le_bytes());
                o.extend(user_id.to_le_bytes());
                if !text.is_empty() {
                    string(&mut o, text, MAX_TEXT_BYTES)?;
                }
            }
            Self::GuildNotice { subtype, user_id } => {
                o.extend(subtype.to_le_bytes());
                o.extend(user_id.to_le_bytes());
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
                let page = Page {
                    number: r.u8()?,
                    total: r.u16()?,
                    current: r.u16()?,
                };
                let mut entries = Vec::new();
                while r.remaining() > 0 {
                    entries.push(FriendEntry::decode(&mut r)?);
                }
                Self::FriendList { page, entries }
            }
            0x115 => Self::Presence {
                user_id: r.u32()?,
                status: Presence::try_from(r.u32()? as u8)?,
                channel: ChannelInfo::decode(&mut r)?,
            },
            0x10f => Self::Logout { user_id: r.u32()? },
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
        r.take(99 + 2 + 1 + 2)?;
        let channel = ChannelInfo::decode(r)?;
        let state = Presence::try_from(r.u8()?)?;
        r.take(4)?;
        Ok(Self {
            nickname,
            alias,
            user_id,
            channel,
            state,
            relationship: Relationship::Friend,
            blocked: false,
        })
    }
}
impl ChannelInfo {
    fn decode(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            room_number: r.i16()?,
            room_type: r.i32()?,
            server_id: r.u32()?,
            channel_id: r.u8()? as i8,
            channel_name: r.fixed(64)?,
        })
    }
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum MessageError {
    #[error("message packet is truncated")]
    Truncated,
    #[error("message packet has trailing bytes")]
    Trailing,
    #[error("message packet exceeds a bounded field")]
    Limit,
    #[error("unknown MessageService opcode {0:#x}")]
    UnknownOpcode(u16),
    #[error("unknown MessageService subtype {0:#x}")]
    UnknownSubtype(u16),
    #[error("invalid presence status {0}")]
    InvalidPresence(u8),
    #[error("session is not authenticated")]
    Unauthorized,
    #[error("social operation rejected")]
    Rejected,
}
impl TryFrom<u8> for Presence {
    type Error = MessageError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Playing),
            1 => Ok(Self::Idle),
            3 => Ok(Self::Busy),
            4 => Ok(Self::Online),
            5 => Ok(Self::Offline),
            x => Err(MessageError::InvalidPresence(x)),
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
    fn i16(&mut self) -> Result<i16, MessageError> {
        Ok(i16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| MessageError::Truncated)?,
        ))
    }
    fn i32(&mut self) -> Result<i32, MessageError> {
        Ok(i32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| MessageError::Truncated)?,
        ))
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
}
#[derive(Default, Clone)]
pub struct MemoryStore(Arc<Mutex<StoreData>>);
#[derive(Default)]
struct StoreData {
    users: HashMap<u32, User>,
    friends: HashMap<(u32, u32), FriendState>,
    messages: HashMap<u32, VecDeque<OfflineMessage>>,
    live_messages: HashMap<u32, VecDeque<OfflineMessage>>,
    online: HashMap<u32, (Presence, ChannelInfo)>,
    presence_events: HashMap<u32, VecDeque<(u32, Presence, ChannelInfo)>>,
}
#[derive(Clone, Debug)]
struct FriendState {
    alias: Vec<u8>,
    blocked: bool,
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
            |d| {
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
                    .collect()
            },
        )
    }
    pub fn add_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        if !d.users.contains_key(&a) || !d.users.contains_key(&b) || a == b {
            return Err(MessageError::Rejected);
        };
        d.friends.entry((a, b)).or_insert(FriendState {
            alias: b"Friend".to_vec(),
            blocked: false,
        });
        Ok(())
    }
    pub fn confirm_friend(&self, a: u32, b: u32) -> Result<(), MessageError> {
        self.add_friend(a, b)?;
        self.add_friend(b, a)
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
            let recipients: Vec<u32> = d
                .friends
                .iter()
                .filter_map(|(&(owner, friend), state)| {
                    (friend == id && owner != id && !state.blocked).then_some(owner)
                })
                .collect();
            for recipient in recipients {
                d.presence_events.entry(recipient).or_default().push_back((
                    id,
                    status,
                    channel.clone(),
                ));
            }
        }
    }
    pub fn take_presence_events(&self, id: u32) -> Vec<(u32, Presence, ChannelInfo)> {
        self.0.lock().map_or_else(
            |_| Vec::new(),
            |mut d| {
                d.presence_events
                    .remove(&id)
                    .map_or_else(Vec::new, |events| events.into_iter().collect())
            },
        )
    }
    pub fn queue_message(
        &self,
        sender: u32,
        recipient: u32,
        body: Vec<u8>,
    ) -> Result<(), MessageError> {
        let d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let allowed = d
            .friends
            .get(&(sender, recipient))
            .is_some_and(|f| !f.blocked)
            && d.friends
                .get(&(recipient, sender))
                .is_some_and(|f| !f.blocked);
        drop(d);
        if !allowed {
            return Err(MessageError::Rejected);
        };
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        let nickname = d
            .users
            .get(&sender)
            .map_or_else(Vec::new, |u| u.nickname.clone());
        let message = OfflineMessage {
            sender_id: sender,
            recipient_id: recipient,
            nickname,
            body,
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
        Ok(d.live_messages
            .remove(&id)
            .map_or_else(Vec::new, |q| q.into_iter().collect()))
    }
    pub fn take_messages(&self, id: u32) -> Result<Vec<OfflineMessage>, MessageError> {
        let mut d = self.0.lock().map_err(|_| MessageError::Rejected)?;
        Ok(d.messages
            .remove(&id)
            .map_or_else(Vec::new, |q| q.into_iter().collect()))
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
            capacity,
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

/// A process-local MessageService state machine. Production callers may share `MemoryStore` across
/// listener generations; all durable mutations happen before packets are returned.
pub struct MessageSession {
    store: MemoryStore,
    user_id: Option<u32>,
    nickname: Vec<u8>,
    status: Presence,
    channel: ChannelInfo,
    rate: RateLimiter,
    replay: ReplayGuard,
}
impl MessageSession {
    /// Drains presence and live-chat notifications generated for this session.
    pub fn poll(&self) -> Result<Vec<ServerPacket>, MessageError> {
        let id = self.user_id.ok_or(MessageError::Unauthorized)?;
        let mut responses = self
            .store
            .take_presence_events(id)
            .into_iter()
            .map(|(user_id, status, channel)| ServerPacket::Presence {
                user_id,
                status,
                channel,
            })
            .collect::<Vec<_>>();
        responses.extend(
            self.store
                .take_live_messages(id)?
                .into_iter()
                .map(|message| ServerPacket::Chat {
                    user_id: message.sender_id,
                    nickname: message.nickname,
                    message: message.body,
                    guild: false,
                }),
        );
        Ok(responses)
    }
    #[must_use]
    pub fn new(store: MemoryStore) -> Self {
        Self {
            store,
            user_id: None,
            nickname: Vec::new(),
            status: Presence::Offline,
            channel: ChannelInfo::offline(),
            rate: RateLimiter::new(60),
            replay: ReplayGuard::new(256),
        }
    }
    pub fn admit_nonce(&mut self, nonce: u64) -> Result<(), MessageError> {
        self.replay
            .admit(nonce)
            .then_some(())
            .ok_or(MessageError::Rejected)
    }
    pub fn handle(&mut self, packet: ClientPacket) -> Result<Vec<ServerPacket>, MessageError> {
        if !self.rate.admit() {
            return Err(MessageError::Rejected);
        };
        match packet {
            ClientPacket::CredentialDeclaration {
                user_id,
                user_nickname,
            } => {
                let known_nickname = self
                    .store
                    .0
                    .lock()
                    .map_err(|_| MessageError::Rejected)?
                    .users
                    .get(&user_id)
                    .map(|user| user.nickname.clone());
                if known_nickname.as_deref() != Some(user_nickname.as_slice()) {
                    return Err(MessageError::Rejected);
                }
                self.user_id = Some(user_id);
                self.nickname = user_nickname;
                Ok(vec![ServerPacket::CredentialResponse { user_id }])
            }
            ClientPacket::Hello => {
                let id = self.user_id.ok_or(MessageError::Unauthorized)?;
                let mut out = vec![ServerPacket::Presence {
                    user_id: id,
                    status: self.status,
                    channel: self.channel.clone(),
                }];
                out.push(ServerPacket::FriendList {
                    page: Page {
                        number: 1,
                        total: 0,
                        current: 0,
                    },
                    entries: self.store.friends(id),
                });
                out.extend(
                    self.store
                        .take_messages(id)?
                        .into_iter()
                        .map(|m| ServerPacket::Chat {
                            user_id: m.sender_id,
                            nickname: m.nickname,
                            message: m.body,
                            guild: false,
                        }),
                );
                Ok(out)
            }
            ClientPacket::Goodbye => {
                if let Some(id) = self.user_id {
                    self.store
                        .set_online(id, Presence::Offline, ChannelInfo::offline());
                }
                Ok(Vec::new())
            }
            ClientPacket::Lookup { nickname } => {
                let d = self.store.0.lock().map_err(|_| MessageError::Rejected)?;
                let found = d
                    .users
                    .values()
                    .find(|u| u.nickname.eq_ignore_ascii_case(&nickname));
                Ok(vec![ServerPacket::LookupResponse {
                    status: u32::from(found.is_none()),
                    nickname,
                    user_id: found.map(|u| u.id),
                }])
            }
            ClientPacket::AddFriend { user_id, nickname } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                let d = self.store.0.lock().map_err(|_| MessageError::Rejected)?;
                if d.users
                    .get(&user_id)
                    .is_none_or(|u| !u.nickname.eq_ignore_ascii_case(&nickname))
                {
                    return Err(MessageError::Rejected);
                }
                drop(d);
                self.store.add_friend(me, user_id)?;
                Ok(vec![ServerPacket::FriendRequest {
                    status: 0,
                    entry: self
                        .store
                        .friends(me)
                        .into_iter()
                        .find(|f| f.user_id == user_id)
                        .ok_or(MessageError::Rejected)?,
                }])
            }
            ClientPacket::ConfirmFriend { user_id } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.store.confirm_friend(me, user_id)?;
                Ok(Vec::new())
            }
            ClientPacket::BlockFriend { user_id } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.store.block_friend(me, user_id)?;
                Ok(vec![ServerPacket::Mutation {
                    subtype: 0x10c,
                    status: 0,
                    user_id,
                    text: Vec::new(),
                }])
            }
            ClientPacket::UnblockFriend { user_id } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.store.unblock_friend(me, user_id)?;
                Ok(vec![ServerPacket::Mutation {
                    subtype: 0x10d,
                    status: 0,
                    user_id,
                    text: Vec::new(),
                }])
            }
            ClientPacket::DeleteFriend { user_id, .. } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.store.delete_friend(me, user_id)?;
                Ok(vec![ServerPacket::Mutation {
                    subtype: 0x10b,
                    status: 0,
                    user_id,
                    text: Vec::new(),
                }])
            }
            ClientPacket::Status { status } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.status = status;
                self.store.set_online(me, status, self.channel.clone());
                let mut responses = vec![ServerPacket::Presence {
                    user_id: me,
                    status,
                    channel: self.channel.clone(),
                }];
                responses.extend(self.store.take_presence_events(me).into_iter().map(
                    |(user_id, status, channel)| ServerPacket::Presence {
                        user_id,
                        status,
                        channel,
                    },
                ));
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
                self.store.set_online(me, self.status, self.channel.clone());
                Ok(vec![ServerPacket::FriendList {
                    page: Page {
                        number: 1,
                        total: 0,
                        current: 0,
                    },
                    entries: self.store.friends(me),
                }])
            }
            ClientPacket::Chat { user_id, message } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                self.store.queue_message(me, user_id, message)?;
                Ok(Vec::new())
            }
            ClientPacket::Alias { user_id, alias } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                let mut d = self.store.0.lock().map_err(|_| MessageError::Rejected)?;
                if let Some(f) = d.friends.get_mut(&(me, user_id)) {
                    f.alias = alias.clone();
                    Ok(vec![ServerPacket::Mutation {
                        subtype: 0x119,
                        status: 0,
                        user_id,
                        text: alias,
                    }])
                } else {
                    Err(MessageError::Rejected)
                }
            }
            ClientPacket::GuildChat { message } => {
                let me = self.user_id.ok_or(MessageError::Unauthorized)?;
                let d = self.store.0.lock().map_err(|_| MessageError::Rejected)?;
                let guild = d
                    .users
                    .get(&me)
                    .and_then(|u| u.guild_id)
                    .ok_or(MessageError::Rejected)?;
                let ids = d
                    .users
                    .values()
                    .filter(|u| u.guild_id == Some(guild))
                    .map(|u| u.id)
                    .collect::<Vec<_>>();
                drop(d);
                for id in ids {
                    if id != me {
                        let _ = self.store.queue_message(me, id, message.clone());
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
