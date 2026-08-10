//! Tokio MessageService listener.

use futures_util::{SinkExt as _, StreamExt as _};
use pangya_protocol::{FrameCodec, OutboundFrame, ServiceKind, us852_game_hello};
use rand::{RngCore as _, rngs::OsRng};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    time::{Duration, interval},
};
use tokio_util::{codec::Framed, sync::CancellationToken};

use crate::{ClientPacket, MemoryStore, MessageSession, MessageStore, ServerPacket};

/// Bounded MessageService listener composition.
#[derive(Clone)]
pub struct MessageService {
    store: std::sync::Arc<dyn MessageStore>,
    key: u8,
    limits: pangya_protocol::CodecLimits,
}
impl MessageService {
    /// Creates a listener around shared social state.
    #[must_use]
    pub fn new(store: MemoryStore, key: u8, limits: pangya_protocol::CodecLimits) -> Self {
        Self {
            store: std::sync::Arc::new(store),
            key,
            limits,
        }
    }
    /// Composes a listener from durable storage.
    #[must_use]
    pub fn with_store(
        store: std::sync::Arc<dyn MessageStore>,
        key: u8,
        limits: pangya_protocol::CodecLimits,
    ) -> Self {
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
        // A zero constructor key requests the same per-connection random 4-bit key used by
        // LoginService. Nonzero keys remain available to deterministic integration tests.
        let key = if self.key == 0 {
            (OsRng.next_u32() & 0x0f) as u8
        } else {
            self.key
        };
        let hello = us852_game_hello(key).map_err(|_| MessageRuntimeError::Protocol)?;
        stream
            .write_all(&hello)
            .await
            .map_err(|_| MessageRuntimeError::Io)?;
        let mut framed = Framed::new(
            stream,
            FrameCodec::new(key, ServiceKind::Message, self.limits),
        );
        let mut session = MessageSession::with_store(self.store.clone());
        let mut ticker = interval(Duration::from_millis(50));
        ticker.tick().await; // interval's immediate tick is not a useful poll
        let mut response_salt = 0;
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => { let _ = session.disconnect().await; return Ok(()); },
                _ = ticker.tick() => {
                    if session.is_authenticated() {
                        let responses = session.poll().await.map_err(|_| MessageRuntimeError::Rejected)?;
                        Self::send_responses(&mut framed, responses, response_salt).await?;
                    }
                }
                next = framed.next() => {
                    let Some(frame) = next else {
                        let _ = session.disconnect().await;
                        return Ok(());
                    };
                    let frame = frame.map_err(|_| MessageRuntimeError::Protocol)?;
                    response_salt = frame.metadata.salt;
                    let nonce = (u64::from(frame.opcode) << 8) | u64::from(frame.metadata.salt);
                    session.admit_nonce(nonce).map_err(|_| MessageRuntimeError::Rejected)?;
                    let packet = ClientPacket::decode(frame.opcode, &frame.payload).map_err(|_| MessageRuntimeError::Protocol)?;
                    let goodbye = matches!(&packet, ClientPacket::Goodbye);
                    let mut responses = session.handle(packet).await.map_err(|_| MessageRuntimeError::Rejected)?;
                    responses.extend(session.poll().await.map_err(|_| MessageRuntimeError::Rejected)?);
                    Self::send_responses(&mut framed, responses, response_salt).await?;
                    if goodbye {
                        let _ = session.disconnect().await;
                        return Ok(());
                    }
                }
            }
        }
    }

    async fn send_responses<S>(
        framed: &mut Framed<S, FrameCodec>,
        responses: Vec<ServerPacket>,
        salt: u8,
    ) -> Result<(), MessageRuntimeError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
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
                    salt,
                })
                .await
                .map_err(|_| MessageRuntimeError::Io)?;
        }
        Ok(())
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
