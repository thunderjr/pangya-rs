#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! A headless U.S. 852 retail client, for taking the second seat in a versus room.
//!
//! A versus hole needs two players, and two real clients cannot be driven on one desktop: the
//! game reads its mouse through DirectInput and only the instance holding input focus acts on a
//! click, which cannot be moved back to a client that has lost it. The failed attempts are
//! recorded in `scripts/windows/pangya-client.ps1`.
//!
//! This removes the problem rather than solving it. One real client is driven by a person, and
//! this takes the other seat: it speaks the same retail wire, so what the real client
//! experiences is a genuine two-player match, and every frame either side exchanges is printed
//! in order. It is a test instrument, never a bot — it plays a fixed script and claims nothing.

use std::{net::SocketAddr, time::Duration};

use clap::Parser;
use pangya_protocol::{
    CompatibilityProfile, EncodePacket, PacketWriter, ROOM_PLAYER_RECORD_BYTES, RetailGameAuth,
    RoomPlayerFlags, US852_SERVER_VERSION, encode_packet_payload,
};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpStream,
    time::timeout,
};
use zeroize::Zeroizing;

/// Ceiling on one decompressed server frame, matching the shipped protocol limit.
const MAX_PLAINTEXT: usize = 8 * 1024 * 1024;
/// Ceiling on decompression expansion, matching the shipped protocol limit.
const MAX_EXPANSION: usize = 128;
/// How long to wait for any one frame before giving up on a step.
const FRAME_TIMEOUT: Duration = Duration::from_secs(30);

/// Retail client opcodes this instrument sends.
mod client_opcode {
    /// Channel selection; a one-byte sub-server id.
    pub const SELECT_CHANNEL: u16 = 0x0004;
    /// Room creation.
    pub const ROOM_CREATE: u16 = 0x0008;
    /// Room join by number.
    pub const ROOM_JOIN: u16 = 0x0009;
    /// Match start; only the room master may send it.
    pub const START_MATCH: u16 = 0x000e;
    /// Ready state; a single byte, zero meaning ready.
    pub const ROOM_READY: u16 = 0x000d;
    /// Hole loading finished.
    pub const HOLE_LOAD_FINISHED: u16 = 0x0011;
    /// Committed shot.
    pub const SHOT_COMMIT: u16 = 0x0012;
    /// Post-shot barrier that ends a turn.
    pub const SHOT_END: u16 = 0x001c;
    /// This player's ball is in the hole.
    pub const HOLE_FINISH: u16 = 0x0031;
}

/// Retail server opcodes this instrument reacts to.
mod server_opcode {
    /// Channel entry accepted.
    pub const CHANNEL_JOINED: u16 = 0x004e;
    /// Room join result; the first two bytes are a status word.
    pub const ROOM_JOIN_RESULT: u16 = 0x0049;
    /// Room roster.
    pub const ROOM_CENSUS: u16 = 0x0048;
    /// Match plan; the hole is loading from here.
    pub const MATCH_INFO: u16 = 0x0052;
    /// A turn was handed to a player.
    pub const TURN_START: u16 = 0x0063;
    /// The hole finished.
    pub const FINISH_HOLE: u16 = 0x0065;
    /// Final standings.
    pub const MATCH_FINISH: u16 = 0x0066;
}

#[derive(Debug, Error)]
enum ClientError {
    #[error("connect failed")]
    Connect,
    #[error("the connection closed while waiting for {0}")]
    Closed(&'static str),
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    #[error("the server hello was not the nine-byte retail form")]
    Hello,
    #[error("frame encoding failed")]
    Encode,
    #[error("frame decryption failed")]
    Decrypt,
    #[error("the room refused this client")]
    RoomRefused,
    #[error("give either --room to join one or --host to open one")]
    NoSeat,
    #[error("nobody joined the room")]
    NobodyJoined,
}

/// A headless retail client that takes the second seat in a versus room.
#[derive(Debug, Parser)]
#[command(about, long_about = None)]
struct Args {
    /// GameService address, as the real client reaches it.
    #[arg(long)]
    game: SocketAddr,
    /// Numeric account id this seat plays as.
    #[arg(long)]
    account_id: u32,
    /// Account username. Not authoritative; the bearer is.
    #[arg(long)]
    username: String,
    /// Handover bearer, from `pangya-server account handover`. Single use.
    #[arg(long, env = "PANGYA_HANDOVER")]
    handover: String,
    /// Room number to join, as the host's client shows it. Omit to host instead.
    #[arg(long, conflicts_with = "host")]
    room: Option<u16>,
    /// Host a two-player versus room and start it once someone joins.
    #[arg(long)]
    host: bool,
    /// Room name when hosting.
    #[arg(long, default_value = "PangYa-RS")]
    room_name: String,
    /// Channel sub-server id.
    #[arg(long, default_value_t = 1)]
    channel: u8,
    /// Strokes to play before holing out.
    #[arg(long, default_value_t = 2)]
    strokes: u8,
}

#[tokio::main]
async fn main() -> Result<(), ClientError> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();
    let mut session = Session::connect(args.game).await?;
    session.authenticate(&args).await?;
    session.enter_channel(args.channel).await?;
    match args.room {
        Some(room) => {
            session.join_room(room).await?;
            session.ready().await?;
        }
        None if args.host => session.host_room(&args.room_name).await?,
        None => return Err(ClientError::NoSeat),
    }
    session.play_hole(args.strokes).await?;
    tracing::info!("the hole settled; this seat is done");
    Ok(())
}

/// Whether a census lists exactly two players and every one of them but the master is ready.
///
/// The master's own button reads Start rather than Ready, so it never sets the flag; requiring
/// it would wait forever. Everyone else must have it, or the start is refused.
fn census_is_ready_pair(census: &[u8]) -> bool {
    const HEADER: usize = 4;
    const FLAGS_OFFSET: usize = 4 + 22 + 17 + 1 + 4 + 4 + 4 + 16 + 4 + 4;
    if census.first().copied() != Some(0) || census.get(3).copied() != Some(2) {
        return false;
    }
    (0..2).all(|index| {
        let at = HEADER + index * ROOM_PLAYER_RECORD_BYTES + FLAGS_OFFSET;
        let Some(low) = census.get(at) else {
            return false;
        };
        let Some(high) = census.get(at.saturating_add(1)) else {
            return false;
        };
        let flags = u16::from_le_bytes([*low, *high]);
        flags & RoomPlayerFlags::MASTER != 0 || flags & RoomPlayerFlags::READY != 0
    })
}

/// One encrypted GameService connection and the salt counter its frames carry.
struct Session {
    stream: TcpStream,
    key: u8,
    salt: u8,
}

impl Session {
    async fn connect(address: SocketAddr) -> Result<Self, ClientError> {
        let mut stream = TcpStream::connect(address)
            .await
            .map_err(|_| ClientError::Connect)?;
        // The retail hello is nine plaintext bytes and the key is the last of them.
        let mut hello = [0_u8; 9];
        timeout(FRAME_TIMEOUT, stream.read_exact(&mut hello))
            .await
            .map_err(|_| ClientError::Timeout("hello"))?
            .map_err(|_| ClientError::Closed("hello"))?;
        let key = *hello.get(8).ok_or(ClientError::Hello)?;
        if key > 0x0f {
            return Err(ClientError::Hello);
        }
        tracing::info!(key, "connected");
        Ok(Self {
            stream,
            key,
            salt: 1,
        })
    }

    async fn authenticate(&mut self, args: &Args) -> Result<(), ClientError> {
        let auth = RetailGameAuth {
            username: args.username.as_bytes().to_vec(),
            user_id: args.account_id,
            login_key: Zeroizing::new(args.handover.as_bytes().to_vec()),
            client_version: US852_SERVER_VERSION.to_vec(),
            session_key: Zeroizing::new(Vec::new()),
        };
        let payload = encode_packet_payload(&auth, &CompatibilityProfile::US_852)
            .map_err(|_| ClientError::Encode)?;
        self.send(RetailGameAuth::OPCODE, &payload).await?;
        // The bootstrap is a run of frames the client only has to read. Draining until the
        // channel-select reply would be wrong — that comes later — so this drains until the
        // server goes quiet, which is what the bootstrap ending looks like.
        let frames = self.drain(Duration::from_secs(5)).await?;
        tracing::info!(frames, "bootstrap received");
        Ok(())
    }

    async fn enter_channel(&mut self, channel: u8) -> Result<(), ClientError> {
        self.send(client_opcode::SELECT_CHANNEL, &[channel]).await?;
        self.wait_for(server_opcode::CHANNEL_JOINED, "channel entry")
            .await?;
        let _notice = self.drain(Duration::from_secs(2)).await?;
        tracing::info!("in the channel");
        Ok(())
    }

    async fn join_room(&mut self, room: u16) -> Result<(), ClientError> {
        let mut writer = PacketWriter::default();
        writer.u16_le(room);
        writer.pstring(b"", 64).map_err(|_| ClientError::Encode)?;
        self.send(client_opcode::ROOM_JOIN, &writer.into_inner())
            .await?;
        let body = self
            .wait_for(server_opcode::ROOM_JOIN_RESULT, "room join")
            .await?;
        // The same opcode carries acceptance and refusal; the status word is what separates
        // them, and reading only the opcode is how a refused join looks like a joined room.
        let status = u16::from_le_bytes([
            *body.first().ok_or(ClientError::RoomRefused)?,
            *body.get(1).ok_or(ClientError::RoomRefused)?,
        ]);
        if status != 0 {
            tracing::error!(status, "the room refused this client");
            return Err(ClientError::RoomRefused);
        }
        self.wait_for(server_opcode::ROOM_CENSUS, "room roster")
            .await?;
        tracing::info!(room, "in the room");
        Ok(())
    }

    /// Opens a two-player versus room, waits for the other seat, and starts the match.
    ///
    /// One hole and a capacity of two, because that is what this server settles and the smallest
    /// a real client's Make Room dialog offers. Pressing Start is also what says the master is
    /// ready — a real client's button reads Start for the master and Ready for everyone else.
    async fn host_room(&mut self, name: &str) -> Result<(), ClientError> {
        let mut writer = PacketWriter::default();
        writer.u8(0); // room kind: versus
        writer.u32_le(30_000); // shot timer
        writer.u32_le(600_000); // game timer
        writer.u8(2); // capacity
        writer.u8(0); // hole progression
        writer.u8(1); // holes
        writer.u8(0); // course
        writer.bytes(&[0; 5]);
        writer
            .pstring(name.as_bytes(), 64)
            .map_err(|_| ClientError::Encode)?;
        writer.pstring(b"", 64).map_err(|_| ClientError::Encode)?;
        self.send(client_opcode::ROOM_CREATE, &writer.into_inner())
            .await?;
        let body = self
            .wait_for(server_opcode::ROOM_JOIN_RESULT, "room create")
            .await?;
        let status = u16::from_le_bytes([
            *body.first().ok_or(ClientError::RoomRefused)?,
            *body.get(1).ok_or(ClientError::RoomRefused)?,
        ]);
        if status != 0 {
            return Err(ClientError::RoomRefused);
        }
        // The room number sits after the status, the name, the settings block and the identity
        // fields. It is what the other seat is told to join.
        let room = u16::from_le_bytes([
            *body
                .get(2 + 64 + 5 + 17 + 3)
                .ok_or(ClientError::RoomRefused)?,
            *body
                .get(2 + 64 + 5 + 17 + 4)
                .ok_or(ClientError::RoomRefused)?,
        ]);
        tracing::info!(room, "hosting; join this room number with the other seat");
        // A census reporting two occupants, both ready, is what unlocks Start on a real client:
        // the room filling up is not enough, and starting on the arrival alone races the other
        // seat's ready packet and is refused.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(ClientError::NobodyJoined);
            }
            let (opcode, census) = self.receive("the room to fill").await?;
            if opcode == server_opcode::ROOM_CENSUS && census_is_ready_pair(&census) {
                break;
            }
        }
        tracing::info!("the room is full; starting");
        self.send(client_opcode::START_MATCH, &0_u32.to_le_bytes())
            .await?;
        Ok(())
    }

    async fn ready(&mut self) -> Result<(), ClientError> {
        // Zero means ready. Only the host can start, so this seat says it and waits — and it
        // must not drain afterwards: the host can start the moment it sees this, and a drain
        // would swallow the match plan that follows.
        self.send(client_opcode::ROOM_READY, &[0]).await?;
        tracing::info!("ready; waiting for the host to start");
        Ok(())
    }

    async fn play_hole(&mut self, strokes: u8) -> Result<(), ClientError> {
        self.wait_for(server_opcode::MATCH_INFO, "the match plan")
            .await?;
        let _conditions = self.drain(Duration::from_secs(3)).await?;
        self.send(client_opcode::HOLE_LOAD_FINISHED, &[]).await?;
        tracing::info!("loaded; waiting for a turn");

        // The turn frames name a connection this seat has not been told is its own, so rather
        // than guess it plays on every turn it is offered: the aggregate ignores a command from
        // whoever does not own the turn, so an out-of-turn shot costs nothing but a round trip.
        let mut played = 0_u8;
        loop {
            let opcode = self.next_opcode("a turn").await?;
            match opcode {
                server_opcode::TURN_START => {
                    if played >= strokes {
                        self.send(client_opcode::HOLE_FINISH, &[]).await?;
                        tracing::info!(played, "holed out");
                        continue;
                    }
                    // The payload is the client's own and nothing reads it here; a real client
                    // sends its trajectory inputs, which the server relays without interpreting.
                    self.send(client_opcode::SHOT_COMMIT, &[0; 8]).await?;
                    self.send(client_opcode::SHOT_END, &[]).await?;
                    played = played.saturating_add(1);
                    tracing::info!(played, "played a stroke");
                }
                server_opcode::FINISH_HOLE => tracing::info!("the hole finished"),
                server_opcode::MATCH_FINISH => return Ok(()),
                _ => {}
            }
        }
    }

    async fn send(&mut self, opcode: u16, payload: &[u8]) -> Result<(), ClientError> {
        let mut plain = Vec::with_capacity(payload.len().saturating_add(2));
        plain.extend_from_slice(&opcode.to_le_bytes());
        plain.extend_from_slice(payload);
        let frame = pangya_crypto::client_encrypt(&plain, self.key, self.salt)
            .map_err(|_| ClientError::Encode)?;
        self.salt = self.salt.wrapping_add(1) & 0x0f;
        self.stream
            .write_all(&frame)
            .await
            .map_err(|_| ClientError::Closed("a send"))?;
        tracing::debug!(
            direction = "out",
            opcode = format!("{opcode:#06x}"),
            "frame"
        );
        Ok(())
    }

    async fn receive(&mut self, what: &'static str) -> Result<(u16, Vec<u8>), ClientError> {
        let mut header = [0_u8; 3];
        timeout(FRAME_TIMEOUT, self.stream.read_exact(&mut header))
            .await
            .map_err(|_| ClientError::Timeout(what))?
            .map_err(|_| ClientError::Closed(what))?;
        let total = usize::from(u16::from_le_bytes([header[1], header[2]])).saturating_add(3);
        let mut frame = vec![0_u8; total];
        frame
            .get_mut(..3)
            .ok_or(ClientError::Decrypt)?
            .copy_from_slice(&header);
        timeout(
            FRAME_TIMEOUT,
            self.stream
                .read_exact(frame.get_mut(3..).ok_or(ClientError::Decrypt)?),
        )
        .await
        .map_err(|_| ClientError::Timeout(what))?
        .map_err(|_| ClientError::Closed(what))?;
        let plain = pangya_crypto::server_decrypt(&frame, self.key, MAX_PLAINTEXT, MAX_EXPANSION)
            .map_err(|_| ClientError::Decrypt)?;
        let opcode = u16::from_le_bytes([
            *plain.first().ok_or(ClientError::Decrypt)?,
            *plain.get(1).ok_or(ClientError::Decrypt)?,
        ]);
        let body = plain.get(2..).unwrap_or_default().to_vec();
        tracing::debug!(
            direction = "in",
            opcode = format!("{opcode:#06x}"),
            bytes = body.len(),
            "frame"
        );
        Ok((opcode, body))
    }

    async fn next_opcode(&mut self, what: &'static str) -> Result<u16, ClientError> {
        // A turn can be a long wait — a person is aiming at the other end — so this one step
        // does not use the ordinary frame timeout.
        {
            let mut header = [0_u8; 3];
            self.stream
                .read_exact(&mut header)
                .await
                .map_err(|_| ClientError::Closed(what))?;
            let total = usize::from(u16::from_le_bytes([header[1], header[2]])).saturating_add(3);
            let mut frame = vec![0_u8; total];
            frame
                .get_mut(..3)
                .ok_or(ClientError::Decrypt)?
                .copy_from_slice(&header);
            self.stream
                .read_exact(frame.get_mut(3..).ok_or(ClientError::Decrypt)?)
                .await
                .map_err(|_| ClientError::Closed(what))?;
            let plain =
                pangya_crypto::server_decrypt(&frame, self.key, MAX_PLAINTEXT, MAX_EXPANSION)
                    .map_err(|_| ClientError::Decrypt)?;
            let opcode = u16::from_le_bytes([
                *plain.first().ok_or(ClientError::Decrypt)?,
                *plain.get(1).ok_or(ClientError::Decrypt)?,
            ]);
            tracing::debug!(direction = "in", opcode = format!("{opcode:#06x}"), "frame");
            Ok(opcode)
        }
    }

    async fn wait_for(&mut self, opcode: u16, what: &'static str) -> Result<Vec<u8>, ClientError> {
        loop {
            let (seen, body) = self.receive(what).await?;
            if seen == opcode {
                return Ok(body);
            }
        }
    }

    /// Reads frames until the server goes quiet, returning how many arrived.
    async fn drain(&mut self, quiet: Duration) -> Result<usize, ClientError> {
        let mut seen = 0_usize;
        loop {
            let mut header = [0_u8; 3];
            match timeout(quiet, self.stream.read_exact(&mut header)).await {
                Err(_) => return Ok(seen),
                Ok(Err(_)) => return Err(ClientError::Closed("a drain")),
                Ok(Ok(_)) => {}
            }
            let total = usize::from(u16::from_le_bytes([header[1], header[2]])).saturating_add(3);
            let mut frame = vec![0_u8; total];
            frame
                .get_mut(..3)
                .ok_or(ClientError::Decrypt)?
                .copy_from_slice(&header);
            timeout(
                FRAME_TIMEOUT,
                self.stream
                    .read_exact(frame.get_mut(3..).ok_or(ClientError::Decrypt)?),
            )
            .await
            .map_err(|_| ClientError::Timeout("a drain"))?
            .map_err(|_| ClientError::Closed("a drain"))?;
            let plain =
                pangya_crypto::server_decrypt(&frame, self.key, MAX_PLAINTEXT, MAX_EXPANSION)
                    .map_err(|_| ClientError::Decrypt)?;
            if let (Some(low), Some(high)) = (plain.first(), plain.get(1)) {
                tracing::debug!(
                    direction = "in",
                    opcode = format!("{:#06x}", u16::from_le_bytes([*low, *high])),
                    "frame"
                );
            }
            seen = seen.saturating_add(1);
        }
    }
}
