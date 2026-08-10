//! Tokio MessageService listener.

use futures_util::{SinkExt as _, StreamExt as _};
use pangya_protocol::{FrameCodec, OutboundFrame, ServiceKind, us852_game_hello};
use thiserror::Error;
use tokio::{
    io::AsyncWriteExt as _,
    net::{TcpListener, TcpStream},
};
use tokio_util::{codec::Framed, sync::CancellationToken};

use crate::{ClientPacket, MemoryStore, MessageSession, ServerPacket};

/// Bounded MessageService listener composition.
#[derive(Clone)]
pub struct MessageService {
    store: MemoryStore,
    key: u8,
    limits: pangya_protocol::CodecLimits,
}
impl MessageService {
    /// Creates a listener around shared social state.
    #[must_use]
    pub fn new(store: MemoryStore, key: u8, limits: pangya_protocol::CodecLimits) -> Self {
        Self { store, key, limits }
    }
    /// Runs until cancellation. Each accepted connection gets a fresh authenticated session.
    pub async fn serve(
        &self,
        listener: TcpListener,
        shutdown: CancellationToken,
    ) -> Result<(), MessageRuntimeError> {
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return Ok(()),
                accepted = listener.accept() => {
                    let (stream, _) = accepted.map_err(|_| MessageRuntimeError::Accept)?;
                    let service = self.clone();
                    let token = shutdown.child_token();
                    tokio::spawn(async move { let _ = service.connection(stream, token).await; });
                }
            }
        }
    }
    async fn connection(
        &self,
        mut stream: TcpStream,
        shutdown: CancellationToken,
    ) -> Result<(), MessageRuntimeError> {
        let hello = us852_game_hello(self.key).map_err(|_| MessageRuntimeError::Protocol)?;
        stream
            .write_all(&hello)
            .await
            .map_err(|_| MessageRuntimeError::Io)?;
        let mut framed = Framed::new(
            stream,
            FrameCodec::new(self.key, ServiceKind::Message, self.limits),
        );
        let mut session = MessageSession::new(self.store.clone());
        loop {
            let frame = tokio::select! {
                biased;
                () = shutdown.cancelled() => return Ok(()),
                next = framed.next() => next,
            };
            let Some(frame) = frame else {
                return Ok(());
            };
            let frame = frame.map_err(|_| MessageRuntimeError::Protocol)?;
            let nonce = (u64::from(frame.opcode) << 8) | u64::from(frame.metadata.salt);
            session
                .admit_nonce(nonce)
                .map_err(|_| MessageRuntimeError::Rejected)?;
            let packet = ClientPacket::decode(frame.opcode, &frame.payload)
                .map_err(|_| MessageRuntimeError::Protocol)?;
            let responses = session
                .handle(packet)
                .map_err(|_| MessageRuntimeError::Rejected)?;
            for response in responses {
                let opcode = match response {
                    ServerPacket::CredentialResponse { .. } => 0x2f,
                    _ => 0x30,
                };
                let payload = response
                    .encode_payload()
                    .map_err(|_| MessageRuntimeError::Protocol)?;
                framed
                    .send(OutboundFrame {
                        opcode,
                        payload: zeroize::Zeroizing::new(payload),
                        salt: frame.metadata.salt,
                    })
                    .await
                    .map_err(|_| MessageRuntimeError::Io)?;
            }
        }
    }
}
/// Redacted listener failure.
#[derive(Debug, Error)]
pub enum MessageRuntimeError {
    #[error("MessageService listener failed")]
    Accept,
    #[error("MessageService I/O failed")]
    Io,
    #[error("MessageService protocol rejected the connection")]
    Protocol,
    #[error("MessageService authentication or operation was rejected")]
    Rejected,
}
