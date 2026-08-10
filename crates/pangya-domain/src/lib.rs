#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Technology-neutral domain types and repository contracts for account storage.

use std::{
    collections::BTreeMap,
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
uuid_id!(
    EconomyOperationId,
    "A client-generated idempotency key for one authenticated economy command."
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

/// Operator authorisation level held by an account.
///
/// This is deliberately a property of the same account a player logs in with rather than a
/// separate operator identity: an admin action is then attributable to a real, auditable
/// account, and there is one credential policy to keep correct instead of two.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AccountRole {
    /// No operator authority.
    #[default]
    Player,
    /// May use the operator admin surface.
    Admin,
}

impl AccountRole {
    /// Returns the stored spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Admin => "admin",
        }
    }

    /// Parses a stored spelling.
    ///
    /// # Errors
    /// Returns [`IdError::OutOfRange`] for any value the schema does not permit.
    pub fn parse(value: &str) -> Result<Self, IdError> {
        match value {
            "player" => Ok(Self::Player),
            "admin" => Ok(Self::Admin),
            _ => Err(IdError::OutOfRange),
        }
    }
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
    /// Stored profile nickname, when setup has selected one.
    pub nickname: Option<String>,
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

/// Closed inventory classification persisted independently from mutable equipment.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InventoryClass {
    /// Row created before M7; catalog validation resolves its effective family.
    Legacy,
    /// Unique club-set row.
    ClubSet,
    /// Unique ball row.
    Ball,
    /// Stackable consumable row.
    Consumable,
    /// Unique character-part row (purchase only in M7).
    CharacterPart,
    /// Owned caddie row.
    Caddie,
    /// Caddie-equippable item row.
    CaddieItem,
    /// Owned mascot row.
    Mascot,
    /// Owned card row.
    Card,
    /// My-room furniture row.
    Furniture,
    /// Character skin row.
    Skin,
    /// Hair style row.
    HairStyle,
    /// Bundled clothing set row.
    SetItem,
}

/// Current durability for an inventory row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InventoryDurability {
    /// The catalog definition is not durable.
    Nondurable,
    /// Remaining points for a durable item, including zero when depleted.
    Durable(u32),
}

/// Persisted inventory projection. `starter_key` remains the bounded acquisition key so
/// existing starter construction/replay remains source- and data-compatible.
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
    /// Stable starter or `purchase.<uuid-simple>` acquisition key.
    pub starter_key: StarterKey,
    /// Persisted closed classification (`Legacy` for all pre-M7 rows).
    pub class: InventoryClass,
    /// Closed current durability state.
    pub durability: InventoryDurability,
    /// Optional expiry instant; M7 does not create expiring purchases.
    pub expires_at: Option<SystemTime>,
}

/// Closed catalog item family used by economy requests and validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ItemKind {
    /// Owned character.
    Character,
    /// Club set.
    ClubSet,
    /// Ball.
    Ball,
    /// Stackable consumable.
    Consumable,
    /// Character-compatible part (equip is deferred).
    CharacterPart,
    /// Owned caddie.
    Caddie,
    /// Caddie-equippable item.
    CaddieItem,
    /// Owned mascot.
    Mascot,
    /// Owned card.
    Card,
    /// My-room furniture.
    Furniture,
    /// Character skin.
    Skin,
    /// Hair style.
    HairStyle,
    /// Bundled clothing set.
    SetItem,
}

/// Closed shop sale policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemSale {
    /// The definition cannot be bought.
    NotSold,
    /// Authoritative Pang unit price.
    Pang(u64),
}

/// Closed quantity semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemStacking {
    /// Exactly one owned row with quantity one per purchase.
    Unique,
    /// One owner/type stack capped at this positive quantity.
    Stackable {
        /// Positive catalog cap, bounded by the parser.
        max_stack: u32,
    },
}

/// Closed durability semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemDurability {
    /// No durability column value and no repair.
    Nondurable,
    /// Positive maximum and positive Pang repair rate per missing point.
    Durable {
        /// Maximum and initial purchased durability.
        max: u32,
        /// Authoritative repair Pang per missing point.
        repair_pang_per_point: u32,
    },
}

/// Optional character compatibility for a character part.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemCompatibility {
    /// No character restriction.
    Any,
    /// Exact compatible character catalog type.
    Character(ItemTypeId),
}

/// Immutable server-resolved catalog definition crossing into storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemDefinition {
    /// Globally unique catalog type.
    pub type_id: ItemTypeId,
    /// Closed family.
    pub kind: ItemKind,
    /// Sale policy and authoritative price.
    pub sale: ItemSale,
    /// Quantity policy.
    pub stacking: ItemStacking,
    /// Durability policy.
    pub durability: ItemDurability,
    /// Character compatibility where applicable.
    pub compatibility: ItemCompatibility,
}

impl ItemDefinition {
    /// Returns the Pang price only when this is a sold Pang offer.
    #[must_use]
    pub const fn pang_price(self) -> Option<u64> {
        match self.sale {
            ItemSale::NotSold => None,
            ItemSale::Pang(price) => Some(price),
        }
    }
}

/// One inventory selector paired with its server-resolved immutable definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EconomyItemSelector {
    /// Owned inventory row.
    pub inventory_id: InventoryItemId,
    /// Definition resolved from the active catalog, never from the wire.
    pub definition: ItemDefinition,
}

/// Atomic purchase input. Price/outcome/balance are deliberately absent from client input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PurchaseRequest {
    /// Authenticated account.
    pub account_id: AccountId,
    /// Stable command key.
    pub operation_id: EconomyOperationId,
    /// Active catalog fingerprint.
    pub catalog: CatalogFingerprint,
    /// Server-resolved definition containing authoritative price and rules.
    pub definition: ItemDefinition,
    /// Client-selected positive quantity; unique definitions require one.
    pub quantity: u32,
}

/// Exact committed purchase result stored for replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PurchaseResult {
    /// Operation key.
    pub operation_id: EconomyOperationId,
    /// Granted or stacked inventory row.
    pub inventory_id: InventoryItemId,
    /// Catalog type.
    pub item_type_id: ItemTypeId,
    /// Quantity after commit.
    pub quantity_after: u32,
    /// Durability after commit, if durable.
    pub durability: Option<u32>,
    /// Pang balance after commit.
    pub pang_balance: u64,
}

/// Owned-compatible equipment mutation input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EquipmentChange {
    /// Authenticated account.
    pub account_id: AccountId,
    /// Stable command key.
    pub operation_id: EconomyOperationId,
    /// Active catalog fingerprint.
    pub catalog: CatalogFingerprint,
    /// Expected optimistic equipment version.
    pub expected_version: u32,
    /// Selected owned character row.
    pub character_id: CharacterId,
    /// Server-resolved character type used to reject drift/corruption.
    pub character_type_id: ItemTypeId,
    /// Optional selected owned club.
    pub club: Option<EconomyItemSelector>,
    /// Optional selected owned ball.
    pub ball: Option<EconomyItemSelector>,
}

/// Exact committed equipment result stored for replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EquipmentChangeResult {
    /// Operation key.
    pub operation_id: EconomyOperationId,
    /// Selected character.
    pub character_id: CharacterId,
    /// Selected club.
    pub club_item_id: Option<InventoryItemId>,
    /// Selected ball.
    pub ball_item_id: Option<InventoryItemId>,
    /// Incremented version.
    pub version: u32,
}

/// Consume exactly one server-validated consumable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsumeItem {
    /// Authenticated account.
    pub account_id: AccountId,
    /// Stable command key.
    pub operation_id: EconomyOperationId,
    /// Active catalog fingerprint.
    pub catalog: CatalogFingerprint,
    /// Owned row and authoritative definition.
    pub item: EconomyItemSelector,
}

/// Exact committed consumption result stored for replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsumeItemResult {
    /// Operation key.
    pub operation_id: EconomyOperationId,
    /// Consumed row (possibly deleted).
    pub inventory_id: InventoryItemId,
    /// Type identifier.
    pub item_type_id: ItemTypeId,
    /// Remaining quantity; zero means the row was deleted.
    pub quantity_after: u32,
}

/// Restore one durable club to its catalog maximum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepairItem {
    /// Authenticated account.
    pub account_id: AccountId,
    /// Stable command key.
    pub operation_id: EconomyOperationId,
    /// Active catalog fingerprint.
    pub catalog: CatalogFingerprint,
    /// Owned club and authoritative repair definition.
    pub item: EconomyItemSelector,
}

/// Exact committed repair result stored for replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepairItemResult {
    /// Operation key.
    pub operation_id: EconomyOperationId,
    /// Repaired row.
    pub inventory_id: InventoryItemId,
    /// Restored durability.
    pub durability: u32,
    /// Authoritative Pang cost.
    pub pang_cost: u64,
    /// Pang balance after commit.
    pub pang_balance: u64,
}

/// Whether an exact successful result was newly committed or replayed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EconomyCommit<T> {
    /// This call committed the mutation.
    Committed(T),
    /// This call returned an immutable prior successful result.
    Replayed(T),
}

impl<T> EconomyCommit<T> {
    /// Returns whether this result applied a new mutation.
    #[must_use]
    pub const fn was_applied(&self) -> bool {
        matches!(self, Self::Committed(_))
    }

    /// Splits the durable result from its applied-versus-replayed outcome.
    #[must_use]
    pub fn into_parts(self) -> (T, bool) {
        match self {
            Self::Committed(value) => (value, true),
            Self::Replayed(value) => (value, false),
        }
    }
}

/// Stable economy failures safe for application outcome mapping.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum EconomyError {
    /// Definition/quantity/operation identifier is invalid or the item is not sold.
    #[error("economy request is invalid")]
    Invalid,
    /// Account is not active.
    #[error("account is not active")]
    AccountInactive,
    /// Pang balance cannot cover the authoritative cost.
    #[error("insufficient Pang")]
    InsufficientPang,
    /// Owned character or item does not exist for the authenticated account.
    #[error("item is not owned")]
    NotOwned,
    /// Item family or part compatibility is wrong for this operation.
    #[error("item is incompatible")]
    Incompatible,
    /// Item has expired.
    #[error("item has expired")]
    Expired,
    /// Durable item has no usable durability.
    #[error("item is depleted")]
    Depleted,
    /// Consumable stack would exceed its catalog cap.
    #[error("item stack is full")]
    StackFull,
    /// Equipment optimistic version changed.
    #[error("equipment version conflicts")]
    VersionConflict,
    /// A successful operation key was reused with different normalized client input.
    #[error("economy operation input drifted")]
    IdempotencyDrift,
    /// Checked price, quantity, balance, durability, or version arithmetic overflowed.
    #[error("economy arithmetic overflow")]
    ArithmeticOverflow,
    /// Persisted rows violate catalog/domain invariants.
    #[error("persisted economy data is invalid")]
    CorruptData,
    /// PostgreSQL operation failed.
    #[error("economy storage operation failed: {0}")]
    Storage(StorageFault),
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

/// The nonsecret selector for an operator admin session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AdminSessionId(Uuid);

impl AdminSessionId {
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

impl From<Uuid> for AdminSessionId {
    fn from(value: Uuid) -> Self {
        Self::new(value)
    }
}

/// Digest-only admin session persistence request.
///
/// Mirrors [`NewHandover`] deliberately: an admin session is a bearer like any other, so it
/// gets the same nonsecret-selector-plus-digest treatment rather than a second scheme.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAdminSession {
    /// Nonsecret selector.
    pub id: AdminSessionId,
    /// Authenticated account.
    pub account_id: AccountId,
    /// Stored bearer digest.
    pub digest: HandoverDigest,
    /// Privacy-minimized source network; a raw peer address is never persisted.
    pub source_address_prefix: SourceAddressPrefix,
    /// Creation time supplied by the application clock.
    pub issued_at: SystemTime,
    /// Strict expiry time.
    pub expires_at: SystemTime,
}

/// Request to validate a presented admin session bearer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveAdminSession {
    /// Nonsecret selector.
    pub id: AdminSessionId,
    /// Digest derived from the presented bearer.
    pub digest: HandoverDigest,
    /// Current time supplied by the application clock.
    pub now: SystemTime,
}

/// An authenticated, still-valid admin session and the account behind it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminSession {
    /// Selector, retained for audit correlation and revocation.
    pub id: AdminSessionId,
    /// Authenticated account.
    pub account_id: AccountId,
    /// Display username, so the panel need not issue a second query to greet its operator.
    pub username_display: String,
    /// Authorisation level at resolution time, not at issue time.
    pub role: AccountRole,
    /// Strict expiry time.
    pub expires_at: SystemTime,
}

/// One append-only record of an operator action taken through the admin surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAdminAuditEvent {
    /// Signed-in account that performed the action.
    pub actor_account_id: AccountId,
    /// Lowercase dotted verb, for example `account.balance.grant`.
    pub action: String,
    /// Account the action was performed against, when there is one.
    pub target_account_id: Option<AccountId>,
    /// Nonsecret structured detail. Must serialise to a JSON object.
    pub detail: String,
}

/// One persisted admin audit row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminAuditEvent {
    /// Monotonic identifier.
    pub id: i64,
    /// Signed-in account that performed the action.
    pub actor_account_id: AccountId,
    /// Display username of the actor.
    pub actor_username: String,
    /// Lowercase dotted verb.
    pub action: String,
    /// Account the action was performed against, when there is one.
    pub target_account_id: Option<AccountId>,
    /// Nonsecret structured detail as JSON text.
    pub detail: String,
    /// Server time the row was written.
    pub occurred_at: SystemTime,
}

/// Validation failure for checked match values, including retail whole-card plans.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MatchValueError {
    /// Course zero is reserved and cannot identify a configured course.
    #[error("course identifier must be nonzero")]
    InvalidCourse,
    /// Hole count must be in `1..=18`.
    #[error("hole count is outside policy")]
    InvalidHole,
    /// Hole progression must be one of front, back, random, or shuffle.
    #[error("hole progression is outside policy")]
    InvalidHoleMode,
    /// Match par must be in `1..=10`.
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

/// Immutable match plan carrying the selected course and hole progression.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MatchPlan {
    course_id: CourseId,
    hole_count: u8,
    hole_mode: u8,
    par: u8,
}

impl MatchPlan {
    /// Lowest accepted par.
    pub const MIN_PAR: u8 = 1;
    /// Highest accepted par.
    pub const MAX_PAR: u8 = 10;

    /// Reports whether a par is inside the accepted range.
    ///
    /// Exposed so a caller that has a par but no course yet — configuration validation, for
    /// one — can check the value against this one definition instead of restating the bound.
    #[must_use]
    pub const fn par_in_range(par: u8) -> bool {
        par >= Self::MIN_PAR && par <= Self::MAX_PAR
    }

    /// Validates a room-driven plan with an explicit hole count and progression mode.
    pub const fn with_holes(
        course_id: CourseId,
        hole_count: u8,
        hole_mode: u8,
        par: u8,
    ) -> Result<Self, MatchValueError> {
        if hole_count == 0 || hole_count > 18 {
            return Err(MatchValueError::InvalidHole);
        }
        if hole_mode > 3 {
            return Err(MatchValueError::InvalidHoleMode);
        }
        if Self::par_in_range(par) {
            Ok(Self {
                course_id,
                hole_count,
                hole_mode,
                par,
            })
        } else {
            Err(MatchValueError::InvalidPar)
        }
    }

    /// Configured course identifier.
    #[must_use]
    pub const fn course_id(self) -> CourseId {
        self.course_id
    }

    /// Number of holes in this plan.
    #[must_use]
    pub const fn hole_count(self) -> u8 {
        self.hole_count
    }

    /// Hole progression mode from the room settings.
    #[must_use]
    pub const fn hole_mode(self) -> u8 {
        self.hole_mode
    }

    /// Configured first-hole ordinal for consumers that address the current card entry.
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

/// Immutable request to begin one synthetic solo card.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BeginSoloMatch {
    match_id: MatchId,
    result_key: MatchResultKey,
    account_id: AccountId,
    config: MatchPlan,
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
        config: MatchPlan,
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
    pub const fn config(&self) -> MatchPlan {
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

/// Request to commit one completed solo card. Rewards and balances are intentionally absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitSoloHole {
    match_id: MatchId,
    result_key: MatchResultKey,
    account_id: AccountId,
    config: MatchPlan,
    strokes: StrokeCount,
}

impl CommitSoloHole {
    /// Constructs a commit request from authoritative identity/config and client stroke evidence.
    #[must_use]
    pub const fn new(
        match_id: MatchId,
        result_key: MatchResultKey,
        account_id: AccountId,
        config: MatchPlan,
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
    /// Authoritative whole-card configuration.
    #[must_use]
    pub const fn config(self) -> MatchPlan {
        self.config
    }
    /// Checked stroke evidence.
    #[must_use]
    pub const fn strokes(self) -> StrokeCount {
        self.strokes
    }
}

/// Checked score and server-computed rewards for one synthetic card.
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
    config: MatchPlan,
    strokes: StrokeCount,
) -> Result<SoloReward, MatchValueError> {
    let strokes_i32 = i32::from(strokes.get());
    let par_i32 = i32::from(config.par())
        .checked_mul(i32::from(config.hole_count()))
        .ok_or(MatchValueError::ArithmeticOverflow)?;
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
    config: MatchPlan,
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
        config: MatchPlan,
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
    pub const fn config(&self) -> MatchPlan {
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
    /// The opponent forfeited; no golf score or course record is fabricated.
    WinnerByForfeit,
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
        matches!(
            self,
            Self::GiveUp | Self::Disconnect | Self::TurnTimeout | Self::GameTimeout
        )
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
    /// Holed/stroke-cap finishes require a positive persisted `SMALLINT`; forfeits and a
    /// truthful winner-by-forfeit permit zero authoritative strokes.
    pub const fn new(
        participant: StrokeParticipant,
        strokes: u16,
        place: StrokePlace,
        completion: StrokeCompletion,
    ) -> Result<Self, MatchValueError> {
        if strokes > i16::MAX as u16
            || (matches!(
                completion,
                StrokeCompletion::Holed | StrokeCompletion::StrokeCap
            ) && strokes == 0)
        {
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
    /// Authoritative strokes; zero is allowed for forfeits and winner-by-forfeit.
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
    config: MatchPlan,
    players: [StrokePlayerCommit; 2],
}

impl CommitStrokeMatch {
    /// Validates roster order, distinct keys/accounts, unique places, and forfeit pairing.
    ///
    /// # Errors
    /// Returns a typed value error for any aggregate cardinality/standing drift.
    pub fn new(
        match_id: MatchId,
        result_key: MatchResultKey,
        config: MatchPlan,
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
        let winner_by_forfeit = players.iter().filter(|player| {
            player.completion == StrokeCompletion::WinnerByForfeit
                && player.place == StrokePlace::First
        });
        let direct_forfeit = players.iter().filter(|player| {
            matches!(
                player.completion,
                StrokeCompletion::GiveUp
                    | StrokeCompletion::Disconnect
                    | StrokeCompletion::TurnTimeout
            ) && player.place == StrokePlace::Second
        });
        let winner_count = players
            .iter()
            .filter(|player| player.completion == StrokeCompletion::WinnerByForfeit)
            .count();
        let direct_forfeit_count = players
            .iter()
            .filter(|player| {
                matches!(
                    player.completion,
                    StrokeCompletion::GiveUp
                        | StrokeCompletion::Disconnect
                        | StrokeCompletion::TurnTimeout
                )
            })
            .count();
        if (winner_count, direct_forfeit_count) != (0, 0)
            && (
                winner_by_forfeit.count(),
                direct_forfeit.count(),
                winner_count,
                direct_forfeit_count,
            ) != (1, 1, 1, 1)
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
    /// Authoritative whole-card configuration.
    #[must_use]
    pub const fn config(self) -> MatchPlan {
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
/// Holed/stroke-cap finishes reuse checked `solo-v1`; a truthful winner-by-forfeit has no
/// score and fixed Pang 10/EXP 5; forfeits have no score and zero reward.
///
/// # Errors
/// Returns [`MatchValueError::ArithmeticOverflow`] from checked non-forfeit math.
pub fn synthetic_stroke_reward_v1(
    config: MatchPlan,
    strokes: u16,
    completion: StrokeCompletion,
) -> Result<StrokeReward, MatchValueError> {
    if completion.is_forfeit() {
        if strokes > i16::MAX as u16 {
            return Err(MatchValueError::InvalidStrokeSettlement);
        }
        return Ok(StrokeReward::from_persisted(None, 0, 0));
    }
    if completion == StrokeCompletion::WinnerByForfeit {
        if strokes > i16::MAX as u16 {
            return Err(MatchValueError::InvalidStrokeSettlement);
        }
        return Ok(StrokeReward::from_persisted(None, 10, 5));
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
    /// Course, hole-count, progression, or par configuration does not match.
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
    #[error("match storage operation failed: {0}")]
    Storage(StorageFault),
}

/// Technology-neutral match persistence contract.
pub trait MatchRepository: Send + Sync {
    /// Starts or exactly replays immutable exactly-two stroke input.
    fn begin_stroke(
        &self,
        _request: BeginStrokeMatch,
    ) -> RepositoryFuture<'_, Result<BeginStrokeMatchOutcome, MatchRepositoryError>> {
        Box::pin(async { Err(MatchRepositoryError::Storage(StorageFault::Unsupported)) })
    }

    /// Marks an exact loaded stroke aggregate in-game, idempotently.
    fn mark_stroke_in_game(
        &self,
        _request: MarkStrokeInGame,
    ) -> RepositoryFuture<'_, Result<MarkStrokeInGameOutcome, MatchRepositoryError>> {
        Box::pin(async { Err(MatchRepositoryError::Storage(StorageFault::Unsupported)) })
    }

    /// Aborts a noncommitted stroke aggregate and every captured participant.
    fn abort_stroke(
        &self,
        _request: AbortStrokeMatch,
    ) -> RepositoryFuture<'_, Result<AbortStrokeMatchOutcome, MatchRepositoryError>> {
        Box::pin(async { Err(MatchRepositoryError::Storage(StorageFault::Unsupported)) })
    }

    /// Atomically settles both authoritative stroke participants.
    fn commit_stroke_match(
        &self,
        _request: CommitStrokeMatch,
    ) -> RepositoryFuture<'_, Result<StrokeMatchResult, MatchRepositoryError>> {
        Box::pin(async { Err(MatchRepositoryError::Storage(StorageFault::Unsupported)) })
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

/// Bounded, nonsensitive classification of a storage failure.
///
/// Every value is derived from a `SQLSTATE` class, a driver-level failure kind, or a
/// server-side consistency check. None of them is derived from server message text,
/// statement text, bound parameters, or row contents, so a fault is always safe to log,
/// export as a metric label, and return to a caller.
///
/// The set is deliberately closed and fixed: it is used as a metric label, so its
/// cardinality is a hard bound rather than an implementation detail. Unrecognized
/// `SQLSTATE` values collapse into [`StorageFault::Other`] rather than widening it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageFault {
    /// `SQLSTATE` class `08` — the connection failed or was lost.
    Connection,
    /// `SQLSTATE` class `22` — a value was out of range or otherwise invalid.
    DataException,
    /// `SQLSTATE` class `23` — a constraint rejected the write.
    IntegrityConstraint,
    /// `SQLSTATE` class `25` — the transaction was in the wrong state.
    TransactionState,
    /// `SQLSTATE` `40001` — the transaction lost a serialization race.
    Serialization,
    /// `SQLSTATE` `40P01` — PostgreSQL chose this transaction to break a lock cycle.
    Deadlock,
    /// `SQLSTATE` class `40` other than serialization or deadlock.
    TransactionRollback,
    /// `SQLSTATE` class `42` — the statement was rejected as invalid or unauthorized.
    SyntaxOrAccess,
    /// `SQLSTATE` class `53` — the server ran out of connections, memory, or disk.
    InsufficientResources,
    /// `SQLSTATE` class `54` — a program limit such as row or column count was exceeded.
    ProgramLimitExceeded,
    /// `SQLSTATE` class `55` — a required object was not in a usable state, including
    /// `55P03` lock-not-available.
    ObjectNotInPrerequisiteState,
    /// `SQLSTATE` class `57` — the operator or a timeout cancelled the statement.
    OperatorIntervention,
    /// `SQLSTATE` class `58` — the server hit an external system error.
    SystemError,
    /// `SQLSTATE` class `P0` — a `PL/pgSQL` `RAISE`, which the suite uses to inject faults.
    PlPgSqlRaise,
    /// `SQLSTATE` class `XX` — the server reported internal corruption.
    InternalError,
    /// The pool did not hand out a connection within its acquire timeout.
    PoolTimedOut,
    /// The pool was already closed when a connection was requested.
    PoolClosed,
    /// A socket or TLS failure interrupted the exchange.
    Io,
    /// A row could not be decoded into its expected Rust type.
    Decode,
    /// The driver and server disagreed about the protocol.
    DriverProtocol,
    /// A statement affected a row count the surrounding invariant forbids. This is a
    /// server-side consistency failure, not a database error.
    UnexpectedRowCount,
    /// A value read back after a write did not match what was written. This is a
    /// server-side consistency failure, not a database error.
    WriteVerification,
    /// The repository does not implement the requested operation at all. This is a
    /// composition error rather than a runtime failure.
    Unsupported,
    /// Any failure that does not match a classified case above.
    Other,
}

impl StorageFault {
    /// Fixed metric-label and log token for this fault.
    ///
    /// The returned set is closed, so it is safe as a bounded-cardinality label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connection => "connection",
            Self::DataException => "data_exception",
            Self::IntegrityConstraint => "integrity_constraint",
            Self::TransactionState => "transaction_state",
            Self::Serialization => "serialization",
            Self::Deadlock => "deadlock",
            Self::TransactionRollback => "transaction_rollback",
            Self::SyntaxOrAccess => "syntax_or_access",
            Self::InsufficientResources => "insufficient_resources",
            Self::ProgramLimitExceeded => "program_limit_exceeded",
            Self::ObjectNotInPrerequisiteState => "object_not_in_prerequisite_state",
            Self::OperatorIntervention => "operator_intervention",
            Self::SystemError => "system_error",
            Self::PlPgSqlRaise => "plpgsql_raise",
            Self::InternalError => "internal_error",
            Self::PoolTimedOut => "pool_timed_out",
            Self::PoolClosed => "pool_closed",
            Self::Io => "io",
            Self::Decode => "decode",
            Self::DriverProtocol => "driver_protocol",
            Self::UnexpectedRowCount => "unexpected_row_count",
            Self::WriteVerification => "write_verification",
            Self::Unsupported => "unsupported",
            Self::Other => "other",
        }
    }

    /// Every fault in declaration order, so exporters can bound their own storage.
    pub const ALL: [Self; 24] = [
        Self::Connection,
        Self::DataException,
        Self::IntegrityConstraint,
        Self::TransactionState,
        Self::Serialization,
        Self::Deadlock,
        Self::TransactionRollback,
        Self::SyntaxOrAccess,
        Self::InsufficientResources,
        Self::ProgramLimitExceeded,
        Self::ObjectNotInPrerequisiteState,
        Self::OperatorIntervention,
        Self::SystemError,
        Self::PlPgSqlRaise,
        Self::InternalError,
        Self::PoolTimedOut,
        Self::PoolClosed,
        Self::Io,
        Self::Decode,
        Self::DriverProtocol,
        Self::UnexpectedRowCount,
        Self::WriteVerification,
        Self::Unsupported,
        Self::Other,
    ];

    /// Dense index matching [`StorageFault::ALL`], for fixed-size counter arrays.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Classifies a `SQLSTATE` without inspecting any other part of the server reply.
    ///
    /// `SQLSTATE` is a five-character identifier defined by the SQL standard and by
    /// PostgreSQL; it never carries caller data, which is why it is the only part of a
    /// database error this type is allowed to read.
    #[must_use]
    pub fn from_sqlstate(code: &str) -> Self {
        match code {
            "40001" => return Self::Serialization,
            "40P01" => return Self::Deadlock,
            _ => {}
        }
        match code.as_bytes() {
            [b'0', b'8', ..] => Self::Connection,
            [b'2', b'2', ..] => Self::DataException,
            [b'2', b'3', ..] => Self::IntegrityConstraint,
            [b'2', b'5', ..] => Self::TransactionState,
            [b'4', b'0', ..] => Self::TransactionRollback,
            [b'4', b'2', ..] => Self::SyntaxOrAccess,
            [b'5', b'3', ..] => Self::InsufficientResources,
            [b'5', b'4', ..] => Self::ProgramLimitExceeded,
            [b'5', b'5', ..] => Self::ObjectNotInPrerequisiteState,
            [b'5', b'7', ..] => Self::OperatorIntervention,
            [b'5', b'8', ..] => Self::SystemError,
            [b'P', b'0', ..] => Self::PlPgSqlRaise,
            [b'X', b'X', ..] => Self::InternalError,
            _ => Self::Other,
        }
    }
}

impl fmt::Display for StorageFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Exposes the classified fault carried by a storage failure, if it is one.
///
/// Implemented by every repository error that can report a storage failure, so a caller
/// can observe faults uniformly without matching each enum separately.
pub trait StorageFaulted {
    /// Returns the fault when this error is a storage failure, otherwise `None`.
    fn storage_fault(&self) -> Option<StorageFault>;
}

impl StorageFaulted for RepositoryError {
    fn storage_fault(&self) -> Option<StorageFault> {
        match self {
            Self::Storage(fault) => Some(*fault),
            _ => None,
        }
    }
}

impl StorageFaulted for HandoverError {
    fn storage_fault(&self) -> Option<StorageFault> {
        match self {
            Self::Storage(fault) => Some(*fault),
            _ => None,
        }
    }
}

impl StorageFaulted for MatchRepositoryError {
    fn storage_fault(&self) -> Option<StorageFault> {
        match self {
            Self::Storage(fault) => Some(*fault),
            _ => None,
        }
    }
}

impl StorageFaulted for EconomyError {
    fn storage_fault(&self) -> Option<StorageFault> {
        match self {
            Self::Storage(fault) => Some(*fault),
            _ => None,
        }
    }
}

/// Receives every classified storage failure at the point it is produced.
///
/// Implementations must stay allocation-free and nonblocking: they run inside repository
/// error paths, including those taken while a transaction is unwinding.
pub trait StorageObserver: Send + Sync + 'static {
    /// Records one classified failure.
    fn storage_fault(&self, _fault: StorageFault) {}
}

/// Discards every fault, so a repository can be built without an exporter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoopStorageObserver;

impl StorageObserver for NoopStorageObserver {}

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
    /// A credit would take a balance past its representable ceiling.
    ///
    /// Refusing is the only safe outcome: wrapping would silently destroy a balance.
    #[error("balance would overflow")]
    BalanceOverflow,
    /// A fixed-price operation cannot be covered by the current Pang balance.
    #[error("balance is insufficient")]
    BalanceInsufficient,
    /// A shop publish is already queued or running.
    ///
    /// Enqueuing a second would either race two workers over one client tree or silently
    /// discard the first operator's intent, so it refuses instead.
    #[error("a shop publish is already in flight")]
    ShopPublishInFlight,
    /// Storage is temporarily or permanently unavailable.
    #[error("storage operation failed: {0}")]
    Storage(StorageFault),
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
    #[error("handover storage operation failed: {0}")]
    Storage(StorageFault),
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

    /// Repoints an account's provisional starter character while setup is incomplete.
    ///
    /// The starter grant is deliberately strict: replaying it with a different character is a
    /// drift error, because after setup an account's character must not silently change. But a
    /// real client chooses its character *after* the account exists, so the character the grant
    /// provisionally created is not the one the player picked. This is the one operation allowed
    /// to change it, and only while the account has not finished setup.
    ///
    /// It is a no-op when the account already holds the requested character, so a duplicated
    /// selection packet cannot fail.
    fn select_starter_character(
        &self,
        account_id: AccountId,
        item_type_id: ItemTypeId,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>>;

    /// Changes account status and revokes outstanding handovers when inactive.
    fn set_status(
        &self,
        account_id: AccountId,
        status: AccountStatus,
        now: SystemTime,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>>;

    /// Credits an operator-authorised balance grant and returns the resulting balances.
    ///
    /// This is an operator action, not a gameplay one: nothing on the wire can reach it. It exists
    /// so a local deployment can fund an account for shop testing without hand-editing rows, which
    /// is how balances get corrupted. Both amounts are checked against the same balance ceiling the
    /// reward path uses, so an operator cannot overflow an account into an inconsistent state.
    fn grant_balance(
        &self,
        account_id: AccountId,
        grant: BalanceGrant,
    ) -> RepositoryFuture<'_, Result<AccountBalances, RepositoryError>>;

    /// Loads the nine persisted lobby chat macros for LoginService.
    fn load_chat_macros(
        &self,
        _account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<[Vec<u8>; 9], RepositoryError>> {
        Box::pin(std::future::ready(Ok(std::array::from_fn(|_| Vec::new()))))
    }

    /// Persists the nine bounded lobby chat macros.
    fn save_chat_macros(
        &self,
        _account_id: AccountId,
        _macros: [Vec<u8>; 9],
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(std::future::ready(Err(RepositoryError::NotFound)))
    }
}

/// Technology-neutral operator admin surface repository contract.
///
/// Every mutating method here is expected to take the same row locks and satisfy the same
/// triggers the gameplay path does, and to write exactly one [`NewAdminAuditEvent`] inside the
/// same transaction. An admin action that is not audited is a defect, not a convenience.
pub trait AdminRepository: Send + Sync {
    /// Loads the authentication projection for an operator sign-in attempt.
    ///
    /// Returns `None` for an unknown username. It deliberately does not filter on role or
    /// status, so the caller can apply one uniform failure response and avoid turning this
    /// into an account-enumeration oracle.
    fn load_admin_authentication<'a>(
        &'a self,
        username: &'a NormalizedUsername,
    ) -> RepositoryFuture<'a, Result<Option<AdminAuthenticationRecord>, RepositoryError>>;

    /// Persists a digest-only admin session.
    fn issue_admin_session(
        &self,
        request: NewAdminSession,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>>;

    /// Resolves a presented bearer, refreshing `last_seen_at` when it is still valid.
    ///
    /// Returns `None` for an unknown, expired, revoked, or digest-mismatched session, and for
    /// an account that has since lost the admin role or ceased to be active. Authorisation is
    /// therefore re-checked on every request rather than frozen at sign-in.
    fn resolve_admin_session(
        &self,
        request: ResolveAdminSession,
    ) -> RepositoryFuture<'_, Result<Option<AdminSession>, RepositoryError>>;

    /// Revokes one session. Revoking an already-revoked session is not an error.
    fn revoke_admin_session(
        &self,
        id: AdminSessionId,
        now: SystemTime,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>>;

    /// Revokes every outstanding session for an account.
    ///
    /// Called whenever an account loses the admin role or stops being active, so authority is
    /// withdrawn immediately rather than at the next expiry.
    fn revoke_admin_sessions_for_account(
        &self,
        account_id: AccountId,
        now: SystemTime,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>>;

    /// Sets an account's operator authorisation level, revoking its sessions when demoted.
    fn set_account_role(
        &self,
        account_id: AccountId,
        role: AccountRole,
        now: SystemTime,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>>;

    /// Appends one admin audit row.
    fn record_admin_audit(
        &self,
        event: NewAdminAuditEvent,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>>;

    /// Returns admin audit rows newest first.
    fn list_admin_audit(
        &self,
        page: AdminPage,
    ) -> RepositoryFuture<'_, Result<Vec<AdminAuditEvent>, RepositoryError>>;

    /// Lists accounts for the operator console.
    fn list_accounts(
        &self,
        query: AdminAccountQuery,
    ) -> RepositoryFuture<'_, Result<Vec<AdminAccountSummary>, RepositoryError>>;

    /// Loads one account's full operator view.
    fn load_account_detail(
        &self,
        account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<AdminAccountDetail, RepositoryError>>;

    /// Returns one account's merged ledger history, newest first.
    fn list_account_ledger(
        &self,
        account_id: AccountId,
        page: AdminPage,
    ) -> RepositoryFuture<'_, Result<Vec<AdminLedgerEntry>, RepositoryError>>;

    /// Returns the matches one account took part in, newest first.
    fn list_account_matches(
        &self,
        account_id: AccountId,
        page: AdminPage,
    ) -> RepositoryFuture<'_, Result<Vec<AdminMatchEntry>, RepositoryError>>;

    /// Sets an account's balances to exact values under a row lock.
    ///
    /// The credit path ([`AccountRepository::grant_balance`]) refuses to reduce a balance, so
    /// correcting one downward needs its own operation rather than a negative grant.
    fn set_balances(
        &self,
        account_id: AccountId,
        assignment: BalanceAssignment,
    ) -> RepositoryFuture<'_, Result<AccountBalances, RepositoryError>>;

    /// Places an item in an account's inventory and returns the new row.
    fn grant_item(
        &self,
        request: AdminItemGrant,
    ) -> RepositoryFuture<'_, Result<InventoryItem, AdminMutationError>>;

    /// Edits one owned inventory row.
    fn update_item(
        &self,
        request: AdminItemUpdate,
    ) -> RepositoryFuture<'_, Result<InventoryItem, AdminMutationError>>;

    /// Removes one owned inventory row, refusing while it is equipped.
    fn delete_item(
        &self,
        account_id: AccountId,
        inventory_id: InventoryItemId,
    ) -> RepositoryFuture<'_, Result<(), AdminMutationError>>;

    /// Grants an owned character and returns it.
    fn grant_character(
        &self,
        account_id: AccountId,
        item_type_id: ItemTypeId,
    ) -> RepositoryFuture<'_, Result<Character, AdminMutationError>>;

    /// Replaces an account's equipment selection under optimistic concurrency.
    fn set_equipment(
        &self,
        request: AdminEquipmentUpdate,
    ) -> RepositoryFuture<'_, Result<EquipmentSet, AdminMutationError>>;

    /// Loads the current overlay snapshot and its revision.
    fn load_shop_overlay(&self) -> RepositoryFuture<'_, Result<ShopOverlay, RepositoryError>>;

    /// Creates or replaces one override, returning the new revision.
    fn set_shop_override(
        &self,
        actor: AccountId,
        entry: ShopOverride,
        note: Option<String>,
    ) -> RepositoryFuture<'_, Result<i64, RepositoryError>>;

    /// Removes one override, returning the new revision. Removing an absent one is not an error.
    fn clear_shop_override(
        &self,
        item_type_id: ItemTypeId,
    ) -> RepositoryFuture<'_, Result<i64, RepositoryError>>;

    /// Enqueues a request to re-author the client's shop tables from `document`.
    ///
    /// Refuses with [`RepositoryError::ShopPublishInFlight`] when a request is already
    /// outstanding: two workers authoring one client tree would race on the staged archive.
    fn enqueue_shop_publish(
        &self,
        actor: AccountId,
        request: NewShopPublishRequest,
    ) -> RepositoryFuture<'_, Result<i64, RepositoryError>>;

    /// Claims the outstanding request for a worker, returning it with its document.
    ///
    /// Returns `None` when nothing is pending. Claiming is what moves `pending` to `running`,
    /// so a crashed worker leaves a visibly stuck request rather than a silently lost one.
    fn claim_shop_publish(
        &self,
    ) -> RepositoryFuture<'_, Result<Option<ClaimedShopPublish>, RepositoryError>>;

    /// Records the outcome of a claimed request.
    fn finish_shop_publish(
        &self,
        id: i64,
        outcome: ShopPublishOutcome,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>>;

    /// Returns the newest requests, newest first, without their documents.
    ///
    /// The documents are hundreds of kilobytes each and no console view needs them, so the
    /// listing deliberately cannot fetch one by accident.
    fn list_shop_publishes(
        &self,
        limit: i64,
    ) -> RepositoryFuture<'_, Result<Vec<ShopPublishSummary>, RepositoryError>>;

    /// Returns course records, best first.
    fn list_leaderboard(
        &self,
        course_id: Option<CourseId>,
        page: AdminPage,
    ) -> RepositoryFuture<'_, Result<Vec<AdminLeaderboardEntry>, RepositoryError>>;

    /// Replaces an account's stored credential.
    ///
    /// Takes an already-hashed value: hashing is expensive and belongs on a bounded worker,
    /// not inside a repository call.
    fn set_credential(
        &self,
        account_id: AccountId,
        hash: CredentialHash,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>>;
}

/// One row of the operator account list.
///
/// Deliberately a flat projection rather than a `PlayerSnapshot`: a list of two hundred
/// accounts must not load two hundred inventories.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminAccountSummary {
    /// Identifier.
    pub id: AccountId,
    /// Display username.
    pub username: String,
    /// Display nickname, absent until first-login setup picks one.
    pub nickname: Option<String>,
    /// Access status.
    pub status: AccountStatus,
    /// Operator authorisation level.
    pub role: AccountRole,
    /// Progress through first-login setup.
    pub setup_state: SetupState,
    /// Rank. There is no separate level column; the retail wire hardcodes level 1.
    pub rank: u32,
    /// Accumulated experience.
    pub experience: u64,
    /// Pang balance.
    pub pang: u64,
    /// Point ("cookie") balance.
    pub points: u64,
    /// Owned character count.
    pub character_count: i64,
    /// Owned inventory row count.
    pub inventory_count: i64,
    /// Creation time.
    pub created_at: SystemTime,
}

/// Filter and ordering for the operator account list.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AdminAccountQuery {
    /// Case-insensitive substring matched against username and nickname.
    pub search: Option<String>,
    /// Restricts to one access status.
    pub status: Option<AccountStatus>,
    /// Restricts to one authorisation level.
    pub role: Option<AccountRole>,
    /// Ordering.
    pub sort: AdminAccountSort,
    /// Bounded page.
    pub page: AdminPage,
}

/// Closed ordering vocabulary for the account list.
///
/// A closed enum rather than a column name from the request: the query is built with the
/// checked `query_as!` macro, so an operator-supplied ordering string could not reach SQL
/// anyway, and this keeps it that way if the query ever becomes dynamic.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AdminAccountSort {
    /// Newest accounts first.
    #[default]
    CreatedDesc,
    /// Oldest accounts first.
    CreatedAsc,
    /// Highest pang balance first.
    PangDesc,
    /// Highest experience first.
    ExperienceDesc,
    /// Alphabetical by normalized username.
    UsernameAsc,
}

/// The full operator view of one account.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminAccountDetail {
    /// Flat summary fields.
    pub summary: AdminAccountSummary,
    /// Owned characters.
    pub characters: Vec<Character>,
    /// Owned inventory rows.
    pub inventory: Vec<InventoryItem>,
    /// Equipment selection, absent only for an account whose setup never completed.
    pub equipment: Option<EquipmentSet>,
    /// Currently selected character, when one is set.
    pub selected_character_id: Option<CharacterId>,
}

/// One merged balance-affecting event, drawn from every ledger an account touches.
///
/// The four ledgers are separate tables with different authority triggers, but an operator
/// asking "where did this player's pang go" needs one ordered answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminLedgerEntry {
    /// Which ledger the row came from.
    pub source: AdminLedgerSource,
    /// Signed change. Match rewards are positive; purchases and repairs are negative.
    pub delta: i64,
    /// Balance after the change, when the source records one.
    pub balance_after: Option<i64>,
    /// The reason recorded by the ledger.
    pub reason: String,
    /// Correlating identifier, as text: a match id, an operation id, or an inventory id.
    pub reference: String,
    /// When it happened.
    pub created_at: SystemTime,
}

/// Which ledger an [`AdminLedgerEntry`] came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminLedgerSource {
    /// `currency_ledger` — pang awarded by a committed match.
    MatchPang,
    /// `progression_ledger` — experience awarded by a committed match.
    MatchExperience,
    /// `shop_currency_ledger` — pang spent on a purchase or repair.
    ShopPang,
    /// `item_ledger` — an inventory quantity or durability change.
    Item,
}

impl AdminLedgerSource {
    /// Returns the stable wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MatchPang => "match_pang",
            Self::MatchExperience => "match_experience",
            Self::ShopPang => "shop_pang",
            Self::Item => "item",
        }
    }
}

/// One committed match an account took part in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminMatchEntry {
    /// Match identifier.
    pub match_id: MatchId,
    /// Mode.
    pub mode: String,
    /// Course.
    pub course_id: CourseId,
    /// Lifecycle status.
    pub status: String,
    /// Strokes taken, when the row settled.
    pub strokes: Option<i16>,
    /// Score relative to par, when the row settled.
    pub score: Option<i16>,
    /// Finishing place, when the mode ranks.
    pub place: Option<i16>,
    /// How the player's participation ended.
    pub completion: Option<String>,
    /// Pang awarded.
    pub pang_reward: Option<i64>,
    /// Experience awarded.
    pub experience_reward: Option<i64>,
    /// When the match was created.
    pub created_at: SystemTime,
}

/// An operator's absolute balance assignment.
///
/// Distinct from [`BalanceGrant`], which credits. Setting is the operation an operator
/// actually wants when correcting a broken balance, and it cannot be expressed as a credit
/// because credits refuse to go down.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BalanceAssignment {
    /// New pang balance, or `None` to leave it alone.
    pub pang: Option<u64>,
    /// New point balance, or `None` to leave it alone.
    pub points: Option<u64>,
}

impl BalanceAssignment {
    /// Returns whether this assignment would change anything.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.pang.is_none() && self.points.is_none()
    }
}

/// An operator's request to place an item in an account's inventory.
///
/// Deliberately *not* routed through `EconomyRepository::purchase`: that path debits a
/// balance and writes `economy_operations` plus its ledgers, which exist to record what a
/// *player* did. An operator grant is a different act and gets a different record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminItemGrant {
    /// Owning account.
    pub account_id: AccountId,
    /// Catalog type.
    pub item_type_id: ItemTypeId,
    /// Persisted classification, which the schema cross-checks against quantity.
    pub class: InventoryClass,
    /// How many. Must be one for every class except `Consumable`.
    pub quantity: u32,
    /// Optional durability, refused for a consumable.
    pub durability: Option<u32>,
}

/// An operator's edit to one owned inventory row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminItemUpdate {
    /// Owning account, checked against the row so one account cannot edit another's item.
    pub account_id: AccountId,
    /// Target row.
    pub inventory_id: InventoryItemId,
    /// New quantity, or `None` to leave it.
    pub quantity: Option<u32>,
    /// New durability, or `None` to leave it. `Some(None)` clears it.
    pub durability: Option<Option<u32>>,
}

/// An operator's replacement of an account's equipment selection.
///
/// Carries `expected_version` for exactly the reason the in-game path does: two writers must
/// not silently overwrite each other, and a client holding a stale version must be told.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminEquipmentUpdate {
    /// Owning account.
    pub account_id: AccountId,
    /// Selected owned character.
    pub character_id: CharacterId,
    /// Equipped owned club set, or `None` to clear.
    pub club_item_id: Option<InventoryItemId>,
    /// Equipped owned ball, or `None` to clear.
    pub ball_item_id: Option<InventoryItemId>,
    /// The version the operator read.
    pub expected_version: u32,
}

/// Why an operator inventory or equipment write was refused.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminMutationError {
    /// The addressed row or account does not exist.
    #[error("record was not found")]
    NotFound,
    /// The row exists but belongs to a different account.
    #[error("record belongs to another account")]
    NotOwned,
    /// The requested shape violates a schema invariant, such as a stacked club set.
    #[error("requested item shape is invalid")]
    InvalidShape,
    /// The account already holds a stacked row of this consumable type.
    #[error("a stacked row of this type already exists")]
    AlreadyStacked,
    /// The item is currently equipped and must be unequipped first.
    #[error("item is currently equipped")]
    Equipped,
    /// Another writer changed the equipment first.
    #[error("equipment version is stale")]
    VersionConflict,
    /// Persisted data violated domain range or invariant checks.
    #[error("persisted data is invalid")]
    CorruptData,
    /// Storage is temporarily or permanently unavailable.
    #[error("storage is unavailable")]
    Storage(StorageFault),
}

impl StorageFaulted for AdminMutationError {
    fn storage_fault(&self) -> Option<StorageFault> {
        match self {
            Self::Storage(fault) => Some(*fault),
            _ => None,
        }
    }
}

/// One operator override layered on top of the immutable catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShopOverride {
    /// Catalog type this applies to.
    pub item_type_id: ItemTypeId,
    /// Whether the server offers it. `None` inherits the client's own shop flag.
    pub enabled: Option<bool>,
    /// What the server charges. `None` inherits the client's own price.
    pub pang: Option<u64>,
}

/// A request to re-author the client's shop tables, as the console renders it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewShopPublishRequest {
    /// The overlay revision `document` was rendered at.
    pub overlay_revision: i64,
    /// The catalog document `scripts/author-client-iff.py` will consume, as JSON text.
    ///
    /// Carried as text rather than a parsed value for the same reason
    /// [`NewAdminAuditEvent::detail`] is: the digest below is over exact bytes, and a parse and
    /// re-serialize round trip is free to move them.
    pub document: String,
    /// SHA-256 over `document`, so the worker can prove it authored the bytes the operator
    /// approved rather than a re-render of them.
    pub document_sha256: [u8; 32],
    /// How many offers the document carries, kept out of the JSON so a listing can show it.
    pub offer_count: i32,
}

/// A publish request handed to a worker, with the document it must author.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimedShopPublish {
    /// Row identity, used to report the outcome.
    pub id: i64,
    /// The overlay revision this document was rendered at.
    pub overlay_revision: i64,
    /// The catalog document to author, as JSON text.
    pub document: String,
    /// Expected digest of the document.
    pub document_sha256: [u8; 32],
}

/// What a worker reports back about a claimed request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShopPublishOutcome {
    /// The archive was authored and staged.
    Published {
        /// The archive name the client will download it as.
        client_pak_name: String,
        /// Its SHA-256, matching what `/launcher/v1/manifest` will serve.
        client_pak_sha256: [u8; 32],
    },
    /// Authoring or staging refused, with the reason an operator needs.
    Failed {
        /// Operator-facing explanation. A failure that says nothing is indistinguishable from a
        /// worker that never ran.
        detail: String,
    },
}

/// One row of the publish history, without its document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShopPublishSummary {
    /// Row identity.
    pub id: i64,
    /// Who asked for it.
    pub requested_by: AccountId,
    /// The overlay revision the document was rendered at.
    pub overlay_revision: i64,
    /// Digest of the document this attempt carried.
    ///
    /// Answering "are players' clients showing the current shop?" compares this against a fresh
    /// render. Comparing revisions instead would be wrong: an overlay edit that cancels out an
    /// earlier one leaves the revision higher but the shop identical.
    pub document_sha256: [u8; 32],
    /// How many offers it carried.
    pub offer_count: i32,
    /// `pending`, `running`, `published` or `failed`.
    pub status: String,
    /// The worker's outcome text, when it reported one.
    pub detail: Option<String>,
    /// The archive name that reached the client tree, on success.
    pub client_pak_name: Option<String>,
    /// When it was enqueued.
    pub requested_at: SystemTime,
    /// When a worker claimed it.
    pub started_at: Option<SystemTime>,
    /// When it reached a terminal state.
    pub finished_at: Option<SystemTime>,
}

/// The complete set of overrides, plus the revision it was read at.
///
/// Held as one immutable snapshot rather than queried per purchase: a purchase is on the
/// player's critical path, and the overlay changes at operator speed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShopOverlay {
    revision: i64,
    entries: BTreeMap<u32, ShopOverride>,
}

impl ShopOverlay {
    /// Builds a snapshot from repository rows.
    #[must_use]
    pub fn new(revision: i64, entries: Vec<ShopOverride>) -> Self {
        Self {
            revision,
            entries: entries
                .into_iter()
                .map(|entry| (entry.item_type_id.get(), entry))
                .collect(),
        }
    }

    /// Returns the revision this snapshot was read at.
    #[must_use]
    pub const fn revision(&self) -> i64 {
        self.revision
    }

    /// Returns the override for one type, if any.
    #[must_use]
    pub fn get(&self, type_id: ItemTypeId) -> Option<&ShopOverride> {
        self.entries.get(&type_id.get())
    }

    /// Returns every override, in type-ID order.
    pub fn entries(&self) -> impl Iterator<Item = &ShopOverride> {
        self.entries.values()
    }

    /// Returns how many overrides this snapshot holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether this snapshot holds no overrides.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Applies this overlay to a catalog definition.
    ///
    /// Returns the definition the server should trade on, or `None` when the result is not
    /// for sale. Unlike the startup `price_override_pang` aid, this **can** make an item the
    /// client does not sell purchasable — that is the whole point — but it can only reach
    /// items the catalog already knows, so an id the client has never heard of stays
    /// unreachable.
    #[must_use]
    pub fn resolve(&self, definition: ItemDefinition) -> Option<ItemDefinition> {
        let entry = self.get(definition.type_id);
        let inherited_enabled = matches!(definition.sale, ItemSale::Pang(_));
        let enabled = entry
            .and_then(|entry| entry.enabled)
            .unwrap_or(inherited_enabled);
        if !enabled {
            return None;
        }
        let price = entry
            .and_then(|entry| entry.pang)
            .or(match definition.sale {
                ItemSale::Pang(price) => Some(price),
                ItemSale::NotSold => None,
            })?;
        if price == 0 {
            // A zero price is how the client's tables spell "unavailable"; honouring it as
            // "free" would let an overlay hand out unlimited items by accident.
            return None;
        }
        Some(ItemDefinition {
            sale: ItemSale::Pang(price),
            ..definition
        })
    }
}

/// One row of the course-record leaderboard.
///
/// `course_records` is re-derived from `matches` and `match_players` by an authority trigger
/// on every write, so these rows cannot be fabricated — which is exactly why they are worth
/// showing an operator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminLeaderboardEntry {
    /// Holder.
    pub account_id: AccountId,
    /// Holder's display username.
    pub username: String,
    /// Course.
    pub course_id: CourseId,
    /// Mode; only `stroke_two` records exist today.
    pub mode: String,
    /// Best score relative to par.
    pub best_score: i16,
    /// Strokes taken in that round.
    pub best_strokes: i16,
    /// Rounds completed on this course.
    pub rounds_completed: i64,
    /// When the record was first set.
    pub first_achieved_at: SystemTime,
}

/// The minimum projection an operator sign-in needs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminAuthenticationRecord {
    /// Account identifier.
    pub account_id: AccountId,
    /// Display username.
    pub username_display: String,
    /// Stored credential hash, in the single supported scheme.
    pub credential_hash: CredentialHash,
    /// Access status.
    pub status: AccountStatus,
    /// Authorisation level.
    pub role: AccountRole,
}

/// A bounded offset page for admin listings.
///
/// Both bounds are enforced by the constructor rather than by each caller, so a listing
/// endpoint cannot be turned into an unbounded table scan by a query parameter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminPage {
    limit: i64,
    offset: i64,
}

impl AdminPage {
    /// Largest page any admin listing will return.
    pub const MAX_LIMIT: i64 = 200;
    /// Largest offset any admin listing will accept.
    pub const MAX_OFFSET: i64 = 1_000_000;

    /// Creates a page, clamping the limit and rejecting an out-of-range offset.
    ///
    /// # Errors
    /// Returns [`IdError::OutOfRange`] for a negative or excessive offset.
    pub fn new(limit: i64, offset: i64) -> Result<Self, IdError> {
        if !(0..=Self::MAX_OFFSET).contains(&offset) {
            return Err(IdError::OutOfRange);
        }
        Ok(Self {
            limit: limit.clamp(1, Self::MAX_LIMIT),
            offset,
        })
    }

    /// Returns the clamped row limit.
    #[must_use]
    pub const fn limit(self) -> i64 {
        self.limit
    }

    /// Returns the validated row offset.
    #[must_use]
    pub const fn offset(self) -> i64 {
        self.offset
    }
}

impl Default for AdminPage {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
        }
    }
}

/// An operator-authorised credit to one account's balances.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BalanceGrant {
    /// Pang to add.
    pub pang: u64,
    /// Points ("cookies") to add.
    pub points: u64,
}

impl BalanceGrant {
    /// Returns whether this grant would change anything.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.pang == 0 && self.points == 0
    }
}

/// Balances after an operator grant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountBalances {
    /// Resulting pang balance.
    pub pang: u64,
    /// Resulting point balance.
    pub points: u64,
}

/// Technology-neutral transactional economy repository contract.
pub trait EconomyRepository: Send + Sync {
    /// Purchases or exactly replays a catalog-priced grant.
    fn purchase(
        &self,
        request: PurchaseRequest,
    ) -> RepositoryFuture<'_, Result<EconomyCommit<PurchaseResult>, EconomyError>>;

    /// Changes owned equipment or exactly replays the committed state.
    fn equip(
        &self,
        request: EquipmentChange,
    ) -> RepositoryFuture<'_, Result<EconomyCommit<EquipmentChangeResult>, EconomyError>>;

    /// Consumes exactly one unit or exactly replays the committed remainder.
    fn consume_one(
        &self,
        request: ConsumeItem,
    ) -> RepositoryFuture<'_, Result<EconomyCommit<ConsumeItemResult>, EconomyError>>;

    /// Repairs one durable club or exactly replays its committed result.
    fn repair(
        &self,
        request: RepairItem,
    ) -> RepositoryFuture<'_, Result<EconomyCommit<RepairItemResult>, EconomyError>>;
}

/// Durable retail equipment selection beyond the minimum character/club/ball aggregate.
///
/// The values are catalog ids on the wire; storage resolves each nonzero value to an owned
/// inventory row in the same transaction before writing the projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetailEquipmentChange {
    /// Replace the equipped caddie, or clear it with zero.
    Caddie(u32),
    /// Replace all ten equipped consumable catalog ids.
    Consumables([u32; 10]),
    /// Replace all six profile decoration catalog ids.
    Decoration([u32; 6]),
    /// Replace the equipped mascot, or clear it with zero.
    Mascot(u32),
    /// Replace the opaque subtype-9 cut-in bytes for an owned character.
    CutIn {
        /// Character roster/inventory id.
        character_id: CharacterId,
        /// PacketDoc-defined opaque bytes; their meaning is intentionally not inferred.
        data: [u8; 16],
    },
    /// Replace all 24 worn character parts for an owned character.
    CharacterParts {
        /// Character roster/inventory id.
        character_id: CharacterId,
        /// Catalog ids, zero means empty.
        type_ids: [u32; 24],
        /// Owned inventory row ids, zero means empty.
        inventory_ids: [u32; 24],
        /// Hair colour stored on the character row.
        hair_color: u8,
    },
}

/// Public projection of durable retail equipment selections.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetailEquipmentState {
    /// Caddie inventory row and catalog id.
    pub caddie: Option<(InventoryItemId, u32)>,
    /// Ten consumable catalog ids.
    pub consumables: [u32; 10],
    /// Six decoration catalog ids.
    pub decoration: [u32; 6],
    /// Mascot inventory row and catalog id.
    pub mascot: Option<(InventoryItemId, u32)>,
    /// Cut-in character and PacketDoc-defined opaque bytes.
    pub cut_in: Option<(CharacterId, [u8; 16])>,
    /// Hair colour persisted with character-part changes.
    pub character_hair_color: u8,
    /// Inventory slots and catalog ids for the six proven decoration fields.
    pub decoration_slots: [u32; 6],
    /// Worn character part catalog ids and owned row ids.
    pub character_parts: Option<(CharacterId, [u32; 24], [u32; 24])>,
}

/// Opaque furniture row persisted for a player's My Room.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MyRoomFurniture {
    /// PacketDoc-defined opaque four-byte prefix.
    pub unknown_prefix: [u8; 4],
    /// Furniture.iff catalog id.
    pub item_type_id: u32,
    /// PacketDoc-defined opaque nineteen-byte suffix.
    pub unknown_suffix: [u8; 19],
}

/// A bounded mascot message update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MascotMessageUpdate {
    /// Owning mascot inventory row.
    pub inventory_item_id: InventoryItemId,
    /// UTF-8 message bytes, at most 30 bytes and never empty.
    pub message: Vec<u8>,
}

/// A durable visitor-visible My Room projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MyRoomProjection {
    /// Persisted furniture in deterministic slot order.
    pub furniture: Vec<MyRoomFurniture>,
    /// Message attached to the equipped mascot, if one is set.
    pub mascot_message: Option<Vec<u8>>,
}

/// Explicit refusal code for UCC upload infrastructure that is not configured.
pub const UCC_UPLOAD_UNSUPPORTED_ERROR: u32 = 0x0510_0100;

/// An offline note request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfflineNoteRequest {
    /// Authenticated sender account.
    pub sender_id: AccountId,
    /// Recipient account from the wire's user id.
    pub recipient_id: AccountId,
    /// Stable digest of the exact authenticated request payload.
    pub operation_id: [u8; 32],
    /// Bounded note text.
    pub message: Vec<u8>,
}

/// One leased offline note pending delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfflineNote {
    /// Durable note row id.
    pub id: i64,
    /// Opaque lease token fencing stale acknowledgements.
    pub lease_token: [u8; 16],
    /// Sender's display nickname.
    pub sender_nickname: Vec<u8>,
    /// Note text.
    pub message: Vec<u8>,
}

/// A successful outbound delivery acknowledgement for one leased note.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OfflineNoteClaim {
    /// Durable note row id.
    pub id: i64,
    /// Lease token returned by the claim operation.
    pub lease_token: [u8; 16],
}

/// Durable result of an offline-note submission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OfflineNoteCommit {
    /// Sender's balance after the operation.
    pub pang: u64,
    /// Whether this request inserted a new note rather than replaying one.
    pub accepted: bool,
}

/// Technology-neutral coherent player-bootstrap repository contract.
/// One bounded recent-player entry shown in the retail lobby.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecentPlayer {
    /// Account id of the encountered player.
    pub account_id: AccountId,
    /// Display nickname at encounter time.
    pub nickname: String,
    /// Encounter timestamp.
    pub seen_at: SystemTime,
}

/// Maximum persisted recent-player entries per account.
pub const MAX_RECENT_PLAYERS: usize = 20;

/// Durable player projections and bounded social history.
pub trait PlayerRepository: Send + Sync {
    /// Loads one coherent active/complete player bootstrap snapshot by authenticated account ID.
    fn load_player_snapshot(
        &self,
        account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<PlayerSnapshot, RepositoryError>>;

    /// Loads the bounded, newest-first retail recent-player history.
    fn load_recent_players(
        &self,
        _account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<Vec<RecentPlayer>, RepositoryError>> {
        Box::pin(std::future::ready(Ok(Vec::new())))
    }

    /// Records one encounter, retaining at most [`MAX_RECENT_PLAYERS`] newest distinct accounts.
    fn record_recent_player(
        &self,
        _account_id: AccountId,
        _recent: RecentPlayer,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(std::future::ready(Ok(())))
    }

    /// Loads bounded visitor-visible My Room state for an account.
    fn load_my_room(
        &self,
        _account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<MyRoomProjection, RepositoryError>> {
        Box::pin(std::future::ready(Ok(MyRoomProjection {
            furniture: Vec::new(),
            mascot_message: None,
        })))
    }

    /// Persists one mascot message after ownership and policy validation.
    fn save_mascot_message(
        &self,
        _account_id: AccountId,
        _update: MascotMessageUpdate,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(std::future::ready(Err(RepositoryError::NotFound)))
    }

    /// Loads the durable retail equipment projection. Legacy/test repositories return the empty
    /// projection by default so retail equipment remains additive to the minimum contract.
    fn load_retail_equipment(
        &self,
        _account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<RetailEquipmentState, RepositoryError>> {
        Box::pin(async { Ok(RetailEquipmentState::default()) })
    }

    /// Validates ownership and atomically persists or replays one retail equipment update against
    /// the same optimistic equipment version used by the minimum aggregate. The operation key is
    /// durable and is not a wire field; callers derive it from the exact authenticated frame.
    fn update_retail_equipment(
        &self,
        _account_id: AccountId,
        _operation_id: EconomyOperationId,
        _expected_version: u32,
        _change: RetailEquipmentChange,
    ) -> RepositoryFuture<'_, Result<EconomyCommit<RetailEquipmentState>, RepositoryError>> {
        Box::pin(async { Err(RepositoryError::Storage(StorageFault::Other)) })
    }

    /// Persists the nine bounded lobby chat macros for the authenticated player.
    fn save_chat_macros(
        &self,
        _account_id: AccountId,
        _macros: [Vec<u8>; 9],
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(std::future::ready(Err(RepositoryError::NotFound)))
    }

    /// Leases pending notes for delivery after a recipient authenticates.
    ///
    /// A lease is recoverable after expiry; implementations must not mark a row delivered here.
    fn claim_offline_notes(
        &self,
        _recipient_id: AccountId,
    ) -> RepositoryFuture<'_, Result<Vec<OfflineNote>, RepositoryError>> {
        Box::pin(std::future::ready(Ok(Vec::new())))
    }

    /// Acknowledges one note only after its outbound socket write succeeds.
    ///
    /// Returns whether exactly one pending row carried this lease token. The token fences a
    /// delayed acknowledgement from an expired lease and a later claimant.
    fn ack_offline_note(
        &self,
        _claim: OfflineNoteClaim,
    ) -> RepositoryFuture<'_, Result<bool, RepositoryError>> {
        Box::pin(std::future::ready(Ok(false)))
    }

    /// Stores an offline note and debits its fixed 10-Pang cost in one transaction.
    ///
    /// Implementations must resolve the recipient by account id (not presence), insert or replay
    /// the operation idempotently, and never debit a rejected or failed insert.
    fn accept_offline_note(
        &self,
        _request: OfflineNoteRequest,
    ) -> RepositoryFuture<'_, Result<OfflineNoteCommit, RepositoryError>> {
        Box::pin(std::future::ready(Err(RepositoryError::NotFound)))
    }
}

/// Short-lived server-side LoginService → MessageService eligibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewMessageEligibility {
    /// Authenticated account.
    pub account_id: AccountId,
    /// Exact profile nickname sent on MessageService 0x0012.
    pub nickname: String,
    /// Exact peer address observed by LoginService.
    pub peer_ip: std::net::IpAddr,
    /// Creation time supplied by the application clock.
    pub issued_at: SystemTime,
    /// Strict expiry time.
    pub expires_at: SystemTime,
}

/// Repository contract for one-time LoginService → MessageService eligibility.
pub trait MessageEligibilityRepository: Send + Sync {
    /// Stores/replaces the bounded eligibility after verifying the account is active.
    fn issue_message_eligibility(
        &self,
        eligibility: NewMessageEligibility,
    ) -> RepositoryFuture<'_, Result<(), HandoverError>>;
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

impl Clone for RoomPassword {
    fn clone(&self) -> Self {
        Self(self.0.clone())
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

/// The shape of the game a room was opened to play.
///
/// A room is not just a name and a capacity: the client that opened it chose a mode, a course,
/// a number of holes and its timers, and it renders its own room header and gates its Start
/// button on getting those back. Nothing here is enforced by the match — it is what the room
/// says of itself.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RoomProfile {
    /// Wire value of the mode: versus, chat, tournament, battle, or a client-private mode
    /// such as practice. Carried verbatim, because a mode this server does not model is still
    /// one the client renders.
    pub mode: u8,
    /// Course ordinal.
    pub course: u8,
    /// Holes the room says it will play.
    pub hole_count: u8,
    /// Hole progression ordinal.
    pub hole_progression: u8,
    /// Per-shot timer in milliseconds.
    pub shot_timer_ms: u32,
    /// Whole-game timer in milliseconds.
    pub game_timer_ms: u32,
    /// Artifact catalog id selected for this room (reference server defaults to zero).
    pub artifact_id: u32,
    /// Whether wind varies naturally.
    pub natural_wind: bool,
}

/// Validated mutable room settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomSettings {
    max_members: u8,
    profile: RoomProfile,
}

impl RoomSettings {
    /// Creates settings with a member capacity in `1..=30`.
    ///
    /// One is a real capacity, not a degenerate one: a practice room holds exactly the player
    /// who opened it, and a client that asks for one and is refused drops back to the lobby
    /// with no explanation. Modes that need two players enforce that where they start, not
    /// here.
    ///
    /// # Errors
    /// Returns [`RoomValueError::InvalidCapacity`] outside the range.
    pub const fn new(max_members: u8) -> Result<Self, RoomValueError> {
        if max_members >= 1 && max_members <= 30 {
            Ok(Self {
                max_members,
                profile: RoomProfile {
                    mode: 0,
                    course: 0,
                    hole_count: 1,
                    hole_progression: 0,
                    shot_timer_ms: 30_000,
                    game_timer_ms: 600_000,
                    artifact_id: 0,
                    natural_wind: false,
                },
            })
        } else {
            Err(RoomValueError::InvalidCapacity)
        }
    }

    /// Returns the maximum room membership.
    #[must_use]
    pub const fn max_members(self) -> u8 {
        self.max_members
    }

    /// Returns the same settings describing the game the room was opened to play.
    #[must_use]
    pub const fn with_profile(mut self, profile: RoomProfile) -> Self {
        self.profile = profile;
        self
    }

    /// What the room says of the game it will play.
    #[must_use]
    pub const fn profile(self) -> RoomProfile {
        self.profile
    }
}

/// What the other players in a room see of a member.
///
/// A roster is not a list of names: a client builds every other player's model, portrait and
/// scoreboard line from these, so they travel with the member rather than being looked up per
/// frame. Nothing secret belongs here — it is published to everyone in the room.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemberCard {
    /// Account name.
    pub username: String,
    /// Equipped character's catalog id.
    pub character_iff_id: u32,
    /// Equipped character's inventory id.
    pub character_uid: u32,
    /// Equipped caddie's inventory id.
    pub caddie_uid: u32,
    /// Equipped club set's inventory id.
    pub club_set_uid: u32,
    /// Catalog id of the equipped club set.
    pub club_set_iff_id: u32,
    /// Equipped ball's catalog id.
    pub comet_iff_id: u32,
    /// Accumulated experience.
    pub experience: u32,
    /// Pang balance.
    pub pang: u64,
}

/// Immutable public member projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberSnapshot {
    connection_id: PlayerConnectionId,
    account_id: AccountId,
    nickname: String,
    owner: bool,
    ready: bool,
    character_id: Option<CharacterId>,
    character_iff_id: Option<u32>,
    team: u8,
    card: MemberCard,
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
        character_id: Option<CharacterId>,
        character_iff_id: Option<u32>,
    ) -> Self {
        Self {
            connection_id,
            account_id,
            nickname,
            owner,
            ready,
            character_id,
            character_iff_id,
            team: 0,
            card: MemberCard::default(),
        }
    }

    /// Attaches the room team projection.
    #[must_use]
    pub const fn with_team(mut self, team: u8) -> Self {
        self.team = team;
        self
    }

    /// Team selected by this member.
    #[must_use]
    pub const fn team(&self) -> u8 {
        self.team
    }

    /// Attaches what the rest of the room sees of this member.
    #[must_use]
    pub fn with_card(mut self, card: MemberCard) -> Self {
        self.card = card;
        self
    }

    /// What the rest of the room sees of this member.
    #[must_use]
    pub const fn card(&self) -> &MemberCard {
        &self.card
    }

    /// The catalog id of the member's character, which the client resolves its model by.
    ///
    /// Distinct from [`Self::character_id`], which is the inventory id and means nothing to a
    /// client: it has no inventory but its own.
    #[must_use]
    pub const fn character_iff_id(&self) -> Option<u32> {
        self.character_iff_id
    }

    /// The member's selected character, when one is known.
    ///
    /// A room roster without it renders every player as an empty slot, so it travels with the
    /// member rather than being looked up per frame.
    #[must_use]
    pub const fn character_id(&self) -> Option<CharacterId> {
        self.character_id
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
    profile: RoomProfile,
    channel: Option<u8>,
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
        profile: RoomProfile,
    ) -> Self {
        Self {
            id,
            name,
            owner_nickname,
            members,
            max_members,
            password_protected,
            profile,
            channel: None,
        }
    }

    /// Associates this summary with the retail channel that owns it.
    #[must_use]
    pub const fn with_channel(mut self, channel: u8) -> Self {
        self.channel = Some(channel);
        self
    }

    /// Retail channel ownership, when the lobby is channel-scoped.
    #[must_use]
    pub const fn channel(&self) -> Option<u8> {
        self.channel
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
    /// What the room says of the game it will play.
    #[must_use]
    pub const fn profile(&self) -> RoomProfile {
        self.profile
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
    /// A team value outside the retail red/blue set was requested.
    #[error("room team is invalid")]
    InvalidTeam,
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
    fn economy_commit_exposes_applied_vs_replayed_metadata() {
        let committed = EconomyCommit::Committed(7_u8);
        assert!(committed.was_applied());
        assert_eq!(committed.into_parts(), (7, true));

        let replayed = EconomyCommit::Replayed(9_u8);
        assert!(!replayed.was_applied());
        assert_eq!(replayed.into_parts(), (9, false));
    }

    #[test]
    fn match_plan_retains_full_card_shape() {
        let course = CourseId::new(7).expect("course");
        let plan = MatchPlan::with_holes(course, 18, 3, 4).expect("plan");
        assert_eq!(plan.course_id(), course);
        assert_eq!(plan.hole_count(), 18);
        assert_eq!(plan.hole_mode(), 3);
        assert_eq!(plan.par(), 4);
        assert_eq!(
            MatchPlan::with_holes(course, 19, 0, 4),
            Err(MatchValueError::InvalidHole)
        );
        assert_eq!(
            MatchPlan::with_holes(course, 18, 4, 4),
            Err(MatchValueError::InvalidHoleMode)
        );
    }

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
            MatchPlan::with_holes(CourseId::new(7).expect("course"), 1, 0, 3)
                .expect("configuration"),
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
        let config = MatchPlan::with_holes(course, 1, 0, 3).expect("configuration");
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
            MatchPlan::with_holes(course, 1, 0, 0),
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
        assert_eq!(
            synthetic_stroke_reward_v1(config, 0, StrokeCompletion::WinnerByForfeit),
            Ok(StrokeReward::from_persisted(None, 10, 5))
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
        let config = MatchPlan::with_holes(CourseId::new(7).expect("course"), 1, 0, 3)
            .expect("configuration");
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
    fn stroke_winner_by_forfeit_requires_exact_direct_forfeit_pair() {
        let aggregate_key = MatchResultKey::new(Uuid::from_u128(10));
        let participants = [
            StrokeParticipant::new(
                AccountId::new(10).expect("account"),
                StrokeRosterOrder::First,
                MatchResultKey::new(Uuid::from_u128(11)),
            ),
            StrokeParticipant::new(
                AccountId::new(20).expect("account"),
                StrokeRosterOrder::Second,
                MatchResultKey::new(Uuid::from_u128(12)),
            ),
        ];
        let config = MatchPlan::with_holes(CourseId::new(7).expect("course"), 1, 0, 4)
            .expect("configuration");
        let commit = |left: (StrokePlace, StrokeCompletion),
                      right: (StrokePlace, StrokeCompletion)| {
            let strokes = |completion| {
                u16::from(matches!(
                    completion,
                    StrokeCompletion::Holed | StrokeCompletion::StrokeCap
                ))
            };
            let players = [
                StrokePlayerCommit::new(participants[0], strokes(left.1), left.0, left.1)
                    .expect("left"),
                StrokePlayerCommit::new(participants[1], strokes(right.1), right.0, right.1)
                    .expect("right"),
            ];
            CommitStrokeMatch::new(
                MatchId::new(Uuid::from_u128(13)),
                aggregate_key,
                config,
                players,
            )
        };
        for direct in [
            StrokeCompletion::GiveUp,
            StrokeCompletion::Disconnect,
            StrokeCompletion::TurnTimeout,
        ] {
            assert!(
                commit(
                    (StrokePlace::First, StrokeCompletion::WinnerByForfeit),
                    (StrokePlace::Second, direct),
                )
                .is_ok()
            );
        }
        for malformed in [
            (
                (StrokePlace::First, StrokeCompletion::WinnerByForfeit),
                (StrokePlace::Second, StrokeCompletion::WinnerByForfeit),
            ),
            (
                (StrokePlace::First, StrokeCompletion::GiveUp),
                (StrokePlace::Second, StrokeCompletion::WinnerByForfeit),
            ),
            (
                (StrokePlace::First, StrokeCompletion::WinnerByForfeit),
                (StrokePlace::Second, StrokeCompletion::GameTimeout),
            ),
            (
                (StrokePlace::First, StrokeCompletion::GameTimeout),
                (StrokePlace::Second, StrokeCompletion::WinnerByForfeit),
            ),
            (
                (StrokePlace::First, StrokeCompletion::Holed),
                (StrokePlace::Second, StrokeCompletion::GiveUp),
            ),
        ] {
            assert_eq!(
                commit(malformed.0, malformed.1),
                Err(MatchValueError::InvalidStrokeSettlement)
            );
        }
        assert!(
            commit(
                (StrokePlace::First, StrokeCompletion::GameTimeout),
                (StrokePlace::Second, StrokeCompletion::GameTimeout),
            )
            .is_ok(),
            "normal no-winner standings remain valid"
        );
    }

    #[test]
    fn password_is_redacted_and_settings_are_bounded() {
        let password = RoomPassword::parse("secret").expect("valid password");
        assert_eq!(password.expose_bytes(), b"secret");
        assert_eq!(format!("{password:?}"), "RoomPassword([REDACTED])");
        assert_eq!(RoomPassword::parse(""), Err(RoomValueError::InvalidLength));
        assert_eq!(
            RoomSettings::new(1).map(RoomSettings::max_members),
            Ok(1),
            "a practice room holds one player"
        );
        assert_eq!(RoomSettings::new(0), Err(RoomValueError::InvalidCapacity));
        assert_eq!(RoomSettings::new(30).map(RoomSettings::max_members), Ok(30));
    }

    #[test]
    fn storage_fault_labels_are_unique_dense_and_bounded() {
        // The label set is a metric dimension, so its width is a contract: duplicates
        // would silently merge two causes, and a sparse index would read the wrong slot.
        let mut labels: Vec<&str> = StorageFault::ALL.iter().map(|f| f.as_str()).collect();
        let total = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), total, "every fault label is distinct");
        for (position, fault) in StorageFault::ALL.into_iter().enumerate() {
            assert_eq!(fault.index(), position, "index is dense over ALL");
        }
        for label in labels {
            assert!(!label.is_empty());
            assert!(
                label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_'),
                "label {label} stays a bare Prometheus-safe token"
            );
        }
    }

    #[test]
    fn sqlstate_classification_pins_every_class_and_the_two_specific_codes() {
        for (code, expected) in [
            ("08006", StorageFault::Connection),
            ("22003", StorageFault::DataException),
            ("23505", StorageFault::IntegrityConstraint),
            ("25P02", StorageFault::TransactionState),
            ("40001", StorageFault::Serialization),
            ("40P01", StorageFault::Deadlock),
            ("40002", StorageFault::TransactionRollback),
            ("42601", StorageFault::SyntaxOrAccess),
            ("53300", StorageFault::InsufficientResources),
            ("54001", StorageFault::ProgramLimitExceeded),
            ("55P03", StorageFault::ObjectNotInPrerequisiteState),
            ("57014", StorageFault::OperatorIntervention),
            ("58030", StorageFault::SystemError),
            ("P0001", StorageFault::PlPgSqlRaise),
            ("XX001", StorageFault::InternalError),
        ] {
            assert_eq!(
                StorageFault::from_sqlstate(code),
                expected,
                "SQLSTATE {code}"
            );
        }
        // Serialization and deadlock must win over their shared `40` class.
        assert_ne!(
            StorageFault::from_sqlstate("40001"),
            StorageFault::from_sqlstate("40002")
        );
        assert_ne!(
            StorageFault::from_sqlstate("40P01"),
            StorageFault::from_sqlstate("40002")
        );
        // Unknown and malformed input collapses instead of widening the label set.
        for code in ["", "9", "99999", "zz", "\u{1f600}"] {
            assert_eq!(StorageFault::from_sqlstate(code), StorageFault::Other);
        }
    }

    #[test]
    fn storage_errors_expose_their_fault_and_never_widen_other_variants() {
        assert_eq!(
            RepositoryError::Storage(StorageFault::Deadlock).storage_fault(),
            Some(StorageFault::Deadlock)
        );
        assert_eq!(
            HandoverError::Storage(StorageFault::Io).storage_fault(),
            Some(StorageFault::Io)
        );
        assert_eq!(
            MatchRepositoryError::Storage(StorageFault::UnexpectedRowCount).storage_fault(),
            Some(StorageFault::UnexpectedRowCount)
        );
        assert_eq!(
            EconomyError::Storage(StorageFault::PoolTimedOut).storage_fault(),
            Some(StorageFault::PoolTimedOut)
        );
        assert_eq!(RepositoryError::NotFound.storage_fault(), None);
        assert_eq!(HandoverError::Expired.storage_fault(), None);
        assert_eq!(MatchRepositoryError::Aborted.storage_fault(), None);
        assert_eq!(EconomyError::StackFull.storage_fault(), None);
    }

    #[test]
    fn rendered_storage_errors_carry_only_the_bounded_fault_token() {
        // A fault is exported as a metric label and returned to callers, so its rendering
        // must stay inside the closed token set and must not gain free-form text.
        for fault in StorageFault::ALL {
            let displayed = MatchRepositoryError::Storage(fault).to_string();
            assert!(
                displayed.ends_with(fault.as_str()),
                "{displayed} ends with its bounded token"
            );
            assert_eq!(fault.to_string(), fault.as_str());
            assert!(!format!("{fault:?}").is_empty());
        }
    }
}
