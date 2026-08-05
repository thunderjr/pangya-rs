#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Safe, bounded PangYa U.S. 852 transport transforms.
//!
//! Oracle data and transform behavior are adapted from the ISC-licensed
//! pangbox/pangcrypt project. See the repository provenance and notices.

mod oracle;

use oracle::ORACLE;
use thiserror::Error;
use zeroize::Zeroizing;

const CLIENT_HEADER: usize = 5;
const SERVER_HEADER: usize = 8;
const MAX_KEY: u8 = 0x0f;

/// Failures produced by PangYa transport transforms.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// The negotiated transport key is outside `0x00..=0x0f`.
    #[error("transport key 0x{0:02x} is outside 0x00..=0x0f")]
    InvalidKey(u8),
    /// A transport header is incomplete.
    #[error("{kind} header needs {needed} bytes but only {actual} are present")]
    HeaderTooShort {
        /// Transport frame kind.
        kind: &'static str,
        /// Required header size.
        needed: usize,
        /// Supplied byte count.
        actual: usize,
    },
    /// The header's declared frame length disagrees with the supplied frame.
    #[error("{kind} frame declares {declared} bytes but contains {actual}")]
    LengthMismatch {
        /// Transport frame kind.
        kind: &'static str,
        /// Header-derived total size.
        declared: usize,
        /// Supplied byte count.
        actual: usize,
    },
    /// Checked length arithmetic or a protocol field overflowed.
    #[error("{0} length cannot be represented by the protocol")]
    LengthOverflow(&'static str),
    /// A production server packet does not contain a complete `u16` opcode.
    #[error("server plaintext needs at least {minimum} bytes but only {actual} are present")]
    PlaintextTooShort {
        /// Supplied plaintext size.
        actual: usize,
        /// Required opcode width.
        minimum: usize,
    },
    /// Plaintext exceeds the configured absolute cap.
    #[error("plaintext size {actual} exceeds cap {limit}")]
    PlaintextLimit {
        /// Supplied plaintext size.
        actual: usize,
        /// Configured absolute maximum.
        limit: usize,
    },
    /// The expansion cap cannot hold any output.
    #[error("decompression expansion bound is {limit} bytes")]
    ExpansionLimit {
        /// Maximum bounded destination size.
        limit: usize,
    },
    /// LZO1X rejected the input or exhausted the bounded output buffer.
    #[error("LZO1X operation failed: {0}")]
    Lzo(String),
}

fn oracle_index(key: u8, salt: u8) -> Result<usize, CryptoError> {
    if key > MAX_KEY {
        return Err(CryptoError::InvalidKey(key));
    }
    Ok((usize::from(key) << 8) + usize::from(salt))
}

fn declared_total(frame: &[u8], header: usize, kind: &'static str) -> Result<(), CryptoError> {
    if frame.len() < header {
        return Err(CryptoError::HeaderTooShort {
            kind,
            needed: header,
            actual: frame.len(),
        });
    }
    let encoded = usize::from(u16::from_le_bytes([frame[1], frame[2]]));
    let base = if header == CLIENT_HEADER { 4 } else { 3 };
    let declared = encoded
        .checked_add(base)
        .ok_or(CryptoError::LengthOverflow(kind))?;
    if declared != frame.len() {
        return Err(CryptoError::LengthMismatch {
            kind,
            declared,
            actual: frame.len(),
        });
    }
    Ok(())
}

/// Encrypts a client-to-server plaintext packet, including opcode.
///
/// # Errors
/// Returns an error for an invalid key or a packet too large for the client frame's `u16` length field.
pub fn client_encrypt(plain: &[u8], key: u8, salt: u8) -> Result<Vec<u8>, CryptoError> {
    let index = oracle_index(key, salt)?;
    let total = plain
        .len()
        .checked_add(CLIENT_HEADER)
        .ok_or(CryptoError::LengthOverflow("client"))?;
    let encoded = total
        .checked_sub(4)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(CryptoError::LengthOverflow("client"))?;
    let mut output = vec![0; total];
    output[CLIENT_HEADER..].copy_from_slice(plain);
    output[0] = salt;
    output[1..3].copy_from_slice(&encoded.to_le_bytes());
    output[4] = ORACLE[1][index];
    for offset in (8..output.len()).rev() {
        output[offset] ^= output[offset - 4];
    }
    output[4] ^= ORACLE[0][index];
    Ok(output)
}

/// Decrypts one complete client-to-server frame.
///
/// # Errors
/// Returns an error for an invalid key, incomplete header, or inconsistent declared frame length.
pub fn client_decrypt(frame: &[u8], key: u8) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if frame.len() < CLIENT_HEADER {
        return Err(CryptoError::HeaderTooShort {
            kind: "client",
            needed: CLIENT_HEADER,
            actual: frame.len(),
        });
    }
    let index = oracle_index(key, frame[0])?;
    declared_total(frame, CLIENT_HEADER, "client")?;
    let mut output = Zeroizing::new(frame.to_vec());
    output[4] = ORACLE[1][index];
    for offset in 8..output.len() {
        output[offset] ^= output[offset - 4];
    }
    Ok(Zeroizing::new(output[CLIENT_HEADER..].to_vec()))
}

/// Compresses and encrypts a server-to-client plaintext packet.
///
/// # Errors
/// Returns an error for invalid keys, limits, length overflow, or compression failure.
pub fn server_encrypt(
    plain: &[u8],
    key: u8,
    salt: u8,
    max_plaintext: usize,
) -> Result<Vec<u8>, CryptoError> {
    let index = oracle_index(key, salt)?;
    if plain.len() < 2 {
        return Err(CryptoError::PlaintextTooShort {
            actual: plain.len(),
            minimum: 2,
        });
    }
    if plain.len() > max_plaintext {
        return Err(CryptoError::PlaintextLimit {
            actual: plain.len(),
            limit: max_plaintext,
        });
    }
    let compressed = Zeroizing::new(
        lzokay::compress::compress(plain).map_err(|error| CryptoError::Lzo(error.to_string()))?,
    );
    let total = compressed
        .len()
        .checked_add(SERVER_HEADER)
        .ok_or(CryptoError::LengthOverflow("server"))?;
    let encoded = total
        .checked_sub(3)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(CryptoError::LengthOverflow("server"))?;
    let mut output = vec![0; total];
    output[SERVER_HEADER..].copy_from_slice(&compressed);
    output[0] = salt;
    output[1..3].copy_from_slice(&encoded.to_le_bytes());
    output[3] = ORACLE[0][index] ^ ORACLE[1][index];
    let u = plain.len();
    let x = u
        .checked_add(u / 255)
        .ok_or(CryptoError::LengthOverflow("metadata"))?
        & 0xff;
    let v = u
        .checked_sub(x)
        .ok_or(CryptoError::LengthOverflow("metadata"))?
        / 255;
    let y = v
        .checked_add(v / 255)
        .ok_or(CryptoError::LengthOverflow("metadata"))?
        & 0xff;
    let w = v
        .checked_sub(y)
        .ok_or(CryptoError::LengthOverflow("metadata"))?
        / 255;
    let z = w
        .checked_add(w / 255)
        .ok_or(CryptoError::LengthOverflow("metadata"))?
        & 0xff;
    output[7] = u8::try_from(x).map_err(|_| CryptoError::LengthOverflow("metadata"))?;
    output[6] = u8::try_from(y).map_err(|_| CryptoError::LengthOverflow("metadata"))?;
    output[5] = u8::try_from(z).map_err(|_| CryptoError::LengthOverflow("metadata"))?;
    for offset in (10..output.len()).rev() {
        output[offset] ^= output[offset - 4];
    }
    output[7] ^= ORACLE[1][index];
    Ok(output)
}

/// Decrypts and boundedly decompresses one complete server frame.
///
/// The destination allocation is `min(max_plaintext, compressed_len * max_expansion_ratio)`.
///
/// # Errors
/// Returns an error for invalid keys/headers, inconsistent lengths, invalid limits, malformed LZO1X, or output exceeding either cap.
pub fn server_decrypt(
    frame: &[u8],
    key: u8,
    max_plaintext: usize,
    max_expansion_ratio: usize,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if frame.len() < SERVER_HEADER {
        return Err(CryptoError::HeaderTooShort {
            kind: "server",
            needed: SERVER_HEADER,
            actual: frame.len(),
        });
    }
    let index = oracle_index(key, frame[0])?;
    declared_total(frame, SERVER_HEADER, "server")?;
    let mut decrypted = Zeroizing::new(frame.to_vec());
    decrypted[7] ^= ORACLE[1][index];
    for offset in 10..decrypted.len() {
        decrypted[offset] ^= decrypted[offset - 4];
    }
    let compressed = &decrypted[SERVER_HEADER..];
    let ratio_cap = compressed
        .len()
        .checked_mul(max_expansion_ratio)
        .ok_or(CryptoError::LengthOverflow("expansion ratio"))?;
    let output_cap = max_plaintext.min(ratio_cap);
    if output_cap == 0 && !compressed.is_empty() {
        return Err(CryptoError::ExpansionLimit { limit: output_cap });
    }
    let mut output = Zeroizing::new(vec![0; output_cap]);
    let written = lzokay::decompress::decompress(compressed, &mut output)
        .map_err(|error| CryptoError::Lzo(error.to_string()))?;
    if written < 2 {
        return Err(CryptoError::PlaintextTooShort {
            actual: written,
            minimum: 2,
        });
    }
    output.truncate(written);
    Ok(output)
}

#[cfg(test)]
mod tests;
