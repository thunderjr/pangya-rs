#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Technology-neutral domain types and repository contracts for account storage.

use std::{
    fmt,
    future::Future,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    pin::Pin,
    str::FromStr,
    time::SystemTime,
};

use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroize as _;

/// Failure converting a database or external identifier into a domain ID.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum IdError {
    /// A signed identifier was zero or negative.
    #[error("identifier must be positive")]
    NotPositive,
    /// An integer cannot be represented by the destination ID type.
    #[error("identifier is outside its supported range")]
    OutOfRange,
}

macro_rules! positive_id {
    ($name:ident, $docs:literal) => {
        #[doc = $docs]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(i64);

        impl $name {
            /// Creates an ID after checking that it is positive.
            ///
            /// # Errors
            /// Returns [`IdError::NotPositive`] for zero or negative values.
            pub const fn new(value: i64) -> Result<Self, IdError> {
                if value > 0 {
                    Ok(Self(value))
                } else {
                    Err(IdError::NotPositive)
                }
            }

            /// Returns the checked database representation.
            #[must_use]
            pub const fn get(self) -> i64 {
                self.0
            }
        }

        impl TryFrom<i64> for $name {
            type Error = IdError;

            fn try_from(value: i64) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

positive_id!(AccountId, "A durable account identifier.");
positive_id!(CharacterId, "An owned character identifier.");
positive_id!(InventoryItemId, "An owned inventory-row identifier.");
positive_id!(EquipmentSetId, "An equipment aggregate identifier.");

/// A static catalog item type represented on the wire as an unsigned 32-bit value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemTypeId(u32);

impl ItemTypeId {
    /// Creates an item type ID from its full supported range.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the unsigned catalog representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl From<u32> for ItemTypeId {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl TryFrom<i64> for ItemTypeId {
    type Error = IdError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        u32::try_from(value)
            .map(Self)
            .map_err(|_| IdError::OutOfRange)
    }
}

/// The nonsecret selector for a login-to-service handover.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HandoverId(Uuid);

impl HandoverId {
    /// Creates a selector from a UUID.
    #[must_use]
    pub const fn new(value: Uuid) -> Self {
        Self(value)
    }

    /// Returns the UUID representation.
    #[must_use]
    pub const fn get(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for HandoverId {
    fn from(value: Uuid) -> Self {
        Self::new(value)
    }
}

macro_rules! uuid_id {
    ($name:ident, $docs:literal) => {
        #[doc = $docs]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Uuid);

        impl $name {
            /// Creates the identifier from its UUID representation.
            #[must_use]
            pub const fn new(value: Uuid) -> Self {
                Self(value)
            }

            /// Returns the UUID representation.
            #[must_use]
            pub const fn get(self) -> Uuid {
                self.0
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self::new(value)
            }
        }
    };
}

uuid_id!(MatchId, "A durable match identifier.");
uuid_id!(
    MatchResultKey,
    "A stable, server-generated match-result idempotency key."
);

/// Validation failure for a display/normalized account name.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NameError {
    /// The trimmed value is outside the policy's byte length.
    #[error("name length is outside policy")]
    InvalidLength,
    /// The value contains a character outside the ASCII policy.
    #[error("name contains an unsupported character")]
    InvalidCharacter,
}

macro_rules! normalized_name {
    ($name:ident, $min:expr, $max:expr, $valid:expr, $docs:literal) => {
        #[doc = $docs]
        #[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// Applies ASCII trim/lowercase normalization and validates the M2 policy.
            ///
            /// # Errors
            /// Returns [`NameError`] when length or characters violate policy.
            pub fn parse(value: &str) -> Result<Self, NameError> {
                let trimmed = value.trim_matches(|character: char| character.is_ascii_whitespace());
                if !($min..=$max).contains(&trimmed.len()) {
                    return Err(NameError::InvalidLength);
                }
                if !trimmed.bytes().all($valid) {
                    return Err(NameError::InvalidCharacter);
                }
                Ok(Self(trimmed.to_ascii_lowercase()))
            }

            /// Returns the canonical normalized value.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.0)
                    .finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = NameError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }
    };
}

normalized_name!(
    NormalizedUsername,
    3,
    32,
    |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_',
    "A username normalized by ASCII trim and lowercase."
);
normalized_name!(
    NormalizedNickname,
    3,
    16,
    |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'),
    "A nickname normalized by ASCII trim and lowercase."
);

/// A validated display username retained separately from its normalized key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Username {
    display: String,
    normalized: NormalizedUsername,
}

impl Username {
    /// Validates a display username and derives its normalized key.
    ///
    /// # Errors
    /// Returns [`NameError`] when the value violates username policy.
    pub fn parse(value: &str) -> Result<Self, NameError> {
        let display = value
            .trim_matches(|character: char| character.is_ascii_whitespace())
            .to_owned();
        let normalized = NormalizedUsername::parse(&display)?;
        Ok(Self {
            display,
            normalized,
        })
    }

    /// Returns the display spelling.
    #[must_use]
    pub fn display(&self) -> &str {
        &self.display
    }

    /// Returns the normalized uniqueness key.
    #[must_use]
    pub const fn normalized(&self) -> &NormalizedUsername {
        &self.normalized
    }
}

/// A validated display nickname retained separately from its normalized key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Nickname {
    display: String,
    normalized: NormalizedNickname,
}

impl Nickname {
    /// Validates a display nickname and derives its normalized key.
    ///
    /// # Errors
    /// Returns [`NameError`] when the value violates nickname policy.
    pub fn parse(value: &str) -> Result<Self, NameError> {
        let display = value
            .trim_matches(|character: char| character.is_ascii_whitespace())
            .to_owned();
        let normalized = NormalizedNickname::parse(&display)?;
        Ok(Self {
            display,
            normalized,
        })
    }

    /// Returns the display spelling.
    #[must_use]
    pub fn display(&self) -> &str {
        &self.display
    }

    /// Returns the normalized uniqueness key.
    #[must_use]
    pub const fn normalized(&self) -> &NormalizedNickname {
        &self.normalized
    }
}

/// Account access status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountStatus {
    /// Normal login and handover are permitted.
    Active,
    /// Access is administratively banned.
    Banned,
    /// Access is administratively disabled.
    Disabled,
}

/// Progress through first-login setup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupState {
    /// A nickname must still be selected.
    NeedsNickname,
    /// Starter identity and equipment must still be granted.
    NeedsStarter,
    /// The minimum account aggregate is ready.
    Complete,
}

/// Service eligible to consume a handover.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceKind {
    /// Game service.
    Game,
    /// Message service.
    Message,
}

/// A PHC credential hash whose formatting is always redacted.
#[derive(Clone, Eq, PartialEq)]
pub struct CredentialHash(String);

impl CredentialHash {
    /// Wraps a PHC string after a security service has validated/generated it.
    #[must_use]
    pub fn new(phc: String) -> Self {
        Self(phc)
    }

    /// Exposes the PHC string only for persistence or verification.
    #[must_use]
    pub fn expose_phc(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CredentialHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CredentialHash([REDACTED])")
    }
}

/// The fixed-size digest stored for a handover bearer secret.
#[derive(Clone, Eq, PartialEq)]
pub struct HandoverDigest([u8; 32]);

impl HandoverDigest {
    /// Creates a digest from exactly 32 bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns digest bytes for persistence and constant-time comparison.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Parses database bytes without truncation.
    ///
    /// # Errors
    /// Returns [`IdError::OutOfRange`] unless the slice is exactly 32 bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, IdError> {
        bytes.try_into().map(Self).map_err(|_| IdError::OutOfRange)
    }
}

impl fmt::Debug for HandoverDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HandoverDigest([REDACTED])")
    }
}

/// Stable idempotency key for a configured starter object.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StarterKey(String);

impl StarterKey {
    /// Validates a stable lowercase ASCII key.
    ///
    /// # Errors
    /// Returns [`NameError`] for an empty/long or unsupported key.
    pub fn parse(value: &str) -> Result<Self, NameError> {
        if !(1..=64).contains(&value.len()) {
            return Err(NameError::InvalidLength);
        }
        if !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.')
        }) {
            return Err(NameError::InvalidCharacter);
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the stable key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Configured starter character.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StarterCharacter {
    /// Stable replay key.
    pub key: StarterKey,
    /// Catalog type ID, validated against IFF in M3.
    pub item_type_id: ItemTypeId,
}

/// Configured starter inventory row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StarterItem {
    /// Stable replay key.
    pub key: StarterKey,
    /// Catalog type ID, validated against IFF in M3.
    pub item_type_id: ItemTypeId,
    /// Positive quantity.
    pub quantity: u32,
}

/// Maximum starter inventory rows accepted at every public composition/storage boundary.
pub const MAX_STARTER_ITEMS: usize = 256;

/// Minimum idempotent starter aggregate configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StarterGrant {
    /// Stable starter character.
    pub character: StarterCharacter,
    /// Stable starter inventory rows.
    pub items: Vec<StarterItem>,
    /// Optional stable key of the equipped club inventory row.
    pub equipped_club_key: Option<StarterKey>,
    /// Optional stable key of the equipped ball inventory row.
    pub equipped_ball_key: Option<StarterKey>,
}

/// Input for atomic account aggregate creation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAccount {
    /// Display and normalized username.
    pub username: Username,
    /// Versioned PHC hash.
    pub credential_hash: CredentialHash,
    /// Optional initial nickname.
    pub nickname: Option<Nickname>,
    /// Idempotent minimum starter grant.
    pub starter: StarterGrant,
}

/// Persisted account identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Account {
    /// Identifier.
    pub id: AccountId,
    /// Display username.
    pub username_display: String,
    /// Normalized username.
    pub username_normalized: NormalizedUsername,
    /// Access status.
    pub status: AccountStatus,
}

/// Persisted profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Profile {
    /// Owning account.
    pub account_id: AccountId,
    /// Optional display nickname.
    pub nickname: Option<String>,
    /// Setup progress.
    pub setup_state: SetupState,
    /// Nonnegative Pang balance.
    pub pang: u64,
    /// Nonnegative point balance.
    pub points: u64,
    /// Nonnegative experience.
    pub experience: u64,
}

/// Authentication projection loaded by normalized username.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticationRecord {
    /// Account identity.
    pub account: Account,
    /// Stored PHC credential.
    pub credential_hash: CredentialHash,
    /// Current setup state.
    pub setup_state: SetupState,
}

/// Persisted starter character projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Character {
    /// Identifier.
    pub id: CharacterId,
    /// Owner.
    pub account_id: AccountId,
    /// Catalog type ID.
    pub item_type_id: ItemTypeId,
    /// Stable grant key.
    pub starter_key: StarterKey,
}

/// Persisted starter inventory projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryItem {
    /// Identifier.
    pub id: InventoryItemId,
    /// Owner.
    pub account_id: AccountId,
    /// Catalog type ID.
    pub item_type_id: ItemTypeId,
    /// Quantity.
    pub quantity: u32,
    /// Stable grant key.
    pub starter_key: StarterKey,
}

/// Persisted minimum equipment aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EquipmentSet {
    /// Identifier.
    pub id: EquipmentSetId,
    /// Owner.
    pub account_id: AccountId,
    /// Selected owned character.
    pub character_id: CharacterId,
    /// Optional equipped owned club.
    pub club_item_id: Option<InventoryItemId>,
    /// Optional equipped owned ball.
    pub ball_item_id: Option<InventoryItemId>,
    /// Optimistic version.
    pub version: u32,
}

/// Coherent minimum persisted account aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountAggregate {
    /// Identity.
    pub account: Account,
    /// Profile.
    pub profile: Profile,
    /// Starter character.
    pub character: Character,
    /// Starter inventory.
    pub inventory: Vec<InventoryItem>,
    /// Equipment selection.
    pub equipment: EquipmentSet,
}

/// Maximum characters loaded into one bounded player bootstrap snapshot.
pub const MAX_PLAYER_CHARACTERS: usize = 64;
/// Maximum inventory rows loaded into one bounded player bootstrap snapshot.
pub const MAX_PLAYER_INVENTORY: usize = 10_000;

/// Coherent active, fully configured player projection used by GameService bootstrap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSnapshot {
    /// Active account identity.
    pub account: Account,
    /// Complete profile and balances.
    pub profile: Profile,
    /// Bounded owned characters, including the selected character.
    pub characters: Vec<Character>,
    /// Bounded owned inventory rows.
    pub inventory: Vec<InventoryItem>,
    /// Owned equipment references.
    pub equipment: EquipmentSet,
}

/// Failure parsing a canonical privacy-minimized source prefix.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SourceAddressError {
    /// The prefix was malformed, used the wrong mask, or retained host bits.
    #[error("source address prefix is invalid")]
    Invalid,
}

/// Privacy-minimized source address: IPv4 `/24` or IPv6 `/56`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceAddressPrefix(String);

impl SourceAddressPrefix {
    /// Masks a raw peer address immediately, before it crosses the domain/storage boundary.
    #[must_use]
    pub fn from_ip(address: IpAddr) -> Self {
        match address {
            IpAddr::V4(address) => {
                let [first, second, third, _] = address.octets();
                Self(format!("{}/24", Ipv4Addr::new(first, second, third, 0)))
            }
            IpAddr::V6(address) => {
                let mut octets = address.octets();
                octets[7..].fill(0);
                Self(format!("{}/56", Ipv6Addr::from(octets)))
            }
        }
    }

    /// Parses only the canonical masked representation produced by [`Self::from_ip`].
    ///
    /// # Errors
    /// Returns [`SourceAddressError::Invalid`] for raw addresses, wrong masks, host bits,
    /// noncanonical textual forms, or malformed input.
    pub fn parse(value: &str) -> Result<Self, SourceAddressError> {
        let (address, mask) = value.split_once('/').ok_or(SourceAddressError::Invalid)?;
        let address: IpAddr = address.parse().map_err(|_| SourceAddressError::Invalid)?;
        let expected_mask = match address {
            IpAddr::V4(_) => "24",
            IpAddr::V6(_) => "56",
        };
        if mask != expected_mask {
            return Err(SourceAddressError::Invalid);
        }
        let canonical = Self::from_ip(address);
        if canonical.0 != value {
            return Err(SourceAddressError::Invalid);
        }
        Ok(canonical)
    }

    /// Returns canonical prefix text safe for privacy-minimized persistence.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceAddressPrefix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for SourceAddressPrefix {
    type Err = SourceAddressError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// Persistable handover generated by the security boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewHandover {
    /// Nonsecret selector.
    pub id: HandoverId,
    /// Account being handed over.
    pub account_id: AccountId,
    /// Stored bearer digest.
    pub digest: HandoverDigest,
    /// Intended consumer service.
    pub target: ServiceKind,
    /// Privacy-minimized source network; a raw peer address is never persisted.
    pub source_address_prefix: SourceAddressPrefix,
    /// Creation time supplied by the application clock.
    pub issued_at: SystemTime,
    /// Strict expiry time.
    pub expires_at: SystemTime,
}

/// Request to atomically consume a handover.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumeHandover {
    /// Nonsecret selector.
    pub id: HandoverId,
    /// Digest derived from the presented bearer.
    pub digest: HandoverDigest,
    /// Actual consuming service.
    pub target: ServiceKind,
    /// Current time supplied by the application clock.
    pub now: SystemTime,
}

/// Session identity returned after successful handover consumption.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedSession {
    /// Authenticated account.
    pub account_id: AccountId,
    /// Consumed selector for audit correlation.
    pub handover_id: HandoverId,
}

/// Validation failure for synthetic one-hole match values.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MatchValueError {
    /// Course zero is reserved and cannot identify a configured course.
    #[error("course identifier must be nonzero")]
    InvalidCourse,
    /// Synthetic one-hole par must be in `1..=10`.
    #[error("hole par is outside policy")]
    InvalidPar,
    /// A stroke count must fit the persisted positive `SMALLINT` range.
    #[error("stroke count is outside policy")]
    InvalidStrokes,
    /// Wind speed must be in `0..=150` tenths.
    #[error("wind speed is outside policy")]
    InvalidWindSpeed,
    /// Wind angle must be in `0..=359` degrees.
    #[error("wind angle is outside policy")]
    InvalidWindAngle,
    /// A catalog fingerprint or deterministic seed had the wrong byte length.
    #[error("fixed-size match bytes have the wrong length")]
    InvalidBytes,
    /// Checked score or reward arithmetic overflowed.
    #[error("match result arithmetic overflowed")]
    ArithmeticOverflow,
    /// A two-player roster did not contain exactly one distinct participant in each order.
    #[error("stroke roster is invalid")]
    InvalidStrokeRoster,
    /// A two-player settlement had invalid strokes, places, or completion consistency.
    #[error("stroke settlement is invalid")]
    InvalidStrokeSettlement,
    /// A persisted course record violated its count or score invariants.
    #[error("course record is invalid")]
    InvalidCourseRecord,
}

/// A checked, nonzero synthetic course identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CourseId(std::num::NonZeroU32);

impl CourseId {
    /// Creates a nonzero course identifier.
    ///
    /// # Errors
    /// Returns [`MatchValueError::InvalidCourse`] for zero.
    pub const fn new(value: u32) -> Result<Self, MatchValueError> {
        match std::num::NonZeroU32::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(MatchValueError::InvalidCourse),
        }
    }

    /// Returns the unsigned catalog/database representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl TryFrom<u32> for CourseId {
    type Error = MatchValueError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<i64> for CourseId {
    type Error = MatchValueError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        u32::try_from(value)
            .map_err(|_| MatchValueError::InvalidCourse)
            .and_then(Self::new)
    }
}

/// Server-selected weather for a synthetic solo hole.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Weather {
    /// Clear weather.
    Clear,
    /// Cloud cover without rain.
    Cloudy,
    /// Rain.
    Rain,
}

/// Checked deterministic wind selected by the server for one match.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WindConditions {
    speed_tenths: u16,
    angle_degrees: u16,
}

impl WindConditions {
    /// Validates persisted wind bounds.
    ///
    /// # Errors
    /// Returns [`MatchValueError::InvalidWindSpeed`] above 150 tenths or
    /// [`MatchValueError::InvalidWindAngle`] above 359 degrees.
    pub const fn new(speed_tenths: u16, angle_degrees: u16) -> Result<Self, MatchValueError> {
        if speed_tenths > 150 {
            Err(MatchValueError::InvalidWindSpeed)
        } else if angle_degrees > 359 {
            Err(MatchValueError::InvalidWindAngle)
        } else {
            Ok(Self {
                speed_tenths,
                angle_degrees,
            })
        }
    }

    /// Wind speed in tenths of the local gameplay unit.
    #[must_use]
    pub const fn speed_tenths(self) -> u16 {
        self.speed_tenths
    }

    /// Wind direction in degrees in `0..=359`.
    #[must_use]
    pub const fn angle_degrees(self) -> u16 {
        self.angle_degrees
    }
}

/// Immutable one-hole synthetic course configuration. Hole number is always one.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OneHoleConfig {
    course_id: CourseId,
    par: u8,
}

impl OneHoleConfig {
    /// Validates a local one-hole course configuration.
    ///
    /// # Errors
    /// Returns [`MatchValueError::InvalidPar`] unless par is in `1..=10`.
    pub const fn new(course_id: CourseId, par: u8) -> Result<Self, MatchValueError> {
        if par >= 1 && par <= 10 {
            Ok(Self { course_id, par })
        } else {
            Err(MatchValueError::InvalidPar)
        }
    }

    /// Configured course identifier.
    #[must_use]
    pub const fn course_id(self) -> CourseId {
        self.course_id
    }

    /// Fixed local hole number.
    #[must_use]
    pub const fn hole(self) -> u8 {
        1
    }

    /// Configured par.
    #[must_use]
    pub const fn par(self) -> u8 {
        self.par
    }
}

/// Checked positive number of strokes persisted for one hole.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StrokeCount(u16);

impl StrokeCount {
    /// Validates the positive PostgreSQL `SMALLINT` range.
    ///
    /// # Errors
    /// Returns [`MatchValueError::InvalidStrokes`] for zero or values above `i16::MAX`.
    pub const fn new(value: u16) -> Result<Self, MatchValueError> {
        if value >= 1 && value <= i16::MAX as u16 {
            Ok(Self(value))
        } else {
            Err(MatchValueError::InvalidStrokes)
        }
    }

    /// Returns the unsigned stroke count.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Stable SHA-256 fingerprint of the catalog declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CatalogFingerprint([u8; 32]);

impl CatalogFingerprint {
    /// Creates a fingerprint from exactly 32 digest bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Parses exact database bytes without truncation.
    ///
    /// # Errors
    /// Returns [`MatchValueError::InvalidBytes`] unless exactly 32 bytes are supplied.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, MatchValueError> {
        bytes
            .try_into()
            .map(Self)
            .map_err(|_| MatchValueError::InvalidBytes)
    }

    /// Borrows the fingerprint bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Server-generated deterministic match seed.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct MatchSeed([u8; 32]);

impl MatchSeed {
    /// Creates a seed from exactly 32 bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Parses exact persisted bytes without truncation.
    ///
    /// # Errors
    /// Returns [`MatchValueError::InvalidBytes`] unless exactly 32 bytes are supplied.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, MatchValueError> {
        bytes
            .try_into()
            .map(Self)
            .map_err(|_| MatchValueError::InvalidBytes)
    }

    /// Borrows seed bytes for persistence. Seed formatting is intentionally unavailable.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for MatchSeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MatchSeed([REDACTED])")
    }
}

/// Immutable request to begin one synthetic solo match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BeginSoloMatch {
    match_id: MatchId,
    result_key: MatchResultKey,
    account_id: AccountId,
    config: OneHoleConfig,
    catalog_fingerprint: CatalogFingerprint,
    seed: MatchSeed,
    weather: Weather,
    wind: WindConditions,
}

impl BeginSoloMatch {
    /// Constructs a server-owned begin request from already checked values.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        match_id: MatchId,
        result_key: MatchResultKey,
        account_id: AccountId,
        config: OneHoleConfig,
        catalog_fingerprint: CatalogFingerprint,
        seed: MatchSeed,
        weather: Weather,
        wind: WindConditions,
    ) -> Self {
        Self {
            match_id,
            result_key,
            account_id,
            config,
            catalog_fingerprint,
            seed,
            weather,
            wind,
        }
    }

    /// Durable match ID.
    #[must_use]
    pub const fn match_id(&self) -> MatchId {
        self.match_id
    }
    /// Result idempotency key.
    #[must_use]
    pub const fn result_key(&self) -> MatchResultKey {
        self.result_key
    }
    /// Authoritative participant account.
    #[must_use]
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }
    /// Persisted course configuration.
    #[must_use]
    pub const fn config(&self) -> OneHoleConfig {
        self.config
    }
    /// Persisted catalog fingerprint.
    #[must_use]
    pub const fn catalog_fingerprint(&self) -> CatalogFingerprint {
        self.catalog_fingerprint
    }
    /// Persisted deterministic seed.
    #[must_use]
    pub const fn seed(&self) -> MatchSeed {
        self.seed
    }
    /// Persisted server-selected weather.
    #[must_use]
    pub const fn weather(&self) -> Weather {
        self.weather
    }
    /// Persisted deterministic wind.
    #[must_use]
    pub const fn wind(&self) -> WindConditions {
        self.wind
    }
}

/// Whether beginning a match inserted it or exactly replayed immutable input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BeginSoloMatchOutcome {
    /// A new match and started audit were persisted.
    Begun,
    /// The exact immutable request was already persisted.
    Existing,
}

/// Authoritative identity used to durably mark a loaded solo match in game.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkSoloInGame {
    match_id: MatchId,
    result_key: MatchResultKey,
    account_id: AccountId,
}

impl MarkSoloInGame {
    /// Constructs an in-game transition request from server-owned begin identity.
    #[must_use]
    pub const fn new(match_id: MatchId, result_key: MatchResultKey, account_id: AccountId) -> Self {
        Self {
            match_id,
            result_key,
            account_id,
        }
    }

    /// Durable match ID.
    #[must_use]
    pub const fn match_id(self) -> MatchId {
        self.match_id
    }

    /// Authoritative result key.
    #[must_use]
    pub const fn result_key(self) -> MatchResultKey {
        self.result_key
    }

    /// Authoritative participant.
    #[must_use]
    pub const fn account_id(self) -> AccountId {
        self.account_id
    }
}

/// Outcome of the checked loading-to-in-game persistence transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkSoloInGameOutcome {
    /// Loading was transitioned to in-game.
    Marked,
    /// The exact authoritative match was already in-game.
    Existing,
}

/// Stable reason for terminating a synthetic match without reward.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MatchAbortReason {
    /// The sole participant disconnected.
    Disconnect,
    /// Loading exceeded the application deadline.
    LoadingTimeout,
    /// Local service shutdown interrupted the match.
    Shutdown,
    /// Local startup recovery found a nonterminal match.
    StartupRecovery,
    /// Persistence failed after a runtime reservation or durable transition became ambiguous.
    PersistenceFailure,
}

/// Request to abort one authoritative match without accepting reward data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AbortMatch {
    match_id: MatchId,
    result_key: MatchResultKey,
    account_id: AccountId,
    reason: MatchAbortReason,
}

impl AbortMatch {
    /// Constructs an abort request.
    #[must_use]
    pub const fn new(
        match_id: MatchId,
        result_key: MatchResultKey,
        account_id: AccountId,
        reason: MatchAbortReason,
    ) -> Self {
        Self {
            match_id,
            result_key,
            account_id,
            reason,
        }
    }
    /// Durable match ID.
    #[must_use]
    pub const fn match_id(self) -> MatchId {
        self.match_id
    }
    /// Authoritative result key.
    #[must_use]
    pub const fn result_key(self) -> MatchResultKey {
        self.result_key
    }
    /// Authoritative participant.
    #[must_use]
    pub const fn account_id(self) -> AccountId {
        self.account_id
    }
    /// Stable abort reason.
    #[must_use]
    pub const fn reason(self) -> MatchAbortReason {
        self.reason
    }
}

/// Request to commit one completed solo hole. Rewards and balances are intentionally absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitSoloHole {
    match_id: MatchId,
    result_key: MatchResultKey,
    account_id: AccountId,
    config: OneHoleConfig,
    strokes: StrokeCount,
}

impl CommitSoloHole {
    /// Constructs a commit request from authoritative identity/config and client stroke evidence.
    #[must_use]
    pub const fn new(
        match_id: MatchId,
        result_key: MatchResultKey,
        account_id: AccountId,
        config: OneHoleConfig,
        strokes: StrokeCount,
    ) -> Self {
        Self {
            match_id,
            result_key,
            account_id,
            config,
            strokes,
        }
    }
    /// Durable match ID.
    #[must_use]
    pub const fn match_id(self) -> MatchId {
        self.match_id
    }
    /// Authoritative result key.
    #[must_use]
    pub const fn result_key(self) -> MatchResultKey {
        self.result_key
    }
    /// Authoritative participant.
    #[must_use]
    pub const fn account_id(self) -> AccountId {
        self.account_id
    }
    /// Authoritative one-hole configuration.
    #[must_use]
    pub const fn config(self) -> OneHoleConfig {
        self.config
    }
    /// Checked stroke evidence.
    #[must_use]
    pub const fn strokes(self) -> StrokeCount {
        self.strokes
    }
}

/// Checked score and server-computed rewards for one synthetic hole.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoloReward {
    score: i16,
    pang: u64,
    experience: u64,
}

impl SoloReward {
    /// Reconstructs already validated persisted reward columns.
    #[must_use]
    pub const fn from_persisted(score: i16, pang: u64, experience: u64) -> Self {
        Self {
            score,
            pang,
            experience,
        }
    }
    /// Signed score relative to par.
    #[must_use]
    pub const fn score(self) -> i16 {
        self.score
    }
    /// Pang reward.
    #[must_use]
    pub const fn pang(self) -> u64 {
        self.pang
    }
    /// Experience reward.
    #[must_use]
    pub const fn experience(self) -> u64 {
        self.experience
    }
}

/// Server balances immediately after an atomic match commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServerBalances {
    pang: u64,
    experience: u64,
}

impl ServerBalances {
    /// Reconstructs checked nonnegative persisted balances.
    #[must_use]
    pub const fn from_persisted(pang: u64, experience: u64) -> Self {
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

/// Server-computed synthetic solo result including post-commit balances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoloMatchResult {
    match_id: MatchId,
    result_key: MatchResultKey,
    account_id: AccountId,
    strokes: StrokeCount,
    score: i16,
    pang_reward: u64,
    experience_reward: u64,
    pang_balance: u64,
    experience_balance: u64,
}

impl SoloMatchResult {
    /// Constructs a result at the trusted persistence boundary.
    #[must_use]
    pub const fn new(
        match_id: MatchId,
        result_key: MatchResultKey,
        account_id: AccountId,
        strokes: StrokeCount,
        reward: SoloReward,
        balances: ServerBalances,
    ) -> Self {
        Self {
            match_id,
            result_key,
            account_id,
            strokes,
            score: reward.score(),
            pang_reward: reward.pang(),
            experience_reward: reward.experience(),
            pang_balance: balances.pang(),
            experience_balance: balances.experience(),
        }
    }
    /// Match ID.
    #[must_use]
    pub const fn match_id(self) -> MatchId {
        self.match_id
    }
    /// Result idempotency key.
    #[must_use]
    pub const fn result_key(self) -> MatchResultKey {
        self.result_key
    }
    /// Rewarded account.
    #[must_use]
    pub const fn account_id(self) -> AccountId {
        self.account_id
    }
    /// Final strokes.
    #[must_use]
    pub const fn strokes(self) -> StrokeCount {
        self.strokes
    }
    /// Signed score relative to par.
    #[must_use]
    pub const fn score(self) -> i16 {
        self.score
    }
    /// Server-computed Pang reward.
    #[must_use]
    pub const fn pang_reward(self) -> u64 {
        self.pang_reward
    }
    /// Server-computed experience reward.
    #[must_use]
    pub const fn experience_reward(self) -> u64 {
        self.experience_reward
    }
    /// Pang balance after commit.
    #[must_use]
    pub const fn pang_balance(self) -> u64 {
        self.pang_balance
    }
    /// Experience balance after commit.
    #[must_use]
    pub const fn experience_balance(self) -> u64 {
        self.experience_balance
    }
}

/// Computes synthetic reward formula `solo-v1` using checked integer arithmetic.
///
/// Formula: `score = strokes - par`, `Pang = 10 + 2 * max(par - strokes, 0)`, `EXP = 5`.
///
/// # Errors
/// Returns [`MatchValueError::ArithmeticOverflow`] if an intermediate cannot be represented.
pub fn synthetic_solo_reward_v1(
    config: OneHoleConfig,
    strokes: StrokeCount,
) -> Result<SoloReward, MatchValueError> {
    let strokes_i32 = i32::from(strokes.get());
    let par_i32 = i32::from(config.par());
    let score = strokes_i32
        .checked_sub(par_i32)
        .and_then(|value| i16::try_from(value).ok())
        .ok_or(MatchValueError::ArithmeticOverflow)?;
    let under_par = par_i32
        .checked_sub(strokes_i32)
        .ok_or(MatchValueError::ArithmeticOverflow)?
        .max(0);
    let bonus = u64::try_from(under_par)
        .ok()
        .and_then(|value| value.checked_mul(2))
        .ok_or(MatchValueError::ArithmeticOverflow)?;
    let pang = 10_u64
        .checked_add(bonus)
        .ok_or(MatchValueError::ArithmeticOverflow)?;
    Ok(SoloReward::from_persisted(score, pang, 5))
}

/// Stable captured position in an exactly-two stroke roster.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StrokeRosterOrder {
    /// Captured roster order zero.
    First,
    /// Captured roster order one.
    Second,
}

impl StrokeRosterOrder {
    /// Database representation constrained to `0..=1`.
    #[must_use]
    pub const fn get(self) -> u8 {
        match self {
            Self::First => 0,
            Self::Second => 1,
        }
    }

    /// Parses the constrained database representation.
    ///
    /// # Errors
    /// Returns [`MatchValueError::InvalidStrokeRoster`] outside `0..=1`.
    pub const fn from_persisted(value: i16) -> Result<Self, MatchValueError> {
        match value {
            0 => Ok(Self::First),
            1 => Ok(Self::Second),
            _ => Err(MatchValueError::InvalidStrokeRoster),
        }
    }
}

/// Captured authoritative identity for one stroke participant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeParticipant {
    account_id: AccountId,
    roster_order: StrokeRosterOrder,
    player_result_key: MatchResultKey,
}

impl StrokeParticipant {
    /// Constructs one server-owned roster entry.
    #[must_use]
    pub const fn new(
        account_id: AccountId,
        roster_order: StrokeRosterOrder,
        player_result_key: MatchResultKey,
    ) -> Self {
        Self {
            account_id,
            roster_order,
            player_result_key,
        }
    }

    /// Participant account.
    #[must_use]
    pub const fn account_id(self) -> AccountId {
        self.account_id
    }
    /// Stable roster order.
    #[must_use]
    pub const fn roster_order(self) -> StrokeRosterOrder {
        self.roster_order
    }
    /// Per-player settlement idempotency key.
    #[must_use]
    pub const fn player_result_key(self) -> MatchResultKey {
        self.player_result_key
    }
}

fn validate_stroke_roster(participants: &[StrokeParticipant; 2]) -> Result<(), MatchValueError> {
    if participants[0].roster_order != StrokeRosterOrder::First
        || participants[1].roster_order != StrokeRosterOrder::Second
        || participants[0].account_id == participants[1].account_id
        || participants[0].player_result_key == participants[1].player_result_key
    {
        Err(MatchValueError::InvalidStrokeRoster)
    } else {
        Ok(())
    }
}

/// Immutable request to reserve one exactly-two synthetic stroke match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BeginStrokeMatch {
    match_id: MatchId,
    result_key: MatchResultKey,
    participants: [StrokeParticipant; 2],
    config: OneHoleConfig,
    catalog_fingerprint: CatalogFingerprint,
    seed: MatchSeed,
    weather: Weather,
    wind: WindConditions,
}

impl BeginStrokeMatch {
    /// Validates and constructs a complete authoritative begin request.
    ///
    /// # Errors
    /// Returns [`MatchValueError::InvalidStrokeRoster`] for duplicate identities/keys or
    /// missing/reordered roster positions.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        match_id: MatchId,
        result_key: MatchResultKey,
        participants: [StrokeParticipant; 2],
        config: OneHoleConfig,
        catalog_fingerprint: CatalogFingerprint,
        seed: MatchSeed,
        weather: Weather,
        wind: WindConditions,
    ) -> Result<Self, MatchValueError> {
        validate_stroke_roster(&participants)?;
        if participants
            .iter()
            .any(|participant| participant.player_result_key == result_key)
        {
            return Err(MatchValueError::InvalidStrokeRoster);
        }
        Ok(Self {
            match_id,
            result_key,
            participants,
            config,
            catalog_fingerprint,
            seed,
            weather,
            wind,
        })
    }

    /// Durable aggregate ID.
    #[must_use]
    pub const fn match_id(&self) -> MatchId {
        self.match_id
    }
    /// Aggregate commit idempotency key.
    #[must_use]
    pub const fn result_key(&self) -> MatchResultKey {
        self.result_key
    }
    /// Exact captured roster in order zero then one.
    #[must_use]
    pub const fn participants(&self) -> &[StrokeParticipant; 2] {
        &self.participants
    }
    /// Persisted course configuration.
    #[must_use]
    pub const fn config(&self) -> OneHoleConfig {
        self.config
    }
    /// Persisted catalog fingerprint.
    #[must_use]
    pub const fn catalog_fingerprint(&self) -> CatalogFingerprint {
        self.catalog_fingerprint
    }
    /// Persisted deterministic seed.
    #[must_use]
    pub const fn seed(&self) -> MatchSeed {
        self.seed
    }
    /// Persisted weather.
    #[must_use]
    pub const fn weather(&self) -> Weather {
        self.weather
    }
    /// Persisted wind.
    #[must_use]
    pub const fn wind(&self) -> WindConditions {
        self.wind
    }
}

/// Result of an idempotent exactly-two begin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BeginStrokeMatchOutcome {
    /// Aggregate, roster, and started audit were inserted atomically.
    Begun,
    /// Exact immutable input was already present.
    Existing,
}

/// Authoritative aggregate identity for loading-to-in-game transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkStrokeInGame {
    match_id: MatchId,
    result_key: MatchResultKey,
}

impl MarkStrokeInGame {
    /// Constructs a server-owned transition request.
    #[must_use]
    pub const fn new(match_id: MatchId, result_key: MatchResultKey) -> Self {
        Self {
            match_id,
            result_key,
        }
    }
    /// Durable aggregate ID.
    #[must_use]
    pub const fn match_id(self) -> MatchId {
        self.match_id
    }
    /// Aggregate result key.
    #[must_use]
    pub const fn result_key(self) -> MatchResultKey {
        self.result_key
    }
}

/// Result of an idempotent stroke loading transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkStrokeInGameOutcome {
    /// Loading changed to in-game.
    Marked,
    /// Exact aggregate was already in-game.
    Existing,
}

/// Aggregate abort request; participant authority is loaded from the captured roster.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AbortStrokeMatch {
    match_id: MatchId,
    result_key: MatchResultKey,
    reason: MatchAbortReason,
}

impl AbortStrokeMatch {
    /// Constructs a no-reward abort request.
    #[must_use]
    pub const fn new(
        match_id: MatchId,
        result_key: MatchResultKey,
        reason: MatchAbortReason,
    ) -> Self {
        Self {
            match_id,
            result_key,
            reason,
        }
    }
    /// Durable aggregate ID.
    #[must_use]
    pub const fn match_id(self) -> MatchId {
        self.match_id
    }
    /// Aggregate result key.
    #[must_use]
    pub const fn result_key(self) -> MatchResultKey {
        self.result_key
    }
    /// Durable reason.
    #[must_use]
    pub const fn reason(self) -> MatchAbortReason {
        self.reason
    }
}

/// Authoritative completion class for one stroke participant.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StrokeCompletion {
    /// Ball was holed and is course-record eligible.
    Holed,
    /// Configured stroke cap was reached.
    StrokeCap,
    /// Participant voluntarily gave up.
    GiveUp,
    /// Participant disconnected in game.
    Disconnect,
    /// Participant exceeded the active-turn deadline.
    TurnTimeout,
    /// Participant remained unfinished at the game deadline.
    GameTimeout,
}

impl StrokeCompletion {
    /// Whether this completion receives the non-forfeit formula.
    #[must_use]
    pub const fn is_forfeit(self) -> bool {
        !matches!(self, Self::Holed | Self::StrokeCap)
    }

    /// Whether this completion may update a course record.
    #[must_use]
    pub const fn is_record_eligible(self) -> bool {
        matches!(self, Self::Holed)
    }
}

/// Unique synthetic standing assigned within the exactly-two aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StrokePlace {
    /// First place.
    First,
    /// Second place.
    Second,
}

impl StrokePlace {
    /// Database place representation.
    #[must_use]
    pub const fn get(self) -> u8 {
        match self {
            Self::First => 1,
            Self::Second => 2,
        }
    }

    /// Parses the constrained database representation.
    ///
    /// # Errors
    /// Returns [`MatchValueError::InvalidStrokeSettlement`] outside `1..=2`.
    pub const fn from_persisted(value: i16) -> Result<Self, MatchValueError> {
        match value {
            1 => Ok(Self::First),
            2 => Ok(Self::Second),
            _ => Err(MatchValueError::InvalidStrokeSettlement),
        }
    }
}

/// One authoritative participant input to an aggregate commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokePlayerCommit {
    participant: StrokeParticipant,
    strokes: u16,
    place: StrokePlace,
    completion: StrokeCompletion,
}

impl StrokePlayerCommit {
    /// Validates completion/stroke consistency.
    ///
    /// # Errors
    /// Non-forfeits require a positive persisted `SMALLINT`; forfeits permit zero.
    pub const fn new(
        participant: StrokeParticipant,
        strokes: u16,
        place: StrokePlace,
        completion: StrokeCompletion,
    ) -> Result<Self, MatchValueError> {
        if strokes > i16::MAX as u16 || (!completion.is_forfeit() && strokes == 0) {
            Err(MatchValueError::InvalidStrokeSettlement)
        } else {
            Ok(Self {
                participant,
                strokes,
                place,
                completion,
            })
        }
    }
    /// Captured participant authority.
    #[must_use]
    pub const fn participant(self) -> StrokeParticipant {
        self.participant
    }
    /// Authoritative strokes; zero is allowed only for forfeits.
    #[must_use]
    pub const fn strokes(self) -> u16 {
        self.strokes
    }
    /// Unique place.
    #[must_use]
    pub const fn place(self) -> StrokePlace {
        self.place
    }
    /// Completion reason.
    #[must_use]
    pub const fn completion(self) -> StrokeCompletion {
        self.completion
    }
}

/// Request to settle exactly two authoritative player outcomes atomically.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitStrokeMatch {
    match_id: MatchId,
    result_key: MatchResultKey,
    config: OneHoleConfig,
    players: [StrokePlayerCommit; 2],
}

impl CommitStrokeMatch {
    /// Validates roster order, distinct keys/accounts, and unique places.
    ///
    /// # Errors
    /// Returns a typed value error for any aggregate cardinality/standing drift.
    pub fn new(
        match_id: MatchId,
        result_key: MatchResultKey,
        config: OneHoleConfig,
        players: [StrokePlayerCommit; 2],
    ) -> Result<Self, MatchValueError> {
        let participants = [players[0].participant, players[1].participant];
        validate_stroke_roster(&participants)?;
        if players[0].place == players[1].place
            || participants
                .iter()
                .any(|participant| participant.player_result_key == result_key)
        {
            return Err(MatchValueError::InvalidStrokeSettlement);
        }
        Ok(Self {
            match_id,
            result_key,
            config,
            players,
        })
    }
    /// Durable aggregate ID.
    #[must_use]
    pub const fn match_id(self) -> MatchId {
        self.match_id
    }
    /// Aggregate commit key.
    #[must_use]
    pub const fn result_key(self) -> MatchResultKey {
        self.result_key
    }
    /// Authoritative one-hole configuration.
    #[must_use]
    pub const fn config(self) -> OneHoleConfig {
        self.config
    }
    /// Exact roster-ordered player inputs.
    #[must_use]
    pub const fn players(&self) -> &[StrokePlayerCommit; 2] {
        &self.players
    }
}

/// Checked score and server-calculated reward for one stroke participant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeReward {
    score: Option<i16>,
    pang: u64,
    experience: u64,
}

impl StrokeReward {
    /// Reconstructs trusted persisted values.
    #[must_use]
    pub const fn from_persisted(score: Option<i16>, pang: u64, experience: u64) -> Self {
        Self {
            score,
            pang,
            experience,
        }
    }
    /// Golf score, absent for forfeits.
    #[must_use]
    pub const fn score(self) -> Option<i16> {
        self.score
    }
    /// Pang reward.
    #[must_use]
    pub const fn pang(self) -> u64 {
        self.pang
    }
    /// Experience reward.
    #[must_use]
    pub const fn experience(self) -> u64 {
        self.experience
    }
}

/// Computes the server-only `stroke-two-v1` formula.
///
/// Non-forfeits reuse checked `solo-v1`; forfeits have no score and zero reward.
///
/// # Errors
/// Returns [`MatchValueError::ArithmeticOverflow`] from checked non-forfeit math.
pub fn synthetic_stroke_reward_v1(
    config: OneHoleConfig,
    strokes: u16,
    completion: StrokeCompletion,
) -> Result<StrokeReward, MatchValueError> {
    if completion.is_forfeit() {
        if strokes > i16::MAX as u16 {
            return Err(MatchValueError::InvalidStrokeSettlement);
        }
        return Ok(StrokeReward::from_persisted(None, 0, 0));
    }
    let strokes = StrokeCount::new(strokes)?;
    let reward = synthetic_solo_reward_v1(config, strokes)?;
    Ok(StrokeReward::from_persisted(
        Some(reward.score()),
        reward.pang(),
        reward.experience(),
    ))
}

/// Exact persisted result for one participant, including its own post-commit balances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokePlayerResult {
    participant: StrokeParticipant,
    strokes: u16,
    place: StrokePlace,
    completion: StrokeCompletion,
    score: Option<i16>,
    pang_reward: u64,
    experience_reward: u64,
    pang_balance: u64,
    experience_balance: u64,
}

impl StrokePlayerResult {
    /// Constructs a result at the trusted persistence boundary.
    #[must_use]
    pub const fn new(
        input: StrokePlayerCommit,
        reward: StrokeReward,
        balances: ServerBalances,
    ) -> Self {
        Self {
            participant: input.participant,
            strokes: input.strokes,
            place: input.place,
            completion: input.completion,
            score: reward.score,
            pang_reward: reward.pang,
            experience_reward: reward.experience,
            pang_balance: balances.pang,
            experience_balance: balances.experience,
        }
    }
    /// Participant authority.
    #[must_use]
    pub const fn participant(self) -> StrokeParticipant {
        self.participant
    }
    /// Final authoritative strokes.
    #[must_use]
    pub const fn strokes(self) -> u16 {
        self.strokes
    }
    /// Final unique place.
    #[must_use]
    pub const fn place(self) -> StrokePlace {
        self.place
    }
    /// Final completion.
    #[must_use]
    pub const fn completion(self) -> StrokeCompletion {
        self.completion
    }
    /// Golf score, absent for forfeits.
    #[must_use]
    pub const fn score(self) -> Option<i16> {
        self.score
    }
    /// Pang reward.
    #[must_use]
    pub const fn pang_reward(self) -> u64 {
        self.pang_reward
    }
    /// Experience reward.
    #[must_use]
    pub const fn experience_reward(self) -> u64 {
        self.experience_reward
    }
    /// Pang balance after aggregate commit.
    #[must_use]
    pub const fn pang_balance(self) -> u64 {
        self.pang_balance
    }
    /// Experience balance after aggregate commit.
    #[must_use]
    pub const fn experience_balance(self) -> u64 {
        self.experience_balance
    }
}

/// Exact persisted exactly-two aggregate result in captured roster order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeMatchResult {
    match_id: MatchId,
    result_key: MatchResultKey,
    players: [StrokePlayerResult; 2],
}

impl StrokeMatchResult {
    /// Constructs an already validated persisted aggregate result.
    #[must_use]
    pub const fn new(
        match_id: MatchId,
        result_key: MatchResultKey,
        players: [StrokePlayerResult; 2],
    ) -> Self {
        Self {
            match_id,
            result_key,
            players,
        }
    }
    /// Durable aggregate ID.
    #[must_use]
    pub const fn match_id(self) -> MatchId {
        self.match_id
    }
    /// Aggregate result key.
    #[must_use]
    pub const fn result_key(self) -> MatchResultKey {
        self.result_key
    }
    /// Exact roster-ordered participant results.
    #[must_use]
    pub const fn players(&self) -> &[StrokePlayerResult; 2] {
        &self.players
    }
}

/// Rebuildable best-round projection for one account/course in stroke-two mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CourseRecord {
    account_id: AccountId,
    course_id: CourseId,
    best_score: i16,
    best_strokes: StrokeCount,
    rounds_completed: u64,
    best_match_id: MatchId,
    best_player_result_key: MatchResultKey,
    first_achieved_at: SystemTime,
    updated_at: SystemTime,
}

impl CourseRecord {
    /// Validates and reconstructs a persisted projection row.
    ///
    /// # Errors
    /// Returns [`MatchValueError::InvalidCourseRecord`] for a zero round count or inverted time.
    #[allow(clippy::too_many_arguments)]
    pub fn from_persisted(
        account_id: AccountId,
        course_id: CourseId,
        best_score: i16,
        best_strokes: StrokeCount,
        rounds_completed: u64,
        best_match_id: MatchId,
        best_player_result_key: MatchResultKey,
        first_achieved_at: SystemTime,
        updated_at: SystemTime,
    ) -> Result<Self, MatchValueError> {
        if rounds_completed == 0 || updated_at.duration_since(first_achieved_at).is_err() {
            return Err(MatchValueError::InvalidCourseRecord);
        }
        Ok(Self {
            account_id,
            course_id,
            best_score,
            best_strokes,
            rounds_completed,
            best_match_id,
            best_player_result_key,
            first_achieved_at,
            updated_at,
        })
    }
    /// Record owner.
    #[must_use]
    pub const fn account_id(self) -> AccountId {
        self.account_id
    }
    /// Course identity.
    #[must_use]
    pub const fn course_id(self) -> CourseId {
        self.course_id
    }
    /// Lowest score, with strokes as deterministic secondary ordering.
    #[must_use]
    pub const fn best_score(self) -> i16 {
        self.best_score
    }
    /// Best strokes.
    #[must_use]
    pub const fn best_strokes(self) -> StrokeCount {
        self.best_strokes
    }
    /// Count of eligible holed rounds.
    #[must_use]
    pub const fn rounds_completed(self) -> u64 {
        self.rounds_completed
    }
    /// Match currently establishing the best.
    #[must_use]
    pub const fn best_match_id(self) -> MatchId {
        self.best_match_id
    }
    /// Player settlement currently establishing the best.
    #[must_use]
    pub const fn best_player_result_key(self) -> MatchResultKey {
        self.best_player_result_key
    }
    /// First time the current best was achieved.
    #[must_use]
    pub const fn first_achieved_at(self) -> SystemTime {
        self.first_achieved_at
    }
    /// Last eligible projection update.
    #[must_use]
    pub const fn updated_at(self) -> SystemTime {
        self.updated_at
    }
}

/// Outcome of an idempotent stroke abort request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbortStrokeMatchOutcome {
    /// Nonterminal aggregate and all participants were aborted without reward.
    Aborted,
    /// Aggregate was already aborted.
    AlreadyAborted,
    /// A committed aggregate wins the abort race.
    AlreadyCommitted(StrokeMatchResult),
}

/// Outcome of an idempotent abort request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbortMatchOutcome {
    /// This request changed the match to aborted.
    Aborted,
    /// The match was already aborted; no row was mutated.
    AlreadyAborted,
    /// The match was already committed; abort is an explicit no-op.
    AlreadyCommitted(SoloMatchResult),
}

/// Checked maximum work for local startup cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IncompleteMatchAbortLimit(u32);

impl IncompleteMatchAbortLimit {
    /// Hard maximum number of matches processed in one startup cleanup.
    pub const MAX: u32 = 10_000;

    /// Validates a nonzero bounded cleanup limit.
    ///
    /// # Errors
    /// Returns [`MatchValueError::InvalidStrokes`] outside `1..=10000`.
    pub const fn new(value: u32) -> Result<Self, MatchValueError> {
        if value >= 1 && value <= Self::MAX {
            Ok(Self(value))
        } else {
            Err(MatchValueError::InvalidStrokes)
        }
    }

    /// Returns the SQL/application work cap.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Typed match persistence failures safe for application state mapping.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MatchRepositoryError {
    /// Match ID was not found.
    #[error("match was not found")]
    NotFound,
    /// A begin replay changed immutable input or reused an idempotency key.
    #[error("match begin input does not match persisted input")]
    InputDrift,
    /// The participant does not match the authoritative persisted account.
    #[error("match account does not match")]
    WrongAccount,
    /// Result idempotency key does not match.
    #[error("match result key does not match")]
    WrongResultKey,
    /// The requested lifecycle API does not match the persisted match mode/formula.
    #[error("match mode does not match the requested lifecycle API")]
    WrongMode,
    /// Course or one-hole configuration does not match.
    #[error("match configuration does not match")]
    WrongConfig,
    /// The match was aborted and cannot commit.
    #[error("match was aborted")]
    Aborted,
    /// Persisted lifecycle state cannot accept the operation.
    #[error("match status does not permit the operation")]
    InvalidStatus,
    /// Reward addition would exceed PostgreSQL's nonnegative `BIGINT` balance range.
    #[error("match reward would overflow the balance")]
    BalanceOverflow,
    /// Persisted match data violated domain invariants.
    #[error("persisted match data is invalid")]
    CorruptData,
    /// Startup recovery found more nonterminal rows than its explicit work cap.
    #[error("incomplete match recovery limit was exceeded")]
    RecoveryLimitExceeded,
    /// PostgreSQL operation failed.
    #[error("match storage operation failed")]
    Storage,
}

/// Technology-neutral match persistence contract.
pub trait MatchRepository: Send + Sync {
    /// Starts or exactly replays immutable exactly-two stroke input.
    fn begin_stroke(
        &self,
        _request: BeginStrokeMatch,
    ) -> RepositoryFuture<'_, Result<BeginStrokeMatchOutcome, MatchRepositoryError>> {
        Box::pin(async { Err(MatchRepositoryError::Storage) })
    }

    /// Marks an exact loaded stroke aggregate in-game, idempotently.
    fn mark_stroke_in_game(
        &self,
        _request: MarkStrokeInGame,
    ) -> RepositoryFuture<'_, Result<MarkStrokeInGameOutcome, MatchRepositoryError>> {
        Box::pin(async { Err(MatchRepositoryError::Storage) })
    }

    /// Aborts a noncommitted stroke aggregate and every captured participant.
    fn abort_stroke(
        &self,
        _request: AbortStrokeMatch,
    ) -> RepositoryFuture<'_, Result<AbortStrokeMatchOutcome, MatchRepositoryError>> {
        Box::pin(async { Err(MatchRepositoryError::Storage) })
    }

    /// Atomically settles both authoritative stroke participants.
    fn commit_stroke_match(
        &self,
        _request: CommitStrokeMatch,
    ) -> RepositoryFuture<'_, Result<StrokeMatchResult, MatchRepositoryError>> {
        Box::pin(async { Err(MatchRepositoryError::Storage) })
    }

    /// Starts or exactly replays immutable solo-match input.
    fn begin_solo(
        &self,
        request: BeginSoloMatch,
    ) -> RepositoryFuture<'_, Result<BeginSoloMatchOutcome, MatchRepositoryError>>;

    /// Marks an exact loaded solo match in-game, idempotently.
    fn mark_solo_in_game(
        &self,
        request: MarkSoloInGame,
    ) -> RepositoryFuture<'_, Result<MarkSoloInGameOutcome, MatchRepositoryError>>;

    /// Aborts a noncommitted match without reward, idempotently.
    fn abort(
        &self,
        request: AbortMatch,
    ) -> RepositoryFuture<'_, Result<AbortMatchOutcome, MatchRepositoryError>>;

    /// Commits server-computed rewards and returns the exact persisted result on replay.
    fn commit_solo_hole(
        &self,
        request: CommitSoloHole,
    ) -> RepositoryFuture<'_, Result<SoloMatchResult, MatchRepositoryError>>;

    /// Aborts at most `limit` nonterminal local matches during startup recovery.
    fn abort_incomplete_matches(
        &self,
        limit: IncompleteMatchAbortLimit,
    ) -> RepositoryFuture<'_, Result<u32, MatchRepositoryError>>;
}

/// Typed repository failures safe to map to user-facing outcomes.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RepositoryError {
    /// Normalized username already exists.
    #[error("username is already in use")]
    DuplicateUsername,
    /// Normalized nickname already exists.
    #[error("nickname is already in use")]
    DuplicateNickname,
    /// Requested aggregate was not found.
    #[error("record was not found")]
    NotFound,
    /// An account was not active.
    #[error("account is not active")]
    AccountInactive,
    /// Starter request was internally inconsistent.
    #[error("starter grant is invalid")]
    InvalidStarterGrant,
    /// Persisted data violated domain range/invariant checks.
    #[error("persisted data is invalid")]
    CorruptData,
    /// Storage is temporarily or permanently unavailable.
    #[error("storage operation failed")]
    Storage,
}

/// Typed handover consumption failures that do not reveal bearer material.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum HandoverError {
    /// Selector or digest did not identify a valid handover.
    #[error("handover is invalid")]
    Invalid,
    /// Handover was already consumed or revoked.
    #[error("handover is no longer available")]
    AlreadyConsumed,
    /// Handover expired.
    #[error("handover has expired")]
    Expired,
    /// Handover was presented to the wrong service.
    #[error("handover target does not match")]
    WrongTarget,
    /// Account is banned or disabled.
    #[error("account is not active")]
    AccountInactive,
    /// Storage failure.
    #[error("handover storage operation failed")]
    Storage,
}

/// Heap-allocated repository future used to keep domain contracts runtime-neutral.
pub type RepositoryFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Technology-neutral account aggregate repository contract.
pub trait AccountRepository: Send + Sync {
    /// Creates identity, credential, profile, and starter aggregate atomically.
    fn create_account(
        &self,
        request: NewAccount,
    ) -> RepositoryFuture<'_, Result<AccountAggregate, RepositoryError>>;

    /// Loads the minimum authentication projection.
    fn load_authentication<'a>(
        &'a self,
        username: &'a NormalizedUsername,
    ) -> RepositoryFuture<'a, Result<Option<AuthenticationRecord>, RepositoryError>>;

    /// Sets a nickname under database-enforced normalized uniqueness.
    fn set_nickname(
        &self,
        account_id: AccountId,
        nickname: Nickname,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>>;

    /// Returns whether a normalized nickname is currently unused.
    fn nickname_available<'a>(
        &'a self,
        nickname: &'a NormalizedNickname,
    ) -> RepositoryFuture<'a, Result<bool, RepositoryError>>;

    /// Applies/replays the configured starter grant without duplication.
    fn grant_starter(
        &self,
        account_id: AccountId,
        grant: StarterGrant,
    ) -> RepositoryFuture<'_, Result<AccountAggregate, RepositoryError>>;

    /// Changes account status and revokes outstanding handovers when inactive.
    fn set_status(
        &self,
        account_id: AccountId,
        status: AccountStatus,
        now: SystemTime,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>>;
}

/// Technology-neutral coherent player-bootstrap repository contract.
pub trait PlayerRepository: Send + Sync {
    /// Loads one coherent active/complete player bootstrap snapshot by authenticated account ID.
    fn load_player_snapshot(
        &self,
        account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<PlayerSnapshot, RepositoryError>>;
}

/// Technology-neutral single-use handover repository contract.
pub trait HandoverRepository: Send + Sync {
    /// Persists a generated digest after verifying that the account is active.
    fn issue(&self, handover: NewHandover) -> RepositoryFuture<'_, Result<(), HandoverError>>;

    /// Locks, validates, consumes, and commits one handover atomically.
    fn consume(
        &self,
        request: ConsumeHandover,
    ) -> RepositoryFuture<'_, Result<AuthenticatedSession, HandoverError>>;
}

/// Validation failure for bounded room input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RoomValueError {
    /// The UTF-8 byte length is outside the accepted range.
    #[error("room value length is outside policy")]
    InvalidLength,
    /// The value contains NUL or a control character.
    #[error("room value contains a control character")]
    ControlCharacter,
    /// The room member capacity is outside `2..=30`.
    #[error("room capacity is outside policy")]
    InvalidCapacity,
}

/// A checked, nonzero process-local room identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RoomId(std::num::NonZeroU32);

impl RoomId {
    /// Creates a nonzero room identifier.
    ///
    /// # Errors
    /// Returns [`IdError::NotPositive`] for zero.
    pub const fn new(value: u32) -> Result<Self, IdError> {
        match std::num::NonZeroU32::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(IdError::NotPositive),
        }
    }

    /// Returns the unsigned identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl TryFrom<u32> for RoomId {
    type Error = IdError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// A checked, nonzero process-local player connection identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlayerConnectionId(std::num::NonZeroU64);

impl PlayerConnectionId {
    /// Creates a nonzero connection identifier.
    ///
    /// # Errors
    /// Returns [`IdError::NotPositive`] for zero.
    pub const fn new(value: u64) -> Result<Self, IdError> {
        match std::num::NonZeroU64::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(IdError::NotPositive),
        }
    }

    /// Returns the unsigned identifier.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl TryFrom<u64> for PlayerConnectionId {
    type Error = IdError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// A trimmed, bounded room display name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RoomName(String);

impl RoomName {
    /// Trims Unicode whitespace and validates `1..=32` UTF-8 bytes without controls.
    ///
    /// # Errors
    /// Returns [`RoomValueError`] when the input violates policy.
    pub fn parse(value: &str) -> Result<Self, RoomValueError> {
        let value = value.trim();
        validate_room_text(value, 32)?;
        Ok(Self(value.to_owned()))
    }

    /// Returns the validated display name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoomName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for RoomName {
    type Err = RoomValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// A bounded chat message preserved exactly as entered.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ChatText(String);

impl ChatText {
    /// Validates `1..=128` UTF-8 bytes without NUL or control characters.
    ///
    /// # Errors
    /// Returns [`RoomValueError`] when the input violates policy.
    pub fn parse(value: &str) -> Result<Self, RoomValueError> {
        validate_room_text(value, 128)?;
        Ok(Self(value.to_owned()))
    }

    /// Returns the validated message.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ChatText {
    type Err = RoomValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

fn validate_room_text(value: &str, maximum: usize) -> Result<(), RoomValueError> {
    if !(1..=maximum).contains(&value.len()) {
        return Err(RoomValueError::InvalidLength);
    }
    if value.chars().any(char::is_control) {
        return Err(RoomValueError::ControlCharacter);
    }
    Ok(())
}

/// Ephemeral room password input that redacts formatting and zeroes its allocation on drop.
#[derive(Eq, PartialEq)]
pub struct RoomPassword(Vec<u8>);

impl RoomPassword {
    /// Copies a `1..=16` byte password into zeroizing storage.
    ///
    /// # Errors
    /// Returns [`RoomValueError::InvalidLength`] outside the byte bound.
    pub fn parse(value: &str) -> Result<Self, RoomValueError> {
        if !(1..=16).contains(&value.len()) {
            return Err(RoomValueError::InvalidLength);
        }
        Ok(Self(value.as_bytes().to_vec()))
    }

    /// Borrows password bytes only for immediate digesting or verification.
    #[must_use]
    pub fn expose_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for RoomPassword {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for RoomPassword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RoomPassword([REDACTED])")
    }
}

/// Validated mutable room settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomSettings {
    max_members: u8,
}

impl RoomSettings {
    /// Creates settings with a member capacity in `2..=30`.
    ///
    /// # Errors
    /// Returns [`RoomValueError::InvalidCapacity`] outside the range.
    pub const fn new(max_members: u8) -> Result<Self, RoomValueError> {
        if max_members >= 2 && max_members <= 30 {
            Ok(Self { max_members })
        } else {
            Err(RoomValueError::InvalidCapacity)
        }
    }

    /// Returns the maximum room membership.
    #[must_use]
    pub const fn max_members(self) -> u8 {
        self.max_members
    }
}

/// Immutable public member projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberSnapshot {
    connection_id: PlayerConnectionId,
    account_id: AccountId,
    nickname: String,
    owner: bool,
    ready: bool,
}

impl MemberSnapshot {
    /// Constructs a projection at the owning room boundary.
    #[must_use]
    pub fn new(
        connection_id: PlayerConnectionId,
        account_id: AccountId,
        nickname: String,
        owner: bool,
        ready: bool,
    ) -> Self {
        Self {
            connection_id,
            account_id,
            nickname,
            owner,
            ready,
        }
    }

    /// Connection identity.
    #[must_use]
    pub const fn connection_id(&self) -> PlayerConnectionId {
        self.connection_id
    }
    /// Durable account identity.
    #[must_use]
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }
    /// Display nickname.
    #[must_use]
    pub fn nickname(&self) -> &str {
        &self.nickname
    }
    /// Whether this member owns the room.
    #[must_use]
    pub const fn is_owner(&self) -> bool {
        self.owner
    }
    /// Whether this member is ready.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.ready
    }
}

/// Immutable lobby projection of a room.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomSummary {
    id: RoomId,
    name: RoomName,
    owner_nickname: String,
    members: u8,
    max_members: u8,
    password_protected: bool,
}

impl RoomSummary {
    /// Constructs a summary at the owning room boundary.
    #[must_use]
    pub fn new(
        id: RoomId,
        name: RoomName,
        owner_nickname: String,
        members: u8,
        max_members: u8,
        password_protected: bool,
    ) -> Self {
        Self {
            id,
            name,
            owner_nickname,
            members,
            max_members,
            password_protected,
        }
    }
    /// Room identity.
    #[must_use]
    pub const fn id(&self) -> RoomId {
        self.id
    }
    /// Display name.
    #[must_use]
    pub const fn name(&self) -> &RoomName {
        &self.name
    }
    /// Current owner's display nickname.
    #[must_use]
    pub fn owner_nickname(&self) -> &str {
        &self.owner_nickname
    }
    /// Current membership count.
    #[must_use]
    pub const fn members(&self) -> u8 {
        self.members
    }
    /// Current capacity.
    #[must_use]
    pub const fn max_members(&self) -> u8 {
        self.max_members
    }
    /// Whether joining requires a password.
    #[must_use]
    pub const fn password_protected(&self) -> bool {
        self.password_protected
    }
}

/// Immutable full room projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomSnapshot {
    summary: RoomSummary,
    members: Vec<MemberSnapshot>,
}

impl RoomSnapshot {
    /// Constructs a room projection at the owning room boundary.
    #[must_use]
    pub fn new(summary: RoomSummary, members: Vec<MemberSnapshot>) -> Self {
        Self { summary, members }
    }
    /// Public room summary.
    #[must_use]
    pub const fn summary(&self) -> &RoomSummary {
        &self.summary
    }
    /// Members in deterministic join order.
    #[must_use]
    pub fn members(&self) -> &[MemberSnapshot] {
        &self.members
    }
}

/// Stable room/lobby operation failure safe for application mapping.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RoomError {
    /// A bounded command queue has no remaining capacity.
    #[error("room command queue is full")]
    QueueFull,
    /// The room or registry actor is no longer available.
    #[error("room is closed")]
    Closed,
    /// The connection already belongs to this or another room.
    #[error("connection already belongs to a room")]
    AlreadyMember,
    /// The room has reached capacity.
    #[error("room is full")]
    Full,
    /// The supplied password did not authenticate.
    #[error("room password is invalid")]
    InvalidPassword,
    /// The connection is not a room member.
    #[error("connection is not a room member")]
    NotMember,
    /// The operation requires the current owner.
    #[error("operation requires room owner")]
    NotOwner,
    /// An owner cannot kick itself.
    #[error("room owner cannot kick itself")]
    CannotKickSelf,
    /// The requested member was not found.
    #[error("room member was not found")]
    MemberNotFound,
    /// Capacity cannot be lowered below current occupancy.
    #[error("room capacity is below current occupancy")]
    CapacityBelowOccupancy,
    /// The lobby room limit has been reached.
    #[error("lobby room limit reached")]
    MaxRooms,
    /// The requested room does not exist.
    #[error("room was not found")]
    RoomNotFound,
    /// No unused room identifier can be allocated.
    #[error("room identifier space exhausted")]
    IdExhausted,
    /// A bounded control or shutdown deadline elapsed.
    #[error("room operation timed out")]
    Timeout,
    /// Room membership/settings/chat mutations are blocked by an active solo match.
    #[error("room operation is blocked while a match is active")]
    MatchActive,
}

/// Marker retained for the M1 crate-boundary test.
#[must_use]
pub const fn crate_boundary() -> &'static str {
    "domain"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_keeps_display_separate() {
        let username = Username::parse("  Player_One\t").expect("valid username");
        assert_eq!(username.display(), "Player_One");
        assert_eq!(username.normalized().as_str(), "player_one");

        let nickname = Nickname::parse(" Pang-Ya ").expect("valid nickname");
        assert_eq!(nickname.display(), "Pang-Ya");
        assert_eq!(nickname.normalized().as_str(), "pang-ya");
    }

    #[test]
    fn identifiers_check_database_ranges() {
        assert_eq!(AccountId::new(0), Err(IdError::NotPositive));
        assert_eq!(ItemTypeId::try_from(-1_i64), Err(IdError::OutOfRange));
        assert_eq!(
            ItemTypeId::try_from(i64::from(u32::MAX) + 1),
            Err(IdError::OutOfRange)
        );
    }

    #[test]
    fn sensitive_values_are_redacted() {
        let credential = CredentialHash::new("secret-phc".to_owned());
        let digest = HandoverDigest::new([7; 32]);
        assert!(!format!("{credential:?}").contains("secret-phc"));
        assert!(!format!("{digest:?}").contains('7'));
    }

    #[test]
    fn match_seed_and_begin_request_debug_redact_seed_bytes() {
        let seed = MatchSeed::new([231; 32]);
        assert_eq!(format!("{seed:?}"), "MatchSeed([REDACTED])");
        assert!(!format!("{seed:?}").contains("231"));

        let begin = BeginSoloMatch::new(
            MatchId::new(Uuid::nil()),
            MatchResultKey::new(Uuid::nil()),
            AccountId::new(1).expect("account"),
            OneHoleConfig::new(CourseId::new(7).expect("course"), 3).expect("configuration"),
            CatalogFingerprint::new([0; 32]),
            seed,
            Weather::Clear,
            WindConditions::new(87, 231).expect("wind"),
        );
        let debug = format!("{begin:?}");
        assert!(debug.contains("MatchSeed([REDACTED])"));
        assert!(debug.contains("speed_tenths: 87"));
        assert!(debug.contains("angle_degrees: 231"));
        assert!(!debug.contains("[231, 231"));
    }

    #[test]
    fn source_addresses_are_masked_and_only_canonical_prefixes_parse() {
        let ipv4 = SourceAddressPrefix::from_ip("192.0.2.199".parse().expect("IPv4"));
        assert_eq!(ipv4.as_str(), "192.0.2.0/24");
        assert_eq!(SourceAddressPrefix::parse(ipv4.as_str()), Ok(ipv4));

        let ipv6 = SourceAddressPrefix::from_ip("2001:db8:1234:56ff::1".parse().expect("IPv6"));
        assert_eq!(ipv6.as_str(), "2001:db8:1234:5600::/56");
        assert_eq!(SourceAddressPrefix::parse(ipv6.as_str()), Ok(ipv6));
        assert_eq!(
            SourceAddressPrefix::parse("192.0.2.199/24"),
            Err(SourceAddressError::Invalid)
        );
        assert_eq!(
            SourceAddressPrefix::parse("2001:db8:1234:5600::/64"),
            Err(SourceAddressError::Invalid)
        );
    }

    #[test]
    fn room_ids_are_checked_nonzero() {
        assert_eq!(RoomId::new(0), Err(IdError::NotPositive));
        assert_eq!(PlayerConnectionId::new(0), Err(IdError::NotPositive));
        assert_eq!(RoomId::new(7).map(RoomId::get), Ok(7));
        assert_eq!(
            PlayerConnectionId::new(9).map(PlayerConnectionId::get),
            Ok(9)
        );
    }

    #[test]
    fn room_text_is_utf8_byte_bounded_and_control_free() {
        assert_eq!(
            RoomName::parse("  Pangya  ").map(|name| name.0),
            Ok("Pangya".into())
        );
        assert_eq!(RoomName::parse("\0"), Err(RoomValueError::ControlCharacter));
        assert_eq!(
            RoomName::parse(&"é".repeat(17)),
            Err(RoomValueError::InvalidLength)
        );
        assert_eq!(
            ChatText::parse("hello\n"),
            Err(RoomValueError::ControlCharacter)
        );
        assert!(ChatText::parse(&"é".repeat(64)).is_ok());
        assert_eq!(
            ChatText::parse(&"é".repeat(65)),
            Err(RoomValueError::InvalidLength)
        );
    }

    #[test]
    fn synthetic_match_values_and_rewards_are_checked() {
        let course = CourseId::new(7).expect("course");
        let config = OneHoleConfig::new(course, 3).expect("configuration");
        assert_eq!(config.hole(), 1);
        assert_eq!(
            synthetic_solo_reward_v1(config, StrokeCount::new(2).expect("strokes")),
            Ok(SoloReward::from_persisted(-1, 12, 5))
        );
        assert_eq!(
            synthetic_solo_reward_v1(config, StrokeCount::new(5).expect("strokes")),
            Ok(SoloReward::from_persisted(2, 10, 5))
        );
        assert_eq!(CourseId::new(0), Err(MatchValueError::InvalidCourse));
        assert_eq!(
            OneHoleConfig::new(course, 0),
            Err(MatchValueError::InvalidPar)
        );
        assert_eq!(StrokeCount::new(0), Err(MatchValueError::InvalidStrokes));
        assert_eq!(
            WindConditions::new(151, 0),
            Err(MatchValueError::InvalidWindSpeed)
        );
        assert_eq!(
            WindConditions::new(0, 360),
            Err(MatchValueError::InvalidWindAngle)
        );
        let wind = WindConditions::new(150, 359).expect("maximum wind");
        assert_eq!(wind.speed_tenths(), 150);
        assert_eq!(wind.angle_degrees(), 359);
        assert_eq!(
            synthetic_stroke_reward_v1(config, 2, StrokeCompletion::Holed),
            Ok(StrokeReward::from_persisted(Some(-1), 12, 5))
        );
        assert_eq!(
            synthetic_stroke_reward_v1(config, 0, StrokeCompletion::Disconnect),
            Ok(StrokeReward::from_persisted(None, 0, 0))
        );
    }

    #[test]
    fn stroke_aggregate_values_enforce_exact_roster_and_unique_places() {
        let account_a = AccountId::new(1).expect("account");
        let account_b = AccountId::new(2).expect("account");
        let aggregate_key = MatchResultKey::new(Uuid::from_u128(1));
        let first = StrokeParticipant::new(
            account_a,
            StrokeRosterOrder::First,
            MatchResultKey::new(Uuid::from_u128(2)),
        );
        let second = StrokeParticipant::new(
            account_b,
            StrokeRosterOrder::Second,
            MatchResultKey::new(Uuid::from_u128(3)),
        );
        let config =
            OneHoleConfig::new(CourseId::new(7).expect("course"), 3).expect("configuration");
        assert!(
            BeginStrokeMatch::new(
                MatchId::new(Uuid::from_u128(4)),
                aggregate_key,
                [first, second],
                config,
                CatalogFingerprint::new([1; 32]),
                MatchSeed::new([2; 32]),
                Weather::Clear,
                WindConditions::new(0, 0).expect("wind"),
            )
            .is_ok()
        );
        assert_eq!(
            BeginStrokeMatch::new(
                MatchId::new(Uuid::from_u128(4)),
                aggregate_key,
                [first, first],
                config,
                CatalogFingerprint::new([1; 32]),
                MatchSeed::new([2; 32]),
                Weather::Clear,
                WindConditions::new(0, 0).expect("wind"),
            ),
            Err(MatchValueError::InvalidStrokeRoster)
        );
        let first_commit =
            StrokePlayerCommit::new(first, 2, StrokePlace::First, StrokeCompletion::Holed)
                .expect("first result");
        let duplicate_place =
            StrokePlayerCommit::new(second, 0, StrokePlace::First, StrokeCompletion::GiveUp)
                .expect("forfeit result");
        assert_eq!(
            CommitStrokeMatch::new(
                MatchId::new(Uuid::from_u128(4)),
                aggregate_key,
                config,
                [first_commit, duplicate_place],
            ),
            Err(MatchValueError::InvalidStrokeSettlement)
        );
    }

    #[test]
    fn password_is_redacted_and_settings_are_bounded() {
        let password = RoomPassword::parse("secret").expect("valid password");
        assert_eq!(password.expose_bytes(), b"secret");
        assert_eq!(format!("{password:?}"), "RoomPassword([REDACTED])");
        assert_eq!(RoomPassword::parse(""), Err(RoomValueError::InvalidLength));
        assert_eq!(RoomSettings::new(1), Err(RoomValueError::InvalidCapacity));
        assert_eq!(RoomSettings::new(30).map(RoomSettings::max_members), Ok(30));
    }
}
