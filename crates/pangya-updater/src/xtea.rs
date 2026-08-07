//! PangYa's XTEA variant, used for the client's patch `updatelist`.
//!
//! # Provenance
//!
//! Behaviour is adapted from `pangbox/pangfiles` (`crypto/pyxtea`), ISC licensed, © 2018-2020
//! John Chadwick. See `docs/PROVENANCE.md`. Only the algorithm is reproduced here; the Rust
//! implementation, bounds, and error model are this project's own.
//!
//! The variant differs from textbook XTEA in the delta constant's sign handling: the round
//! function *subtracts* `0x61C88647` from the running sum while encrypting, so `sum` walks the
//! same sequence a conventional implementation reaches by adding `0x9E3779B9`. It is
//! reproduced exactly rather than normalised, because the client is the specification.

/// Bytes in one XTEA block.
pub const BLOCK_BYTES: usize = 8;

const ROUNDS: usize = 16;
const DELTA: u32 = 0x61C8_8647;
const DECRYPT_INITIAL_SUM: u32 = 0xE377_9B90;

/// A 128-bit PangYa XTEA key as four little-endian words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XteaKey([u32; 4]);

impl XteaKey {
    /// Builds a key from four words.
    #[must_use]
    pub const fn new(words: [u32; 4]) -> Self {
        Self(words)
    }

    /// Returns the key words.
    #[must_use]
    pub const fn words(self) -> [u32; 4] {
        self.0
    }
}

/// A client region, which selects the `updatelist` key.
///
/// Only regions with a key recorded upstream are modelled. A region is added here only
/// together with the profile that needs it, so an unsupported region cannot be selected by
/// accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UpdateListRegion {
    /// United States.
    Us,
    /// Japan.
    Jp,
    /// Thailand.
    Th,
    /// Europe.
    Eu,
    /// Indonesia.
    Id,
    /// Korea.
    Kr,
}

impl UpdateListRegion {
    /// Returns the XTEA key this region's client expects.
    #[must_use]
    pub const fn key(self) -> XteaKey {
        match self {
            Self::Us => XteaKey::new([0x03F6_07A9, 0x036F_5A3E, 0x0110_02B4, 0x04AB_00EA]),
            Self::Jp => XteaKey::new([0x020A_5FD4, 0x01EE_BDFF, 0x02B3_C6A0, 0x04F6_A3E1]),
            Self::Th => XteaKey::new([0x050A_D33B, 0x00BA_FF09, 0x0452_FFDA, 0x02CB_4422]),
            Self::Eu => XteaKey::new([0x01E9_86D8, 0x0581_8479, 0x03D2_B0BB, 0x02C9_B030]),
            Self::Id => XteaKey::new([0x0164_0DB7, 0x0145_5A9B, 0x027F_1AB7, 0x0591_8B54]),
            Self::Kr => XteaKey::new([0x0485_B576, 0x0514_8E02, 0x0514_1D96, 0x028F_A9D6]),
        }
    }

    /// Parses a lowercase region code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "us" => Some(Self::Us),
            "jp" => Some(Self::Jp),
            "th" => Some(Self::Th),
            "eu" => Some(Self::Eu),
            "id" => Some(Self::Id),
            "kr" => Some(Self::Kr),
            _ => None,
        }
    }
}

/// One round's mixing term, shared by both directions so they cannot drift apart.
#[inline]
fn mix(value: u32, sum: u32, key_word: u32) -> u32 {
    let shifted = ((value << 4) ^ (value >> 5)).wrapping_add(value);
    shifted ^ sum.wrapping_add(key_word)
}

/// Encrypts one 8-byte block in place.
pub fn encrypt_block(key: XteaKey, block: &mut [u8; BLOCK_BYTES]) {
    let key = key.words();
    let (mut left, mut right) = split(block);
    let mut sum: u32 = 0;
    for _ in 0..ROUNDS {
        left = left.wrapping_add(mix(right, sum, key[(sum & 3) as usize]));
        sum = sum.wrapping_sub(DELTA);
        right = right.wrapping_add(mix(left, sum, key[((sum >> 11) & 3) as usize]));
    }
    join(block, left, right);
}

/// Decrypts one 8-byte block in place.
pub fn decrypt_block(key: XteaKey, block: &mut [u8; BLOCK_BYTES]) {
    let key = key.words();
    let (mut left, mut right) = split(block);
    let mut sum: u32 = DECRYPT_INITIAL_SUM;
    for _ in 0..ROUNDS {
        right = right.wrapping_sub(mix(left, sum, key[((sum >> 11) & 3) as usize]));
        sum = sum.wrapping_add(DELTA);
        left = left.wrapping_sub(mix(right, sum, key[(sum & 3) as usize]));
    }
    join(block, left, right);
}

fn split(block: &[u8; BLOCK_BYTES]) -> (u32, u32) {
    let (left, right) = block.split_at(4);
    (
        u32::from_le_bytes([left[0], left[1], left[2], left[3]]),
        u32::from_le_bytes([right[0], right[1], right[2], right[3]]),
    )
}

fn join(block: &mut [u8; BLOCK_BYTES], left: u32, right: u32) {
    block[..4].copy_from_slice(&left.to_le_bytes());
    block[4..].copy_from_slice(&right.to_le_bytes());
}

/// Encrypts a buffer, padding the tail with NUL bytes to the block size.
///
/// The client accepts this padding because the plaintext is XML whose parse ends at the
/// closing tag; [`decipher_trim_nul`] is the inverse used by tooling.
#[must_use]
pub fn encipher_pad_nul(key: XteaKey, plaintext: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(plaintext.len().next_multiple_of(BLOCK_BYTES));
    let mut chunks = plaintext.chunks_exact(BLOCK_BYTES);
    for chunk in chunks.by_ref() {
        let mut block = [0_u8; BLOCK_BYTES];
        block.copy_from_slice(chunk);
        encrypt_block(key, &mut block);
        out.extend_from_slice(&block);
    }
    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        let mut block = [0_u8; BLOCK_BYTES];
        block[..remainder.len()].copy_from_slice(remainder);
        encrypt_block(key, &mut block);
        out.extend_from_slice(&block);
    }
    out
}

/// A ciphertext whose length is not a whole number of XTEA blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("ciphertext length is not a multiple of the XTEA block size")]
pub struct XteaLengthError;

/// Decrypts a buffer and removes the NUL padding [`encipher_pad_nul`] added.
///
/// # Errors
/// Returns [`XteaLengthError`] when the input is not block aligned. Trailing NUL bytes that
/// were part of the plaintext are indistinguishable from padding and are also removed; the
/// documents this crate produces never end in one.
pub fn decipher_trim_nul(key: XteaKey, ciphertext: &[u8]) -> Result<Vec<u8>, XteaLengthError> {
    if !ciphertext.len().is_multiple_of(BLOCK_BYTES) {
        return Err(XteaLengthError);
    }
    let mut out = Vec::with_capacity(ciphertext.len());
    for chunk in ciphertext.chunks_exact(BLOCK_BYTES) {
        let mut block = [0_u8; BLOCK_BYTES];
        block.copy_from_slice(chunk);
        decrypt_block(key, &mut block);
        out.extend_from_slice(&block);
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_round_trips_for_every_region() {
        for region in [
            UpdateListRegion::Us,
            UpdateListRegion::Jp,
            UpdateListRegion::Th,
            UpdateListRegion::Eu,
            UpdateListRegion::Id,
            UpdateListRegion::Kr,
        ] {
            let original = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];
            let mut block = original;
            encrypt_block(region.key(), &mut block);
            assert_ne!(block, original, "{region:?} encryption was a no-op");
            decrypt_block(region.key(), &mut block);
            assert_eq!(block, original, "{region:?} did not round trip");
        }
    }

    #[test]
    fn padding_round_trips_for_every_tail_length() {
        let key = UpdateListRegion::Us.key();
        for length in 0..64_usize {
            // A trailing NUL would be eaten by the trim, so keep the plaintext NUL free.
            let plaintext: Vec<u8> = (0..length).map(|index| (index % 255 + 1) as u8).collect();
            let ciphertext = encipher_pad_nul(key, &plaintext);
            assert!(ciphertext.len().is_multiple_of(BLOCK_BYTES));
            assert_eq!(
                decipher_trim_nul(key, &ciphertext).expect("aligned"),
                plaintext,
                "length {length} did not round trip"
            );
        }
    }

    #[test]
    fn unaligned_ciphertext_is_rejected() {
        let key = UpdateListRegion::Us.key();
        assert_eq!(decipher_trim_nul(key, &[0; 7]), Err(XteaLengthError));
        assert_eq!(decipher_trim_nul(key, &[0; 9]), Err(XteaLengthError));
    }

    #[test]
    fn region_codes_round_trip_and_reject_unknown() {
        assert_eq!(
            UpdateListRegion::from_code("us"),
            Some(UpdateListRegion::Us)
        );
        assert_eq!(
            UpdateListRegion::from_code("kr"),
            Some(UpdateListRegion::Kr)
        );
        assert_eq!(UpdateListRegion::from_code("US"), None);
        assert_eq!(UpdateListRegion::from_code("zz"), None);
    }
}
