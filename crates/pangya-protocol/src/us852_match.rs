//! Reference-derived U.S. 852 retail match packets.
//!
//! Derived from the vendored `pangbox--packetdoc` definitions and corroborated against a
//! GB.852-targeting reference server's observable protocol behavior. **None has been
//! accepted by a real client.** These supersede the synthetic `0x7f20`/`0x7f30` families.

use crate::{
    CompatibilityProfile, DecodePacket, EncodePacket, PacketDecodeError, PacketEncodeError,
    PacketReader, PacketWriter, RetailPlayerData,
};

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

/// One seat in the match roster.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailMatchPlayer {
    /// One-based seat of this player within the match.
    ///
    /// The references disagree on this field and only one reading can be right per client.
    /// `Acrisio-Filho/SuperSS-Dev` (`Server Lib/Game Server/GAME/versus_base.cpp`
    /// `VersusBase::sendInitialData`) writes `p.addInt16(m_ri.numero)`, the room's own number,
    /// and `pangbox/packetdoc` (`common/user_name_data.ksy`) calls the field `room_id`, "0xFFFF
    /// (-1) when not in room". `pangbox/server` (`game/room/room.go`) instead writes the seat,
    /// numbered from one, and that is the reading verified against real clients.
    ///
    /// The seat is what this server sends, because the client files each roster entry into a
    /// per-player array by it: a room number is the same value for every player in the match,
    /// so every entry lands in one slot and every other slot stays empty. The client walks
    /// that array by index when it builds the hole.
    pub slot: u16,
    /// The whole player, as the lobby describes them.
    pub player: RetailPlayerData,
    /// Packed match start time.
    pub start_time: [u8; 16],
}

/// Match roster, server opcode `0x0076`.
///
/// This is what the client builds every player in the hole from, so it carries each of them
/// whole. The subtype chooses between that and a bare timestamp, and the two bodies share no
/// shape: a versus match must send [`Self::Roster`], because a client told "full" and handed a
/// timestamp reads the first byte of it as a player count and then parses the rest of the
/// frame as a player record several kilobytes long. It builds a player out of whatever it
/// finds and dereferences it when the hole loads.
///
/// # Provenance
///
/// Subtypes and both bodies from `pangbox/packetdoc`
/// (`gameservice/server/0076.ksy`, `full_payload` and `minimal_payload`) and `pangbox/server`
/// (`game/packet/server.go` `ServerGameInit`, sent from `game/room/room.go` `handleRoomStartGame`
/// with `GameInitTypeFull` and every player), ISC licensed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetailMatchStart {
    /// Every player in the match, whole. What a versus hole needs.
    Roster(Vec<RetailMatchPlayer>),
    /// A bare timestamp, for the modes that carry no roster.
    TimeOnly {
        /// Packed match start time.
        start_time: [u8; 16],
    },
}

impl EncodePacket for RetailMatchStart {
    const OPCODE: u16 = 0x0076;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        match self {
            Self::Roster(players) => {
                let count = u8::try_from(players.len()).map_err(|_| PacketEncodeError::Limit {
                    field: "match players",
                    actual: players.len(),
                    maximum: usize::from(u8::MAX),
                })?;
                writer.u8(0x00);
                writer.u8(count);
                for seat in players {
                    let entry = writer.as_slice().len();
                    writer.u16_le(seat.slot);
                    seat.player.encode_body(writer)?;
                    writer.bytes(&seat.start_time);
                    // Cards in hand, then that many card records. Zero here, but the byte is
                    // not optional: `Acrisio-Filho/SuperSS-Dev`
                    // (`GAME/versus_base.cpp` `VersusBase::sendInitialData`) writes
                    // `addUint8(count)` after the start time and then the records, so a
                    // client reading a roster without it takes the next entry's first byte as
                    // the count and consumes that many card records out of the entry itself.
                    writer.u8(0);
                    if std::env::var_os("PANGYA_MARK_ROSTER").is_some() {
                        writer.mark_zero_words_from(entry);
                    }
                }
            }
            Self::TimeOnly { start_time } => {
                writer.u8(0x04);
                writer.u32_le(1);
                writer.bytes(start_time);
            }
        }
        Ok(())
    }
}

/// Bytes of one rate table's fixed body.
pub const RATE_TABLE_BYTES: usize = 100;

/// Voice and effect rate table, server opcode `0x0115`.
///
/// Three of these follow `0x0053` at the start of every hole. Their contents drive the
/// client's caddie voice and effect selection; the tables here are the ones a real server was
/// captured sending, carried verbatim because nothing in them is derivable.
///
/// # Provenance
///
/// Opcode from `Acrisio-Filho/SuperSS-Dev`
/// (`Server Lib/Game Server/GAME/versus_base.cpp` `VersusBase::sendRatesOfVersusBase`, which
/// broadcasts three `0x115` frames, each a `addString(name)` followed by a fixed table) and
/// from `hsreina/pangya-server` (`Game.pas`, three `0x0115` frames in `HandlePlayerLoadOk`).
/// The payloads are the captured ones in `pangbox/server` (`game/room/room.go` `startHole`,
/// commented there as "taken from an old packet dump"); `pangbox` labels the opcode `0x0151`,
/// which is the byte-swapped reading and collides with the client's own quest-status opcode,
/// so the two servers that agree on `0x0115` are followed here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailRateTable {
    /// Table name, without its length prefix.
    pub name: &'static [u8],
    /// The table itself.
    pub table: &'static [u8; RATE_TABLE_BYTES],
}

impl RetailRateTable {
    /// The three tables a hole opens with, in the order upstream sends them.
    #[must_use]
    pub const fn hole_tables() -> [Self; 3] {
        [
            Self {
                name: b"W_BIGBONGDARI",
                table: &TABLE_W_BIGBONGDARI,
            },
            Self {
                name: b"R_BIGBONGDARI",
                table: &TABLE_R_BIGBONGDARI,
            },
            Self {
                name: b"CLUBSET_MIRACLE",
                table: &TABLE_CLUBSET_MIRACLE,
            },
        ]
    }
}

impl EncodePacket for RetailRateTable {
    const OPCODE: u16 = 0x0115;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.pstring(self.name, 64)?;
        writer.bytes(self.table);
        Ok(())
    }
}

static TABLE_W_BIGBONGDARI: [u8; RATE_TABLE_BYTES] = [
    0x00, 0x03, 0x01, 0x03, 0x02, 0x03, 0x03, 0x02, 0x00, 0x02, 0x02, 0x02, 0x03, 0x01, 0x01, 0x00,
    0x01, 0x00, 0x03, 0x02, 0x00, 0x00, 0x00, 0x02, 0x03, 0x03, 0x00, 0x01, 0x01, 0x03, 0x00, 0x02,
    0x03, 0x01, 0x03, 0x03, 0x01, 0x02, 0x00, 0x03, 0x00, 0x02, 0x00, 0x00, 0x02, 0x00, 0x03, 0x03,
    0x03, 0x02, 0x02, 0x02, 0x03, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x02, 0x01, 0x01, 0x00,
    0x03, 0x00, 0x01, 0x00, 0x03, 0x03, 0x03, 0x02, 0x00, 0x01, 0x01, 0x02, 0x03, 0x03, 0x01, 0x02,
    0x00, 0x00, 0x02, 0x03, 0x02, 0x00, 0x00, 0x03, 0x02, 0x03, 0x00, 0x03, 0x00, 0x03, 0x02, 0x03,
    0x02, 0x03, 0x00, 0x03,
];

static TABLE_R_BIGBONGDARI: [u8; RATE_TABLE_BYTES] = [
    0x01, 0x02, 0x00, 0x00, 0x01, 0x03, 0x01, 0x00, 0x01, 0x02, 0x02, 0x02, 0x03, 0x03, 0x02, 0x02,
    0x01, 0x01, 0x03, 0x03, 0x00, 0x02, 0x02, 0x02, 0x03, 0x01, 0x02, 0x02, 0x03, 0x00, 0x00, 0x00,
    0x02, 0x03, 0x03, 0x01, 0x02, 0x01, 0x03, 0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02,
    0x03, 0x01, 0x00, 0x02, 0x02, 0x00, 0x02, 0x03, 0x00, 0x03, 0x03, 0x01, 0x03, 0x02, 0x01, 0x02,
    0x03, 0x03, 0x03, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x01, 0x03, 0x00, 0x01, 0x02, 0x00,
    0x00, 0x02, 0x02, 0x03, 0x00, 0x01, 0x02, 0x01, 0x02, 0x01, 0x03, 0x02, 0x01, 0x01, 0x03, 0x01,
    0x02, 0x00, 0x02, 0x01,
];

static TABLE_CLUBSET_MIRACLE: [u8; RATE_TABLE_BYTES] = [
    0x01, 0x01, 0x01, 0x02, 0x02, 0x00, 0x02, 0x01, 0x02, 0x03, 0x01, 0x03, 0x00, 0x02, 0x02, 0x03,
    0x03, 0x01, 0x01, 0x02, 0x02, 0x00, 0x03, 0x02, 0x01, 0x01, 0x01, 0x03, 0x01, 0x00, 0x02, 0x01,
    0x03, 0x03, 0x03, 0x02, 0x01, 0x03, 0x03, 0x03, 0x02, 0x03, 0x01, 0x00, 0x00, 0x03, 0x00, 0x01,
    0x02, 0x00, 0x02, 0x03, 0x02, 0x02, 0x02, 0x00, 0x03, 0x02, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,
    0x01, 0x01, 0x01, 0x01, 0x02, 0x02, 0x03, 0x02, 0x01, 0x00, 0x01, 0x03, 0x03, 0x03, 0x00, 0x03,
    0x02, 0x02, 0x02, 0x03, 0x00, 0x00, 0x02, 0x02, 0x00, 0x00, 0x00, 0x02, 0x01, 0x01, 0x03, 0x01,
    0x00, 0x01, 0x02, 0x00,
];

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
        // Eighteen hole records, always, however few are played: the client reads the array by
        // its fixed width and not by the `hole_count` beside it, so a short one leaves it
        // parsing the seed and the collectible table as hole descriptors. The entries past
        // `hole_count` are zeroed rather than filled, which is what `pangbox/server` sends and
        // what a real client has accepted.
        for index in 0..MAX_MATCH_HOLES {
            self.holes
                .get(index)
                .copied()
                .unwrap_or(RetailHole {
                    random_id: 0,
                    pin: 0,
                    course: 0,
                    number: 0,
                })
                .encode_body(writer);
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

/// Opens the pre-match framing a retail client expects, server opcodes `0x0230` and `0x0231`.
///
/// Both bodies are empty; the client reacts to the opcode alone. They are the first two frames
/// upstream sends once a room starts, ahead of the roster.
///
/// # Provenance
///
/// From `pangbox/server` (`game/packet/server.go` `Server0230`/`Server0231`, sent by
/// `game/room/room.go`), ISC licensed, and `Acrisio-Filho/SuperSS-Dev`
/// (`Server/GameServer/room.cpp` `requestStartGame`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailMatchOpen;

impl EncodePacket for RetailMatchOpen {
    const OPCODE: u16 = 0x0230;

    fn encode(
        &self,
        _writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)
    }
}

/// The second half of the pre-match framing pair, server opcode `0x0231`.
///
/// See [`RetailMatchOpen`] for provenance; this is the frame that follows it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailMatchOpenAck;

impl EncodePacket for RetailMatchOpenAck {
    const OPCODE: u16 = 0x0231;

    fn encode(
        &self,
        _writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)
    }
}

/// The pang multiplier for the match about to start, server opcode `0x0077`.
///
/// A percentage: 100 is the plain rate. Every reference server sends this before the roster.
///
/// # Provenance
///
/// From `pangbox/server` (`game/packet/server.go` `Server0077`, sent with `0x64` by
/// `game/room/room.go`), ISC licensed; `Acrisio-Filho/SuperSS-Dev` sends the configured pang
/// rate here and `hsreina/pangya-server` sends a literal `0x64`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailPangRate {
    /// Percentage applied to pang earned this match.
    pub percent: u32,
}

impl Default for RetailPangRate {
    fn default() -> Self {
        Self { percent: 100 }
    }
}

impl EncodePacket for RetailPangRate {
    const OPCODE: u16 = 0x0077;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.percent);
        Ok(())
    }
}

/// Closes the pre-match framing, server opcode `0x016a`.
///
/// Seeds the client's mascot effects. All three reference servers send it immediately after the
/// match plan; the value is opaque and they disagree on it, so any small one will do.
///
/// # Provenance
///
/// From `pangbox/server` (`game/packet/server.go` `Server016A`, sent by `game/room/room.go`
/// after the plan), ISC licensed; `Acrisio-Filho/SuperSS-Dev` sends a bare random `u32` it
/// calls the mascot effect seed, and `hsreina/pangya-server` a fixed one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailMascotSeed {
    /// Seed the client uses for mascot effects.
    pub seed: u32,
}

impl Default for RetailMascotSeed {
    fn default() -> Self {
        Self { seed: 0x24bd }
    }
}

impl EncodePacket for RetailMascotSeed {
    const OPCODE: u16 = 0x016a;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.seed);
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

/// One player's hole-loading progress, server opcode `0x00a3`.
///
/// The client draws a loading bar per player and waits on all of them, so it has to be told
/// about everyone else's. Upstream broadcasts one of these for every client `0x0048`.
///
/// # Provenance
///
/// From `pangbox/server` (`game/packet/server.go` `ServerPlayerLoadProgress`, broadcast by
/// `game/room/room.go` `handleRoomLoadingProgress`), ISC licensed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailLoadProgress {
    /// Whose progress this is.
    pub connection_id: u32,
    /// How far along they are, as the client reports it.
    pub progress: u8,
}

impl EncodePacket for RetailLoadProgress {
    const OPCODE: u16 = 0x00a3;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.connection_id);
        writer.u8(self.progress);
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
///
/// The byte following the connection ID is a collectable count, not a `u32`. With no field
/// collectables the exact body is therefore five bytes. PacketDoc's `00cc.ksy` and
/// SuperSS-Dev `TourneyBase::sendEndShot` independently agree on that shape.
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
        writer.u8(0); // no field collectables
        Ok(())
    }
}

/// Client-owned post-shot state submitted by Practice as client opcode `0x001b`.
///
/// The full request is the packed 38-byte `ShotSyncData`; only fields needed to produce the
/// Practice response are exposed. SuperSS-Dev defines the exact field order in
/// `TYPE/game_type.hpp:222-350` and reads it in `TourneyBase::requestSyncShot`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetailPracticeShotSyncRequest {
    /// Connection/object ID named by the client.
    pub connection_id: u32,
    /// Ball X coordinate.
    pub x: f32,
    /// Ball Z coordinate.
    pub z: f32,
    /// Coarse lie outcome (`2` playable, `3` out of bounds, `4` holed, `5` unplayable).
    pub state: u8,
    /// Client shot-state bitfield returned by the server.
    pub shot_state: u32,
    /// Client-observed shot duration.
    pub shot_time: u16,
}

impl DecodePacket for RetailPracticeShotSyncRequest {
    const OPCODE: u16 = 0x001b;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile
            .require_us852()
            .map_err(|error| reader.invalid(error.to_string()))?;
        let connection_id = reader.u32_le()?;
        let x = reader.f32_le()?;
        let _y = reader.f32_le()?;
        let z = reader.f32_le()?;
        let state = reader.u8()?;
        let _bunker = reader.u8()?;
        let _unknown = reader.u8()?;
        let _pang = reader.u32_le()?;
        let _bonus_pang = reader.u32_le()?;
        let _display_state = reader.u32_le()?;
        let shot_state = reader.u32_le()?;
        let shot_time = reader.u16_le()?;
        let _grand_prix_penalty = reader.u8()?;
        if !x.is_finite() || !z.is_finite() {
            return Err(reader.invalid("Practice shot coordinates must be finite"));
        }
        if !(2..=5).contains(&state) {
            return Err(reader.invalid("Practice shot state is outside the closed retail set"));
        }
        Ok(Self {
            connection_id,
            x,
            z,
            state,
            shot_state,
            shot_time,
        })
    }
}

/// Practice post-shot state reply, server opcode `0x006e`.
///
/// This is deliberately distinct from versus `0x0064`: Practice/Tourney responds immediately
/// to its sole player with a reduced 19-byte projection. Provenance: SuperSS-Dev
/// `TourneyBase::sendSyncShot` (`tourney_base.cpp:2118-2137`) and PacketDoc `006e.ksy`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetailPracticeShotSync {
    /// Connection/object ID.
    pub connection_id: u32,
    /// One-based hole number.
    pub hole: u8,
    /// Ball X coordinate.
    pub x: f32,
    /// Ball Z coordinate.
    pub z: f32,
    /// Shot-state bitfield.
    pub shot_state: u32,
    /// Client-observed shot duration.
    pub shot_time: u16,
}

impl EncodePacket for RetailPracticeShotSync {
    const OPCODE: u16 = 0x006e;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.connection_id);
        writer.u8(self.hole);
        writer.f32_le(self.x);
        writer.f32_le(self.z);
        writer.u32_le(self.shot_state);
        writer.u16_le(self.shot_time);
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

/// Client aim rotation, opcode `0x0013`.
///
/// # Provenance
///
/// Layout from `pangbox/packetdoc` `gameservice/client/0013.ksy`; the corresponding
/// announcement is emitted by `pangbox/server` `game/room/room.go:580-584` (ISC licensed).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetailAimRotateRequest {
    /// New bearing.
    pub rotation: f32,
}

impl DecodePacket for RetailAimRotateRequest {
    const OPCODE: u16 = 0x0013;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile
            .require_us852()
            .map_err(|error| reader.invalid(error.to_string()))?;
        let rotation = reader.f32_le()?;
        if !rotation.is_finite() || reader.remaining() != 0 {
            return Err(reader.invalid("aim rotation must be finite and exactly four bytes"));
        }
        Ok(Self { rotation })
    }
}

/// Client extra-power selection, opcode `0x0015`.
///
/// # Provenance
///
/// Layout and closed value set from `pangbox/packetdoc` `gameservice/client/0015.ksy`;
/// `pangbox/server` `game/room/room.go:587-592` announces it as `0x0058` (ISC licensed).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailShotPowerRequest {
    /// `0` normal, `1` power, or `2` double power.
    pub level: u8,
}

impl DecodePacket for RetailShotPowerRequest {
    const OPCODE: u16 = 0x0015;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile
            .require_us852()
            .map_err(|error| reader.invalid(error.to_string()))?;
        let level = reader.u8()?;
        if level > 2 || reader.remaining() != 0 {
            return Err(reader.invalid("shot power must be 0, 1, or 2 and exactly one byte"));
        }
        Ok(Self { level })
    }
}

/// Extra-power selection relayed to room members, server opcode `0x0058`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailShotPower {
    /// Who selected the power level.
    pub connection_id: u32,
    /// `0` normal, `1` power, or `2` double power.
    pub level: u8,
}

impl EncodePacket for RetailShotPower {
    const OPCODE: u16 = 0x0058;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.connection_id);
        writer.u8(self.level);
        Ok(())
    }
}

/// Client active-club selection, opcode `0x0016`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailClubChangeRequest {
    /// Zero-based club index, ending with the putter at `13`.
    pub club: u8,
}

impl DecodePacket for RetailClubChangeRequest {
    const OPCODE: u16 = 0x0016;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile
            .require_us852()
            .map_err(|error| reader.invalid(error.to_string()))?;
        let club = reader.u8()?;
        if club > 13 || reader.remaining() != 0 {
            return Err(reader.invalid("club must be in the retail range 0 through 13"));
        }
        Ok(Self { club })
    }
}

/// Active-club selection relayed to room members, server opcode `0x0059`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailClubChange {
    /// Who changed club.
    pub connection_id: u32,
    /// Zero-based club index.
    pub club: u8,
}

impl EncodePacket for RetailClubChange {
    const OPCODE: u16 = 0x0059;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.connection_id);
        writer.u8(self.club);
        Ok(())
    }
}

/// Client in-match item selection, opcode `0x0017`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailItemUseRequest {
    /// The selected item type ID.
    pub item_type_id: u32,
}

impl DecodePacket for RetailItemUseRequest {
    const OPCODE: u16 = 0x0017;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile
            .require_us852()
            .map_err(|error| reader.invalid(error.to_string()))?;
        let item_type_id = reader.u32_le()?;
        if reader.remaining() != 0 {
            return Err(reader.invalid("item-use request has trailing bytes"));
        }
        Ok(Self { item_type_id })
    }
}

/// In-match item selection relayed to room members, server opcode `0x005a`.
///
/// The unknown field is zero in `pangbox/server` `game/packet/server.go:563-568`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailItemUse {
    /// The selected item type ID.
    pub item_type_id: u32,
    /// Who selected the item.
    pub connection_id: u32,
}

impl EncodePacket for RetailItemUse {
    const OPCODE: u16 = 0x005a;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.item_type_id);
        writer.u32_le(0);
        writer.u32_le(self.connection_id);
        Ok(())
    }
}

/// Client comet-relief location, opcode `0x0019`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetailCometReliefRequest {
    /// Client-selected coordinates.
    pub position: [f32; 3],
}

impl DecodePacket for RetailCometReliefRequest {
    const OPCODE: u16 = 0x0019;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        profile
            .require_us852()
            .map_err(|error| reader.invalid(error.to_string()))?;
        let position = [reader.f32_le()?, reader.f32_le()?, reader.f32_le()?];
        if !position.iter().all(|value| value.is_finite()) || reader.remaining() != 0 {
            return Err(
                reader.invalid("comet-relief coordinates must be finite and exactly 12 bytes")
            );
        }
        Ok(Self { position })
    }
}

/// Comet-relief location relayed to room members, server opcode `0x0060`.
///
/// `pangbox/server` includes the authoritative connection ID before the three coordinates in
/// `game/packet/server.go:585-589` and broadcasts it from `game/room/room.go:615-621`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetailCometRelief {
    /// Who selected the relief location.
    pub connection_id: u32,
    /// Client-selected coordinates.
    pub position: [f32; 3],
}

impl EncodePacket for RetailCometRelief {
    const OPCODE: u16 = 0x0060;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.connection_id);
        for coordinate in self.position {
            writer.f32_le(coordinate);
        }
        Ok(())
    }
}

/// Shot relayed to the other players, server opcode `0x0055`.
///
/// The client computes trajectory, so the server relays the committed shot rather than
/// recomputing it. `shot` is the opaque common shot body after removing client `0x0012`'s
/// subtype and optional putt prefix; server `0x0055` carries only connection ID plus that body.
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
    use crate::{ServiceKind, decode_packet_payload, encode_packet_payload};

    fn profile() -> CompatibilityProfile {
        CompatibilityProfile::US_852
    }

    #[test]
    fn practice_sync_decodes_full_client_state_and_projects_exact_replies() {
        let mut request = Vec::new();
        request.extend_from_slice(&77_u32.to_le_bytes());
        request.extend_from_slice(&12.5_f32.to_le_bytes());
        request.extend_from_slice(&3.0_f32.to_le_bytes());
        request.extend_from_slice(&(-8.25_f32).to_le_bytes());
        request.extend_from_slice(&[2, 0, 0]);
        request.extend_from_slice(&11_u32.to_le_bytes());
        request.extend_from_slice(&7_u32.to_le_bytes());
        request.extend_from_slice(&0x100_u32.to_le_bytes());
        request.extend_from_slice(&0x200_u32.to_le_bytes());
        request.extend_from_slice(&345_u16.to_le_bytes());
        request.push(0);
        assert_eq!(request.len(), 38);
        let decoded = decode_packet_payload::<RetailPracticeShotSyncRequest>(
            &request,
            &profile(),
            ServiceKind::Game,
        )
        .expect("decode Practice sync");
        assert_eq!(decoded.connection_id, 77);
        assert_eq!(decoded.x, 12.5);
        assert_eq!(decoded.z, -8.25);
        assert_eq!(decoded.state, 2);
        assert_eq!(decoded.shot_state, 0x200);
        assert_eq!(decoded.shot_time, 345);

        let reply = RetailPracticeShotSync {
            connection_id: decoded.connection_id,
            hole: 1,
            x: decoded.x,
            z: decoded.z,
            shot_state: decoded.shot_state,
            shot_time: decoded.shot_time,
        };
        let payload = encode_packet_payload(&reply, &profile()).expect("encode Practice sync");
        assert_eq!(payload.len(), 19);
        assert_eq!(&payload[..4], &77_u32.to_le_bytes());
        assert_eq!(payload[4], 1);

        let turn_end = encode_packet_payload(&RetailTurnEnd { connection_id: 77 }, &profile())
            .expect("encode Practice end shot");
        assert_eq!(turn_end.as_slice(), [77, 0, 0, 0, 0]);
    }

    #[test]
    fn match_info_always_carries_eighteen_holes_and_collectible_counts() {
        let info = RetailMatchInfo {
            course: 7,
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
        // 4 header bytes, 3 u32 fields, EIGHTEEN holes of 7 bytes whatever `hole_count` says,
        // the seed, then 18 count bytes. The client reads the hole array by its fixed width.
        assert_eq!(
            payload.len(),
            4 + 12 + MAX_MATCH_HOLES * 7 + 4 + MAX_MATCH_HOLES
        );
        assert!(
            payload[payload.len() - MAX_MATCH_HOLES..]
                .iter()
                .all(|b| *b == 0)
        );
        // The entries past `hole_count` are zeroed, as `pangbox/server` sends them.
        for index in 3..MAX_MATCH_HOLES {
            let at = 4 + 12 + index * 7;
            assert_eq!(payload[at..at + 7], [0; 7], "hole {index}");
        }
        // The seed follows all eighteen, not the third.
        let seed_at = 4 + 12 + MAX_MATCH_HOLES * 7;
        assert_eq!(
            u32::from_le_bytes([
                payload[seed_at],
                payload[seed_at + 1],
                payload[seed_at + 2],
                payload[seed_at + 3],
            ]),
            99
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
        assert_eq!(end.as_slice(), &[5, 0, 0, 0, 0]);
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

    // `pangbox/server` `game/room/room.go:580-621` broadcasts these five projections. The
    // request layouts and closed enums are the paired PacketDoc client definitions.
    #[test]
    fn preview_requests_decode_only_the_documented_complete_values() {
        let aim = decode_packet_payload::<RetailAimRotateRequest>(
            &1.25_f32.to_le_bytes(),
            &profile(),
            ServiceKind::Game,
        )
        .expect("aim");
        assert_eq!(aim.rotation, 1.25);
        assert!(
            decode_packet_payload::<RetailAimRotateRequest>(&[0; 3], &profile(), ServiceKind::Game)
                .is_err()
        );
        assert!(
            decode_packet_payload::<RetailShotPowerRequest>(&[3], &profile(), ServiceKind::Game)
                .is_err()
        );
        assert!(
            decode_packet_payload::<RetailClubChangeRequest>(&[14], &profile(), ServiceKind::Game)
                .is_err()
        );
        assert!(
            decode_packet_payload::<RetailItemUseRequest>(&[0; 3], &profile(), ServiceKind::Game)
                .is_err()
        );
        assert!(
            decode_packet_payload::<RetailCometReliefRequest>(
                &[0; 11],
                &profile(),
                ServiceKind::Game
            )
            .is_err()
        );
    }

    #[test]
    fn preview_announcements_prefix_the_authoritative_connection_id() {
        let aim = encode_packet_payload(
            &RetailAimRotate {
                connection_id: 77,
                rotation: 1.25,
            },
            &profile(),
        )
        .expect("aim");
        assert_eq!(aim.as_slice(), &[77, 0, 0, 0, 0, 0, 160, 63]);
        let power = encode_packet_payload(
            &RetailShotPower {
                connection_id: 77,
                level: 2,
            },
            &profile(),
        )
        .expect("power");
        assert_eq!(power.as_slice(), &[77, 0, 0, 0, 2]);
        let club = encode_packet_payload(
            &RetailClubChange {
                connection_id: 77,
                club: 13,
            },
            &profile(),
        )
        .expect("club");
        assert_eq!(club.as_slice(), &[77, 0, 0, 0, 13]);
        let item = encode_packet_payload(
            &RetailItemUse {
                item_type_id: 0x1a2b_3c4d,
                connection_id: 77,
            },
            &profile(),
        )
        .expect("item");
        assert_eq!(
            item.as_slice(),
            &[0x4d, 0x3c, 0x2b, 0x1a, 0, 0, 0, 0, 77, 0, 0, 0]
        );
        let comet = encode_packet_payload(
            &RetailCometRelief {
                connection_id: 77,
                position: [1.0, -2.0, 3.5],
            },
            &profile(),
        )
        .expect("comet relief");
        assert_eq!(
            comet.as_slice(),
            &[77, 0, 0, 0, 0, 0, 128, 63, 0, 0, 0, 192, 0, 0, 96, 64]
        );
    }
}
