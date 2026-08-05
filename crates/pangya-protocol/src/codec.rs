use crate::{Direction, ErrorClass, PacketDecodeError, PacketEncodeError, ServiceKind};
use bytes::{BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};
use zeroize::Zeroizing;

const CLIENT_HEADER: usize = 5;

/// Per-connection codec allocation limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecLimits {
    /// Operational encrypted client-frame cap.
    pub max_client_frame_bytes: usize,
    /// Absolute server plaintext/decompression cap.
    pub max_server_plaintext_bytes: usize,
    /// Maximum decompression expansion multiplier.
    pub max_expansion_ratio: usize,
}
impl Default for CodecLimits {
    fn default() -> Self {
        Self {
            max_client_frame_bytes: 65_535,
            max_server_plaintext_bytes: 8 * 1024 * 1024,
            max_expansion_ratio: 128,
        }
    }
}
/// Redacted metadata retained for an inbound frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMetadata {
    /// Encrypted byte count.
    pub encrypted_len: usize,
    /// Client salt byte.
    pub salt: u8,
}
/// Decrypted client frame split into opcode and payload.
#[derive(Clone, PartialEq, Eq)]
pub struct InboundFrame {
    /// Wire opcode.
    pub opcode: u16,
    /// Zeroizing bytes following opcode.
    pub payload: Zeroizing<Vec<u8>>,
    /// Non-secret framing metadata.
    pub metadata: FrameMetadata,
}
impl std::fmt::Debug for InboundFrame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InboundFrame")
            .field("opcode", &self.opcode)
            .field("payload_len", &self.payload.len())
            .field("metadata", &self.metadata)
            .finish()
    }
}
/// Plain server packet to compress and encrypt.
#[derive(Clone, PartialEq, Eq)]
pub struct OutboundFrame {
    /// Wire opcode.
    pub opcode: u16,
    /// Zeroizing bytes following opcode.
    pub payload: Zeroizing<Vec<u8>>,
    /// Explicit salt (deterministic injection is for tests).
    pub salt: u8,
}
impl std::fmt::Debug for OutboundFrame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OutboundFrame")
            .field("opcode", &self.opcode)
            .field("payload_len", &self.payload.len())
            .field("salt", &self.salt)
            .finish()
    }
}
/// Bounded PangYa client decoder/server encoder.
#[derive(Debug, Clone)]
pub struct FrameCodec {
    key: u8,
    service: ServiceKind,
    limits: CodecLimits,
}
impl FrameCodec {
    /// Creates a codec for one immutable connection key.
    #[must_use]
    pub const fn new(key: u8, service: ServiceKind, limits: CodecLimits) -> Self {
        Self {
            key,
            service,
            limits,
        }
    }
    fn decode_error(
        &self,
        offset: usize,
        class: ErrorClass,
        detail: impl Into<String>,
    ) -> PacketDecodeError {
        PacketDecodeError::new(
            Direction::ClientToServer,
            self.service,
            None,
            offset,
            class,
            detail,
        )
    }
}
impl Decoder for FrameCodec {
    type Item = InboundFrame;
    type Error = PacketDecodeError;
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None);
        }
        let encoded = usize::from(u16::from_le_bytes([src[1], src[2]]));
        let total = encoded.checked_add(4).ok_or_else(|| {
            self.decode_error(1, ErrorClass::Overflow, "client frame length overflow")
        })?;
        if total < CLIENT_HEADER {
            return Err(self.decode_error(
                1,
                ErrorClass::Invalid,
                format!("client total length {total} is below {CLIENT_HEADER}"),
            ));
        }
        if total > self.limits.max_client_frame_bytes {
            return Err(self.decode_error(
                1,
                ErrorClass::Limit,
                format!(
                    "client frame {total} exceeds {}",
                    self.limits.max_client_frame_bytes
                ),
            ));
        }
        if src.len() < total {
            return Ok(None);
        }
        let encrypted = src.split_to(total);
        let salt = encrypted[0];
        let plain = pangya_crypto::client_decrypt(&encrypted, self.key)
            .map_err(|error| self.decode_error(0, ErrorClass::Crypto, error.to_string()))?;
        if plain.len() < 2 {
            return Err(self.decode_error(
                CLIENT_HEADER,
                ErrorClass::Truncated,
                "decrypted packet has no complete opcode",
            ));
        }
        let opcode = u16::from_le_bytes([plain[0], plain[1]]);
        Ok(Some(InboundFrame {
            opcode,
            payload: Zeroizing::new(plain[2..].to_vec()),
            metadata: FrameMetadata {
                encrypted_len: total,
                salt,
            },
        }))
    }
    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.decode(src)? {
            some @ Some(_) => Ok(some),
            None if src.is_empty() => Ok(None),
            None => Err(self.decode_error(
                src.len(),
                ErrorClass::Truncated,
                "connection ended with an incomplete client frame",
            )),
        }
    }
}
impl Encoder<OutboundFrame> for FrameCodec {
    type Error = PacketEncodeError;
    fn encode(&mut self, item: OutboundFrame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let plain_len = item
            .payload
            .len()
            .checked_add(2)
            .ok_or(PacketEncodeError::Overflow("server plaintext"))?;
        if plain_len > self.limits.max_server_plaintext_bytes {
            return Err(PacketEncodeError::Limit {
                field: "server plaintext",
                actual: plain_len,
                maximum: self.limits.max_server_plaintext_bytes,
            });
        }
        let mut plain = Zeroizing::new(Vec::with_capacity(plain_len));
        plain.extend_from_slice(&item.opcode.to_le_bytes());
        plain.extend_from_slice(&item.payload);
        let encrypted = pangya_crypto::server_encrypt(
            &plain,
            self.key,
            item.salt,
            self.limits.max_server_plaintext_bytes,
        )?;
        dst.reserve(encrypted.len());
        dst.put_slice(&encrypted);
        Ok(())
    }
}
