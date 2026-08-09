//! Admin session bearers.
//!
//! Deliberately the same construction as the login-to-game handover bearer in `pangya-login`:
//! a nonsecret UUID selector, 256 OS-random bits, only the SHA-256 digest persisted, and a
//! constant-time comparison at the boundary. Two bearer schemes in one codebase is one more
//! than anyone can keep correct, so this mirrors the proven one rather than inventing another.

use std::{
    fmt::{self, Write as _},
    time::{Duration, SystemTime},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use pangya_domain::{
    AccountId, AdminSessionId, HandoverDigest, NewAdminSession, SourceAddressPrefix,
};
use rand::{CryptoRng, RngCore, rngs::OsRng};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Random bytes in the secret component.
const SECRET_BYTES: usize = 32;
/// Unpadded URL-safe Base64 length of [`SECRET_BYTES`].
const SECRET_ENCODED_BYTES: usize = 43;
/// Canonical UUID text length.
const SELECTOR_BYTES: usize = 36;

/// A session bearer whose formatting is always redacted.
#[derive(Clone, Eq, PartialEq)]
pub struct SessionToken(Zeroizing<String>);

impl SessionToken {
    /// Exposes the bearer only when setting the response cookie.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionToken([REDACTED])")
    }
}

/// A generated bearer paired with its digest-only persistence request.
#[derive(Clone, Debug)]
pub struct GeneratedSession {
    /// Bearer returned to the browser.
    pub token: SessionToken,
    /// Digest-only record persisted by the repository.
    pub record: NewAdminSession,
}

/// A parsed bearer's selector and digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedSession {
    /// Nonsecret selector.
    pub id: AdminSessionId,
    /// SHA-256 digest of the secret component.
    pub digest: HandoverDigest,
}

/// Redacted bearer generation or parsing failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SessionTokenError {
    /// Syntax, selector, or secret length was invalid.
    #[error("session token is invalid")]
    Invalid,
    /// The requested lifetime was zero or overflowed `SystemTime`.
    #[error("session lifetime is invalid")]
    InvalidLifetime,
}

/// Generates a selector and 256-bit bearer for one admin session.
///
/// # Errors
/// Returns [`SessionTokenError::InvalidLifetime`] for a zero or overflowing lifetime.
pub fn generate(
    account_id: AccountId,
    source_address_prefix: SourceAddressPrefix,
    now: SystemTime,
    lifetime: Duration,
) -> Result<GeneratedSession, SessionTokenError> {
    generate_with_rng(account_id, source_address_prefix, now, lifetime, &mut OsRng)
}

fn generate_with_rng<R>(
    account_id: AccountId,
    source_address_prefix: SourceAddressPrefix,
    now: SystemTime,
    lifetime: Duration,
    rng: &mut R,
) -> Result<GeneratedSession, SessionTokenError>
where
    R: CryptoRng + RngCore,
{
    if lifetime.is_zero() {
        return Err(SessionTokenError::InvalidLifetime);
    }
    let expires_at = now
        .checked_add(lifetime)
        .ok_or(SessionTokenError::InvalidLifetime)?;

    let mut selector_bytes = [0_u8; 16];
    rng.fill_bytes(&mut selector_bytes);
    selector_bytes[6] = (selector_bytes[6] & 0x0f) | 0x40;
    selector_bytes[8] = (selector_bytes[8] & 0x3f) | 0x80;
    let selector = Uuid::from_bytes(selector_bytes);

    let mut secret = Zeroizing::new([0_u8; SECRET_BYTES]);
    rng.fill_bytes(secret.as_mut());
    let encoded = Zeroizing::new(URL_SAFE_NO_PAD.encode(secret.as_ref()));
    let mut value = Zeroizing::new(String::with_capacity(
        SELECTOR_BYTES + 1 + SECRET_ENCODED_BYTES,
    ));
    write!(value, "{selector}.{}", encoded.as_str()).map_err(|_| SessionTokenError::Invalid)?;

    Ok(GeneratedSession {
        token: SessionToken(value),
        record: NewAdminSession {
            id: AdminSessionId::new(selector),
            account_id,
            digest: digest_secret(&secret),
            source_address_prefix,
            issued_at: now,
            expires_at,
        },
    })
}

/// Strictly parses a bearer and derives the digest the repository stores.
///
/// # Errors
/// Returns [`SessionTokenError::Invalid`] without echoing any bearer content.
pub fn parse(value: &str) -> Result<ParsedSession, SessionTokenError> {
    let (selector_text, secret_text) = value.split_once('.').ok_or(SessionTokenError::Invalid)?;
    if selector_text.len() != SELECTOR_BYTES || secret_text.len() != SECRET_ENCODED_BYTES {
        return Err(SessionTokenError::Invalid);
    }
    let selector = Uuid::parse_str(selector_text).map_err(|_| SessionTokenError::Invalid)?;
    // Rejects the non-hyphenated and uppercase spellings a parser would otherwise accept, so
    // one bearer has exactly one representation.
    if selector.to_string() != selector_text {
        return Err(SessionTokenError::Invalid);
    }
    let decoded = Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(secret_text)
            .map_err(|_| SessionTokenError::Invalid)?,
    );
    let secret = Zeroizing::new(
        <[u8; SECRET_BYTES]>::try_from(decoded.as_slice())
            .map_err(|_| SessionTokenError::Invalid)?,
    );
    Ok(ParsedSession {
        id: AdminSessionId::new(selector),
        digest: digest_secret(&secret),
    })
}

fn digest_secret(secret: &[u8; SECRET_BYTES]) -> HandoverDigest {
    HandoverDigest::new(Sha256::digest(secret).into())
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use rand::SeedableRng as _;
    use rand_chacha::ChaCha20Rng;

    use super::*;

    fn prefix() -> SourceAddressPrefix {
        SourceAddressPrefix::from_ip(IpAddr::V4(Ipv4Addr::LOCALHOST))
    }

    fn account() -> AccountId {
        AccountId::new(1).expect("positive account id")
    }

    #[test]
    fn generated_bearer_round_trips_to_the_persisted_digest() {
        let mut rng = ChaCha20Rng::seed_from_u64(7);
        let generated = generate_with_rng(
            account(),
            prefix(),
            SystemTime::UNIX_EPOCH,
            Duration::from_secs(3600),
            &mut rng,
        )
        .expect("valid lifetime");
        let parsed = parse(generated.token.expose_secret()).expect("canonical bearer");
        assert_eq!(parsed.id, generated.record.id);
        assert_eq!(parsed.digest, generated.record.digest);
    }

    #[test]
    fn two_generations_never_share_a_selector_or_digest() {
        let mut rng = ChaCha20Rng::seed_from_u64(11);
        let first = generate_with_rng(
            account(),
            prefix(),
            SystemTime::UNIX_EPOCH,
            Duration::from_secs(60),
            &mut rng,
        )
        .expect("valid lifetime");
        let second = generate_with_rng(
            account(),
            prefix(),
            SystemTime::UNIX_EPOCH,
            Duration::from_secs(60),
            &mut rng,
        )
        .expect("valid lifetime");
        assert_ne!(first.record.id, second.record.id);
        assert_ne!(first.record.digest, second.record.digest);
    }

    #[test]
    fn a_zero_lifetime_is_refused() {
        let mut rng = ChaCha20Rng::seed_from_u64(3);
        assert_eq!(
            generate_with_rng(
                account(),
                prefix(),
                SystemTime::UNIX_EPOCH,
                Duration::ZERO,
                &mut rng
            )
            .err(),
            Some(SessionTokenError::InvalidLifetime)
        );
    }

    #[test]
    fn malformed_bearers_are_refused_without_panicking() {
        let mut rng = ChaCha20Rng::seed_from_u64(5);
        let generated = generate_with_rng(
            account(),
            prefix(),
            SystemTime::UNIX_EPOCH,
            Duration::from_secs(60),
            &mut rng,
        )
        .expect("valid lifetime");
        let token = generated.token.expose_secret();
        let (selector, secret) = token.split_once('.').expect("canonical bearer");
        for candidate in [
            String::new(),
            ".".to_owned(),
            selector.to_owned(),
            format!("{selector}."),
            format!(".{secret}"),
            format!("{}.{secret}", selector.replace('-', "")),
            format!("{}.{secret}", selector.to_uppercase()),
            format!("{selector}.{secret}extra"),
            format!("{selector}.{}", &secret[..secret.len() - 1]),
            format!("{selector}.{}", "!".repeat(SECRET_ENCODED_BYTES)),
        ] {
            assert_eq!(
                parse(&candidate),
                Err(SessionTokenError::Invalid),
                "accepted {candidate:?}"
            );
        }
    }

    #[test]
    fn debug_never_echoes_bearer_content() {
        let mut rng = ChaCha20Rng::seed_from_u64(13);
        let generated = generate_with_rng(
            account(),
            prefix(),
            SystemTime::UNIX_EPOCH,
            Duration::from_secs(60),
            &mut rng,
        )
        .expect("valid lifetime");
        let rendered = format!("{:?}", generated.token);
        assert_eq!(rendered, "SessionToken([REDACTED])");
        assert!(!rendered.contains(generated.token.expose_secret()));
    }
}
