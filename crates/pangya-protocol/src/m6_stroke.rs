//! Strict provisional, local-only synthetic M6 two-player stroke packet models.
//!
//! These generated layouts do not claim compatibility with a retail client.

use crate::{
    ClientVersion, CompatibilityProfile, ConnectionState, DecodePacket, Direction, EncodePacket,
    Lie, PacketDecodeError, PacketEncodeError, PacketReader, PacketRegistry, PacketWriter,
    RegistryKey, ServiceKind, Weather, Wind,
};
use std::{fmt, num::NonZeroU32, num::NonZeroU64};
use uuid::Uuid;

/// Provisional C->S start-two-player-stroke opcode.
pub const SYNTHETIC_M6_C2S_START_STROKE_TWO: u16 = 0x7f30;
/// Provisional C->S loading-complete opcode.
pub const SYNTHETIC_M6_C2S_LOADING_COMPLETE: u16 = 0x7f31;
/// Provisional C->S shot-action opcode.
pub const SYNTHETIC_M6_C2S_SHOT_ACTION: u16 = 0x7f32;
/// Provisional C->S shot-result opcode.
pub const SYNTHETIC_M6_C2S_SHOT_RESULT: u16 = 0x7f33;
/// Provisional C->S give-up opcode.
pub const SYNTHETIC_M6_C2S_GIVE_UP: u16 = 0x7f34;

/// Provisional S->C match-started opcode.
pub const SYNTHETIC_M6_S2C_MATCH_STARTED: u16 = 0x7fb0;
/// Provisional S->C phase opcode.
pub const SYNTHETIC_M6_S2C_PHASE: u16 = 0x7fb1;
/// Provisional S->C turn-started opcode.
pub const SYNTHETIC_M6_S2C_TURN_STARTED: u16 = 0x7fb2;
/// Provisional S->C action-relay opcode.
pub const SYNTHETIC_M6_S2C_ACTION_RELAY: u16 = 0x7fb3;
/// Provisional S->C result-relay opcode.
pub const SYNTHETIC_M6_S2C_RESULT_RELAY: u16 = 0x7fb4;
/// Provisional S->C standings opcode.
pub const SYNTHETIC_M6_S2C_STANDINGS: u16 = 0x7fb5;
/// Provisional S->C command-result opcode.
pub const SYNTHETIC_M6_S2C_COMMAND_RESULT: u16 = 0x7fb6;
/// Provisional S->C match-aborted opcode.
pub const SYNTHETIC_M6_S2C_MATCH_ABORTED: u16 = 0x7fb7;
/// Provisional S->C own-balance opcode.
pub const SYNTHETIC_M6_S2C_BALANCE_UPDATE: u16 = 0x7fb8;

const HOLE_ONE: u8 = 1;
const PARTICIPANT_COUNT: u8 = 2;

fn require_end(reader: &PacketReader<'_>) -> Result<(), PacketDecodeError> {
    if reader.remaining() == 0 {
        Ok(())
    } else {
        Err(reader.invalid("synthetic M6 packet has trailing bytes"))
    }
}

fn decode_uuid(reader: &mut PacketReader<'_>) -> Result<Uuid, PacketDecodeError> {
    Ok(Uuid::from_bytes(reader.array::<16>()?))
}

fn decode_non_nil_uuid(
    reader: &mut PacketReader<'_>,
    field: &'static str,
) -> Result<Uuid, PacketDecodeError> {
    let value = decode_uuid(reader)?;
    if value.is_nil() {
        Err(reader.invalid(format!("{field} must be non-nil")))
    } else {
        Ok(value)
    }
}

fn encode_uuid(writer: &mut PacketWriter, value: Uuid) {
    writer.bytes(value.as_bytes());
}

fn decode_nonzero_u32(
    reader: &mut PacketReader<'_>,
    field: &'static str,
) -> Result<NonZeroU32, PacketDecodeError> {
    NonZeroU32::new(reader.u32_le()?)
        .ok_or_else(|| reader.invalid(format!("{field} must be nonzero")))
}

fn decode_nonzero_u64(
    reader: &mut PacketReader<'_>,
    field: &'static str,
) -> Result<NonZeroU64, PacketDecodeError> {
    NonZeroU64::new(reader.u64_le()?)
        .ok_or_else(|| reader.invalid(format!("{field} must be nonzero")))
}

fn validate_f32(
    value: f32,
    minimum: f32,
    maximum: f32,
    field: &'static str,
) -> Result<(), PacketEncodeError> {
    if value.is_finite() && (minimum..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(PacketEncodeError::Invalid { field })
    }
}

fn decode_f32(
    reader: &mut PacketReader<'_>,
    minimum: f32,
    maximum: f32,
    field: &'static str,
) -> Result<f32, PacketDecodeError> {
    let value = reader.f32_le()?;
    if value.is_finite() && (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(reader.invalid(format!("{field} is non-finite or outside its range")))
    }
}

fn decode_lie(reader: &mut PacketReader<'_>) -> Result<Lie, PacketDecodeError> {
    match reader.u8()? {
        0 => Ok(Lie::Tee),
        1 => Ok(Lie::Fairway),
        2 => Ok(Lie::Rough),
        3 => Ok(Lie::Bunker),
        4 => Ok(Lie::Green),
        5 => Ok(Lie::Fringe),
        _ => Err(reader.invalid("unknown lie discriminator")),
    }
}

fn decode_weather(reader: &mut PacketReader<'_>) -> Result<Weather, PacketDecodeError> {
    match reader.u8()? {
        0 => Ok(Weather::Clear),
        1 => Ok(Weather::Cloudy),
        2 => Ok(Weather::Rain),
        _ => Err(reader.invalid("unknown weather discriminator")),
    }
}

/// Empty owner request to start a generated two-player stroke match.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StartStrokeTwo;

impl StartStrokeTwo {
    /// Constructs the empty request.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}
impl DecodePacket for StartStrokeTwo {
    const OPCODE: u16 = SYNTHETIC_M6_C2S_START_STROKE_TWO;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        require_end(reader)?;
        Ok(Self)
    }
}
impl EncodePacket for StartStrokeTwo {
    const OPCODE: u16 = SYNTHETIC_M6_C2S_START_STROKE_TWO;
    fn encode(
        &self,
        _writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        Ok(())
    }
}

/// Loading completion whose only canonical progress is 100.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeLoadingComplete {
    progress: u8,
}
impl StrokeLoadingComplete {
    /// Constructs canonical loading completion.
    ///
    /// # Errors
    /// Rejects any progress other than exactly 100.
    pub const fn new(progress: u8) -> Result<Self, PacketEncodeError> {
        if progress == 100 {
            Ok(Self { progress })
        } else {
            Err(PacketEncodeError::Invalid { field: "progress" })
        }
    }
    /// Returns 100.
    #[must_use]
    pub const fn progress(self) -> u8 {
        self.progress
    }
}
impl DecodePacket for StrokeLoadingComplete {
    const OPCODE: u16 = SYNTHETIC_M6_C2S_LOADING_COMPLETE;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let progress = reader.u8()?;
        if progress != 100 {
            return Err(reader.invalid("loading progress must be exactly 100"));
        }
        require_end(reader)?;
        Ok(Self { progress })
    }
}
impl EncodePacket for StrokeLoadingComplete {
    const OPCODE: u16 = SYNTHETIC_M6_C2S_LOADING_COMPLETE;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        if self.progress != 100 {
            return Err(PacketEncodeError::Invalid { field: "progress" });
        }
        writer.u8(self.progress);
        Ok(())
    }
}

/// Validated M5-compatible shot-action fields in the M6 namespace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeShotAction {
    sequence: NonZeroU32,
    club: u8,
    power: f32,
    angle: f32,
    spin: f32,
    curve: f32,
}
impl StrokeShotAction {
    /// Constructs bounded, finite shot input.
    ///
    /// # Errors
    /// Rejects zero sequence, club above 13, or out-of-range/non-finite floats.
    pub fn new(
        sequence: u32,
        club: u8,
        power: f32,
        angle: f32,
        spin: f32,
        curve: f32,
    ) -> Result<Self, PacketEncodeError> {
        let sequence = NonZeroU32::new(sequence).ok_or(PacketEncodeError::Invalid {
            field: "shot sequence",
        })?;
        if club > 13 {
            return Err(PacketEncodeError::Invalid { field: "club" });
        }
        validate_f32(power, 0.0, 500.0, "power")?;
        validate_f32(angle, -360.0, 360.0, "angle")?;
        validate_f32(spin, -1.0, 1.0, "spin")?;
        validate_f32(curve, -1.0, 1.0, "curve")?;
        Ok(Self {
            sequence,
            club,
            power,
            angle,
            spin,
            curve,
        })
    }
    /// Per-player nonzero sequence.
    #[must_use]
    pub const fn sequence(self) -> u32 {
        self.sequence.get()
    }
    /// Selected club.
    #[must_use]
    pub const fn club(self) -> u8 {
        self.club
    }
    /// Shot power.
    #[must_use]
    pub const fn power(self) -> f32 {
        self.power
    }
    /// Shot angle.
    #[must_use]
    pub const fn angle(self) -> f32 {
        self.angle
    }
    /// Shot spin.
    #[must_use]
    pub const fn spin(self) -> f32 {
        self.spin
    }
    /// Shot curve.
    #[must_use]
    pub const fn curve(self) -> f32 {
        self.curve
    }

    fn decode_fields(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        let sequence = decode_nonzero_u32(reader, "shot sequence")?;
        let club = reader.u8()?;
        if club > 13 {
            return Err(reader.invalid("club is outside 0..=13"));
        }
        Ok(Self {
            sequence,
            club,
            power: decode_f32(reader, 0.0, 500.0, "power")?,
            angle: decode_f32(reader, -360.0, 360.0, "angle")?,
            spin: decode_f32(reader, -1.0, 1.0, "spin")?,
            curve: decode_f32(reader, -1.0, 1.0, "curve")?,
        })
    }
    fn encode_fields(&self, writer: &mut PacketWriter) -> Result<(), PacketEncodeError> {
        if self.club > 13 {
            return Err(PacketEncodeError::Invalid { field: "club" });
        }
        validate_f32(self.power, 0.0, 500.0, "power")?;
        validate_f32(self.angle, -360.0, 360.0, "angle")?;
        validate_f32(self.spin, -1.0, 1.0, "spin")?;
        validate_f32(self.curve, -1.0, 1.0, "curve")?;
        writer.u32_le(self.sequence.get());
        writer.u8(self.club);
        writer.f32_le(self.power);
        writer.f32_le(self.angle);
        writer.f32_le(self.spin);
        writer.f32_le(self.curve);
        Ok(())
    }
}
impl DecodePacket for StrokeShotAction {
    const OPCODE: u16 = SYNTHETIC_M6_C2S_SHOT_ACTION;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self::decode_fields(reader)?;
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for StrokeShotAction {
    const OPCODE: u16 = SYNTHETIC_M6_C2S_SHOT_ACTION;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        self.encode_fields(writer)
    }
}

/// Validated M5-compatible shot-result fields in the M6 namespace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeShotResult {
    sequence: NonZeroU32,
    x: f32,
    y: f32,
    z: f32,
    lie: Lie,
    holed: bool,
}
impl StrokeShotResult {
    /// Constructs a finite, bounded result.
    ///
    /// # Errors
    /// Rejects zero sequence or invalid coordinates.
    pub fn new(
        sequence: u32,
        x: f32,
        y: f32,
        z: f32,
        lie: Lie,
        holed: bool,
    ) -> Result<Self, PacketEncodeError> {
        let sequence = NonZeroU32::new(sequence).ok_or(PacketEncodeError::Invalid {
            field: "shot sequence",
        })?;
        validate_f32(x, -100_000.0, 100_000.0, "x")?;
        validate_f32(y, -100_000.0, 100_000.0, "y")?;
        validate_f32(z, -100_000.0, 100_000.0, "z")?;
        Ok(Self {
            sequence,
            x,
            y,
            z,
            lie,
            holed,
        })
    }
    /// Per-player nonzero sequence.
    #[must_use]
    pub const fn sequence(self) -> u32 {
        self.sequence.get()
    }
    /// X coordinate.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }
    /// Y coordinate.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }
    /// Z coordinate.
    #[must_use]
    pub const fn z(self) -> f32 {
        self.z
    }
    /// Closed lie.
    #[must_use]
    pub const fn lie(self) -> Lie {
        self.lie
    }
    /// Canonical holed flag.
    #[must_use]
    pub const fn holed(self) -> bool {
        self.holed
    }

    fn decode_fields(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        let sequence = decode_nonzero_u32(reader, "shot sequence")?;
        let x = decode_f32(reader, -100_000.0, 100_000.0, "x")?;
        let y = decode_f32(reader, -100_000.0, 100_000.0, "y")?;
        let z = decode_f32(reader, -100_000.0, 100_000.0, "z")?;
        let lie = decode_lie(reader)?;
        let holed = match reader.u8()? {
            0 => false,
            1 => true,
            _ => return Err(reader.invalid("holed flag is not a canonical boolean")),
        };
        Ok(Self {
            sequence,
            x,
            y,
            z,
            lie,
            holed,
        })
    }
    fn encode_fields(&self, writer: &mut PacketWriter) -> Result<(), PacketEncodeError> {
        validate_f32(self.x, -100_000.0, 100_000.0, "x")?;
        validate_f32(self.y, -100_000.0, 100_000.0, "y")?;
        validate_f32(self.z, -100_000.0, 100_000.0, "z")?;
        writer.u32_le(self.sequence.get());
        writer.f32_le(self.x);
        writer.f32_le(self.y);
        writer.f32_le(self.z);
        writer.u8(self.lie as u8);
        writer.u8(u8::from(self.holed));
        Ok(())
    }
}
impl DecodePacket for StrokeShotResult {
    const OPCODE: u16 = SYNTHETIC_M6_C2S_SHOT_RESULT;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self::decode_fields(reader)?;
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for StrokeShotResult {
    const OPCODE: u16 = SYNTHETIC_M6_C2S_SHOT_RESULT;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        self.encode_fields(writer)
    }
}

/// Empty voluntary-forfeit request.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StrokeGiveUp;
impl StrokeGiveUp {
    /// Constructs the empty request.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}
impl DecodePacket for StrokeGiveUp {
    const OPCODE: u16 = SYNTHETIC_M6_C2S_GIVE_UP;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        require_end(reader)?;
        Ok(Self)
    }
}
impl EncodePacket for StrokeGiveUp {
    const OPCODE: u16 = SYNTHETIC_M6_C2S_GIVE_UP;
    fn encode(
        &self,
        _writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        Ok(())
    }
}

/// M6 match-start parameters and authoritative two-connection roster.
#[derive(Clone, PartialEq)]
pub struct StrokeMatchStarted {
    match_id: Uuid,
    course_id: NonZeroU32,
    par: u8,
    seed: [u8; 32],
    weather: Weather,
    wind: Wind,
    load_timeout_ms: NonZeroU32,
    turn_timeout_ms: NonZeroU32,
    game_timeout_ms: NonZeroU32,
    participant_connection_ids: [NonZeroU64; 2],
}
impl StrokeMatchStarted {
    /// Constructs a strict generated match start.
    ///
    /// # Errors
    /// Rejects invalid course/par/wind/timeouts or zero/equal connection IDs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        match_id: Uuid,
        course_id: u32,
        par: u8,
        seed: [u8; 32],
        weather: Weather,
        wind: Wind,
        load_timeout_ms: u32,
        turn_timeout_ms: u32,
        game_timeout_ms: u32,
        participant_1_connection_id: u64,
        participant_2_connection_id: u64,
    ) -> Result<Self, PacketEncodeError> {
        let course_id =
            NonZeroU32::new(course_id).ok_or(PacketEncodeError::Invalid { field: "course ID" })?;
        if !(1..=10).contains(&par) {
            return Err(PacketEncodeError::Invalid { field: "par" });
        }
        validate_f32(wind.speed(), 0.0, 15.0, "wind speed")?;
        if !wind.angle().is_finite() || !(0.0..360.0).contains(&wind.angle()) {
            return Err(PacketEncodeError::Invalid {
                field: "wind angle",
            });
        }
        let load_timeout_ms =
            NonZeroU32::new(load_timeout_ms).ok_or(PacketEncodeError::Invalid {
                field: "load timeout",
            })?;
        let turn_timeout_ms =
            NonZeroU32::new(turn_timeout_ms).ok_or(PacketEncodeError::Invalid {
                field: "turn timeout",
            })?;
        let game_timeout_ms =
            NonZeroU32::new(game_timeout_ms).ok_or(PacketEncodeError::Invalid {
                field: "game timeout",
            })?;
        let first =
            NonZeroU64::new(participant_1_connection_id).ok_or(PacketEncodeError::Invalid {
                field: "participant 1 connection ID",
            })?;
        let second =
            NonZeroU64::new(participant_2_connection_id).ok_or(PacketEncodeError::Invalid {
                field: "participant 2 connection ID",
            })?;
        if first == second {
            return Err(PacketEncodeError::Invalid {
                field: "participant connection IDs",
            });
        }
        Ok(Self {
            match_id,
            course_id,
            par,
            seed,
            weather,
            wind,
            load_timeout_ms,
            turn_timeout_ms,
            game_timeout_ms,
            participant_connection_ids: [first, second],
        })
    }
    /// Match UUID in network byte order on wire.
    #[must_use]
    pub const fn match_id(&self) -> Uuid {
        self.match_id
    }
    /// Nonzero course ID.
    #[must_use]
    pub const fn course_id(&self) -> u32 {
        self.course_id.get()
    }
    /// Fixed hole 1.
    #[must_use]
    pub const fn hole(&self) -> u8 {
        HOLE_ONE
    }
    /// Hole par.
    #[must_use]
    pub const fn par(&self) -> u8 {
        self.par
    }
    /// Explicit access to the secret generated seed.
    #[must_use]
    pub const fn seed(&self) -> &[u8; 32] {
        &self.seed
    }
    /// Weather.
    #[must_use]
    pub const fn weather(&self) -> Weather {
        self.weather
    }
    /// Wind.
    #[must_use]
    pub const fn wind(&self) -> Wind {
        self.wind
    }
    /// Loading timeout.
    #[must_use]
    pub const fn load_timeout_ms(&self) -> u32 {
        self.load_timeout_ms.get()
    }
    /// Turn timeout.
    #[must_use]
    pub const fn turn_timeout_ms(&self) -> u32 {
        self.turn_timeout_ms.get()
    }
    /// Game timeout.
    #[must_use]
    pub const fn game_timeout_ms(&self) -> u32 {
        self.game_timeout_ms.get()
    }
    /// Exactly two authoritative connection IDs.
    #[must_use]
    pub const fn participant_connection_ids(&self) -> [u64; 2] {
        [
            self.participant_connection_ids[0].get(),
            self.participant_connection_ids[1].get(),
        ]
    }
}
impl fmt::Debug for StrokeMatchStarted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StrokeMatchStarted")
            .field("match_id", &self.match_id)
            .field("course_id", &self.course_id)
            .field("hole", &HOLE_ONE)
            .field("par", &self.par)
            .field("seed", &"<redacted>")
            .field("weather", &self.weather)
            .field("wind", &self.wind)
            .field("load_timeout_ms", &self.load_timeout_ms)
            .field("turn_timeout_ms", &self.turn_timeout_ms)
            .field("game_timeout_ms", &self.game_timeout_ms)
            .field(
                "participant_connection_ids",
                &self.participant_connection_ids,
            )
            .finish()
    }
}
impl DecodePacket for StrokeMatchStarted {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_MATCH_STARTED;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let match_id = decode_uuid(reader)?;
        let course_id = decode_nonzero_u32(reader, "course ID")?;
        if reader.u8()? != HOLE_ONE {
            return Err(reader.invalid("hole must be exactly 1"));
        }
        let par = reader.u8()?;
        if !(1..=10).contains(&par) {
            return Err(reader.invalid("par is outside 1..=10"));
        }
        let seed = reader.array::<32>()?;
        let weather = decode_weather(reader)?;
        let speed = decode_f32(reader, 0.0, 15.0, "wind speed")?;
        let angle = reader.f32_le()?;
        if !angle.is_finite() || !(0.0..360.0).contains(&angle) {
            return Err(reader.invalid("wind angle is non-finite or outside 0..360"));
        }
        let load_timeout_ms = decode_nonzero_u32(reader, "load timeout")?;
        let turn_timeout_ms = decode_nonzero_u32(reader, "turn timeout")?;
        let game_timeout_ms = decode_nonzero_u32(reader, "game timeout")?;
        if reader.u8()? != PARTICIPANT_COUNT {
            return Err(reader.invalid("participant count must be exactly 2"));
        }
        let first = decode_nonzero_u64(reader, "participant 1 connection ID")?;
        let second = decode_nonzero_u64(reader, "participant 2 connection ID")?;
        if first == second {
            return Err(reader.invalid("participant connection IDs must be distinct"));
        }
        require_end(reader)?;
        Ok(Self {
            match_id,
            course_id,
            par,
            seed,
            weather,
            wind: Wind::new(speed, angle).map_err(|_| reader.invalid("invalid wind"))?,
            load_timeout_ms,
            turn_timeout_ms,
            game_timeout_ms,
            participant_connection_ids: [first, second],
        })
    }
}
impl EncodePacket for StrokeMatchStarted {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_MATCH_STARTED;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        if !(1..=10).contains(&self.par) {
            return Err(PacketEncodeError::Invalid { field: "par" });
        }
        if self.participant_connection_ids[0] == self.participant_connection_ids[1] {
            return Err(PacketEncodeError::Invalid {
                field: "participant connection IDs",
            });
        }
        validate_f32(self.wind.speed(), 0.0, 15.0, "wind speed")?;
        if !self.wind.angle().is_finite() || !(0.0..360.0).contains(&self.wind.angle()) {
            return Err(PacketEncodeError::Invalid {
                field: "wind angle",
            });
        }
        encode_uuid(writer, self.match_id);
        writer.u32_le(self.course_id.get());
        writer.u8(HOLE_ONE);
        writer.u8(self.par);
        writer.bytes(&self.seed);
        writer.u8(self.weather as u8);
        writer.f32_le(self.wind.speed());
        writer.f32_le(self.wind.angle());
        writer.u32_le(self.load_timeout_ms.get());
        writer.u32_le(self.turn_timeout_ms.get());
        writer.u32_le(self.game_timeout_ms.get());
        writer.u8(PARTICIPANT_COUNT);
        writer.u64_le(self.participant_connection_ids[0].get());
        writer.u64_le(self.participant_connection_ids[1].get());
        Ok(())
    }
}

/// Closed M6 phase discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StrokePhaseKind {
    /// Waiting for both players to load.
    Loading = 0,
    /// Alternating authoritative turns.
    Playing = 1,
    /// Settlement is pending.
    ResultsPending = 2,
    /// Settlement is committed and projected.
    Finished = 3,
}
impl StrokePhaseKind {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::Loading),
            1 => Ok(Self::Playing),
            2 => Ok(Self::ResultsPending),
            3 => Ok(Self::Finished),
            _ => Err(reader.invalid("unknown stroke phase discriminator")),
        }
    }
}

/// Authoritative M6 match phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokePhase {
    match_id: Uuid,
    phase: StrokePhaseKind,
}
impl StrokePhase {
    /// Constructs a phase packet.
    #[must_use]
    pub const fn new(match_id: Uuid, phase: StrokePhaseKind) -> Self {
        Self { match_id, phase }
    }
    /// Match UUID.
    #[must_use]
    pub const fn match_id(self) -> Uuid {
        self.match_id
    }
    /// Phase.
    #[must_use]
    pub const fn phase(self) -> StrokePhaseKind {
        self.phase
    }
}
impl DecodePacket for StrokePhase {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_PHASE;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            match_id: decode_uuid(reader)?,
            phase: StrokePhaseKind::decode(reader)?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for StrokePhase {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_PHASE;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        encode_uuid(writer, self.match_id);
        writer.u8(self.phase as u8);
        Ok(())
    }
}

/// Authoritative active-player turn projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeTurnStarted {
    match_id: Uuid,
    turn_number: NonZeroU32,
    active_connection_id: NonZeroU64,
    required_sequence: NonZeroU32,
    turn_timeout_ms: NonZeroU32,
}
impl StrokeTurnStarted {
    /// Constructs a turn packet with all numeric authority fields nonzero.
    ///
    /// # Errors
    /// Rejects any zero numeric field.
    pub fn new(
        match_id: Uuid,
        turn_number: u32,
        active_connection_id: u64,
        required_sequence: u32,
        turn_timeout_ms: u32,
    ) -> Result<Self, PacketEncodeError> {
        Ok(Self {
            match_id,
            turn_number: NonZeroU32::new(turn_number).ok_or(PacketEncodeError::Invalid {
                field: "turn number",
            })?,
            active_connection_id: NonZeroU64::new(active_connection_id).ok_or(
                PacketEncodeError::Invalid {
                    field: "active connection ID",
                },
            )?,
            required_sequence: NonZeroU32::new(required_sequence).ok_or(
                PacketEncodeError::Invalid {
                    field: "required sequence",
                },
            )?,
            turn_timeout_ms: NonZeroU32::new(turn_timeout_ms).ok_or(
                PacketEncodeError::Invalid {
                    field: "turn timeout",
                },
            )?,
        })
    }
    /// Match UUID.
    #[must_use]
    pub const fn match_id(self) -> Uuid {
        self.match_id
    }
    /// Nonzero global turn number.
    #[must_use]
    pub const fn turn_number(self) -> u32 {
        self.turn_number.get()
    }
    /// Active authoritative connection.
    #[must_use]
    pub const fn active_connection_id(self) -> u64 {
        self.active_connection_id.get()
    }
    /// Active player's required sequence.
    #[must_use]
    pub const fn required_sequence(self) -> u32 {
        self.required_sequence.get()
    }
    /// Turn timeout.
    #[must_use]
    pub const fn turn_timeout_ms(self) -> u32 {
        self.turn_timeout_ms.get()
    }
}
impl DecodePacket for StrokeTurnStarted {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_TURN_STARTED;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            match_id: decode_uuid(reader)?,
            turn_number: decode_nonzero_u32(reader, "turn number")?,
            active_connection_id: decode_nonzero_u64(reader, "active connection ID")?,
            required_sequence: decode_nonzero_u32(reader, "required sequence")?,
            turn_timeout_ms: decode_nonzero_u32(reader, "turn timeout")?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for StrokeTurnStarted {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_TURN_STARTED;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        encode_uuid(writer, self.match_id);
        writer.u32_le(self.turn_number.get());
        writer.u64_le(self.active_connection_id.get());
        writer.u32_le(self.required_sequence.get());
        writer.u32_le(self.turn_timeout_ms.get());
        Ok(())
    }
}

/// Authoritative action relay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeActionRelay {
    connection_id: NonZeroU64,
    action: StrokeShotAction,
}
impl StrokeActionRelay {
    /// Constructs a relay.
    ///
    /// # Errors
    /// Rejects zero connection ID.
    pub fn new(connection_id: u64, action: StrokeShotAction) -> Result<Self, PacketEncodeError> {
        Ok(Self {
            connection_id: NonZeroU64::new(connection_id).ok_or(PacketEncodeError::Invalid {
                field: "connection ID",
            })?,
            action,
        })
    }
    /// Authoritative connection ID.
    #[must_use]
    pub const fn connection_id(self) -> u64 {
        self.connection_id.get()
    }
    /// Action fields.
    #[must_use]
    pub const fn action(self) -> StrokeShotAction {
        self.action
    }
}
impl DecodePacket for StrokeActionRelay {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_ACTION_RELAY;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            connection_id: decode_nonzero_u64(reader, "connection ID")?,
            action: StrokeShotAction::decode_fields(reader)?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for StrokeActionRelay {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_ACTION_RELAY;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u64_le(self.connection_id.get());
        self.action.encode_fields(writer)
    }
}

/// Authoritative result relay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeResultRelay {
    connection_id: NonZeroU64,
    result: StrokeShotResult,
}
impl StrokeResultRelay {
    /// Constructs a relay.
    ///
    /// # Errors
    /// Rejects zero connection ID.
    pub fn new(connection_id: u64, result: StrokeShotResult) -> Result<Self, PacketEncodeError> {
        Ok(Self {
            connection_id: NonZeroU64::new(connection_id).ok_or(PacketEncodeError::Invalid {
                field: "connection ID",
            })?,
            result,
        })
    }
    /// Authoritative connection ID.
    #[must_use]
    pub const fn connection_id(self) -> u64 {
        self.connection_id.get()
    }
    /// Result fields.
    #[must_use]
    pub const fn result(self) -> StrokeShotResult {
        self.result
    }
}
impl DecodePacket for StrokeResultRelay {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_RESULT_RELAY;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            connection_id: decode_nonzero_u64(reader, "connection ID")?,
            result: StrokeShotResult::decode_fields(reader)?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for StrokeResultRelay {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_RESULT_RELAY;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u64_le(self.connection_id.get());
        self.result.encode_fields(writer)
    }
}

/// Closed standing completion discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StrokeCompletion {
    /// Player holed out.
    Holed = 0,
    /// Player reached the configured stroke cap.
    StrokeCap = 1,
    /// Player voluntarily gave up.
    GiveUp = 2,
    /// Player disconnected during play.
    Disconnect = 3,
    /// Active player's turn deadline expired.
    TurnTimeout = 4,
    /// Aggregate game deadline expired.
    GameTimeout = 5,
}
impl StrokeCompletion {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::Holed),
            1 => Ok(Self::StrokeCap),
            2 => Ok(Self::GiveUp),
            3 => Ok(Self::Disconnect),
            4 => Ok(Self::TurnTimeout),
            5 => Ok(Self::GameTimeout),
            _ => Err(reader.invalid("unknown stroke completion discriminator")),
        }
    }
    const fn has_golf_score(self) -> bool {
        matches!(self, Self::Holed | Self::StrokeCap)
    }
}

/// One authoritative standing entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeStandingEntry {
    connection_id: NonZeroU64,
    place: u8,
    completion: StrokeCompletion,
    strokes: u16,
    score: Option<i16>,
    pang: u64,
    experience: u64,
    player_result_id: Uuid,
}
impl StrokeStandingEntry {
    /// Constructs one canonical entry.
    ///
    /// # Errors
    /// Rejects zero connection ID, place outside 1..=2, score/completion mismatch,
    /// zero non-forfeit strokes, or a nil player result UUID.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        connection_id: u64,
        place: u8,
        completion: StrokeCompletion,
        strokes: u16,
        score: Option<i16>,
        pang: u64,
        experience: u64,
        player_result_id: Uuid,
    ) -> Result<Self, PacketEncodeError> {
        let connection_id = NonZeroU64::new(connection_id).ok_or(PacketEncodeError::Invalid {
            field: "connection ID",
        })?;
        if !(1..=2).contains(&place) {
            return Err(PacketEncodeError::Invalid { field: "place" });
        }
        if completion.has_golf_score() != score.is_some() {
            return Err(PacketEncodeError::Invalid {
                field: "optional score",
            });
        }
        if completion.has_golf_score() && strokes == 0 {
            return Err(PacketEncodeError::Invalid { field: "strokes" });
        }
        if !completion.has_golf_score() && (pang != 0 || experience != 0) {
            return Err(PacketEncodeError::Invalid {
                field: "forfeit rewards",
            });
        }
        if player_result_id.is_nil() {
            return Err(PacketEncodeError::Invalid {
                field: "player result ID",
            });
        }
        Ok(Self {
            connection_id,
            place,
            completion,
            strokes,
            score,
            pang,
            experience,
            player_result_id,
        })
    }
    /// Connection ID.
    #[must_use]
    pub const fn connection_id(self) -> u64 {
        self.connection_id.get()
    }
    /// Unique place.
    #[must_use]
    pub const fn place(self) -> u8 {
        self.place
    }
    /// Completion status.
    #[must_use]
    pub const fn completion(self) -> StrokeCompletion {
        self.completion
    }
    /// Authoritative strokes.
    #[must_use]
    pub const fn strokes(self) -> u16 {
        self.strokes
    }
    /// Golf score, absent for forfeits.
    #[must_use]
    pub const fn score(self) -> Option<i16> {
        self.score
    }
    /// Pang award.
    #[must_use]
    pub const fn pang(self) -> u64 {
        self.pang
    }
    /// Experience award.
    #[must_use]
    pub const fn experience(self) -> u64 {
        self.experience
    }
    /// Distinct player settlement UUID.
    #[must_use]
    pub const fn player_result_id(self) -> Uuid {
        self.player_result_id
    }

    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        let connection_id = decode_nonzero_u64(reader, "connection ID")?;
        let place = reader.u8()?;
        if !(1..=2).contains(&place) {
            return Err(reader.invalid("place is outside 1..=2"));
        }
        let completion = StrokeCompletion::decode(reader)?;
        let strokes = reader.u16_le()?;
        let has_score = match reader.u8()? {
            0 => false,
            1 => true,
            _ => return Err(reader.invalid("has-score flag is not a canonical boolean")),
        };
        let score_wire = reader.i16_le()?;
        if !has_score && score_wire != 0 {
            return Err(reader.invalid("absent score must have canonical zero payload"));
        }
        if completion.has_golf_score() != has_score {
            return Err(reader.invalid("score presence does not match completion"));
        }
        if completion.has_golf_score() && strokes == 0 {
            return Err(reader.invalid("non-forfeit strokes must be nonzero"));
        }
        let pang = reader.u64_le()?;
        let experience = reader.u64_le()?;
        if !completion.has_golf_score() && (pang != 0 || experience != 0) {
            return Err(reader.invalid("forfeit rewards must be zero"));
        }
        let player_result_id = decode_non_nil_uuid(reader, "player result ID")?;
        Ok(Self {
            connection_id,
            place,
            completion,
            strokes,
            score: has_score.then_some(score_wire),
            pang,
            experience,
            player_result_id,
        })
    }
    fn encode(&self, writer: &mut PacketWriter) -> Result<(), PacketEncodeError> {
        if !(1..=2).contains(&self.place) {
            return Err(PacketEncodeError::Invalid { field: "place" });
        }
        if self.completion.has_golf_score() != self.score.is_some() {
            return Err(PacketEncodeError::Invalid {
                field: "optional score",
            });
        }
        if self.completion.has_golf_score() && self.strokes == 0 {
            return Err(PacketEncodeError::Invalid { field: "strokes" });
        }
        if !self.completion.has_golf_score() && (self.pang != 0 || self.experience != 0) {
            return Err(PacketEncodeError::Invalid {
                field: "forfeit rewards",
            });
        }
        if self.player_result_id.is_nil() {
            return Err(PacketEncodeError::Invalid {
                field: "player result ID",
            });
        }
        writer.u64_le(self.connection_id.get());
        writer.u8(self.place);
        writer.u8(self.completion as u8);
        writer.u16_le(self.strokes);
        writer.u8(u8::from(self.score.is_some()));
        writer.i16_le(self.score.unwrap_or(0));
        writer.u64_le(self.pang);
        writer.u64_le(self.experience);
        encode_uuid(writer, self.player_result_id);
        Ok(())
    }
}

/// Closed, committed two-entry standings in place order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeStandings {
    match_id: Uuid,
    entries: [StrokeStandingEntry; 2],
}
impl StrokeStandings {
    /// Constructs standings with place 1 then 2 and distinct identities.
    ///
    /// # Errors
    /// Rejects wrong place order or repeated connection/result identity.
    pub fn new(
        match_id: Uuid,
        entries: [StrokeStandingEntry; 2],
    ) -> Result<Self, PacketEncodeError> {
        if entries[0].place != 1 || entries[1].place != 2 {
            return Err(PacketEncodeError::Invalid {
                field: "standing place order",
            });
        }
        if entries[0].connection_id == entries[1].connection_id {
            return Err(PacketEncodeError::Invalid {
                field: "standing connection IDs",
            });
        }
        if entries[0].player_result_id == entries[1].player_result_id {
            return Err(PacketEncodeError::Invalid {
                field: "player result IDs",
            });
        }
        Ok(Self { match_id, entries })
    }
    /// Match UUID.
    #[must_use]
    pub const fn match_id(self) -> Uuid {
        self.match_id
    }
    /// Exactly two entries in place order.
    #[must_use]
    pub const fn entries(self) -> [StrokeStandingEntry; 2] {
        self.entries
    }
}
impl DecodePacket for StrokeStandings {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_STANDINGS;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let match_id = decode_uuid(reader)?;
        if reader.u8()? != PARTICIPANT_COUNT {
            return Err(reader.invalid("standing entry count must be exactly 2"));
        }
        let entries = [
            StrokeStandingEntry::decode(reader)?,
            StrokeStandingEntry::decode(reader)?,
        ];
        if entries[0].place != 1 || entries[1].place != 2 {
            return Err(reader.invalid("standings must be in unique place order"));
        }
        if entries[0].connection_id == entries[1].connection_id {
            return Err(reader.invalid("standing connection IDs must be distinct"));
        }
        if entries[0].player_result_id == entries[1].player_result_id {
            return Err(reader.invalid("player result IDs must be distinct"));
        }
        require_end(reader)?;
        Ok(Self { match_id, entries })
    }
}
impl EncodePacket for StrokeStandings {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_STANDINGS;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        if self.entries[0].place != 1 || self.entries[1].place != 2 {
            return Err(PacketEncodeError::Invalid {
                field: "standing place order",
            });
        }
        if self.entries[0].connection_id == self.entries[1].connection_id {
            return Err(PacketEncodeError::Invalid {
                field: "standing connection IDs",
            });
        }
        if self.entries[0].player_result_id == self.entries[1].player_result_id {
            return Err(PacketEncodeError::Invalid {
                field: "player result IDs",
            });
        }
        encode_uuid(writer, self.match_id);
        writer.u8(PARTICIPANT_COUNT);
        self.entries[0].encode(writer)?;
        self.entries[1].encode(writer)
    }
}

/// Closed M6 command discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StrokeCommand {
    /// Start the match.
    Start = 0,
    /// Report loading complete.
    Load = 1,
    /// Submit a shot action.
    Action = 2,
    /// Submit a shot result.
    Result = 3,
    /// Voluntarily give up.
    GiveUp = 4,
}
impl StrokeCommand {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::Start),
            1 => Ok(Self::Load),
            2 => Ok(Self::Action),
            3 => Ok(Self::Result),
            4 => Ok(Self::GiveUp),
            _ => Err(reader.invalid("unknown stroke command discriminator")),
        }
    }
}

/// Closed M6 command outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StrokeCommandOutcome {
    /// Command succeeded.
    Success = 0,
    /// Per-player sequence was invalid.
    InvalidSequence = 1,
    /// Shot fields were invalid.
    InvalidAction = 2,
    /// Command was invalid in the current phase.
    InvalidPhase = 3,
    /// Sender was not the active player.
    InvalidTurn = 4,
    /// Command missed its authoritative deadline.
    Timeout = 5,
}
impl StrokeCommandOutcome {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::Success),
            1 => Ok(Self::InvalidSequence),
            2 => Ok(Self::InvalidAction),
            3 => Ok(Self::InvalidPhase),
            4 => Ok(Self::InvalidTurn),
            5 => Ok(Self::Timeout),
            _ => Err(reader.invalid("unknown stroke command outcome discriminator")),
        }
    }
}

/// Result of one M6 client command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeCommandResult {
    command: StrokeCommand,
    outcome: StrokeCommandOutcome,
}
impl StrokeCommandResult {
    /// Constructs a closed command result.
    #[must_use]
    pub const fn new(command: StrokeCommand, outcome: StrokeCommandOutcome) -> Self {
        Self { command, outcome }
    }
    /// Command.
    #[must_use]
    pub const fn command(self) -> StrokeCommand {
        self.command
    }
    /// Outcome.
    #[must_use]
    pub const fn outcome(self) -> StrokeCommandOutcome {
        self.outcome
    }
}
impl DecodePacket for StrokeCommandResult {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_COMMAND_RESULT;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            command: StrokeCommand::decode(reader)?,
            outcome: StrokeCommandOutcome::decode(reader)?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for StrokeCommandResult {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_COMMAND_RESULT;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u8(self.command as u8);
        writer.u8(self.outcome as u8);
        Ok(())
    }
}

/// Closed M6 aggregate-abort reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StrokeAbortReason {
    /// Loading barrier deadline expired.
    LoadingTimeout = 0,
    /// A participant disconnected while loading.
    LoadingDisconnect = 1,
    /// Server shutdown preempted settlement.
    ServerShutdown = 2,
    /// Durable reservation or settlement failed.
    PersistenceFailure = 3,
    /// Startup recovery aborted an incomplete aggregate.
    StartupRecovery = 4,
}
impl StrokeAbortReason {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::LoadingTimeout),
            1 => Ok(Self::LoadingDisconnect),
            2 => Ok(Self::ServerShutdown),
            3 => Ok(Self::PersistenceFailure),
            4 => Ok(Self::StartupRecovery),
            _ => Err(reader.invalid("unknown stroke abort reason discriminator")),
        }
    }
}

/// Terminal M6 abort notification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeMatchAborted {
    match_id: Uuid,
    reason: StrokeAbortReason,
}
impl StrokeMatchAborted {
    /// Constructs an abort notification.
    #[must_use]
    pub const fn new(match_id: Uuid, reason: StrokeAbortReason) -> Self {
        Self { match_id, reason }
    }
    /// Match UUID.
    #[must_use]
    pub const fn match_id(self) -> Uuid {
        self.match_id
    }
    /// Abort reason.
    #[must_use]
    pub const fn reason(self) -> StrokeAbortReason {
        self.reason
    }
}
impl DecodePacket for StrokeMatchAborted {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_MATCH_ABORTED;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            match_id: decode_uuid(reader)?,
            reason: StrokeAbortReason::decode(reader)?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for StrokeMatchAborted {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_MATCH_ABORTED;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        encode_uuid(writer, self.match_id);
        writer.u8(self.reason as u8);
        Ok(())
    }
}

/// Authoritative balance projection for only the receiving participant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeBalanceUpdate {
    pang: u64,
    experience: u64,
}
impl StrokeBalanceUpdate {
    /// Constructs an own-balance update.
    #[must_use]
    pub const fn new(pang: u64, experience: u64) -> Self {
        Self { pang, experience }
    }
    /// Pang balance.
    #[must_use]
    pub const fn pang(self) -> u64 {
        self.pang
    }
    /// Experience balance.
    #[must_use]
    pub const fn experience(self) -> u64 {
        self.experience
    }
}
impl DecodePacket for StrokeBalanceUpdate {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_BALANCE_UPDATE;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            pang: reader.u64_le()?,
            experience: reader.u64_le()?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for StrokeBalanceUpdate {
    const OPCODE: u16 = SYNTHETIC_M6_S2C_BALANCE_UPDATE;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u64_le(self.pang);
        writer.u64_le(self.experience);
        Ok(())
    }
}

/// Builds the strict M6 inbound registry for one client version.
#[must_use]
pub fn synthetic_m6_registry(version: ClientVersion) -> PacketRegistry {
    let mut registry = PacketRegistry::new();
    for (state, opcode) in [
        (ConnectionState::InRoom, SYNTHETIC_M6_C2S_START_STROKE_TWO),
        (
            ConnectionState::InMatchLoading,
            SYNTHETIC_M6_C2S_LOADING_COMPLETE,
        ),
        (ConnectionState::InMatch, SYNTHETIC_M6_C2S_SHOT_ACTION),
        (ConnectionState::InMatch, SYNTHETIC_M6_C2S_SHOT_RESULT),
        (ConnectionState::InMatch, SYNTHETIC_M6_C2S_GIVE_UP),
    ] {
        registry.register(RegistryKey {
            service: ServiceKind::Game,
            direction: Direction::ClientToServer,
            version,
            state,
            opcode,
        });
    }
    registry
}
