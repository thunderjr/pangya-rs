use crate::{
    CompatibilityProfile, DecodePacket, EncodePacket, PacketDecodeError, PacketEncodeError,
    PacketReader, PacketWriter,
};

/// GameService client opcode for a tutorial mission update.
pub const TUTORIAL_MISSION_OPCODE: u16 = 0x00ae;
/// GameService server opcode for tutorial state/reward updates.
pub const TUTORIAL_STATUS_OPCODE: u16 = 0x011f;

fn check_decode(
    profile: &CompatibilityProfile,
    reader: &PacketReader<'_>,
) -> Result<(), PacketDecodeError> {
    profile
        .require_us852()
        .map_err(|error| reader.invalid(error.to_string()))
}

fn check_encode(profile: &CompatibilityProfile) -> Result<(), PacketEncodeError> {
    profile.require_us852().map_err(Into::into)
}

/// U.S. 852 `0x00ae` tutorial mission request.
///
/// The source layout is exactly `u16 Code` followed by `u32 MissionID`; no tail is accepted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TutorialMission {
    /// Client tutorial family/code.
    pub code: u16,
    /// Client mission completion mask.
    pub mission_id: u32,
}

impl DecodePacket for TutorialMission {
    const OPCODE: u16 = TUTORIAL_MISSION_OPCODE;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode(profile, reader)?;
        let packet = Self {
            code: reader.u16_le()?,
            mission_id: reader.u32_le()?,
        };
        if reader.remaining() != 0 {
            return Err(reader.invalid("tutorial mission has trailing bytes"));
        }
        Ok(packet)
    }
}

/// Exact 19-byte login/tutorial-state body sent as server `0x011f`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TutorialStatusLogin {
    /// Tutorial family/code echoed in the login body.
    pub code: u16,
    /// Durable mission mask.
    pub mission_id: u32,
}

impl EncodePacket for TutorialStatusLogin {
    const OPCODE: u16 = TUTORIAL_STATUS_OPCODE;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode(profile)?;
        writer.u16_le(self.code);
        writer.u32_le(self.mission_id);
        writer.u8(0);
        writer.u32_le(u32::from(self.code));
        writer.bytes(&[0; 8]);
        Ok(())
    }
}

/// Exact 6-byte completion body sent as server `0x011f`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TutorialStatusCompletion {
    /// Tutorial family/code in the completion body.
    pub code: u8,
    /// Durable mission mask after applying the mission.
    pub mission_id: u32,
}

impl EncodePacket for TutorialStatusCompletion {
    const OPCODE: u16 = TUTORIAL_STATUS_OPCODE;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode(profile)?;
        writer.u8(self.code);
        writer.u8(1);
        writer.u32_le(self.mission_id);
        Ok(())
    }
}
