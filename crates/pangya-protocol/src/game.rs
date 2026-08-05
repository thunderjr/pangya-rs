use crate::{
    CompatibilityProfile, DecodePacket, EncodePacket, PacketDecodeError, PacketEncodeError,
    PacketReader, PacketWriter,
};
use zeroize::Zeroizing;

/// Maximum handover bearer bytes accepted by the synthetic GameService auth packet.
pub const MAX_GAME_HANDOVER_BYTES: usize = 128;
/// Maximum inventory records in one synthetic bootstrap segment.
pub const GAME_INVENTORY_SEGMENT_ITEMS: usize = 50;

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

fn require_end(reader: &mut PacketReader<'_>) -> Result<(), PacketDecodeError> {
    if reader.remaining() == 0 {
        Ok(())
    } else {
        Err(reader.invalid("synthetic packet has trailing bytes"))
    }
}

/// Builds the four-byte synthetic M3 GameService hello.
///
/// Only the behavioral property that the negotiated key is the final byte is treated as
/// locally attested. The preceding bytes and complete layout remain synthetic.
///
/// # Errors
/// Rejects transport keys outside `0x00..=0x0f`.
pub fn synthetic_game_hello(key: u8) -> Result<[u8; 4], PacketEncodeError> {
    if key > 0x0f {
        return Err(PacketEncodeError::Limit {
            field: "transport key",
            actual: usize::from(key),
            maximum: 15,
        });
    }
    Ok([0, 0, 0, key])
}

/// Synthetic minimal GameService auth packet, client opcode `0x0002`.
#[derive(Clone, Eq, PartialEq)]
pub struct GameAuth {
    /// Untrusted claimed account ID; authoritative identity comes only from handover consume.
    pub claimed_account_id: u64,
    /// Secret login-to-game bearer.
    pub handover: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for GameAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GameAuth")
            .field("claimed_account_id", &self.claimed_account_id)
            .field("handover", &"<redacted>")
            .finish()
    }
}

impl DecodePacket for GameAuth {
    const OPCODE: u16 = 0x0002;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let claimed_account_id = reader.u64_le()?;
        let handover = Zeroizing::new(reader.pstring(MAX_GAME_HANDOVER_BYTES)?.to_vec());
        require_end(reader)?;
        Ok(Self {
            claimed_account_id,
            handover,
        })
    }
}

/// Synthetic minimal channel selection packet, client opcode `0x0004`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectChannel {
    /// Locally configured synthetic channel ID.
    pub channel_id: u32,
}

impl DecodePacket for SelectChannel {
    const OPCODE: u16 = 0x0004;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let channel_id = reader.u32_le()?;
        require_end(reader)?;
        Ok(Self { channel_id })
    }
}

/// Synthetic minimal player/profile bootstrap, server opcode `0x0070`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerInfo {
    /// Authoritative account ID.
    pub account_id: u64,
    /// Persisted nickname bytes.
    pub nickname: Vec<u8>,
    /// Pang balance.
    pub pang: u64,
    /// Point balance.
    pub points: u64,
    /// Experience balance.
    pub experience: u64,
}

impl EncodePacket for PlayerInfo {
    const OPCODE: u16 = 0x0070;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u64_le(self.account_id);
        writer.pstring(&self.nickname, 64)?;
        writer.u64_le(self.pang);
        writer.u64_le(self.points);
        writer.u64_le(self.experience);
        Ok(())
    }
}

/// One synthetic character bootstrap record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterBootstrap {
    /// Durable character ID.
    pub id: u64,
    /// Catalog type ID.
    pub type_id: u32,
}

/// Synthetic character list, server opcode `0x0072`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterInfo {
    /// Bounded owned characters.
    pub characters: Vec<CharacterBootstrap>,
}

impl EncodePacket for CharacterInfo {
    const OPCODE: u16 = 0x0072;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.count_u16(self.characters.len(), 64)?;
        for character in &self.characters {
            writer.u64_le(character.id);
            writer.u32_le(character.type_id);
        }
        Ok(())
    }
}

/// One synthetic inventory bootstrap record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InventoryBootstrap {
    /// Durable inventory ID.
    pub id: u64,
    /// Catalog type ID.
    pub type_id: u32,
    /// Positive quantity.
    pub quantity: u32,
}

/// One segmented synthetic inventory packet, server opcode `0x0073`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventorySegment {
    /// Zero-based segment index.
    pub segment_index: u16,
    /// Total segment count.
    pub segment_count: u16,
    /// At most 50 inventory records.
    pub items: Vec<InventoryBootstrap>,
}

impl EncodePacket for InventorySegment {
    const OPCODE: u16 = 0x0073;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        if self.segment_count == 0 || self.segment_index >= self.segment_count {
            return Err(PacketEncodeError::Limit {
                field: "inventory segment",
                actual: usize::from(self.segment_index),
                maximum: usize::from(self.segment_count.saturating_sub(1)),
            });
        }
        writer.u16_le(self.segment_index);
        writer.u16_le(self.segment_count);
        writer.count_u16(self.items.len(), GAME_INVENTORY_SEGMENT_ITEMS)?;
        for item in &self.items {
            writer.u64_le(item.id);
            writer.u32_le(item.type_id);
            writer.u32_le(item.quantity);
        }
        Ok(())
    }
}

/// Synthetic equipment selection, server opcode `0x004d`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EquipmentInfo {
    /// Selected character ID.
    pub character_id: u64,
    /// Equipped club inventory ID, or zero.
    pub club_item_id: u64,
    /// Equipped ball inventory ID, or zero.
    pub ball_item_id: u64,
    /// Optimistic equipment version.
    pub version: u32,
}

impl EncodePacket for EquipmentInfo {
    const OPCODE: u16 = 0x004d;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u64_le(self.character_id);
        writer.u64_le(self.club_item_id);
        writer.u64_le(self.ball_item_id);
        writer.u32_le(self.version);
        Ok(())
    }
}

/// Synthetic channel entry acknowledgement, server opcode `0x004e`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelJoined {
    /// Entered synthetic channel ID.
    pub channel_id: u32,
}

impl EncodePacket for ChannelJoined {
    const OPCODE: u16 = 0x004e;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.channel_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_hello_key_is_final_byte_and_range_checked() {
        let hello = synthetic_game_hello(9).expect("hello");
        assert_eq!(hello.last(), Some(&9));
        assert!(synthetic_game_hello(16).is_err());
    }

    #[test]
    fn inventory_segment_enforces_fifty_item_cap() {
        let packet = InventorySegment {
            segment_index: 0,
            segment_count: 1,
            items: vec![
                InventoryBootstrap {
                    id: 1,
                    type_id: 2,
                    quantity: 3,
                };
                51
            ],
        };
        let mut writer = PacketWriter::new();
        assert!(
            packet
                .encode(&mut writer, &CompatibilityProfile::US_852)
                .is_err()
        );
    }
}
