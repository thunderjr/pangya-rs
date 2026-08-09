use crate::PacketEncodeError;
use zeroize::Zeroize as _;

/// Checked packet payload writer.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PacketWriter {
    bytes: Vec<u8>,
}
macro_rules! integer_writer {
    ($name:ident, $ty:ty) => {
        #[doc = concat!("Writes a little-endian `", stringify!($ty), "`.")]
        pub fn $name(&mut self, value: $ty) {
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }
    };
}
impl PacketWriter {
    /// Creates an empty writer.
    #[must_use]
    pub const fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    /// Diagnostic only: replaces every 4-byte-aligned all-zero word in `start..` with a
    /// marker encoding that word's offset from `start`.
    ///
    /// Used to identify which field of a frame the client reads a value from: the value it
    /// ends up holding names its own offset. Never enabled on a normal run.
    #[doc(hidden)]
    pub fn mark_zero_words_from(&mut self, start: usize) {
        let mut offset = 0;
        while start + offset + 4 <= self.bytes.len() {
            let at = start + offset;
            if self.bytes[at..at + 4] == [0, 0, 0, 0] {
                let marker = 0xc000_0000_u32 | (offset as u32 & 0x00ff_ffff);
                self.bytes[at..at + 4].copy_from_slice(&marker.to_le_bytes());
            }
            offset += 4;
        }
    }
    /// Writes raw bytes.
    pub fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
    /// Writes a byte.
    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    /// Writes a signed byte.
    pub fn i8(&mut self, value: i8) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }
    integer_writer!(u16_le, u16);
    integer_writer!(i16_le, i16);
    integer_writer!(u32_le, u32);
    integer_writer!(i32_le, i32);
    integer_writer!(u64_le, u64);
    integer_writer!(i64_le, i64);
    /// Writes an LE IEEE-754 `f32` bit pattern.
    pub fn f32_le(&mut self, value: f32) {
        self.u32_le(value.to_bits());
    }
    /// Writes an LE IEEE-754 `f64` bit pattern.
    pub fn f64_le(&mut self, value: f64) {
        self.u64_le(value.to_bits());
    }
    /// Writes a `u16`-length-prefixed raw PangYa string.
    /// # Errors
    /// Rejects values exceeding either `maximum` or `u16::MAX`.
    pub fn pstring(&mut self, value: &[u8], maximum: usize) -> Result<(), PacketEncodeError> {
        if value.len() > maximum {
            return Err(PacketEncodeError::Limit {
                field: "string",
                actual: value.len(),
                maximum,
            });
        }
        let length = u16::try_from(value.len()).map_err(|_| PacketEncodeError::Limit {
            field: "string",
            actual: value.len(),
            maximum: usize::from(u16::MAX),
        })?;
        self.u16_le(length);
        self.bytes(value);
        Ok(())
    }
    /// Writes a fixed-width NUL-terminated raw string and zero padding.
    /// # Errors
    /// Rejects embedded NUL or values that leave no terminator byte.
    pub fn fixed_nul(&mut self, value: &[u8], width: usize) -> Result<(), PacketEncodeError> {
        if value.contains(&0) || value.len() >= width {
            return Err(PacketEncodeError::Limit {
                field: "fixed string",
                actual: value.len(),
                maximum: width.saturating_sub(1),
            });
        }
        self.bytes(value);
        self.bytes.resize(
            self.bytes
                .len()
                .checked_add(width - value.len())
                .ok_or(PacketEncodeError::Overflow("fixed string"))?,
            0,
        );
        Ok(())
    }
    /// Writes a count as `u16` after checking a semantic maximum.
    /// # Errors
    /// Rejects counts beyond the semantic or wire maximum.
    pub fn count_u16(&mut self, count: usize, maximum: usize) -> Result<(), PacketEncodeError> {
        if count > maximum {
            return Err(PacketEncodeError::Limit {
                field: "count",
                actual: count,
                maximum,
            });
        }
        let value = u16::try_from(count).map_err(|_| PacketEncodeError::Limit {
            field: "count",
            actual: count,
            maximum: usize::from(u16::MAX),
        })?;
        self.u16_le(value);
        Ok(())
    }
    /// Borrows encoded bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
    /// Consumes this writer.
    #[must_use]
    pub fn into_inner(mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
}

impl Drop for PacketWriter {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}
