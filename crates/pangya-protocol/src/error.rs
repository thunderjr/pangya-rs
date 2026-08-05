use crate::{Direction, ServiceKind};
use thiserror::Error;

/// Stable parser error classification suitable for redacted metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Input ended before a field completed.
    Truncated,
    /// A declared length/count exceeded configured limits.
    Limit,
    /// A value cannot be represented.
    Overflow,
    /// A required NUL terminator was absent.
    MissingTerminator,
    /// Transport cryptography or compression failed.
    Crypto,
    /// Packet is invalid in the current context.
    Invalid,
}
/// Context-rich decode failure without packet body or secrets.
#[derive(Debug, Error)]
pub enum PacketDecodeError {
    /// Packet or framed transport parsing failed with protocol context.
    #[error(
        "{class:?} decoding {direction:?} {service:?} opcode {opcode:?} at offset {offset}: {detail}"
    )]
    Context {
        /// Direction being parsed.
        direction: Direction,
        /// Selected service.
        service: ServiceKind,
        /// Opcode when already known.
        opcode: Option<u16>,
        /// Field byte offset.
        offset: usize,
        /// Stable error class.
        class: ErrorClass,
        /// Non-secret diagnostic.
        detail: String,
    },
    /// Context-neutral transport I/O required by `tokio_util::codec::Decoder`.
    #[error("transport I/O failed: {0}")]
    Io(#[from] std::io::Error),
}
impl PacketDecodeError {
    pub(crate) fn new(
        direction: Direction,
        service: ServiceKind,
        opcode: Option<u16>,
        offset: usize,
        class: ErrorClass,
        detail: impl Into<String>,
    ) -> Self {
        Self::Context {
            direction,
            service,
            opcode,
            offset,
            class,
            detail: detail.into(),
        }
    }

    /// Returns contextual fields when this is a packet parsing failure.
    #[must_use]
    pub const fn context(
        &self,
    ) -> Option<(Direction, ServiceKind, Option<u16>, usize, ErrorClass)> {
        match self {
            Self::Context {
                direction,
                service,
                opcode,
                offset,
                class,
                ..
            } => Some((*direction, *service, *opcode, *offset, *class)),
            Self::Io(_) => None,
        }
    }
}
/// Packet construction failure.
#[derive(Debug, Error)]
pub enum PacketEncodeError {
    /// A field violates the synthetic packet model's semantic invariants.
    #[error("invalid value for {field}")]
    Invalid {
        /// Stable non-secret field name.
        field: &'static str,
    },
    /// A value is too large for its wire field or configured cap.
    #[error("{field} value {actual} exceeds maximum {maximum}")]
    Limit {
        /// Field name.
        field: &'static str,
        /// Actual size/value.
        actual: usize,
        /// Maximum representable/configured value.
        maximum: usize,
    },
    /// Checked arithmetic failed.
    #[error("checked arithmetic overflow while encoding {0}")]
    Overflow(&'static str),
    /// The selected compatibility profile is unsupported by this packet family.
    #[error(transparent)]
    Profile(#[from] crate::ProfileError),
    /// Transport encryption or compression failed.
    #[error("transport encode failed: {0}")]
    Crypto(#[from] pangya_crypto::CryptoError),
    /// Framed transport I/O failed.
    #[error("transport I/O failed: {0}")]
    Io(#[from] std::io::Error),
}
