use crate::{Direction, ErrorClass, PacketDecodeError, ServiceKind};

/// Checked, non-allocating reader over one decrypted packet payload.
pub struct PacketReader<'a> {
    bytes: &'a [u8],
    offset: usize,
    direction: Direction,
    service: ServiceKind,
    opcode: Option<u16>,
}
macro_rules! integer_reader {
    ($name:ident, $ty:ty, $size:expr) => {
        #[doc = concat!("Reads a little-endian `", stringify!($ty), "`.\n\n# Errors\nReturns a contextual truncation error when too few bytes remain.")]
        pub fn $name(&mut self) -> Result<$ty, PacketDecodeError> { Ok(<$ty>::from_le_bytes(self.array::<$size>()?)) }
    };
}
impl<'a> PacketReader<'a> {
    /// Creates a contextual reader.
    #[must_use]
    pub const fn new(
        bytes: &'a [u8],
        direction: Direction,
        service: ServiceKind,
        opcode: Option<u16>,
    ) -> Self {
        Self {
            bytes,
            offset: 0,
            direction,
            service,
            opcode,
        }
    }
    fn error(&self, class: ErrorClass, detail: impl Into<String>) -> PacketDecodeError {
        self.error_at(self.offset, class, detail)
    }
    fn error_at(
        &self,
        offset: usize,
        class: ErrorClass,
        detail: impl Into<String>,
    ) -> PacketDecodeError {
        PacketDecodeError::new(
            self.direction,
            self.service,
            self.opcode,
            offset,
            class,
            detail,
        )
    }
    pub(crate) fn invalid(&self, detail: impl Into<String>) -> PacketDecodeError {
        self.error(ErrorClass::Invalid, detail)
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], PacketDecodeError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| self.error(ErrorClass::Overflow, "field offset overflow"))?;
        let value = self.bytes.get(self.offset..end).ok_or_else(|| {
            self.error(
                ErrorClass::Truncated,
                format!("need {count} bytes; {} remain", self.remaining()),
            )
        })?;
        self.offset = end;
        Ok(value)
    }
    /// Reads a fixed byte array. # Errors Returns a truncation error when incomplete.
    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], PacketDecodeError> {
        let start = self.offset;
        let bytes = self.take(N)?;
        bytes.try_into().map_err(|_| {
            PacketDecodeError::new(
                self.direction,
                self.service,
                self.opcode,
                start,
                ErrorClass::Truncated,
                "fixed array incomplete",
            )
        })
    }
    integer_reader!(u8, u8, 1);
    integer_reader!(i8, i8, 1);
    integer_reader!(u16_le, u16, 2);
    integer_reader!(i16_le, i16, 2);
    integer_reader!(u32_le, u32, 4);
    integer_reader!(i32_le, i32, 4);
    integer_reader!(u64_le, u64, 8);
    integer_reader!(i64_le, i64, 8);
    /// Reads an LE IEEE-754 `f32`, preserving its wire bit pattern. # Errors Returns a truncation error.
    pub fn f32_le(&mut self) -> Result<f32, PacketDecodeError> {
        Ok(f32::from_bits(self.u32_le()?))
    }
    /// Reads an LE IEEE-754 `f64`, preserving its wire bit pattern. # Errors Returns a truncation error.
    pub fn f64_le(&mut self) -> Result<f64, PacketDecodeError> {
        Ok(f64::from_bits(self.u64_le()?))
    }
    /// Reads a `u16`-length-prefixed raw PangYa string. # Errors Rejects truncation or a configured length excess.
    pub fn pstring(&mut self, maximum: usize) -> Result<&'a [u8], PacketDecodeError> {
        let start = self.offset;
        let length = usize::from(self.u16_le()?);
        if length > maximum {
            return Err(self.error_at(
                start,
                ErrorClass::Limit,
                format!("string length {length} exceeds {maximum}"),
            ));
        }
        self.take(length)
    }
    /// Reads a fixed-width raw string through its first NUL. # Errors Rejects truncation or missing NUL.
    pub fn fixed_nul(&mut self, width: usize) -> Result<&'a [u8], PacketDecodeError> {
        let start = self.offset;
        let field = self.take(width)?;
        let end = field.iter().position(|byte| *byte == 0).ok_or_else(|| {
            PacketDecodeError::new(
                self.direction,
                self.service,
                self.opcode,
                start,
                ErrorClass::MissingTerminator,
                "fixed string has no NUL",
            )
        })?;
        Ok(&field[..end])
    }
    /// Reads a bounded `u16` count. # Errors Rejects counts above `maximum`.
    pub fn count_u16(&mut self, maximum: usize) -> Result<usize, PacketDecodeError> {
        let start = self.offset;
        let count = usize::from(self.u16_le()?);
        if count > maximum {
            Err(self.error_at(
                start,
                ErrorClass::Limit,
                format!("count {count} exceeds {maximum}"),
            ))
        } else {
            Ok(count)
        }
    }
    /// Applies a fallible item decoder a bounded number of times. # Errors Propagates count/item failures.
    pub fn vector<T>(
        &mut self,
        count: usize,
        maximum: usize,
        mut decode: impl FnMut(&mut Self) -> Result<T, PacketDecodeError>,
    ) -> Result<Vec<T>, PacketDecodeError> {
        if count > maximum {
            return Err(self.error(
                ErrorClass::Limit,
                format!("count {count} exceeds {maximum}"),
            ));
        }
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(decode(self)?);
        }
        Ok(values)
    }
    /// Captures all unclassified trailing bytes.
    #[must_use]
    pub fn unknown_tail(&mut self) -> &'a [u8] {
        let tail = &self.bytes[self.offset..];
        self.offset = self.bytes.len();
        tail
    }
    /// Current field offset.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }
    /// Remaining unread bytes.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
}
