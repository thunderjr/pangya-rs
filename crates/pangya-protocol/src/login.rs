use zeroize::Zeroize as _;

use crate::{
    CompatibilityProfile, DecodePacket, PacketDecodeError, PacketEncodeError, PacketReader,
    UnknownBytes,
};

/// Legacy candidate invalid-credential result code; real-client acceptance remains external.
pub const LOGIN_ERROR_INVALID_CREDENTIALS: u32 = 5_100_143;
/// Legacy candidate duplicate-login result code; real-client acceptance remains external.
pub const LOGIN_ERROR_DUPLICATE_CONNECTION: u32 = 5_100_107;

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

/// Builds the 14-byte U.S. 852 LoginService plaintext hello.
///
/// Unknown words remain the source-observed constants. The negotiated key is
/// encoded as a little-endian `u16` at bytes 6..8.
///
/// # Errors
/// Rejects keys outside `0x00..=0x0f`.
pub fn us852_login_hello(key: u8) -> Result<[u8; 14], PacketEncodeError> {
    if key > 0x0f {
        return Err(PacketEncodeError::Limit {
            field: "transport key",
            actual: usize::from(key),
            maximum: 15,
        });
    }
    Ok([0x00, 0x0b, 0, 0, 0, 0, key, 0, 0, 0, 0x75, 0x27, 0, 0])
}

/// LoginService client opcode `0x0001`.
#[derive(Clone, PartialEq, Eq)]
pub struct LoginRequest {
    /// Raw ASCII-compatible username field.
    pub username: Vec<u8>,
    /// Raw legacy transport-secret field; callers must redact it.
    pub password: Vec<u8>,
    /// Unclassified trailing bytes (17 zero bytes observed in U.S. fixtures).
    pub unknown_tail: Vec<u8>,
}
impl Drop for LoginRequest {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
impl std::fmt::Debug for LoginRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoginRequest")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .field("unknown_tail_len", &self.unknown_tail.len())
            .finish()
    }
}
impl DecodePacket for LoginRequest {
    const OPCODE: u16 = 0x0001;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self {
            username: reader.pstring(64)?.to_vec(),
            password: reader.pstring(128)?.to_vec(),
            unknown_tail: reader.unknown_tail().to_vec(),
        })
    }
}
/// LoginService client opcode `0x0003`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectServer {
    /// Source-observed server identifier.
    pub server_id: u16,
    /// Explicit unclassified/unused bytes.
    pub unknown: UnknownBytes<2>,
}
impl DecodePacket for SelectServer {
    const OPCODE: u16 = 0x0003;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self {
            server_id: reader.u16_le()?,
            unknown: UnknownBytes(reader.array()?),
        })
    }
}
/// LoginService client opcode `0x0006`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetNickname {
    /// Raw ASCII-compatible nickname.
    pub nickname: Vec<u8>,
}
impl DecodePacket for SetNickname {
    const OPCODE: u16 = 0x0006;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self {
            nickname: reader.pstring(64)?.to_vec(),
        })
    }
}
/// LoginService client opcode `0x0007`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckNickname {
    /// Raw ASCII-compatible nickname.
    pub nickname: Vec<u8>,
}
impl DecodePacket for CheckNickname {
    const OPCODE: u16 = 0x0007;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self {
            nickname: reader.pstring(64)?.to_vec(),
        })
    }
}
/// LoginService client opcode `0x0008`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectCharacter {
    /// Source-documented character identifier.
    pub character_id: u32,
    /// Source-documented hair color identifier.
    pub hair_color: u16,
}
impl DecodePacket for SelectCharacter {
    const OPCODE: u16 = 0x0008;
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        Ok(Self {
            character_id: reader.u32_le()?,
            hair_color: reader.u16_le()?,
        })
    }
}

/// Successful LoginService result body for opcode `0x0001`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginSuccess {
    /// Raw username.
    pub username: Vec<u8>,
    /// Legacy numeric user identifier.
    pub user_id: u32,
    /// Unclassified source-observed bytes.
    pub unknown: UnknownBytes<14>,
    /// Raw nickname (observed empty after some setup flows).
    pub nickname: Vec<u8>,
}
/// Evidence-backed LoginService result variants for opcode `0x0001`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginResult {
    /// Status `0x00`.
    Success(LoginSuccess),
    /// Status `0xd9` and its observed `0xffff_ffff` constant.
    ///
    /// Upstream documents the first-login order as nickname, then character, then success. A real
    /// U.S. 852 client answers this status by opening a combined creation screen that carries both
    /// a name field and the character roster, and replies with `0x0008` rather than a nickname
    /// packet.
    NeedSetNickname,
    /// Status `0xda` with no body.
    NeedSelectCharacter,
    /// Status `0xe3` with an otherwise opaque numeric error code.
    Error(u32),
}
impl crate::EncodePacket for LoginResult {
    const OPCODE: u16 = 0x0001;
    fn encode(
        &self,
        writer: &mut crate::PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        match self {
            Self::Success(value) => {
                writer.u8(0);
                writer.pstring(&value.username, 64)?;
                writer.u32_le(value.user_id);
                writer.bytes(&value.unknown.0);
                writer.pstring(&value.nickname, 64)?;
            }
            Self::NeedSetNickname => {
                writer.u8(0xd9);
                writer.u32_le(u32::MAX);
            }
            Self::NeedSelectCharacter => writer.u8(0xda),
            Self::Error(code) => {
                writer.u8(0xe3);
                writer.u32_le(*code);
            }
        }
        Ok(())
    }
}
/// Provisional LoginService nickname-check response for opcode `0x000e`.
///
/// Legacy U.S.-service behavior places an opaque little-endian result before the
/// echoed PangYa string. The field's success/error values and exact client
/// acceptance remain capture-gated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NicknameCheckResult {
    /// Opaque legacy result/status value; runtime policy currently uses `0` for available.
    pub unknown_result: u32,
    /// Nickname bytes echoed exactly from the validated request.
    pub nickname: Vec<u8>,
}
impl crate::EncodePacket for NicknameCheckResult {
    const OPCODE: u16 = 0x000e;

    fn encode(
        &self,
        writer: &mut crate::PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.unknown_result);
        writer.pstring(&self.nickname, 64)
    }
}

/// Source-evidenced LoginService game-server entry for opcode `0x0002`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameServerEntry {
    /// Fixed-width display name.
    pub name: Vec<u8>,
    /// Numeric server ID.
    pub id: u32,
    /// Capacity.
    pub max_users: u32,
    /// Current occupancy.
    pub num_users: u32,
    /// Fixed-width IPv4 text.
    pub ip_address: Vec<u8>,
    /// TCP port.
    pub port: u16,
    /// Unclassified two bytes.
    pub unknown2: UnknownBytes<2>,
    /// Region-specific raw flag bytes.
    pub flags: UnknownBytes<2>,
    /// Unclassified six bytes.
    pub unknown3: UnknownBytes<6>,
    /// Source-documented boost bits.
    pub boosts: u16,
    /// Unclassified six bytes.
    pub unknown4: UnknownBytes<6>,
    /// Character icon ID.
    pub char_icon: u16,
}
/// LoginService opcode `0x0002` game-server list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameServerList {
    /// Entries, at most 255.
    pub servers: Vec<GameServerEntry>,
}
impl crate::EncodePacket for GameServerList {
    const OPCODE: u16 = 0x0002;
    fn encode(
        &self,
        w: &mut crate::PacketWriter,
        p: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(p)?;
        let count = u8::try_from(self.servers.len()).map_err(|_| PacketEncodeError::Limit {
            field: "game servers",
            actual: self.servers.len(),
            maximum: 255,
        })?;
        w.u8(count);
        for s in &self.servers {
            w.fixed_nul(&s.name, 40)?;
            w.u32_le(s.id);
            w.u32_le(s.max_users);
            w.u32_le(s.num_users);
            w.fixed_nul(&s.ip_address, 18)?;
            w.u16_le(s.port);
            w.bytes(&s.unknown2.0);
            w.bytes(&s.flags.0);
            w.bytes(&s.unknown3.0);
            w.u16_le(s.boosts);
            w.bytes(&s.unknown4.0);
            w.u16_le(s.char_icon);
        }
        Ok(())
    }
}
/// LoginService opcode `0x0003` game session key.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionKey {
    /// Unclassified prefix.
    pub unknown: UnknownBytes<4>,
    /// Raw session key.
    pub session_key: Vec<u8>,
}
impl Drop for SessionKey {
    fn drop(&mut self) {
        self.session_key.zeroize();
    }
}
impl std::fmt::Debug for SessionKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionKey")
            .field("unknown", &self.unknown)
            .field("session_key", &"<redacted>")
            .finish()
    }
}
impl crate::EncodePacket for SessionKey {
    const OPCODE: u16 = 3;
    fn encode(
        &self,
        w: &mut crate::PacketWriter,
        p: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(p)?;
        w.bytes(&self.unknown.0);
        w.pstring(&self.session_key, 128)
    }
}
/// LoginService opcode `0x0006`, exactly nine fixed-width macros.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMacros {
    /// Nine raw macro values.
    pub values: [Vec<u8>; 9],
}
impl crate::EncodePacket for ChatMacros {
    const OPCODE: u16 = 6;
    fn encode(
        &self,
        w: &mut crate::PacketWriter,
        p: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(p)?;
        for value in &self.values {
            w.fixed_nul(value, 64)?;
        }
        Ok(())
    }
}
/// LoginService opcode `0x0009` safe empty MessageService list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EmptyMessageServerList;
impl crate::EncodePacket for EmptyMessageServerList {
    const OPCODE: u16 = 9;
    fn encode(
        &self,
        w: &mut crate::PacketWriter,
        p: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(p)?;
        w.u8(0);
        Ok(())
    }
}
/// LoginService opcode `0x0010` login key.
#[derive(Clone, PartialEq, Eq)]
pub struct LoginKey {
    /// Raw login key.
    pub login_key: Vec<u8>,
}
impl Drop for LoginKey {
    fn drop(&mut self) {
        self.login_key.zeroize();
    }
}
impl std::fmt::Debug for LoginKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoginKey")
            .field("login_key", &"<redacted>")
            .finish()
    }
}
impl crate::EncodePacket for LoginKey {
    const OPCODE: u16 = 0x10;
    fn encode(
        &self,
        w: &mut crate::PacketWriter,
        p: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(p)?;
        w.pstring(&self.login_key, 128)
    }
}
