//! U.S. 852 lobby/social packet contracts derived from PacketDoc and K4T.
#![allow(missing_docs)]

use crate::{
    CompatibilityProfile, DecodePacket, EncodePacket, PacketDecodeError, PacketEncodeError,
    PacketReader, PacketWriter,
};

const MAX_TEXT: usize = 128;
const MAX_NAME: usize = 64;
const MAX_ACTION: usize = 512;

fn profile_decode(
    profile: &CompatibilityProfile,
    reader: &PacketReader<'_>,
) -> Result<(), PacketDecodeError> {
    profile
        .require_us852()
        .map_err(|error| reader.invalid(error.to_string()))
}
fn profile_encode(profile: &CompatibilityProfile) -> Result<(), PacketEncodeError> {
    profile.require_us852().map_err(Into::into)
}
fn end(reader: &PacketReader<'_>) -> Result<(), PacketDecodeError> {
    if reader.remaining() == 0 {
        Ok(())
    } else {
        Err(reader.invalid("social packet has trailing bytes"))
    }
}
fn pstring(
    reader: &mut PacketReader<'_>,
    maximum: usize,
    field: &'static str,
) -> Result<Vec<u8>, PacketDecodeError> {
    let bytes = reader.pstring(maximum)?;
    if bytes.contains(&0) {
        return Err(reader.invalid(format!("{field} contains NUL")));
    }
    Ok(bytes.to_vec())
}
fn write_string(
    writer: &mut PacketWriter,
    bytes: &[u8],
    maximum: usize,
    field: &'static str,
) -> Result<(), PacketEncodeError> {
    if bytes.contains(&0) {
        return Err(PacketEncodeError::Invalid { field });
    }
    writer.pstring(bytes, maximum)
}

/// Retail lobby/room chat request, client opcode `0x0003`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameChat {
    pub nickname: Vec<u8>,
    pub message: Vec<u8>,
}
impl GameChat {
    pub fn new(nickname: Vec<u8>, message: Vec<u8>) -> Self {
        Self { nickname, message }
    }
}
impl DecodePacket for GameChat {
    const OPCODE: u16 = 0x0003;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile_decode(profile, reader)?;
        reader.array::<4>()?;
        let nickname = pstring(reader, MAX_NAME, "nickname")?;
        let message = pstring(reader, MAX_TEXT, "message")?;
        end(reader)?;
        Ok(Self { nickname, message })
    }
}
impl EncodePacket for GameChat {
    const OPCODE: u16 = 0x0003;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        writer.bytes(&[0; 4]);
        write_string(writer, &self.nickname, MAX_NAME, "nickname")?;
        write_string(writer, &self.message, MAX_TEXT, "message")
    }
}
/// Retail global chat response, server opcode `0x0040`, subtype `0x00`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameChatResponse {
    pub nickname: Vec<u8>,
    pub message: Vec<u8>,
}
impl EncodePacket for GameChatResponse {
    const OPCODE: u16 = 0x0040;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        writer.u8(0);
        write_string(writer, &self.nickname, MAX_NAME, "nickname")?;
        write_string(writer, &self.message, MAX_TEXT, "message")
    }
}
/// Retail whisper request, client opcode `0x002A`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Whisper {
    pub nickname: Vec<u8>,
    pub message: Vec<u8>,
}
impl Whisper {
    pub fn new(nickname: Vec<u8>, message: Vec<u8>) -> Self {
        Self { nickname, message }
    }
}
impl DecodePacket for Whisper {
    const OPCODE: u16 = 0x002a;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile_decode(profile, reader)?;
        let nickname = pstring(reader, MAX_NAME, "nickname")?;
        let message = pstring(reader, MAX_TEXT, "message")?;
        end(reader)?;
        Ok(Self { nickname, message })
    }
}
impl EncodePacket for Whisper {
    const OPCODE: u16 = 0x002a;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        write_string(writer, &self.nickname, MAX_NAME, "nickname")?;
        write_string(writer, &self.message, MAX_TEXT, "message")
    }
}
/// Retail whisper response, server opcode `0x0084`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhisperResponse {
    pub status: u8,
    pub nickname: Vec<u8>,
    pub message: Vec<u8>,
}
impl EncodePacket for WhisperResponse {
    const OPCODE: u16 = 0x0084;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        // 0/1 are delivery/result statuses; 4 (blocked) and 5 (offline) are
        // documented refusal statuses and are still valid response packets. The
        // retail client stays in the lobby after either response.
        if !matches!(self.status, 0 | 1 | 4 | 5) {
            return Err(PacketEncodeError::Invalid {
                field: "whisper status",
            });
        }
        writer.u8(self.status);
        write_string(writer, &self.nickname, MAX_NAME, "nickname")?;
        write_string(writer, &self.message, MAX_TEXT, "message")
    }
}
/// Retail whisper refusal response, server opcode `0x0040`.
///
/// SuperSS emits refusal statuses 4 (target blocks whispers) and 5 (target is offline) on the
/// message-data opcode with only the target pstring; successful delivery uses `0x0084`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhisperRefusalResponse {
    pub status: u8,
    pub nickname: Vec<u8>,
}
impl EncodePacket for WhisperRefusalResponse {
    const OPCODE: u16 = 0x0040;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if !matches!(self.status, 4 | 5) {
            return Err(PacketEncodeError::Invalid {
                field: "whisper refusal status",
            });
        }
        writer.u8(self.status);
        write_string(writer, &self.nickname, MAX_NAME, "nickname")
    }
}

/// Retail typing indicator, client opcode `0x0018`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypingIndicator {
    pub typing: bool,
}
impl DecodePacket for TypingIndicator {
    const OPCODE: u16 = 0x0018;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile_decode(profile, reader)?;
        let value = reader.i16_le()?;
        end(reader)?;
        match value {
            1 => Ok(Self { typing: true }),
            -1 => Ok(Self { typing: false }),
            _ => Err(reader.invalid("typing indicator is not 1 or -1")),
        }
    }
}
impl EncodePacket for TypingIndicator {
    const OPCODE: u16 = 0x0018;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        writer.i16_le(if self.typing { 1 } else { -1 });
        Ok(())
    }
}
/// Retail typing response, server opcode `0x005D`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypingIndicatorResponse {
    pub connection_id: u32,
    pub typing: bool,
}
impl EncodePacket for TypingIndicatorResponse {
    const OPCODE: u16 = 0x005d;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        writer.u32_le(self.connection_id);
        writer.i16_le(if self.typing { 1 } else { -1 });
        Ok(())
    }
}
/// Retail lounge action request, client opcode `0x0063`. Payload is retained opaque so all
/// K4T action subtypes (rotation, appear, posture, move, animation, effects) relay exactly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoungeAction {
    pub action_type: u8,
    pub action_payload: Vec<u8>,
}
impl LoungeAction {
    pub fn emote(value: Vec<u8>) -> Self {
        Self {
            action_type: 7,
            action_payload: value,
        }
    }
}
impl DecodePacket for LoungeAction {
    const OPCODE: u16 = 0x0063;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile_decode(profile, reader)?;
        let action_type = reader.u8()?;
        if reader.remaining() > MAX_ACTION {
            return Err(reader.invalid("lounge action exceeds bound"));
        }
        let action_payload = reader.unknown_tail().to_vec();
        Ok(Self {
            action_type,
            action_payload,
        })
    }
}
impl EncodePacket for LoungeAction {
    const OPCODE: u16 = 0x0063;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if self.action_payload.len() > MAX_ACTION {
            return Err(PacketEncodeError::Limit {
                field: "lounge action",
                actual: self.action_payload.len(),
                maximum: MAX_ACTION,
            });
        }
        writer.u8(self.action_type);
        writer.bytes(&self.action_payload);
        Ok(())
    }
}
/// Retail lounge action announcement, server opcode `0x00C4`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoungeActionResponse {
    pub connection_id: u32,
    pub action: LoungeAction,
}
impl LoungeActionResponse {
    pub fn new(connection_id: u32, payload: Vec<u8>) -> Self {
        let action_type = payload.first().copied().unwrap_or(0);
        Self {
            connection_id,
            action: LoungeAction {
                action_type,
                action_payload: payload.get(1..).unwrap_or_default().to_vec(),
            },
        }
    }
}
impl DecodePacket for LoungeActionResponse {
    const OPCODE: u16 = 0x00c4;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile_decode(profile, reader)?;
        let connection_id = reader.u32_le()?;
        let action_type = reader.u8()?;
        if reader.remaining() > MAX_ACTION {
            return Err(reader.invalid("lounge action exceeds bound"));
        }
        let action_payload = reader.unknown_tail().to_vec();
        Ok(Self {
            connection_id,
            action: LoungeAction {
                action_type,
                action_payload,
            },
        })
    }
}
impl EncodePacket for LoungeActionResponse {
    const OPCODE: u16 = 0x00c4;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        writer.u32_le(self.connection_id);
        writer.u8(self.action.action_type);
        writer.bytes(&self.action.action_payload);
        Ok(())
    }
}
/// Retail chat macro update, client opcode `0x0069`; nine fixed 64-byte slots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroUpdate {
    pub values: [Vec<u8>; 9],
}
impl MacroUpdate {
    pub fn new(values: [Vec<u8>; 9]) -> Self {
        Self { values }
    }
}
impl DecodePacket for MacroUpdate {
    const OPCODE: u16 = 0x0069;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile_decode(profile, reader)?;
        let values: [Result<Vec<u8>, PacketDecodeError>; 9] =
            std::array::from_fn(|_| reader.fixed_nul(64).map(<[u8]>::to_vec));
        let values: [Vec<u8>; 9] = values
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| reader.invalid("macro count"))?;
        end(reader)?;
        Ok(Self { values })
    }
}
impl EncodePacket for MacroUpdate {
    const OPCODE: u16 = 0x0069;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        for value in &self.values {
            writer.fixed_nul(value, 64)?;
        }
        Ok(())
    }
}
/// Retail user-status request, client opcode `0x0007`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserStatusRequest {
    pub unknown: u8,
    pub username: Vec<u8>,
}
impl DecodePacket for UserStatusRequest {
    const OPCODE: u16 = 0x0007;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile_decode(profile, reader)?;
        let unknown = reader.u8()?;
        let username = pstring(reader, MAX_NAME, "username")?;
        end(reader)?;
        if unknown != 1 {
            return Err(reader.invalid("user status discriminator"));
        }
        Ok(Self { unknown, username })
    }
}

/// Retail user-status refusal/empty response, server opcode `0x00A1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserStatusResponse;
impl EncodePacket for UserStatusResponse {
    const OPCODE: u16 = 0x00a1;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        writer.u8(2);
        Ok(())
    }
}

/// Retail offline-note request, client opcode `0x003C`.
///
/// PacketDoc identifies this as a 10-Pang operation. The final byte is retained because its
/// meaning is not established; it is nevertheless part of the exact request layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteSend {
    pub subtype: u16,
    pub user_id: u32,
    pub message: Vec<u8>,
    pub unknown: u8,
}
impl DecodePacket for NoteSend {
    const OPCODE: u16 = 0x003c;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile_decode(profile, reader)?;
        let subtype = reader.u16_le()?;
        let user_id = reader.u32_le()?;
        let message = pstring(reader, MAX_TEXT, "note message")?;
        let unknown = reader.u8()?;
        end(reader)?;
        Ok(Self {
            subtype,
            user_id,
            message,
            unknown,
        })
    }
}
impl EncodePacket for NoteSend {
    const OPCODE: u16 = 0x003c;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        writer.u16_le(self.subtype);
        writer.u32_le(self.user_id);
        write_string(writer, &self.message, MAX_TEXT, "note message")?;
        writer.u8(self.unknown);
        Ok(())
    }
}

/// Retail request for the message-server list, client opcode `0x008B`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessageServerListRequest;
impl DecodePacket for MessageServerListRequest {
    const OPCODE: u16 = 0x008b;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile_decode(profile, reader)?;
        end(reader)?;
        Ok(Self)
    }
}

/// Retail message-server list response, server opcode `0x00FC`.
///
/// The deployment has no message server, so the truthful response is the exact zero-count form.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MessageServerList;
impl EncodePacket for MessageServerList {
    const OPCODE: u16 = 0x00fc;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        writer.u8(0);
        Ok(())
    }
}

/// Retail lounge-enter request, client opcode `0x00EB`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoungeEnterRequest {
    pub connection_id: u32,
}
impl DecodePacket for LoungeEnterRequest {
    const OPCODE: u16 = 0x00eb;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile_decode(profile, reader)?;
        let connection_id = reader.u32_le()?;
        end(reader)?;
        Ok(Self { connection_id })
    }
}
/// Retail lounge-enter response, server opcode `0x0196`; packetdoc documents five `1.0` values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoungeEnterResponse {
    pub connection_id: u32,
}
impl EncodePacket for LoungeEnterResponse {
    const OPCODE: u16 = 0x0196;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        writer.u32_le(self.connection_id);
        for _ in 0..5 {
            writer.f32_le(1.0);
        }
        Ok(())
    }
}
/// Retail user information request, client opcode `0x002F`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserInfoRequest {
    pub user_id: u32,
    pub request_type: u8,
}
impl DecodePacket for UserInfoRequest {
    const OPCODE: u16 = 0x002f;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile_decode(profile, reader)?;
        let user_id = reader.u32_le()?;
        let request_type = reader.u8()?;
        if !matches!(request_type, 0 | 5) {
            return Err(reader.invalid("user info request type"));
        }
        end(reader)?;
        Ok(Self {
            user_id,
            request_type,
        })
    }
}
impl EncodePacket for UserInfoRequest {
    const OPCODE: u16 = 0x002f;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if !matches!(self.request_type, 0 | 5) {
            return Err(PacketEncodeError::Invalid {
                field: "user info request type",
            });
        }
        writer.u32_le(self.user_id);
        writer.u8(self.request_type);
        Ok(())
    }
}
/// Retail user-name fan-out response, server opcode `0x0157`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserNameInfoResponse {
    pub request_type: u8,
    pub user_id: u32,
    pub username: Vec<u8>,
    pub nickname: Vec<u8>,
}
impl EncodePacket for UserNameInfoResponse {
    const OPCODE: u16 = 0x0157;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if !matches!(self.request_type, 0 | 5) {
            return Err(PacketEncodeError::Invalid {
                field: "user info request type",
            });
        }
        writer.u8(self.request_type);
        writer.u32_le(self.user_id);
        writer.u16_le(0xffff);
        writer.fixed_nul(&self.username, 22)?;
        writer.fixed_nul(&self.nickname, 22)?;
        writer.fixed_nul(&[], 21)?;
        writer.fixed_nul(&[], 24)?;
        writer.u32_le(0);
        writer.bytes(&[0; 12]);
        writer.u32_le(0);
        writer.bytes(&[0; 4]);
        writer.u16_le(0);
        writer.bytes(&[0xff; 6]);
        writer.bytes(&[0; 16]);
        writer.fixed_nul(&[], 128)?;
        writer.u32_le(self.user_id);
        writer.bytes(&[0; 4]);
        Ok(())
    }
}
/// Retail statistics fan-out response, server opcode `0x0158`.
///
/// PacketDoc is authoritative for this response: `u8 request_type`, `u32 user_id`, then the
/// common 239-byte `user_statistic_data` body, including its five literal `0x7f` bytes. SuperSS's
/// conflicting projection uses a different Pang width and is not used here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserStatisticsInfoResponse {
    pub request_type: u8,
    pub user_id: u32,
    pub experience: u32,
    pub pang: u64,
}
impl EncodePacket for UserStatisticsInfoResponse {
    const OPCODE: u16 = 0x0158;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if !matches!(self.request_type, 0 | 5) {
            return Err(PacketEncodeError::Invalid {
                field: "user info request type",
            });
        }
        writer.u8(self.request_type);
        writer.u32_le(self.user_id);
        // PacketDoc `common/user_statistic_data.ksy` puts XP at offset 74 and Pang at 79.
        // Its five-byte `unknown_user_statistic_data_y` field is literally 0x7f, unlike the
        // broader SuperSS projection. Pang is a u32 on this response.
        let mut body = [0_u8; crate::PLAYER_STATISTICS_BYTES];
        body[74..78].copy_from_slice(&self.experience.to_le_bytes());
        let pang =
            u32::try_from(self.pang).map_err(|_| PacketEncodeError::Invalid { field: "pang" })?;
        body[79..83].copy_from_slice(&pang.to_le_bytes());
        body[91..96].fill(0x7f);
        writer.bytes(&body);
        Ok(())
    }
}
/// Retail equipment fan-out response, server opcode `0x0156`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserEquipmentInfoResponse {
    pub request_type: u8,
    pub user_id: u32,
    pub character_uid: u32,
    pub comet_iff_id: u32,
}
impl EncodePacket for UserEquipmentInfoResponse {
    const OPCODE: u16 = 0x0156;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if !matches!(self.request_type, 0 | 5) {
            return Err(PacketEncodeError::Invalid {
                field: "user info request type",
            });
        }
        writer.u8(self.request_type);
        writer.u32_le(self.user_id);
        for index in 0..29 {
            writer.u32_le(match index {
                1 => self.character_uid,
                3 => self.comet_iff_id,
                _ => 0,
            });
        }
        Ok(())
    }
}
/// Retail character fan-out response, server opcode `0x015E`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserCharacterInfoResponse {
    pub user_id: u32,
    pub character_iff_id: u32,
    pub character_uid: u32,
}
impl EncodePacket for UserCharacterInfoResponse {
    const OPCODE: u16 = 0x015e;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        writer.u32_le(self.user_id);
        crate::RetailCharacter {
            iff_id: self.character_iff_id,
            uid: self.character_uid,
            hair_color: 0,
            part_iff_ids: [0; crate::CHARACTER_PARTS],
            part_uids: [0; crate::CHARACTER_PARTS],
            stats: [0; crate::CHARACTER_STATS],
            mastery: 0,
        }
        .encode_body(writer);
        Ok(())
    }
}
/// Retail user guild fan-out response, server opcode `0x015D`.
///
/// PacketDoc defines this response without a request-type byte. Its fixed guild body is emitted
/// with the documented neutral values (including the observed `-1` field and fixed 16-byte tail).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserGuildInfoResponse {
    pub user_id: u32,
}
impl EncodePacket for UserGuildInfoResponse {
    const OPCODE: u16 = 0x015d;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        writer.u32_le(self.user_id);
        writer.u32_le(0);
        writer.fixed_nul(&[], 21)?;
        writer.u32_le(0);
        writer.u32_le(0);
        writer.u32_le(0);
        writer.fixed_nul(&[], 12)?;
        writer.bytes(&[0; 206]);
        writer.u32_le(u32::MAX);
        writer.bytes(&[0; 22]);
        writer.bytes(&[
            0xc3, 0x41, 0x02, 0xf8, 0x28, 0x3a, 0x02, 0x78, 0x23, 0x09, 0x09, 0x60, 0xf1, 0x01,
            0x0b, 0xd0,
        ]);
        Ok(())
    }
}

/// Retail course-record fan-out response, server opcode `0x015C`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserCourseRecordsInfoResponse {
    pub request_type: u8,
    pub user_id: u32,
}
impl EncodePacket for UserCourseRecordsInfoResponse {
    const OPCODE: u16 = 0x015c;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if !matches!(self.request_type, 0 | 5 | 0x0a | 0x0b | 0x33 | 0x34) {
            return Err(PacketEncodeError::Invalid {
                field: "course record request type",
            });
        }
        writer.u8(self.request_type);
        writer.u32_le(self.user_id);
        // PacketDoc's two counted arrays are empty until durable course records are modelled.
        writer.u32_le(0);
        writer.u32_le(0);
        Ok(())
    }
}

/// Retail user-related response, server opcode `0x015B`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserRelatedInfoResponse {
    pub request_type: u8,
    pub user_id: u32,
}
impl EncodePacket for UserRelatedInfoResponse {
    const OPCODE: u16 = 0x015b;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if !matches!(self.request_type, 0 | 5) {
            return Err(PacketEncodeError::Invalid {
                field: "user info request type",
            });
        }
        writer.u8(self.request_type);
        writer.u32_le(self.user_id);
        writer.u16_le(0);
        Ok(())
    }
}

/// Retail special-trophy response, server opcode `0x015A`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserSpecialTrophiesInfoResponse {
    pub request_type: u8,
    pub user_id: u32,
}
impl EncodePacket for UserSpecialTrophiesInfoResponse {
    const OPCODE: u16 = 0x015a;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if !matches!(self.request_type, 0 | 5) {
            return Err(PacketEncodeError::Invalid {
                field: "user info request type",
            });
        }
        writer.u8(self.request_type);
        writer.u32_le(self.user_id);
        writer.u16_le(0);
        Ok(())
    }
}

/// Retail standard-trophy response, server opcode `0x0159`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserTrophiesInfoResponse {
    pub request_type: u8,
    pub user_id: u32,
}
impl EncodePacket for UserTrophiesInfoResponse {
    const OPCODE: u16 = 0x0159;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if !matches!(self.request_type, 0 | 5) {
            return Err(PacketEncodeError::Invalid {
                field: "user info request type",
            });
        }
        writer.u8(self.request_type);
        writer.u32_le(self.user_id);
        writer.bytes(&[0; 13 * 6]);
        Ok(())
    }
}

/// Retail Grand Prix trophy response, server opcode `0x0257`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserGrandPrixTrophiesInfoResponse {
    pub request_type: u8,
    pub user_id: u32,
}
impl EncodePacket for UserGrandPrixTrophiesInfoResponse {
    const OPCODE: u16 = 0x0257;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if !matches!(self.request_type, 0 | 5) {
            return Err(PacketEncodeError::Invalid {
                field: "user info request type",
            });
        }
        writer.u8(self.request_type);
        writer.u32_le(self.user_id);
        writer.u16_le(0);
        Ok(())
    }
}

/// Retail user information acknowledgement, server opcode `0x0089`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserInfoResponse {
    pub status: u32,
    pub request_type: u8,
    pub user_id: u32,
}
impl EncodePacket for UserInfoResponse {
    const OPCODE: u16 = 0x0089;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile_encode(profile)?;
        if !matches!(self.status, 1 | 2) || !matches!(self.request_type, 0 | 5) {
            return Err(PacketEncodeError::Invalid {
                field: "user info response",
            });
        }
        writer.u32_le(self.status);
        writer.u8(self.request_type);
        writer.u32_le(self.user_id);
        Ok(())
    }
}
