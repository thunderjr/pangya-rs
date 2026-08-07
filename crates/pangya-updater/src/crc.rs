//! PangYa's file checksum, the `fcrc` field of an `updatelist` entry.
//!
//! # Provenance
//!
//! Behaviour is adapted from `pangbox/pangfiles` (`hash/pycrc32`), ISC licensed, © 2018-2020
//! John Chadwick. See `docs/PROVENANCE.md`.
//!
//! This is *not* CRC-32/ISO-HDLC. Upstream builds its table with Go's `crc32.MakeTable`, which
//! treats its argument as an already-reflected polynomial, and is handed `0x04C11DB7` — the
//! normal-form CRC-32 polynomial. The result is a reflected CRC whose table is generated from
//! a constant that is not the reflection of the intended one. Pre- and post-inversion come
//! from Go's `crc32.Update`. The combination is reproduced exactly, because the client's
//! expectation is the specification; do not "fix" the constant to `0xEDB88320`.

/// The polynomial constant fed to the reflected table generator.
const TABLE_POLYNOMIAL: u32 = 0x04C1_1DB7;

/// The 256-entry reflected table.
static TABLE: [u32; 256] = build_table();

const fn build_table() -> [u32; 256] {
    let mut table = [0_u32; 256];
    let mut index = 0_usize;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                (value >> 1) ^ TABLE_POLYNOMIAL
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}

/// Incremental PangYa file checksum.
#[derive(Clone, Copy, Debug)]
pub struct FileChecksum {
    state: u32,
}

impl Default for FileChecksum {
    fn default() -> Self {
        Self::new()
    }
}

impl FileChecksum {
    /// Starts a checksum.
    #[must_use]
    pub const fn new() -> Self {
        Self { state: !0 }
    }

    /// Folds more bytes in.
    pub fn update(&mut self, bytes: &[u8]) {
        let mut state = self.state;
        for byte in bytes {
            let index = ((state ^ u32::from(*byte)) & 0xFF) as usize;
            state = TABLE[index] ^ (state >> 8);
        }
        self.state = state;
    }

    /// Finishes and returns the unsigned checksum.
    #[must_use]
    pub const fn finish(self) -> u32 {
        !self.state
    }

    /// Finishes and returns the checksum as the signed value the `fcrc` attribute carries.
    ///
    /// The client's XML writes this field signed, so a checksum above `i32::MAX` appears as a
    /// negative number. Reinterpreting the bits rather than saturating is what makes a
    /// generated document byte-comparable with a captured one.
    #[must_use]
    pub const fn finish_signed(self) -> i32 {
        self.finish() as i32
    }
}

/// Convenience checksum over a whole buffer.
#[must_use]
pub fn checksum(bytes: &[u8]) -> u32 {
    let mut hasher = FileChecksum::new();
    hasher.update(bytes);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_has_the_all_ones_identity() {
        // init 0xFFFFFFFF, no folding, final inversion.
        assert_eq!(checksum(&[]), 0);
    }

    #[test]
    fn incremental_matches_one_shot_at_every_split() {
        let data: Vec<u8> = (0..=255_u8).cycle().take(1000).collect();
        let expected = checksum(&data);
        for split in 0..data.len() {
            let mut hasher = FileChecksum::new();
            hasher.update(&data[..split]);
            hasher.update(&data[split..]);
            assert_eq!(hasher.finish(), expected, "split at {split}");
        }
    }

    #[test]
    fn signed_view_reinterprets_the_high_bit() {
        let mut hasher = FileChecksum::new();
        hasher.update(b"pangya");
        let unsigned = hasher.finish();
        assert_eq!(hasher.finish_signed(), unsigned as i32);
    }

    /// Pinned against the reference implementation's table and folding order.
    ///
    /// These were computed from the same reflected-table construction upstream uses and
    /// cross-checked by generating a whole `updatelist` for the U.S. client that matched the
    /// reference implementation's output byte for byte, `fcrc` fields included.
    #[test]
    fn known_vectors_match_the_reference_implementation() {
        assert_eq!(checksum(b"a"), 0xF9B2_971A);
        assert_eq!(checksum(b"pangya"), 0xF947_B848);
        assert_eq!(checksum(&[0_u8; 8]), 0xFFB1_270F);
    }

    #[test]
    fn signed_vectors_carry_the_negative_form_the_client_reads() {
        let mut hasher = FileChecksum::new();
        hasher.update(b"pangya");
        assert_eq!(hasher.finish_signed(), -112_740_280);
    }
}
