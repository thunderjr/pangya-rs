#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Credential security, bounded execution, and the M2 LoginService runtime.

mod executor;
mod limits;
mod runtime;
mod state;

pub use executor::{BoundedCredentialExecutor, CredentialEngine, CredentialExecutorError};
pub use limits::{
    CapacityRegistry, FixedWindowLimiter, KeyedCapacityGuard, KeyedCapacityRegistry, RateDecision,
    RegistryError, RegistryGuard,
};
pub use runtime::{
    AdvertisedGameServer, ConnectionId, ConnectionTermination, CredentialWorkerOutcome,
    DbQueryClass, LoginObserver, LoginRuntimeConfig, LoginRuntimeError, LoginRuntimeLimits,
    LoginService, NoopLoginObserver, ProtocolMetricClass, RateLimitClass, UnknownOpcodeBucket,
};
pub use state::{LoginEvent, LoginState, LoginStateMachine, TransitionError};

use std::{
    fmt::{self, Write as _},
    time::{Duration, SystemTime},
};

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD},
};
use pangya_domain::{
    AccountId, CredentialHash, HandoverDigest, HandoverId, NewHandover, ServiceKind,
    SourceAddressPrefix,
};
use rand::{CryptoRng, RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const ARGON2_MEMORY_KIB: u32 = 19_456;
const ARGON2_ITERATIONS: u32 = 2;
const ARGON2_PARALLELISM: u32 = 1;
const ARGON2_OUTPUT_BYTES: usize = 32;
const ARGON2_PARAMS_PHC: &str = "m=19456,t=2,p=1";
const HANDOVER_RANDOM_BYTES: usize = 32;
const HANDOVER_SECRET_ENCODED_BYTES: usize = 43;
/// Default login-to-service handover lifetime.
pub const DEFAULT_HANDOVER_LIFETIME: Duration = Duration::from_secs(60);

/// Failure canonicalizing the legacy client transport secret.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TransportSecretError {
    /// The secret was not exactly 32 bytes.
    #[error("transport secret must be exactly 32 ASCII hexadecimal characters")]
    InvalidLength,
    /// At least one byte was not ASCII hexadecimal.
    #[error("transport secret contains non-hexadecimal input")]
    InvalidCharacter,
}

/// Exactly 32 lowercase ASCII hexadecimal bytes, redacted when formatted.
#[derive(Clone, Eq, PartialEq, Zeroize, ZeroizeOnDrop)]
pub struct CanonicalTransportSecret([u8; 32]);

impl CanonicalTransportSecret {
    /// Validates and canonicalizes a client MD5 transport secret.
    ///
    /// # Errors
    /// Returns [`TransportSecretError`] for a value of the wrong length or format.
    pub fn parse(value: &str) -> Result<Self, TransportSecretError> {
        let bytes = value.as_bytes();
        if bytes.len() != 32 {
            return Err(TransportSecretError::InvalidLength);
        }
        if !bytes.iter().all(u8::is_ascii_hexdigit) {
            return Err(TransportSecretError::InvalidCharacter);
        }
        let mut canonical = [0_u8; 32];
        for (destination, source) in canonical.iter_mut().zip(bytes) {
            *destination = source.to_ascii_lowercase();
        }
        Ok(Self(canonical))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for CanonicalTransportSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CanonicalTransportSecret([REDACTED])")
    }
}

/// Credential hashing/verification failure without credential material.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CredentialError {
    /// Hash generation failed.
    #[error("credential hashing failed")]
    Hashing,
    /// Stored data is not the exact supported Argon2id policy.
    #[error("stored credential policy is unsupported")]
    UnsupportedPolicy,
    /// Credential did not verify.
    #[error("credential verification failed")]
    Verification,
}

/// Versioned Argon2id-v19 policy for canonical client transport secrets.
pub struct CredentialPolicy {
    argon2: Argon2<'static>,
}

impl CredentialPolicy {
    /// Builds the fixed `argon2id-client-md5-v1` policy.
    ///
    /// # Errors
    /// Returns [`CredentialError::Hashing`] only if fixed parameters are rejected by the library.
    pub fn new() -> Result<Self, CredentialError> {
        let params = Params::new(
            ARGON2_MEMORY_KIB,
            ARGON2_ITERATIONS,
            ARGON2_PARALLELISM,
            Some(ARGON2_OUTPUT_BYTES),
        )
        .map_err(|_| CredentialError::Hashing)?;
        Ok(Self {
            argon2: Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
        })
    }

    /// Hashes a canonical secret using an OS-generated salt.
    ///
    /// Expensive callers must execute this outside database transactions and use a bounded
    /// blocking worker at runtime.
    ///
    /// # Errors
    /// Returns [`CredentialError::Hashing`] if secure salt generation or PHC encoding fails.
    pub fn hash(
        &self,
        secret: &CanonicalTransportSecret,
    ) -> Result<CredentialHash, CredentialError> {
        self.hash_with_rng(secret, &mut OsRng)
    }

    fn hash_with_rng<R>(
        &self,
        secret: &CanonicalTransportSecret,
        rng: &mut R,
    ) -> Result<CredentialHash, CredentialError>
    where
        R: CryptoRng + RngCore,
    {
        let salt = SaltString::generate(rng);
        self.argon2
            .hash_password(secret.as_bytes(), &salt)
            .map(|hash| CredentialHash::new(hash.to_string()))
            .map_err(|_| CredentialError::Hashing)
    }

    /// Verifies a canonical secret and rejects parameter/version downgrades.
    ///
    /// # Errors
    /// Returns a redacted policy or verification failure.
    pub fn verify(
        &self,
        secret: &CanonicalTransportSecret,
        stored: &CredentialHash,
    ) -> Result<(), CredentialError> {
        let parsed = PasswordHash::new(stored.expose_phc())
            .map_err(|_| CredentialError::UnsupportedPolicy)?;
        if !phc_matches_policy(&parsed) {
            return Err(CredentialError::UnsupportedPolicy);
        }
        self.argon2
            .verify_password(secret.as_bytes(), &parsed)
            .map_err(|_| CredentialError::Verification)
    }
}

fn phc_matches_policy(parsed: &PasswordHash<'_>) -> bool {
    if parsed.algorithm.as_str() != "argon2id"
        || parsed.version != Some(19)
        || parsed.params.as_str() != ARGON2_PARAMS_PHC
        || parsed.params.iter().count() != 3
        || parsed.params.get_decimal("m") != Some(ARGON2_MEMORY_KIB)
        || parsed.params.get_decimal("t") != Some(ARGON2_ITERATIONS)
        || parsed.params.get_decimal("p") != Some(ARGON2_PARALLELISM)
        || parsed
            .params
            .iter()
            .any(|(name, _)| !matches!(name.as_str(), "m" | "t" | "p"))
        || parsed
            .hash
            .is_none_or(|output| output.len() != ARGON2_OUTPUT_BYTES)
    {
        return false;
    }

    let Some(salt) = parsed.salt else {
        return false;
    };
    let mut decoded = [0_u8; 64];
    let Ok(decoded) = salt.decode_b64(&mut decoded) else {
        return false;
    };
    decoded.len() == 16 && salt.len() == 22 && STANDARD_NO_PAD.encode(decoded) == salt.as_str()
}

/// Bearer handover token whose formatting is always redacted.
#[derive(Clone, Eq, PartialEq)]
pub struct HandoverToken(Zeroizing<String>);

impl HandoverToken {
    /// Exposes the token only at the protocol handoff boundary.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for HandoverToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HandoverToken([REDACTED])")
    }
}

/// Generated bearer paired with the digest-only persistence request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedHandover {
    /// Bearer returned to the client.
    pub token: HandoverToken,
    /// Digest-only record persisted by the repository.
    pub record: NewHandover,
}

/// Parsed bearer selector and digest for atomic repository consumption.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedHandover {
    /// Nonsecret selector.
    pub id: HandoverId,
    /// SHA-256 digest of the 256-bit bearer component.
    pub digest: HandoverDigest,
}

/// Redacted token generation/parsing failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum HandoverTokenError {
    /// Token syntax, selector, or secret length was invalid.
    #[error("handover token is invalid")]
    Invalid,
    /// Supplied timestamps overflowed or did not form a future interval.
    #[error("handover expiry is invalid")]
    InvalidExpiry,
}

/// Generates a UUID selector and 256-bit OS-random bearer for a 60-second handover.
///
/// # Errors
/// Returns [`HandoverTokenError::InvalidExpiry`] on `SystemTime` overflow.
pub fn generate_handover(
    account_id: AccountId,
    target: ServiceKind,
    source_address_prefix: SourceAddressPrefix,
    now: SystemTime,
) -> Result<GeneratedHandover, HandoverTokenError> {
    generate_handover_with_rng(
        account_id,
        target,
        source_address_prefix,
        now,
        DEFAULT_HANDOVER_LIFETIME,
        &mut OsRng,
    )
}

fn generate_handover_with_rng<R>(
    account_id: AccountId,
    target: ServiceKind,
    source_address_prefix: SourceAddressPrefix,
    now: SystemTime,
    lifetime: Duration,
    rng: &mut R,
) -> Result<GeneratedHandover, HandoverTokenError>
where
    R: CryptoRng + RngCore,
{
    if lifetime.is_zero() {
        return Err(HandoverTokenError::InvalidExpiry);
    }
    let expires_at = now
        .checked_add(lifetime)
        .ok_or(HandoverTokenError::InvalidExpiry)?;

    let mut selector_bytes = [0_u8; 16];
    rng.fill_bytes(&mut selector_bytes);
    selector_bytes[6] = (selector_bytes[6] & 0x0f) | 0x40;
    selector_bytes[8] = (selector_bytes[8] & 0x3f) | 0x80;
    let selector = Uuid::from_bytes(selector_bytes);

    let mut secret = Zeroizing::new([0_u8; HANDOVER_RANDOM_BYTES]);
    rng.fill_bytes(secret.as_mut());
    let encoded = Zeroizing::new(URL_SAFE_NO_PAD.encode(secret.as_ref()));
    let mut token_value = Zeroizing::new(String::with_capacity(
        36 + 1 + HANDOVER_SECRET_ENCODED_BYTES,
    ));
    write!(token_value, "{selector}.{}", encoded.as_str())
        .map_err(|_| HandoverTokenError::Invalid)?;
    let token = HandoverToken(token_value);
    let digest = digest_secret(&secret);
    Ok(GeneratedHandover {
        token,
        record: NewHandover {
            id: selector.into(),
            account_id,
            digest,
            target,
            source_address_prefix,
            issued_at: now,
            expires_at,
        },
    })
}

/// Strictly parses a canonical bearer and derives the digest stored by PostgreSQL.
///
/// # Errors
/// Returns [`HandoverTokenError::Invalid`] without echoing any bearer content.
pub fn parse_handover(value: &str) -> Result<ParsedHandover, HandoverTokenError> {
    let (selector_text, secret_text) = value.split_once('.').ok_or(HandoverTokenError::Invalid)?;
    if selector_text.len() != 36 || secret_text.len() != HANDOVER_SECRET_ENCODED_BYTES {
        return Err(HandoverTokenError::Invalid);
    }
    let selector = Uuid::parse_str(selector_text).map_err(|_| HandoverTokenError::Invalid)?;
    if selector.to_string() != selector_text {
        return Err(HandoverTokenError::Invalid);
    }
    let decoded = Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(secret_text)
            .map_err(|_| HandoverTokenError::Invalid)?,
    );
    let secret = Zeroizing::new(
        <[u8; HANDOVER_RANDOM_BYTES]>::try_from(decoded.as_slice())
            .map_err(|_| HandoverTokenError::Invalid)?,
    );
    Ok(ParsedHandover {
        id: selector.into(),
        digest: digest_secret(&secret),
    })
}

fn digest_secret(secret: &[u8; HANDOVER_RANDOM_BYTES]) -> HandoverDigest {
    HandoverDigest::new(Sha256::digest(secret).into())
}

/// Compares two stored/presented digest values in constant time.
#[must_use]
pub fn digest_matches(expected: &HandoverDigest, presented: &HandoverDigest) -> bool {
    bool::from(expected.as_bytes().ct_eq(presented.as_bytes()))
}

/// Marker retained for the M1 crate-boundary test.
#[must_use]
pub const fn crate_boundary() -> &'static str {
    "login"
}

#[cfg(test)]
mod tests {
    use rand::{CryptoRng, Error, RngCore};

    use super::*;

    #[derive(Clone)]
    struct FixedRng(u8);

    impl RngCore for FixedRng {
        fn next_u32(&mut self) -> u32 {
            u32::from(self.0).wrapping_mul(0x0101_0101)
        }

        fn next_u64(&mut self) -> u64 {
            u64::from(self.next_u32()) << 32 | u64::from(self.next_u32())
        }

        fn fill_bytes(&mut self, destination: &mut [u8]) {
            for byte in destination {
                *byte = self.0;
                self.0 = self.0.wrapping_add(1);
            }
        }

        fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), Error> {
            self.fill_bytes(destination);
            Ok(())
        }
    }

    impl CryptoRng for FixedRng {}

    #[test]
    fn canonical_transport_secret_is_strict_and_redacted() {
        let secret = CanonicalTransportSecret::parse("0123456789ABCDEF0123456789ABCDEF")
            .expect("valid secret");
        assert_eq!(secret.as_bytes(), b"0123456789abcdef0123456789abcdef");
        assert!(!format!("{secret:?}").contains("012345"));
        assert_eq!(
            CanonicalTransportSecret::parse("not-a-secret"),
            Err(TransportSecretError::InvalidLength)
        );
    }

    #[test]
    fn argon2_policy_hashes_and_verifies_canonical_bytes() {
        let policy = CredentialPolicy::new().expect("fixed policy");
        let secret = CanonicalTransportSecret::parse("0123456789abcdef0123456789abcdef")
            .expect("valid secret");
        let hash = policy
            .hash_with_rng(&secret, &mut FixedRng(1))
            .expect("hash");
        assert!(
            hash.expose_phc()
                .starts_with("$argon2id$v=19$m=19456,t=2,p=1$")
        );
        let parsed = PasswordHash::new(hash.expose_phc()).expect("generated PHC");
        assert_eq!(parsed.salt.expect("salt").len(), 22);
        policy.verify(&secret, &hash).expect("verify");
        let wrong = CanonicalTransportSecret::parse("1123456789abcdef0123456789abcdef")
            .expect("valid wrong secret");
        assert_eq!(
            policy.verify(&wrong, &hash),
            Err(CredentialError::Verification)
        );
    }

    #[test]
    fn argon2_policy_rejects_malformed_downgraded_and_extended_phc_forms() {
        let policy = CredentialPolicy::new().expect("fixed policy");
        let secret = CanonicalTransportSecret::parse("0123456789abcdef0123456789abcdef")
            .expect("valid secret");
        let hash = policy
            .hash_with_rng(&secret, &mut FixedRng(7))
            .expect("hash");
        let valid = hash.expose_phc();
        let short_output = STANDARD_NO_PAD.encode([3_u8; 31]);
        let salt = PasswordHash::new(valid)
            .expect("generated PHC")
            .salt
            .expect("generated salt")
            .as_str();
        let cases = [
            "not-a-phc".to_owned(),
            valid.replacen("argon2id", "argon2i", 1),
            valid.replacen("$v=19", "$v=16", 1),
            valid.replacen("$v=19", "", 1),
            valid.replacen("m=19456,", "", 1),
            valid.replacen("m=19456", "m=4096", 1),
            valid.replacen("m=19456", "m=019456", 1),
            valid.replacen("m=19456,t=2,p=1", "t=2,m=19456,p=1", 1),
            valid.replacen("t=2", "t=1", 1),
            valid.replacen("p=1", "p=2", 1),
            valid.replacen("p=1", "p=1,keyid=abc", 1),
            valid.replacen("p=1", "p=1,data=abc", 1),
            valid.replacen("p=1", "p=1,unknown=1", 1),
            format!("$argon2id$v=19$m=19456,t=2,p=1${salt}${short_output}"),
            valid.replacen(salt, "YWJjZA", 1),
            valid.rsplit_once('$').expect("hash field").0.to_owned(),
        ];
        for candidate in cases {
            assert_eq!(
                policy.verify(&secret, &CredentialHash::new(candidate)),
                Err(CredentialError::UnsupportedPolicy)
            );
        }
    }

    #[test]
    fn deterministic_handover_round_trips_and_is_redacted() {
        let generated = generate_handover_with_rng(
            AccountId::new(7).expect("id"),
            ServiceKind::Game,
            SourceAddressPrefix::from_ip("203.0.113.9".parse().expect("IP")),
            SystemTime::UNIX_EPOCH,
            DEFAULT_HANDOVER_LIFETIME,
            &mut FixedRng(9),
        )
        .expect("generate");
        assert!(!format!("{:?}", generated.token).contains('.'));
        let parsed = parse_handover(generated.token.expose_secret()).expect("parse");
        assert_eq!(parsed.id, generated.record.id);
        assert!(digest_matches(&parsed.digest, &generated.record.digest));
    }
}
