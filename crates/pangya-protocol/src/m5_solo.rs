//! Strict provisional, local-only synthetic M5 solo-match packet models.
//!
//! These fixed layouts are generated for local integration and do not claim
//! compatibility with any retail client protocol.

use crate::{
    ClientVersion, CompatibilityProfile, ConnectionState, DecodePacket, Direction, EncodePacket,
    PacketDecodeError, PacketEncodeError, PacketReader, PacketRegistry, PacketWriter, RegistryKey,
    ServiceKind,
};
use std::{fmt, num::NonZeroU32, num::NonZeroU64};
use uuid::Uuid;

/// Provisional C->S start-solo opcode.
pub const SYNTHETIC_M5_C2S_START_SOLO: u16 = 0x7f20;
/// Provisional C->S loading-complete opcode.
pub const SYNTHETIC_M5_C2S_LOADING_COMPLETE: u16 = 0x7f21;
/// Provisional C->S shot-action opcode.
pub const SYNTHETIC_M5_C2S_SHOT_ACTION: u16 = 0x7f22;
/// Provisional C->S shot-result opcode.
pub const SYNTHETIC_M5_C2S_SHOT_RESULT: u16 = 0x7f23;
/// Provisional C->S finish-hole opcode.
pub const SYNTHETIC_M5_C2S_FINISH_HOLE: u16 = 0x7f24;

/// Provisional S->C match-started opcode.
pub const SYNTHETIC_M5_S2C_MATCH_STARTED: u16 = 0x7fa0;
/// Provisional S->C match-phase opcode.
pub const SYNTHETIC_M5_S2C_MATCH_PHASE: u16 = 0x7fa1;
/// Provisional S->C authoritative shot-action relay opcode.
pub const SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY: u16 = 0x7fa2;
/// Provisional S->C authoritative shot-result relay opcode.
pub const SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY: u16 = 0x7fa3;
/// Provisional S->C hole-result opcode.
pub const SYNTHETIC_M5_S2C_HOLE_RESULT: u16 = 0x7fa4;
/// Provisional S->C balance-update opcode.
pub const SYNTHETIC_M5_S2C_BALANCE_UPDATE: u16 = 0x7fa5;
/// Provisional S->C command-result opcode.
pub const SYNTHETIC_M5_S2C_COMMAND_RESULT: u16 = 0x7fa6;
/// Provisional S->C match-aborted opcode.
pub const SYNTHETIC_M5_S2C_MATCH_ABORTED: u16 = 0x7fa7;

const HOLE_ONE: u8 = 1;

fn require_end(reader: &PacketReader<'_>) -> Result<(), PacketDecodeError> {
    if reader.remaining() == 0 {
        Ok(())
    } else {
        Err(reader.invalid("synthetic M5 packet has trailing bytes"))
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

fn decode_f32_inclusive(
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

fn encode_f32_inclusive(
    writer: &mut PacketWriter,
    value: f32,
    minimum: f32,
    maximum: f32,
    field: &'static str,
) -> Result<(), PacketEncodeError> {
    if !value.is_finite() || !(minimum..=maximum).contains(&value) {
        return Err(PacketEncodeError::Invalid { field });
    }
    writer.f32_le(value);
    Ok(())
}

fn decode_uuid(reader: &mut PacketReader<'_>) -> Result<Uuid, PacketDecodeError> {
    Ok(Uuid::from_bytes(reader.array::<16>()?))
}

fn encode_uuid(writer: &mut PacketWriter, value: Uuid) {
    writer.bytes(value.as_bytes());
}

/// Empty request to start the room owner's local solo match.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StartSolo;

impl StartSolo {
    /// Constructs the empty request.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl DecodePacket for StartSolo {
    const OPCODE: u16 = SYNTHETIC_M5_C2S_START_SOLO;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        require_end(reader)?;
        Ok(Self)
    }
}

impl EncodePacket for StartSolo {
    const OPCODE: u16 = SYNTHETIC_M5_C2S_START_SOLO;

    fn encode(
        &self,
        _writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        Ok(())
    }
}

/// Loading completion; the only accepted progress value is exactly 100.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadingComplete {
    progress: u8,
}

impl LoadingComplete {
    /// Validates loading progress.
    ///
    /// # Errors
    /// Returns an encode error unless `progress` is exactly 100.
    pub const fn new(progress: u8) -> Result<Self, PacketEncodeError> {
        if progress == 100 {
            Ok(Self { progress })
        } else {
            Err(PacketEncodeError::Invalid { field: "progress" })
        }
    }

    /// Returns the canonical progress value.
    #[must_use]
    pub const fn progress(self) -> u8 {
        self.progress
    }
}

impl DecodePacket for LoadingComplete {
    const OPCODE: u16 = SYNTHETIC_M5_C2S_LOADING_COMPLETE;

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

impl EncodePacket for LoadingComplete {
    const OPCODE: u16 = SYNTHETIC_M5_C2S_LOADING_COMPLETE;

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

/// Validated client shot input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotAction {
    sequence: NonZeroU32,
    club: u8,
    power: f32,
    angle: f32,
    spin: f32,
    curve: f32,
}

impl ShotAction {
    /// Constructs a bounded, finite shot action.
    ///
    /// # Errors
    /// Returns a stable field error for a zero sequence, unsupported club, or invalid float.
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

    /// Client action sequence.
    #[must_use]
    pub const fn sequence(self) -> u32 {
        self.sequence.get()
    }
    /// Selected club (`0..=13`).
    #[must_use]
    pub const fn club(self) -> u8 {
        self.club
    }
    /// Shot power (`0..=500`).
    #[must_use]
    pub const fn power(self) -> f32 {
        self.power
    }
    /// Shot angle (`-360..=360`).
    #[must_use]
    pub const fn angle(self) -> f32 {
        self.angle
    }
    /// Shot spin (`-1..=1`).
    #[must_use]
    pub const fn spin(self) -> f32 {
        self.spin
    }
    /// Shot curve (`-1..=1`).
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
        let power = decode_f32_inclusive(reader, 0.0, 500.0, "power")?;
        let angle = decode_f32_inclusive(reader, -360.0, 360.0, "angle")?;
        let spin = decode_f32_inclusive(reader, -1.0, 1.0, "spin")?;
        let curve = decode_f32_inclusive(reader, -1.0, 1.0, "curve")?;
        Ok(Self {
            sequence,
            club,
            power,
            angle,
            spin,
            curve,
        })
    }

    fn encode_fields(&self, writer: &mut PacketWriter) -> Result<(), PacketEncodeError> {
        if self.club > 13 {
            return Err(PacketEncodeError::Invalid { field: "club" });
        }
        writer.u32_le(self.sequence.get());
        writer.u8(self.club);
        encode_f32_inclusive(writer, self.power, 0.0, 500.0, "power")?;
        encode_f32_inclusive(writer, self.angle, -360.0, 360.0, "angle")?;
        encode_f32_inclusive(writer, self.spin, -1.0, 1.0, "spin")?;
        encode_f32_inclusive(writer, self.curve, -1.0, 1.0, "curve")
    }
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

impl DecodePacket for ShotAction {
    const OPCODE: u16 = SYNTHETIC_M5_C2S_SHOT_ACTION;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self::decode_fields(reader)?;
        require_end(reader)?;
        Ok(value)
    }
}

impl EncodePacket for ShotAction {
    const OPCODE: u16 = SYNTHETIC_M5_C2S_SHOT_ACTION;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        self.encode_fields(writer)
    }
}

/// Closed lie discriminator for a reported ball position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Lie {
    /// Tee box.
    Tee = 0,
    /// Fairway.
    Fairway = 1,
    /// Rough.
    Rough = 2,
    /// Bunker.
    Bunker = 3,
    /// Green.
    Green = 4,
    /// Fringe.
    Fringe = 5,
}

impl Lie {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::Tee),
            1 => Ok(Self::Fairway),
            2 => Ok(Self::Rough),
            3 => Ok(Self::Bunker),
            4 => Ok(Self::Green),
            5 => Ok(Self::Fringe),
            _ => Err(reader.invalid("unknown lie discriminator")),
        }
    }
}

/// Validated client report for one shot result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotResult {
    sequence: NonZeroU32,
    x: f32,
    y: f32,
    z: f32,
    lie: Lie,
    holed: bool,
}

impl ShotResult {
    /// Constructs a finite, bounded shot result.
    ///
    /// # Errors
    /// Returns a stable field error for a zero sequence or invalid coordinate.
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

    /// Client action sequence.
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
    /// Closed lie value.
    #[must_use]
    pub const fn lie(self) -> Lie {
        self.lie
    }
    /// Whether the shot was holed.
    #[must_use]
    pub const fn holed(self) -> bool {
        self.holed
    }

    fn decode_fields(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        let sequence = decode_nonzero_u32(reader, "shot sequence")?;
        let x = decode_f32_inclusive(reader, -100_000.0, 100_000.0, "x")?;
        let y = decode_f32_inclusive(reader, -100_000.0, 100_000.0, "y")?;
        let z = decode_f32_inclusive(reader, -100_000.0, 100_000.0, "z")?;
        let lie = Lie::decode(reader)?;
        let holed = decode_bool(reader, "holed flag")?;
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
        writer.u32_le(self.sequence.get());
        encode_f32_inclusive(writer, self.x, -100_000.0, 100_000.0, "x")?;
        encode_f32_inclusive(writer, self.y, -100_000.0, 100_000.0, "y")?;
        encode_f32_inclusive(writer, self.z, -100_000.0, 100_000.0, "z")?;
        writer.u8(self.lie as u8);
        writer.u8(u8::from(self.holed));
        Ok(())
    }
}

impl DecodePacket for ShotResult {
    const OPCODE: u16 = SYNTHETIC_M5_C2S_SHOT_RESULT;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self::decode_fields(reader)?;
        require_end(reader)?;
        Ok(value)
    }
}

impl EncodePacket for ShotResult {
    const OPCODE: u16 = SYNTHETIC_M5_C2S_SHOT_RESULT;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        self.encode_fields(writer)
    }
}

/// Empty request to finish the synthetic first hole.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FinishHole;

impl FinishHole {
    /// Constructs the empty request.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl DecodePacket for FinishHole {
    const OPCODE: u16 = SYNTHETIC_M5_C2S_FINISH_HOLE;

    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        require_end(reader)?;
        Ok(Self)
    }
}

impl EncodePacket for FinishHole {
    const OPCODE: u16 = SYNTHETIC_M5_C2S_FINISH_HOLE;

    fn encode(
        &self,
        _writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        Ok(())
    }
}

/// Closed synthetic weather discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Weather {
    /// Clear sky.
    Clear = 0,
    /// Cloud cover.
    Cloudy = 1,
    /// Rain.
    Rain = 2,
}

impl Weather {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::Clear),
            1 => Ok(Self::Cloudy),
            2 => Ok(Self::Rain),
            _ => Err(reader.invalid("unknown weather discriminator")),
        }
    }
}

/// Validated wind parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wind {
    speed: f32,
    angle: f32,
}

impl Wind {
    /// Constructs finite wind in the local protocol ranges.
    ///
    /// # Errors
    /// Rejects speed outside `0..=15` and angle outside `0..360`.
    pub fn new(speed: f32, angle: f32) -> Result<Self, PacketEncodeError> {
        validate_f32(speed, 0.0, 15.0, "wind speed")?;
        if !angle.is_finite() || !(0.0..360.0).contains(&angle) {
            return Err(PacketEncodeError::Invalid {
                field: "wind angle",
            });
        }
        Ok(Self { speed, angle })
    }

    /// Wind speed.
    #[must_use]
    pub const fn speed(self) -> f32 {
        self.speed
    }
    /// Wind angle.
    #[must_use]
    pub const fn angle(self) -> f32 {
        self.angle
    }
}

/// Match-start parameters, including a secret deterministic seed.
#[derive(Clone, PartialEq)]
pub struct MatchStarted {
    match_id: Uuid,
    course_id: NonZeroU32,
    par: u8,
    seed: [u8; 32],
    weather: Weather,
    wind: Wind,
    load_timeout_ms: NonZeroU32,
}

impl MatchStarted {
    /// Constructs a validated first-hole match start.
    ///
    /// # Errors
    /// Rejects zero course/timeout, par outside `1..=10`, or invalid wind.
    pub fn new(
        match_id: Uuid,
        course_id: u32,
        par: u8,
        seed: [u8; 32],
        weather: Weather,
        wind: Wind,
        load_timeout_ms: u32,
    ) -> Result<Self, PacketEncodeError> {
        let course_id =
            NonZeroU32::new(course_id).ok_or(PacketEncodeError::Invalid { field: "course ID" })?;
        if !(1..=10).contains(&par) {
            return Err(PacketEncodeError::Invalid { field: "par" });
        }
        validate_f32(wind.speed, 0.0, 15.0, "wind speed")?;
        if !wind.angle.is_finite() || !(0.0..360.0).contains(&wind.angle) {
            return Err(PacketEncodeError::Invalid {
                field: "wind angle",
            });
        }
        let load_timeout_ms =
            NonZeroU32::new(load_timeout_ms).ok_or(PacketEncodeError::Invalid {
                field: "load timeout",
            })?;
        Ok(Self {
            match_id,
            course_id,
            par,
            seed,
            weather,
            wind,
            load_timeout_ms,
        })
    }

    /// Match UUID.
    #[must_use]
    pub const fn match_id(&self) -> Uuid {
        self.match_id
    }
    /// Nonzero course ID.
    #[must_use]
    pub const fn course_id(&self) -> u32 {
        self.course_id.get()
    }
    /// Fixed hole number.
    #[must_use]
    pub const fn hole(&self) -> u8 {
        HOLE_ONE
    }
    /// Hole par.
    #[must_use]
    pub const fn par(&self) -> u8 {
        self.par
    }
    /// Explicit access to the secret deterministic seed.
    #[must_use]
    pub const fn seed(&self) -> &[u8; 32] {
        &self.seed
    }
    /// Closed weather value.
    #[must_use]
    pub const fn weather(&self) -> Weather {
        self.weather
    }
    /// Validated wind.
    #[must_use]
    pub const fn wind(&self) -> Wind {
        self.wind
    }
    /// Nonzero loading timeout in milliseconds.
    #[must_use]
    pub const fn load_timeout_ms(&self) -> u32 {
        self.load_timeout_ms.get()
    }
}

impl fmt::Debug for MatchStarted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MatchStarted")
            .field("match_id", &self.match_id)
            .field("course_id", &self.course_id)
            .field("hole", &HOLE_ONE)
            .field("par", &self.par)
            .field("seed", &"<redacted>")
            .field("weather", &self.weather)
            .field("wind", &self.wind)
            .field("load_timeout_ms", &self.load_timeout_ms)
            .finish()
    }
}

impl DecodePacket for MatchStarted {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_MATCH_STARTED;

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
        let weather = Weather::decode(reader)?;
        let speed = decode_f32_inclusive(reader, 0.0, 15.0, "wind speed")?;
        let angle = reader.f32_le()?;
        if !angle.is_finite() || !(0.0..360.0).contains(&angle) {
            return Err(reader.invalid("wind angle is non-finite or outside 0..360"));
        }
        let load_timeout_ms = decode_nonzero_u32(reader, "load timeout")?;
        require_end(reader)?;
        Ok(Self {
            match_id,
            course_id,
            par,
            seed,
            weather,
            wind: Wind { speed, angle },
            load_timeout_ms,
        })
    }
}

impl EncodePacket for MatchStarted {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_MATCH_STARTED;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        if !(1..=10).contains(&self.par) {
            return Err(PacketEncodeError::Invalid { field: "par" });
        }
        encode_uuid(writer, self.match_id);
        writer.u32_le(self.course_id.get());
        writer.u8(HOLE_ONE);
        writer.u8(self.par);
        writer.bytes(&self.seed);
        writer.u8(self.weather as u8);
        encode_f32_inclusive(writer, self.wind.speed, 0.0, 15.0, "wind speed")?;
        if !self.wind.angle.is_finite() || !(0.0..360.0).contains(&self.wind.angle) {
            return Err(PacketEncodeError::Invalid {
                field: "wind angle",
            });
        }
        writer.f32_le(self.wind.angle);
        writer.u32_le(self.load_timeout_ms.get());
        Ok(())
    }
}

/// Closed match phase discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SoloPhase {
    /// Waiting for loading completion.
    Loading = 0,
    /// Accepting actions and results.
    Playing = 1,
    /// Hole result committed.
    HoleComplete = 2,
    /// Match finished.
    Finished = 3,
}

impl SoloPhase {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::Loading),
            1 => Ok(Self::Playing),
            2 => Ok(Self::HoleComplete),
            3 => Ok(Self::Finished),
            _ => Err(reader.invalid("unknown solo phase discriminator")),
        }
    }
}

/// Server match phase update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MatchPhase {
    match_id: Uuid,
    phase: SoloPhase,
}

impl MatchPhase {
    /// Constructs a phase update.
    #[must_use]
    pub const fn new(match_id: Uuid, phase: SoloPhase) -> Self {
        Self { match_id, phase }
    }
    /// Match UUID.
    #[must_use]
    pub const fn match_id(self) -> Uuid {
        self.match_id
    }
    /// Closed phase.
    #[must_use]
    pub const fn phase(self) -> SoloPhase {
        self.phase
    }
}

impl DecodePacket for MatchPhase {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_MATCH_PHASE;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            match_id: decode_uuid(reader)?,
            phase: SoloPhase::decode(reader)?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for MatchPhase {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_MATCH_PHASE;
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

/// Authoritative connection identity plus a validated shot action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotActionRelay {
    connection_id: NonZeroU64,
    action: ShotAction,
}

impl ShotActionRelay {
    /// Constructs an authoritative action relay.
    ///
    /// # Errors
    /// Rejects a zero authoritative connection ID.
    pub fn new(connection_id: u64, action: ShotAction) -> Result<Self, PacketEncodeError> {
        let connection_id = NonZeroU64::new(connection_id).ok_or(PacketEncodeError::Invalid {
            field: "connection ID",
        })?;
        Ok(Self {
            connection_id,
            action,
        })
    }
    /// Authoritative connection ID.
    #[must_use]
    pub const fn connection_id(self) -> u64 {
        self.connection_id.get()
    }
    /// Relayed action.
    #[must_use]
    pub const fn action(self) -> ShotAction {
        self.action
    }
}
impl DecodePacket for ShotActionRelay {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            connection_id: decode_nonzero_u64(reader, "connection ID")?,
            action: ShotAction::decode_fields(reader)?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for ShotActionRelay {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u64_le(self.connection_id.get());
        self.action.encode_fields(writer)
    }
}

/// Authoritative connection identity plus a validated shot result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotResultRelay {
    connection_id: NonZeroU64,
    result: ShotResult,
}

impl ShotResultRelay {
    /// Constructs an authoritative result relay.
    ///
    /// # Errors
    /// Rejects a zero authoritative connection ID.
    pub fn new(connection_id: u64, result: ShotResult) -> Result<Self, PacketEncodeError> {
        let connection_id = NonZeroU64::new(connection_id).ok_or(PacketEncodeError::Invalid {
            field: "connection ID",
        })?;
        Ok(Self {
            connection_id,
            result,
        })
    }
    /// Authoritative connection ID.
    #[must_use]
    pub const fn connection_id(self) -> u64 {
        self.connection_id.get()
    }
    /// Relayed result.
    #[must_use]
    pub const fn result(self) -> ShotResult {
        self.result
    }
}
impl DecodePacket for ShotResultRelay {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            connection_id: decode_nonzero_u64(reader, "connection ID")?,
            result: ShotResult::decode_fields(reader)?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for ShotResultRelay {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        writer.u64_le(self.connection_id.get());
        self.result.encode_fields(writer)
    }
}

/// Committed synthetic hole result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HoleResult {
    match_id: Uuid,
    strokes: NonZeroU32,
    score: i16,
    pang: u64,
    experience: u64,
    result_id: Uuid,
}

impl HoleResult {
    /// Constructs a first-hole result.
    ///
    /// # Errors
    /// Rejects zero strokes or strokes above the `u16` wire range.
    pub fn new(
        match_id: Uuid,
        strokes: u16,
        score: i16,
        pang: u64,
        experience: u64,
        result_id: Uuid,
    ) -> Result<Self, PacketEncodeError> {
        let strokes = NonZeroU32::new(u32::from(strokes))
            .ok_or(PacketEncodeError::Invalid { field: "strokes" })?;
        Ok(Self {
            match_id,
            strokes,
            score,
            pang,
            experience,
            result_id,
        })
    }
    /// Match UUID.
    #[must_use]
    pub const fn match_id(self) -> Uuid {
        self.match_id
    }
    /// Fixed hole number.
    #[must_use]
    pub const fn hole(self) -> u8 {
        HOLE_ONE
    }
    /// Nonzero stroke count.
    #[must_use]
    pub const fn strokes(self) -> u16 {
        self.strokes.get() as u16
    }
    /// Signed score.
    #[must_use]
    pub const fn score(self) -> i16 {
        self.score
    }
    /// Pang awarded.
    #[must_use]
    pub const fn pang(self) -> u64 {
        self.pang
    }
    /// Experience awarded.
    #[must_use]
    pub const fn experience(self) -> u64 {
        self.experience
    }
    /// Result UUID.
    #[must_use]
    pub const fn result_id(self) -> Uuid {
        self.result_id
    }
}
impl DecodePacket for HoleResult {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_HOLE_RESULT;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let match_id = decode_uuid(reader)?;
        if reader.u8()? != HOLE_ONE {
            return Err(reader.invalid("hole must be exactly 1"));
        }
        let strokes_wire = reader.u16_le()?;
        let strokes = NonZeroU32::new(u32::from(strokes_wire))
            .ok_or_else(|| reader.invalid("strokes must be nonzero"))?;
        let score = reader.i16_le()?;
        let pang = reader.u64_le()?;
        let experience = reader.u64_le()?;
        let result_id = decode_uuid(reader)?;
        require_end(reader)?;
        Ok(Self {
            match_id,
            strokes,
            score,
            pang,
            experience,
            result_id,
        })
    }
}
impl EncodePacket for HoleResult {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_HOLE_RESULT;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        _profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        let strokes = u16::try_from(self.strokes.get())
            .map_err(|_| PacketEncodeError::Invalid { field: "strokes" })?;
        encode_uuid(writer, self.match_id);
        writer.u8(HOLE_ONE);
        writer.u16_le(strokes);
        writer.i16_le(self.score);
        writer.u64_le(self.pang);
        writer.u64_le(self.experience);
        encode_uuid(writer, self.result_id);
        Ok(())
    }
}

/// Authoritative post-match balances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BalanceUpdate {
    pang: u64,
    experience: u64,
}
impl BalanceUpdate {
    /// Constructs an authoritative balance update.
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
impl DecodePacket for BalanceUpdate {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_BALANCE_UPDATE;
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
impl EncodePacket for BalanceUpdate {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_BALANCE_UPDATE;
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

/// Closed synthetic solo command discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SoloCommand {
    /// Start a solo match.
    StartSolo = 0,
    /// Complete loading.
    LoadingComplete = 1,
    /// Submit a shot action.
    ShotAction = 2,
    /// Submit a shot result.
    ShotResult = 3,
    /// Finish the hole.
    FinishHole = 4,
}
impl SoloCommand {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::StartSolo),
            1 => Ok(Self::LoadingComplete),
            2 => Ok(Self::ShotAction),
            3 => Ok(Self::ShotResult),
            4 => Ok(Self::FinishHole),
            _ => Err(reader.invalid("unknown solo command discriminator")),
        }
    }
}

/// Closed synthetic command outcome discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SoloCommandOutcome {
    /// Command succeeded.
    Success = 0,
    /// Shot sequence was invalid.
    InvalidSequence = 1,
    /// Action values were invalid.
    InvalidAction = 2,
    /// Command was invalid in the match phase.
    InvalidPhase = 3,
    /// Command timed out.
    Timeout = 4,
}
impl SoloCommandOutcome {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::Success),
            1 => Ok(Self::InvalidSequence),
            2 => Ok(Self::InvalidAction),
            3 => Ok(Self::InvalidPhase),
            4 => Ok(Self::Timeout),
            _ => Err(reader.invalid("unknown solo command result discriminator")),
        }
    }
}

/// Result of one synthetic solo command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoloCommandResult {
    command: SoloCommand,
    result: SoloCommandOutcome,
}
impl SoloCommandResult {
    /// Constructs a command result from closed enums.
    #[must_use]
    pub const fn new(command: SoloCommand, result: SoloCommandOutcome) -> Self {
        Self { command, result }
    }
    /// Command discriminator.
    #[must_use]
    pub const fn command(self) -> SoloCommand {
        self.command
    }
    /// Result discriminator.
    #[must_use]
    pub const fn result(self) -> SoloCommandOutcome {
        self.result
    }
}
impl DecodePacket for SoloCommandResult {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_COMMAND_RESULT;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            command: SoloCommand::decode(reader)?,
            result: SoloCommandOutcome::decode(reader)?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for SoloCommandResult {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_COMMAND_RESULT;
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

/// Closed match-abort reason discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MatchAbortReason {
    /// Player connection ended.
    PlayerDisconnected = 0,
    /// Loading deadline expired.
    LoadingTimeout = 1,
    /// Peer violated the strict protocol.
    ProtocolViolation = 2,
    /// Server began shutdown.
    ServerShutdown = 3,
}
impl MatchAbortReason {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self, PacketDecodeError> {
        match reader.u8()? {
            0 => Ok(Self::PlayerDisconnected),
            1 => Ok(Self::LoadingTimeout),
            2 => Ok(Self::ProtocolViolation),
            3 => Ok(Self::ServerShutdown),
            _ => Err(reader.invalid("unknown match abort reason discriminator")),
        }
    }
}

/// Terminal server match-aborted notification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MatchAborted {
    match_id: Uuid,
    reason: MatchAbortReason,
}
impl MatchAborted {
    /// Constructs an aborted notification.
    #[must_use]
    pub const fn new(match_id: Uuid, reason: MatchAbortReason) -> Self {
        Self { match_id, reason }
    }
    /// Match UUID.
    #[must_use]
    pub const fn match_id(self) -> Uuid {
        self.match_id
    }
    /// Closed abort reason.
    #[must_use]
    pub const fn reason(self) -> MatchAbortReason {
        self.reason
    }
}
impl DecodePacket for MatchAborted {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_MATCH_ABORTED;
    fn decode(
        reader: &mut PacketReader<'_>,
        _profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        let value = Self {
            match_id: decode_uuid(reader)?,
            reason: MatchAbortReason::decode(reader)?,
        };
        require_end(reader)?;
        Ok(value)
    }
}
impl EncodePacket for MatchAborted {
    const OPCODE: u16 = SYNTHETIC_M5_S2C_MATCH_ABORTED;
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

/// Builds the local synthetic M5 inbound registry for one selected client version.
#[must_use]
pub fn synthetic_m5_registry(version: ClientVersion) -> PacketRegistry {
    let mut registry = PacketRegistry::new();
    registry.register(RegistryKey {
        service: ServiceKind::Game,
        direction: Direction::ClientToServer,
        version,
        state: ConnectionState::InRoom,
        opcode: SYNTHETIC_M5_C2S_START_SOLO,
    });
    registry.register(RegistryKey {
        service: ServiceKind::Game,
        direction: Direction::ClientToServer,
        version,
        state: ConnectionState::InMatchLoading,
        opcode: SYNTHETIC_M5_C2S_LOADING_COMPLETE,
    });
    for opcode in [
        SYNTHETIC_M5_C2S_SHOT_ACTION,
        SYNTHETIC_M5_C2S_SHOT_RESULT,
        SYNTHETIC_M5_C2S_FINISH_HOLE,
    ] {
        registry.register(RegistryKey {
            service: ServiceKind::Game,
            direction: Direction::ClientToServer,
            version,
            state: ConnectionState::InMatch,
            opcode,
        });
    }
    registry
}
