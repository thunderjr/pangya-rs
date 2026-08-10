//! Tokio MessageService listener.

use futures_util::{SinkExt as _, StreamExt as _};
use pangya_protocol::{FrameCodec, OutboundFrame, ServiceKind, us852_game_hello};
use rand::{RngCore as _, rngs::OsRng};
use std::sync::Arc;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    time::{Duration, interval, timeout},
};
use tokio_util::{codec::Framed, sync::CancellationToken};

use crate::{
    ClientPacket, MemoryStore, MessageSession, MessageStore, ServerPacket, SessionRegistry,
};

/// Bounded MessageService listener composition.
#[derive(Clone)]
pub struct MessageService {
    store: std::sync::Arc<dyn MessageStore>,
    key: u8,
    limits: pangya_protocol::CodecLimits,
    registry: std::sync::Arc<SessionRegistry>,
    connections: Arc<tokio::sync::Semaphore>,
}
impl MessageService {
    /// Creates a listener around shared social state.
    #[must_use]
    pub fn new(store: MemoryStore, key: u8, limits: pangya_protocol::CodecLimits) -> Self {
        Self {
            store: std::sync::Arc::new(store),
            key,
            limits,
            registry: std::sync::Arc::new(SessionRegistry::default()),
            connections: Arc::new(tokio::sync::Semaphore::new(256)),
        }
    }
    /// Composes a listener from durable storage.
    #[must_use]
    pub fn with_store(
        store: std::sync::Arc<dyn MessageStore>,
        key: u8,
        limits: pangya_protocol::CodecLimits,
    ) -> Self {
        Self {
            store,
            key,
            limits,
            registry: std::sync::Arc::new(SessionRegistry::default()),
            connections: Arc::new(tokio::sync::Semaphore::new(256)),
        }
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
                    let (stream, peer) = accepted.map_err(|_| MessageRuntimeError::Accept)?;
                    let Ok(permit) = self.connections.clone().try_acquire_owned() else {
                        drop(stream);
                        continue;
                    };
                    let service = self.clone();
                    let token = shutdown.child_token();
                    tokio::spawn(async move {
                        let _permit = permit;
                        let _ = service.connection(stream, peer.ip(), token).await;
                    });
                }
            }
        }
    }
    async fn connection(
        &self,
        mut stream: TcpStream,
        peer_ip: std::net::IpAddr,
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
        timeout(Duration::from_secs(5), stream.write_all(&hello))
            .await
            .map_err(|_| MessageRuntimeError::Io)?
            .map_err(|_| MessageRuntimeError::Io)?;
        let mut framed = Framed::new(
            stream,
            FrameCodec::new(key, ServiceKind::Message, self.limits),
        );
        let mut session = MessageSession::with_store(self.store.clone())
            .with_registry(self.registry.clone())
            .with_peer_ip(peer_ip);
        let mut ticker = interval(Duration::from_millis(50));
        ticker.tick().await; // interval's immediate tick is not a useful poll
        let mut response_salt = 0;
        let mut first_frame = true;
        let mut last_activity = tokio::time::Instant::now();
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => { let _ = session.disconnect().await; return Ok(()); },
                _ = tokio::time::sleep_until(last_activity + if first_frame {
                    Duration::from_secs(5)
                } else {
                    Duration::from_secs(120)
                }) => {
                    let _ = session.disconnect().await;
                    return Err(MessageRuntimeError::Timeout);
                }
                _ = ticker.tick() => {
                    if session.is_authenticated() {
                        let responses = match session.poll().await {
                            Ok(responses) => responses,
                            Err(_) => {
                                let _ = session.disconnect().await;
                                return Err(MessageRuntimeError::Rejected);
                            }
                        };
                        if let Err(error) = Self::send_responses(&mut framed, responses, response_salt).await {
                            let _ = session.disconnect().await;
                            return Err(error);
                        }
                        if session.ack_pending().await.is_err() {
                            let _ = session.disconnect().await;
                            return Err(MessageRuntimeError::Rejected);
                        }
                    }
                }
                next = framed.next() => {
                    let Some(frame) = next else {
                        let _ = session.disconnect().await;
                        return Ok(());
                    };
                    let frame = match frame {
                        Ok(frame) => {
                            first_frame = false;
                            last_activity = tokio::time::Instant::now();
                            frame
                        },
                        Err(_) => {
                            let _ = session.disconnect().await;
                            return Err(MessageRuntimeError::Protocol);
                        }
                    };
                    response_salt = frame.metadata.salt;
                    let nonce = (u64::from(frame.opcode) << 8) | u64::from(frame.metadata.salt);
                    if session.admit_nonce(nonce).is_err() {
                        let _ = session.disconnect().await;
                        return Err(MessageRuntimeError::Rejected);
                    }
                    let packet = match ClientPacket::decode(frame.opcode, &frame.payload) {
                        Ok(packet) => packet,
                        Err(_) => {
                            let _ = session.disconnect().await;
                            return Err(MessageRuntimeError::Protocol);
                        }
                    };
                    let goodbye = matches!(&packet, ClientPacket::Goodbye);
                    let mut responses = match session.handle(packet).await {
                        Ok(responses) => responses,
                        Err(_) => {
                            let _ = session.disconnect().await;
                            return Err(MessageRuntimeError::Rejected);
                        }
                    };
                    match session.poll().await {
                        Ok(polled) => responses.extend(polled),
                        Err(_) => {
                            let _ = session.disconnect().await;
                            return Err(MessageRuntimeError::Rejected);
                        }
                    }
                    if let Err(error) = Self::send_responses(&mut framed, responses, response_salt).await {
                        let _ = session.disconnect().await;
                        return Err(error);
                    }
                    if session.ack_pending().await.is_err() {
                        let _ = session.disconnect().await;
                        return Err(MessageRuntimeError::Rejected);
                    }
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
    #[error("MessageService connection timed out")]
    Timeout,
}
