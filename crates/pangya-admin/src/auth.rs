//! Operator sign-in, sign-out, and identity.
//!
//! The browser sends the player's password, not its MD5 digest. The wire protocol's legacy
//! transport secret *is* an MD5 digest (ADR-0007), so this endpoint derives it server-side and
//! feeds the same `CredentialPolicy` the game login path uses. Sending the digest from the
//! browser would gain nothing — it is password-equivalent to whoever holds it — and would put
//! a second hashing implementation in JavaScript.
//!
//! MD5 here is a **format conversion, not a password hash**. Its output is the *input* to the
//! Argon2id policy that actually protects the stored verifier.

use std::time::{Duration, SystemTime};

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::{
        HeaderValue, StatusCode,
        header::{self, HeaderMap},
    },
    response::{IntoResponse, Response},
};
use md5::{Digest as _, Md5};
use pangya_domain::{
    AccountRole, AccountStatus, CredentialHash, NormalizedUsername, SourceAddressPrefix,
};
use pangya_login::CanonicalTransportSecret;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{
    AdminError, AdminState, MaybePeer, Operator, SESSION_COOKIE, admit_login, audit, session,
    source_prefix,
};

/// Operator sign-in request.
#[derive(Deserialize)]
pub struct LoginRequest {
    /// Display or normalized username.
    pub username: String,
    /// Plaintext password. Never logged, never persisted.
    ///
    /// Wrapped on the way out of serde so the field is zeroed on drop. The raw request body
    /// axum buffered before this point is not, and cannot be without owning the body reader —
    /// so this bounds the lifetime of the copy rather than claiming the password never
    /// existed in plain memory.
    #[serde(deserialize_with = "zeroizing_string")]
    pub password: Zeroizing<String>,
}

pub(crate) fn zeroizing_string<'de, D>(deserializer: D) -> Result<Zeroizing<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <String as Deserialize>::deserialize(deserializer).map(Zeroizing::new)
}

#[derive(Serialize)]
struct Identity {
    account_id: i64,
    username: String,
    role: &'static str,
}

/// `POST /auth/login`
pub(crate) async fn login(
    State(state): State<AdminState>,
    MaybePeer(peer): MaybePeer,
    Json(payload): Json<LoginRequest>,
) -> Result<Response, AdminError> {
    let prefix = source_prefix(peer);
    admit_login(&state, &prefix)?;

    // Every failure below returns the same `Unauthorized`. Distinguishing them on the wire
    // would turn this endpoint into an account-enumeration oracle.
    let Ok(username) = NormalizedUsername::parse(&payload.username) else {
        return Err(refusal(&prefix, "username_policy"));
    };
    let Ok(secret) = derive_transport_secret(&payload.password) else {
        return Err(refusal(&prefix, "password_policy"));
    };

    let record = state
        .repository
        .load_admin_authentication(&username)
        .await?;
    let Some(record) = record else {
        // Spend the same verification cost, so absence is not distinguishable by timing.
        let _ = state.credentials.verify(secret, decoy_hash()).await;
        return Err(refusal(&prefix, "unknown_username"));
    };
    let verified = state
        .credentials
        .verify(secret, record.credential_hash.clone())
        .await
        .is_ok();
    if !verified {
        return Err(refusal(&prefix, "bad_credentials"));
    }
    if record.role != AccountRole::Admin {
        return Err(refusal(&prefix, "not_an_admin"));
    }
    if record.status != AccountStatus::Active {
        return Err(refusal(&prefix, "account_inactive"));
    }

    let generated = session::generate(
        record.account_id,
        prefix.clone(),
        SystemTime::now(),
        state.config.session_lifetime,
    )
    .map_err(|_| AdminError::Storage)?;
    state
        .repository
        .issue_admin_session(generated.record)
        .await?;
    audit(
        &state,
        record.account_id,
        "admin.session.open",
        Some(record.account_id),
        &serde_json::json!({ "source_prefix": prefix.as_str() }),
    )
    .await?;
    tracing::info!(
        surface = "admin_api",
        account_id = record.account_id.get(),
        source_prefix = prefix.as_str(),
        "operator signed in"
    );

    let identity = Identity {
        account_id: record.account_id.get(),
        username: record.username_display,
        role: AccountRole::Admin.as_str(),
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        session_cookie(
            generated.token.expose_secret(),
            state.config.session_lifetime,
        )?,
    );
    Ok((StatusCode::OK, headers, Json(identity)).into_response())
}

/// `POST /auth/logout`
///
/// Outside the authentication layer on purpose: signing out an already-invalid session must
/// succeed, so a browser holding a stale cookie can always clear it.
pub(crate) async fn logout(
    State(state): State<AdminState>,
    request: axum::extract::Request,
) -> Result<Response, AdminError> {
    if let Some(presented) = crate::cookie_value(&request, SESSION_COOKIE)
        && let Ok(parsed) = session::parse(&presented)
    {
        state
            .repository
            .revoke_admin_session(parsed.id, SystemTime::now())
            .await?;
    }
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cleared_cookie());
    Ok((StatusCode::NO_CONTENT, headers).into_response())
}

/// `GET /auth/me`
pub(crate) async fn me(Operator(session): Operator) -> Response {
    Json(Identity {
        account_id: session.account_id.get(),
        username: session.username_display,
        role: session.role.as_str(),
    })
    .into_response()
}

/// Derives the legacy 32-hex transport secret from a plaintext password.
pub(crate) fn derive_transport_secret(
    password: &Zeroizing<String>,
) -> Result<CanonicalTransportSecret, AdminError> {
    if password.is_empty() {
        return Err(AdminError::Unauthorized);
    }
    let digest = Zeroizing::new(hex_lower(&Md5::digest(password.as_bytes())));
    CanonicalTransportSecret::parse(&digest).map_err(|_| AdminError::Unauthorized)
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            // Writing into a String is infallible, so the result is deliberately discarded
            // rather than turned into an error path that cannot be reached.
            let _ = write!(text, "{byte:02x}");
            text
        })
}

fn refusal(prefix: &SourceAddressPrefix, reason: &'static str) -> AdminError {
    // Diagnosable in the log, invisible on the wire.
    tracing::info!(
        surface = "admin_api",
        source_prefix = prefix.as_str(),
        reason,
        "operator sign-in refused"
    );
    AdminError::Unauthorized
}

fn session_cookie(token: &str, lifetime: Duration) -> Result<HeaderValue, AdminError> {
    // `HttpOnly` keeps page script from reading it; `SameSite=Strict` keeps a cross-site
    // request from carrying it. `Path=/admin` is tighter than `/`: the panel is served flat at
    // the root by its own service, so the cookie only needs to travel with the proxied
    // `/admin/v1` calls and never with a request for an asset.
    //
    // `Secure` is deliberately absent: the listener is loopback-bound and reached over an
    // already-encrypted tailnet, and setting it would make the cookie unusable over plain
    // HTTP on localhost.
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; Path=/admin; HttpOnly; SameSite=Strict; Max-Age={}",
        lifetime.as_secs()
    ))
    .map_err(|_| AdminError::Storage)
}

fn cleared_cookie() -> HeaderValue {
    HeaderValue::from_static(
        "pangya_admin_session=; Path=/admin; HttpOnly; SameSite=Strict; Max-Age=0",
    )
}

/// A syntactically valid PHC string that no password verifies against.
///
/// Used only so an unknown username costs the same as a known one.
fn decoy_hash() -> CredentialHash {
    CredentialHash::new(
        "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$\
         AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
            .to_owned(),
    )
}

/// Peer address extractor that tolerates a listener served without connection info.
impl<S> axum::extract::FromRequestParts<S> for MaybePeer
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<ConnectInfo<std::net::SocketAddr>>()
                .map(|info| info.0),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_the_rfc_1321_test_suite() {
        let cases: [(&str, &str); 6] = [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("a", "0cc175b9c0f1b6a831c399e269772661"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                "abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
            (
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "d174ab98d277d9f5a5611c2c9f419d9f",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(hex_lower(&Md5::digest(input.as_bytes())), expected);
        }
        // The suite's final case is eight repetitions of "1234567890"; built rather than
        // written out, because a literal miscounted by one digit still looks right.
        let eighty_digits = "1234567890".repeat(8);
        assert_eq!(eighty_digits.len(), 80);
        assert_eq!(
            hex_lower(&Md5::digest(eighty_digits.as_bytes())),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn a_derived_secret_equals_the_canonical_client_form() {
        let derived = derive_transport_secret(&Zeroizing::new("abc".to_owned()))
            .expect("abc derives a valid secret");
        // A real client would send exactly this text, so parsing it independently and
        // comparing proves the admin path reaches the same stored verifier.
        let from_client = CanonicalTransportSecret::parse("900150983cd24fb0d6963f7d28e17f72")
            .expect("known digest");
        assert_eq!(derived, from_client);
    }

    #[test]
    fn an_empty_password_is_refused_before_any_hashing() {
        assert_eq!(
            derive_transport_secret(&Zeroizing::new(String::new())),
            Err(AdminError::Unauthorized)
        );
    }

    #[test]
    fn the_decoy_hash_carries_the_exact_policy_parameters() {
        // A decoy with different parameters would verify faster than a real hash and
        // reintroduce the timing signal it exists to remove.
        assert!(
            decoy_hash()
                .expose_phc()
                .starts_with("$argon2id$v=19$m=19456,t=2,p=1$")
        );
    }

    #[test]
    fn the_cleared_cookie_matches_the_issued_cookie_attributes() {
        // A mismatched Path or SameSite leaves the browser holding the original cookie.
        let issued = session_cookie("token", Duration::from_secs(1)).expect("valid header");
        let cleared = cleared_cookie();
        for attribute in ["Path=/admin", "HttpOnly", "SameSite=Strict"] {
            let issued = issued.to_str().expect("ascii");
            let cleared = cleared.to_str().expect("ascii");
            assert!(issued.contains(attribute), "issued missing {attribute}");
            assert!(cleared.contains(attribute), "cleared missing {attribute}");
        }
        assert!(cleared.to_str().expect("ascii").contains("Max-Age=0"));
    }
}
