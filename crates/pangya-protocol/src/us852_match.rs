//! Reference-derived U.S. 852 retail match packets.
//!
//! Derived from the vendored `pangbox--packetdoc` definitions and corroborated against a
//! GB.852-targeting reference server's observable protocol behavior. **None has been
//! accepted by a real client.** These supersede the synthetic `0x7f20`/`0x7f30` families.

use crate::{CompatibilityProfile, EncodePacket, PacketEncodeError, PacketWriter};

/// Holes a match may carry.
pub const MAX_MATCH_HOLES: usize = 18;

fn check_encode_profile(profile: &CompatibilityProfile) -> Result<(), PacketEncodeError> {
    profile.require_us852().map_err(Into::into)
}

/// Retail per-hole weather.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u16)]
pub enum RetailWeather {
    /// Clear skies.
    #[default]
    Clear = 0,
    /// Overcast.
    Cloudy = 1,
    /// Rain.
    Raining = 2,
}

/// One hole in the match plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailHole {
    /// Per-hole randomiser the client mirrors.
    pub random_id: u32,
    /// Pin placement variant.
    pub pin: u8,
    /// Course ordinal.
    pub course: u8,
    /// Hole number within the course.
    pub number: u8,
}

impl RetailHole {
    fn encode_body(self, writer: &mut PacketWriter) {
        writer.u32_le(self.random_id);
        writer.u8(self.pin);
        writer.u8(self.course);
        writer.u8(self.number);
    }
}

/// Match start notification, server opcode `0x0076`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailMatchStart {
    /// Room mode as the client's UI labels it.
    pub room_ui_type: u8,
    /// Packed server time.
    pub start_time: [u8; 16],
}

impl EncodePacket for RetailMatchStart {
    const OPCODE: u16 = 0x0076;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u8(self.room_ui_type);
        writer.u32_le(1);
        writer.bytes(&self.start_time);
        Ok(())
    }
}

/// Match plan, server opcode `0x0052`.
///
/// Carries the whole hole plan up front; the client uses it to preload courses, so an
/// incomplete plan strands it on the loading screen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailMatchInfo {
    /// Course ordinal.
    pub course: u8,
    /// Room mode as the client's UI labels it.
    pub room_ui_type: u8,
    /// Hole progression mode.
    pub hole_mode: u8,
    /// Holes in this match.
    pub hole_count: u8,
    /// Per-shot timer in milliseconds.
    pub shot_timer_ms: u32,
    /// Whole-game timer in milliseconds.
    pub game_timer_ms: u32,
    /// The hole plan.
    pub holes: Vec<RetailHole>,
    /// Match-wide randomiser seed.
    pub random_seed: u32,
}

impl EncodePacket for RetailMatchInfo {
    const OPCODE: u16 = 0x0052;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        if self.holes.len() > MAX_MATCH_HOLES {
            return Err(PacketEncodeError::Limit {
                field: "match holes",
                actual: self.holes.len(),
                maximum: MAX_MATCH_HOLES,
            });
        }
        writer.u8(self.course);
        writer.u8(self.room_ui_type);
        writer.u8(self.hole_mode);
        writer.u8(self.hole_count);
        writer.u32_le(0); // trophy catalog id
        writer.u32_le(self.shot_timer_ms);
        writer.u32_le(self.game_timer_ms);
        for hole in &self.holes {
            hole.encode_body(writer);
        }
        writer.u32_le(self.random_seed);
        // Per-hole collectible tables. This server places none, but the client still reads
        // one count byte per hole, so all eighteen must be present regardless of hole count.
        for _ in 0..MAX_MATCH_HOLES {
            writer.u8(0);
        }
        Ok(())
    }
}

/// Hole weather, server opcode `0x009e`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetailHoleWeather {
    /// Weather for this hole.
    pub weather: RetailWeather,
}

impl EncodePacket for RetailHoleWeather {
    const OPCODE: u16 = 0x009e;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u16_le(self.weather as u16);
        writer.u8(0);
        Ok(())
    }
}

/// Hole wind, server opcode `0x005b`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetailHoleWind {
    /// Wind strength.
    pub strength: u8,
    /// Wind bearing in degrees.
    pub direction: u16,
}

impl EncodePacket for RetailHoleWind {
    const OPCODE: u16 = 0x005b;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u8(self.strength);
        writer.u8(0); // silent-wind flag
        writer.u16_le(self.direction);
        // 1 sets the value outright; 0 would add to the current wind.
        writer.u8(1);
        Ok(())
    }
}

/// Tells the client to play a player's hole intro, server opcode `0x0053`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailPlayerStartHole {
    /// Whose intro to play.
    pub connection_id: u32,
}

impl EncodePacket for RetailPlayerStartHole {
    const OPCODE: u16 = 0x0053;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.connection_id);
        Ok(())
    }
}

/// Turn handed to a player, server opcode `0x0063`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailTurnStart {
    /// Whose turn it now is.
    pub connection_id: u32,
}

impl EncodePacket for RetailTurnStart {
    const OPCODE: u16 = 0x0063;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.connection_id);
        Ok(())
    }
}

/// Turn relinquished, server opcode `0x00cc`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailTurnEnd {
    /// Whose turn ended.
    pub connection_id: u32,
}

impl EncodePacket for RetailTurnEnd {
    const OPCODE: u16 = 0x00cc;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.connection_id);
        writer.u32_le(0);
        Ok(())
    }
}

/// Aim rotation relayed to the other players, server opcode `0x0056`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetailAimRotate {
    /// Who rotated.
    pub connection_id: u32,
    /// New bearing.
    pub rotation: f32,
}

impl EncodePacket for RetailAimRotate {
    const OPCODE: u16 = 0x0056;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.connection_id);
        writer.f32_le(self.rotation);
        Ok(())
    }
}

/// Shot relayed verbatim to the other players, server opcode `0x0055`.
///
/// The client computes trajectory, so the server relays the committed shot rather than
/// recomputing it. The body stays opaque here for exactly that reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailShotCommitRelay {
    /// Who took the shot.
    pub connection_id: u32,
    /// The client's own shot payload, relayed unchanged.
    pub shot: Vec<u8>,
}

impl EncodePacket for RetailShotCommitRelay {
    const OPCODE: u16 = 0x0055;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.connection_id);
        writer.bytes(&self.shot);
        Ok(())
    }
}

/// Post-shot ball state echoed to every participant, server opcode `0x0064`.
///
/// The client owns ball physics, so the body is the shooting client's own sync payload
/// relayed unchanged. Upstream echoes the first copy it receives to everyone, including the
/// shooter, and this does the same.
///
/// # Provenance
///
/// Opcode and echo behaviour from `pangbox/server` (`game/packet/server.go`
/// `ServerRoomShotSync`, `game/room/room.go` `handleRoomGameShotSync`), ISC licensed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailShotSync {
    /// The client's own sync payload, relayed unchanged.
    pub data: Vec<u8>,
}

impl EncodePacket for RetailShotSync {
    const OPCODE: u16 = 0x0064;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.bytes(&self.data);
        Ok(())
    }
}

/// Players a match result may carry.
pub const MAX_MATCH_STANDINGS: usize = 8;
/// Bytes in one standings record: `u32`, three bytes, `u16`, then three `u64`.
pub const RETAIL_STANDING_BYTES: usize = 33;

/// One player's line on the results screen.
///
/// # Provenance
///
/// Field order and widths from `pangbox/server` (`game/packet/server.go` `PlayerGameResult`),
/// ISC licensed. Two fields are unclassified there and are written as zero here rather than
/// given a guessed meaning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailStanding {
    /// Whose line this is.
    pub connection_id: u32,
    /// One-based finishing place.
    pub place: u8,
    /// Score relative to par.
    pub score: i8,
    /// Experience awarded.
    pub experience: u16,
    /// Pang awarded for play.
    pub pang: u64,
    /// Pang awarded as a completion bonus.
    pub bonus_pang: u64,
}

impl RetailStanding {
    fn encode_body(self, writer: &mut PacketWriter) {
        writer.u32_le(self.connection_id);
        writer.u8(self.place);
        writer.i8(self.score);
        writer.u8(0); // unclassified upstream
        writer.u16_le(self.experience);
        writer.u64_le(self.pang);
        writer.u64_le(self.bonus_pang);
        writer.u64_le(0); // unclassified upstream
    }
}

/// Match complete with final standings, server opcode `0x0066`.
///
/// This is what a real client puts on its results screen, so the values here are the durable
/// server-side settlement — never anything the client claimed.
///
/// # Provenance
///
/// Opcode and layout from `pangbox/server` (`game/packet/server.go` `ServerRoomFinishGame`),
/// ISC licensed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailMatchFinish {
    /// Final standings, best place first.
    pub standings: Vec<RetailStanding>,
}

impl EncodePacket for RetailMatchFinish {
    const OPCODE: u16 = 0x0066;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        if self.standings.len() > MAX_MATCH_STANDINGS {
            return Err(PacketEncodeError::Limit {
                field: "match standings",
                actual: self.standings.len(),
                maximum: MAX_MATCH_STANDINGS,
            });
        }
        let count = u8::try_from(self.standings.len()).map_err(|_| PacketEncodeError::Limit {
            field: "match standings",
            actual: self.standings.len(),
            maximum: MAX_MATCH_STANDINGS,
        })?;
        writer.u8(count);
        for standing in &self.standings {
            standing.encode_body(writer);
        }
        Ok(())
    }
}

/// Hole complete, server opcode `0x0065`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetailFinishHole;

impl EncodePacket for RetailFinishHole {
    const OPCODE: u16 = 0x0065;

    fn encode(
        &self,
        _writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode_packet_payload;

    fn profile() -> CompatibilityProfile {
        CompatibilityProfile::US_852
    }

    #[test]
    fn match_info_always_carries_eighteen_collectible_counts() {
        let info = RetailMatchInfo {
            course: 0,
            room_ui_type: 0,
            hole_mode: 0,
            hole_count: 3,
            shot_timer_ms: 30_000,
            game_timer_ms: 600_000,
            holes: vec![
                RetailHole {
                    random_id: 1,
                    pin: 0,
                    course: 0,
                    number: 1
                };
                3
            ],
            random_seed: 99,
        };
        let payload = encode_packet_payload(&info, &profile()).expect("encode");
        // 4 header bytes, 3 u32 fields, 3 holes of 7 bytes, the seed, then 18 count bytes.
        assert_eq!(payload.len(), 4 + 12 + 3 * 7 + 4 + MAX_MATCH_HOLES);
        assert!(
            payload[payload.len() - MAX_MATCH_HOLES..]
                .iter()
                .all(|b| *b == 0)
        );
    }

    #[test]
    fn match_info_rejects_more_holes_than_a_course_has() {
        let info = RetailMatchInfo {
            course: 0,
            room_ui_type: 0,
            hole_mode: 0,
            hole_count: 19,
            shot_timer_ms: 0,
            game_timer_ms: 0,
            holes: vec![
                RetailHole {
                    random_id: 0,
                    pin: 0,
                    course: 0,
                    number: 1
                };
                19
            ],
            random_seed: 0,
        };
        assert!(encode_packet_payload(&info, &profile()).is_err());
    }

    #[test]
    fn wind_sets_rather_than_accumulates() {
        let payload = encode_packet_payload(
            &RetailHoleWind {
                strength: 7,
                direction: 180,
            },
            &profile(),
        )
        .expect("encode");
        assert_eq!(payload.as_slice(), &[7, 0, 180, 0, 1]);
    }

    #[test]
    fn weather_is_a_two_byte_ordinal() {
        let payload = encode_packet_payload(
            &RetailHoleWeather {
                weather: RetailWeather::Raining,
            },
            &profile(),
        )
        .expect("encode");
        assert_eq!(payload.as_slice(), &[2, 0, 0]);
    }

    #[test]
    fn turn_and_shot_frames_address_players_by_connection() {
        let start = encode_packet_payload(&RetailTurnStart { connection_id: 5 }, &profile())
            .expect("start");
        assert_eq!(start.as_slice(), &[5, 0, 0, 0]);
        let end =
            encode_packet_payload(&RetailTurnEnd { connection_id: 5 }, &profile()).expect("end");
        assert_eq!(end.as_slice(), &[5, 0, 0, 0, 0, 0, 0, 0]);
        let relay = encode_packet_payload(
            &RetailShotCommitRelay {
                connection_id: 5,
                shot: vec![0xaa, 0xbb],
            },
            &profile(),
        )
        .expect("relay");
        assert_eq!(relay.as_slice(), &[5, 0, 0, 0, 0xaa, 0xbb]);
    }

    #[test]
    fn standings_are_counted_then_written_in_place_order() {
        let payload = encode_packet_payload(
            &RetailMatchFinish {
                standings: vec![
                    RetailStanding {
                        connection_id: 1,
                        place: 1,
                        score: -1,
                        experience: 40,
                        pang: 300,
                        bonus_pang: 100,
                    },
                    RetailStanding {
                        connection_id: 2,
                        place: 2,
                        score: 3,
                        experience: 20,
                        pang: 150,
                        bonus_pang: 0,
                    },
                ],
            },
            &profile(),
        )
        .expect("encode");
        // One count byte, then one 33-byte record per player.
        assert_eq!(payload.len(), 1 + 2 * RETAIL_STANDING_BYTES);
        assert_eq!(payload[0], 2);
        assert_eq!(payload[5], 1, "first place");
        assert_eq!(payload[6] as i8, -1, "one under par");
        assert_eq!(
            payload[1 + RETAIL_STANDING_BYTES + 5] as i8,
            3,
            "three over par"
        );
    }

    #[test]
    fn finishing_a_hole_carries_no_body() {
        let payload = encode_packet_payload(&RetailFinishHole, &profile()).expect("encode");
        assert!(payload.is_empty());
    }
}
