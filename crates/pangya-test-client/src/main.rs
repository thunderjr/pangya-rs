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
    CompatibilityProfile, EncodePacket, MAX_BOOTSTRAP_STRING_BYTES, PacketWriter,
    ROOM_PLAYER_RECORD_BYTES, RetailGameAuth, RoomPlayerFlags, US852_SERVER_VERSION,
    encode_packet_payload,
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
/// How long this seat will sit in a room waiting for a person to drive the other one.
const ROOM_WAIT: Duration = Duration::from_secs(600);

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
    /// Post-shot ball-state sync.
    pub const SHOT_SYNC: u16 = 0x001b;
    /// Post-shot barrier that ends a turn.
    pub const SHOT_END: u16 = 0x001c;
    /// This player's ball is in the hole.
    pub const HOLE_FINISH: u16 = 0x0031;
}

/// Retail server opcodes this instrument reacts to.
mod server_opcode {
    /// Full bootstrap handover reply; subtype zero carries this connection's identity.
    pub const HANDOVER_REPLY: u16 = 0x0044;
    /// Channel entry accepted.
    pub const CHANNEL_JOINED: u16 = 0x004e;
    /// Room join result; the first two bytes are a status word.
    pub const ROOM_JOIN_RESULT: u16 = 0x0049;
    /// Room roster.
    pub const ROOM_CENSUS: u16 = 0x0048;
    /// Match plan; the hole is loading from here.
    pub const MATCH_INFO: u16 = 0x0052;
    /// A hole introduction, which also announces the opening player.
    pub const PLAYER_START_HOLE: u16 = 0x0053;
    /// Echoed post-shot state.
    pub const SHOT_SYNC: u16 = 0x0064;
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
    #[error("the full bootstrap reply did not contain a complete retail identity")]
    BootstrapIdentity,
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
    #[arg(long, default_value_t = 2, value_parser = parse_strokes)]
    strokes: u8,
    /// Holes for a hosted room (1, 3, 6, 9, or 18).
    #[arg(long, default_value_t = 1, value_parser = parse_holes)]
    holes: u8,
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
        None if args.host => session.host_room(&args.room_name, args.holes).await?,
        None => return Err(ClientError::NoSeat),
    }
    session.play_hole(args.strokes).await?;
    tracing::info!("the hole settled; this seat is done");
    Ok(())
}

#[cfg(test)]
#[test]
fn holes_accept_retail_room_lengths_only() {
    for value in ["1", "3", "6", "9", "18"] {
        assert!(parse_holes(value).is_ok());
    }
    for value in ["0", "2", "19", "abc"] {
        assert!(parse_holes(value).is_err());
    }
}

#[cfg(test)]
#[test]
fn strokes_reject_zero_at_the_cli_boundary() {
    let error = Args::try_parse_from([
        "pangya-test-client",
        "--game",
        "127.0.0.1:10101",
        "--account-id",
        "1",
        "--username",
        "seat",
        "--handover",
        "token",
        "--host",
        "--strokes",
        "0",
    ])
    .expect_err("zero strokes must be rejected before connecting");
    assert!(
        error
            .to_string()
            .contains("strokes must be between 1 and 255")
    );
}

#[cfg(test)]
#[test]
fn bootstrap_identity_parser_walks_variable_pstrings_and_rejects_truncation() {
    let mut body = vec![0];
    for value in [b"852".as_slice(), b"variable server name".as_slice()] {
        body.extend_from_slice(
            &u16::try_from(value.len())
                .expect("test string")
                .to_le_bytes(),
        );
        body.extend_from_slice(value);
    }
    body.extend_from_slice(&0xffff_u16.to_le_bytes());
    let identity = body.len();
    body.extend_from_slice(&[0; 265]); // RetailPlayerIdentity's fixed wire layout
    body[identity + 22 + 22 + 17 + 24..identity + 22 + 22 + 17 + 24 + 4]
        .copy_from_slice(&0x7856_3412_u32.to_le_bytes());
    assert_eq!(bootstrap_connection_id(&body), Ok(Some(0x7856_3412)));
    let mut zero_connection = body.clone();
    zero_connection[identity + 22 + 22 + 17 + 24..identity + 22 + 22 + 17 + 24 + 4]
        .copy_from_slice(&0_u32.to_le_bytes());
    assert_eq!(bootstrap_connection_id(&zero_connection), Err(()));
    for truncated in 1..body.len() {
        assert_eq!(bootstrap_connection_id(&body[..truncated]), Err(()));
    }
    let mut oversized = vec![0];
    oversized.extend_from_slice(&129_u16.to_le_bytes());
    assert_eq!(bootstrap_connection_id(&oversized), Err(()));
    assert_eq!(bootstrap_connection_id(&[1]), Ok(None));
}

#[cfg(test)]
#[test]
fn turn_announcements_are_exact_four_byte_connection_ids() {
    assert_eq!(
        turn_connection_id(&0x7856_3412_u32.to_le_bytes()),
        Some(0x7856_3412)
    );
    for malformed in [&[][..], &[0; 3], &[0; 5]] {
        assert_eq!(turn_connection_id(malformed), None);
    }
}

#[cfg(test)]
#[test]
fn shot_packets_replay_the_checked_us851_normal_swing_shape() {
    // Restricted capture `/private/tmp/pangya-issue45-room-edit-capture-20260811T112509Z/
    // server.jsonl`, 2026-08-11T12:11:41Z: accepted C2S `0x0012` (64 bytes), followed by
    // `0x001b` (54 bytes) and `0x001c` (`01 00`). The server echoed `0x0055`/`0x0064` and
    // continued normal processing. The payload below is deliberately byte-for-byte replayed;
    // PacketDoc `gameservice/client/0012.ksy:21-29` confirms its leading normal-shot subtype.
    assert_eq!(
        normal_shot_commit(),
        [
            0x00, 0x00, 0x00, 0x00, 0xce, 0x43, 0x00, 0x00, 0x49, 0x43, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2a, 0x04, 0x00, 0x00, 0xc0,
            0x5a, 0x25, 0xbe, 0x4c, 0x74, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x43, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x49, 0x48, 0xb9, 0x41,
        ]
    );

    let sync = shot_sync(0x7856_3412);
    assert_eq!(sync.len(), 54);
    // `oid` is the first `u32` of SuperSS-Dev `TYPE/game_type.hpp:222-350`; it names the
    // current second-seat socket, so it is the one required substitution. No checked evidence
    // supports changing the captured position or any other client-owned bytes.
    assert_eq!(
        sync,
        [
            0x12, 0x34, 0x56, 0x78, 0xae, 0x47, 0xc0, 0xc3, 0x8a, 0x98, 0x0c, 0x43, 0xa4, 0xb0,
            0x64, 0xc4, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x08, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]
    );
    assert_eq!(SHOT_END_BODY, [1, 0], "checked 001c barrier has no entries");
}

#[cfg(test)]
#[test]
fn hole_result_has_the_exact_cumulative_239_byte_shape() {
    let result = hole_result(6, 3, 3);
    assert_eq!(result.len(), 239, "user_course_result_data packed width");
    assert_eq!(&result[0..4], &6_u32.to_le_bytes(), "cumulative strokes");
    assert_eq!(&result[4..8], &0_u32.to_le_bytes(), "putts");
    assert_eq!(&result[16..20], &0.0_f32.to_le_bytes(), "longest drive");
    assert_eq!(&result[36..40], &3_u32.to_le_bytes(), "holes played");
    assert_eq!(&result[58..62], &3_u32.to_le_bytes(), "holes completed");
    assert_eq!(&result[66..70], &0.0_f32.to_le_bytes(), "longest putt");
    assert_eq!(&result[70..74], &0.0_f32.to_le_bytes(), "longest chip");
    assert_eq!(&result[74..78], &(-1_i32).to_le_bytes(), "VS unknown_q");
    assert!(result[78..].iter().all(|byte| *byte == 0));
}

#[cfg(test)]
#[test]
fn one_hole_trace_ignores_foreign_turns_and_holes_out_once() {
    let mut script = HoleScript::new(1);
    assert!(
        script
            .on_turn(server_opcode::PLAYER_START_HOLE, false)
            .is_empty()
    );
    assert!(script.on_turn(server_opcode::TURN_START, false).is_empty());
    assert_eq!(
        script.on_turn(server_opcode::TURN_START, true),
        vec![ScriptWrite::ShotCommit, ScriptWrite::ShotSync]
    );
    assert!(script.on_turn(server_opcode::TURN_START, true).is_empty());
    assert!(script.on_shot_sync(false).is_empty());
    assert_eq!(
        script.on_shot_sync(true),
        vec![ScriptWrite::ShotEnd, ScriptWrite::HoleFinish]
    );
    assert!(script.on_turn(server_opcode::TURN_START, true).is_empty());
}

#[cfg(test)]
#[test]
fn opening_and_handoff_turns_only_write_for_the_own_connection() {
    let writes = vec![ScriptWrite::ShotCommit, ScriptWrite::ShotSync];
    let mut opening_foreign = HoleScript::new(1);
    assert!(
        opening_foreign
            .on_turn(server_opcode::PLAYER_START_HOLE, false)
            .is_empty()
    );
    let mut opening_own = HoleScript::new(1);
    assert_eq!(
        opening_own.on_turn(server_opcode::PLAYER_START_HOLE, true),
        writes
    );

    let mut handoff = HoleScript::new(1);
    assert!(handoff.on_turn(server_opcode::TURN_START, false).is_empty());
    assert_eq!(handoff.on_turn(server_opcode::TURN_START, true), writes);
}

#[cfg(test)]
#[test]
fn pending_shot_requires_exact_own_sync_before_resync_and_hole_finish() {
    let mut script = HoleScript::new(1);
    assert_eq!(
        script.on_turn(server_opcode::PLAYER_START_HOLE, true),
        vec![ScriptWrite::ShotCommit, ScriptWrite::ShotSync]
    );
    assert!(script.pending_shot);
    let own_sync = shot_sync(7);
    assert!(is_own_shot_sync(&own_sync, 7));
    let mut changed_tail = own_sync;
    changed_tail[53] ^= 1;
    assert!(
        !is_own_shot_sync(&changed_tail, 7),
        "a local OID alone must not advance the post-shot barrier"
    );
    assert!(!is_own_shot_sync(&[0; 37], 7));
    assert!(!is_own_shot_sync(&[8; 38], 7));
    assert!(
        script.on_shot_sync(false).is_empty(),
        "foreign/malformed sync"
    );
    assert!(script.pending_shot);
    assert!(script.on_turn(server_opcode::TURN_START, true).is_empty());
    assert_eq!(
        script.on_shot_sync(true),
        vec![ScriptWrite::ShotEnd, ScriptWrite::HoleFinish]
    );
    assert!(!script.pending_shot);
    assert!(script.hole_out_sent);
}

#[cfg(test)]
#[test]
fn three_hole_trace_resets_only_on_next_hole_introduction() {
    let mut script = HoleScript::new(2);
    let shot = vec![ScriptWrite::ShotCommit, ScriptWrite::ShotSync];
    let end = vec![ScriptWrite::ShotEnd];
    let finish = vec![ScriptWrite::ShotEnd, ScriptWrite::HoleFinish];
    for _ in 0..3 {
        assert!(
            script
                .on_turn(server_opcode::PLAYER_START_HOLE, false)
                .is_empty()
        );
        assert!(script.on_turn(server_opcode::TURN_START, false).is_empty());
        assert_eq!(script.on_turn(server_opcode::TURN_START, true), shot);
        assert!(script.on_shot_sync(false).is_empty());
        assert_eq!(script.on_shot_sync(true), end);
        assert!(script.on_turn(server_opcode::TURN_START, false).is_empty());
        assert_eq!(script.on_turn(server_opcode::TURN_START, true), shot);
        assert_eq!(script.on_shot_sync(true), finish);
        assert!(script.on_turn(server_opcode::TURN_START, true).is_empty());
    }
    assert_eq!(script.match_strokes, 6);
    assert_eq!(script.holes_played, 3);
    assert_eq!(script.holes_completed, 3);
    assert_eq!(&script.hole_result()[0..4], &6_u32.to_le_bytes());
    assert_eq!(&script.hole_result()[36..40], &3_u32.to_le_bytes());
    assert_eq!(&script.hole_result()[58..62], &3_u32.to_le_bytes());
}

fn parse_holes(value: &str) -> Result<u8, String> {
    match value.parse() {
        Ok(holes @ (1 | 3 | 6 | 9 | 18)) => Ok(holes),
        _ => Err("holes must be one of: 1, 3, 6, 9, 18".to_owned()),
    }
}

/// A zero-stroke script would send a rejected hole-out before any accepted shot.
fn parse_strokes(value: &str) -> Result<u8, String> {
    match value.parse() {
        Ok(strokes @ 1..) => Ok(strokes),
        _ => Err("strokes must be between 1 and 255".to_owned()),
    }
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

/// Extracts this socket's connection id from a complete `0x0044` subtype-zero body.
///
/// The two preceding PStrings are bounded as `HandoverReply` bounds them, then the fixed
/// `RetailPlayerIdentity` layout is walked through its final account id
/// (`pangya-protocol/src/us852_bootstrap.rs:1095-1172,1201-1247`). This deliberately does not
/// rely on bootstrap frame order or a packet-wide fixed offset.
fn bootstrap_connection_id(body: &[u8]) -> Result<Option<u32>, ()> {
    if body.first().copied() != Some(0) {
        return Ok(None);
    }
    let mut at = 1_usize;
    for _ in 0..2 {
        let length = usize::from(read_u16(body, &mut at)?);
        if length > MAX_BOOTSTRAP_STRING_BYTES {
            return Err(());
        }
        skip(body, &mut at, length)?;
    }
    skip(body, &mut at, 2)?; // room number before RetailPlayerIdentity
    skip(body, &mut at, 22 + 22 + 17 + 24)?;
    let connection_id = read_u32(body, &mut at)?;
    if connection_id == 0 {
        return Err(());
    }
    // Complete the fixed RetailPlayerIdentity layout; accepting a prefix here would make a
    // truncated handover look authoritative.
    skip(body, &mut at, 12 + 4 + 4 + 2 + 6 + 16 + 128 + 4)?;
    Ok(Some(connection_id))
}

fn skip(bytes: &[u8], at: &mut usize, amount: usize) -> Result<(), ()> {
    let end = at.checked_add(amount).ok_or(())?;
    if bytes.get(*at..end).is_none() {
        return Err(());
    }
    *at = end;
    Ok(())
}

fn read_u16(bytes: &[u8], at: &mut usize) -> Result<u16, ()> {
    let value = bytes.get(*at..at.checked_add(2).ok_or(())?).ok_or(())?;
    *at += 2;
    Ok(u16::from_le_bytes(value.try_into().map_err(|_| ())?))
}

fn read_u32(bytes: &[u8], at: &mut usize) -> Result<u32, ()> {
    let value = bytes.get(*at..at.checked_add(4).ok_or(())?).ok_or(())?;
    *at += 4;
    Ok(u32::from_le_bytes(value.try_into().map_err(|_| ())?))
}

/// Both opening `0x0053` and handoff `0x0063` are exactly a little-endian connection id
/// (`pangya-protocol/src/us852_match.rs:510-547`).
fn turn_connection_id(body: &[u8]) -> Option<u32> {
    let bytes: [u8; 4] = body.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

/// Script writes emitted in TCP order for one accepted turn or sync acknowledgement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScriptWrite {
    ShotCommit,
    ShotSync,
    ShotEnd,
    HoleFinish,
}

/// Per-hole and cumulative match state. Only a new `0x0053` resets the per-hole fields.
#[derive(Debug)]
struct HoleScript {
    strokes: u8,
    played: u8,
    pending_shot: bool,
    hole_out_sent: bool,
    match_strokes: u32,
    holes_played: u32,
    holes_completed: u32,
}

impl HoleScript {
    const fn new(strokes: u8) -> Self {
        Self {
            strokes,
            played: 0,
            pending_shot: false,
            hole_out_sent: false,
            match_strokes: 0,
            holes_played: 0,
            holes_completed: 0,
        }
    }

    fn on_turn(&mut self, opcode: u16, is_own_turn: bool) -> Vec<ScriptWrite> {
        if opcode == server_opcode::PLAYER_START_HOLE {
            self.played = 0;
            self.pending_shot = false;
            self.hole_out_sent = false;
            self.holes_played = self.holes_played.saturating_add(1);
        }
        if !is_own_turn || self.pending_shot || self.hole_out_sent {
            return Vec::new();
        }
        self.played = self.played.saturating_add(1);
        self.match_strokes = self.match_strokes.saturating_add(1);
        self.pending_shot = true;
        vec![ScriptWrite::ShotCommit, ScriptWrite::ShotSync]
    }

    /// Advances only after the exact echoed `0x0064` for this socket's pending shot.
    fn on_shot_sync(&mut self, is_own_sync: bool) -> Vec<ScriptWrite> {
        if !is_own_sync || !self.pending_shot {
            return Vec::new();
        }
        self.pending_shot = false;
        let mut writes = vec![ScriptWrite::ShotEnd];
        if self.played == self.strokes {
            self.hole_out_sent = true;
            self.holes_completed = self.holes_completed.saturating_add(1);
            writes.push(ScriptWrite::HoleFinish);
        }
        writes
    }

    fn hole_result(&self) -> [u8; 239] {
        hole_result(self.match_strokes, self.holes_played, self.holes_completed)
    }
}

/// Builds the accepted 64-byte normal `0x0012` capture from the real U.S. 851 client.
///
/// Restricted capture `/private/tmp/pangya-issue45-room-edit-capture-20260811T112509Z/server.jsonl`
/// at `2026-08-11T12:11:41Z` is the only checked source for this revision-specific body.
/// PacketDoc `gameservice/client/0012.ksy:21-29` independently identifies the leading `u16` as
/// the normal-shot subtype. Its fields do not contain a connection identity, so every byte is
/// retained rather than inventing a second-seat substitution.
const fn normal_shot_commit() -> [u8; 64] {
    [
        0x00, 0x00, 0x00, 0x00, 0xce, 0x43, 0x00, 0x00, 0x49, 0x43, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2a, 0x04, 0x00, 0x00, 0xc0, 0x5a, 0x25,
        0xbe, 0x4c, 0x74, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x43, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x80, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x48, 0xb9, 0x41,
    ]
}

/// Builds the 54-byte C2S `0x001b` body accepted in that same real U.S. 851 swing.
///
/// SuperSS-Dev `TYPE/game_type.hpp:222-350` identifies the first four bytes as the player's
/// `oid`; substitute this socket's connection ID so the echoed `0x0064` is attributable to the
/// second seat. The checked capture supplies no evidence that its position or other opaque,
/// client-owned fields must vary, so they remain an exact deterministic replay.
fn shot_sync(connection_id: u32) -> [u8; 54] {
    let mut body = [
        0x0f, 0x00, 0x00, 0x00, 0xae, 0x47, 0xc0, 0xc3, 0x8a, 0x98, 0x0c, 0x43, 0xa4, 0xb0, 0x64,
        0xc4, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x08, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    body[..4].copy_from_slice(&connection_id.to_le_bytes());
    body
}

/// The checked `0x001c` barrier has `unknown_a = 1` and `entry_count = 0`.
/// PacketDoc `gameservice/client/001c.ksy:35-45` confirms the latter means no collectable entries.
const SHOT_END_BODY: [u8; 2] = [1, 0];

/// Tests whether an echoed S2C `0x0064` has the exact checked sync shape for this connection.
fn is_own_shot_sync(body: &[u8], connection_id: u32) -> bool {
    body == shot_sync(connection_id)
}

/// Builds PacketDoc's packed, cumulative 239-byte `user_course_result_data` for C2S `0x0031`
/// (`gameservice/client/0031.ksy`; `common/user_course_result_data.ksy`). `unknown_q` is the
/// documented `-1` value for versus matches; unclassified fields are zero.
fn hole_result(strokes: u32, holes_played: u32, holes_completed: u32) -> [u8; 239] {
    let mut body = [0_u8; 239];
    body[0..4].copy_from_slice(&strokes.to_le_bytes());
    // [4..8] putts, [8..16] unknown counters, [16..20] longest drive, and [20..36] opaque
    // distance fields remain documented zero defaults.
    body[36..40].copy_from_slice(&holes_played.to_le_bytes());
    body[58..62].copy_from_slice(&holes_completed.to_le_bytes());
    // [62..66] completed-by-putting and [66..74] finite longest putt/chip distances are zero.
    body[74..78].copy_from_slice(&(-1_i32).to_le_bytes());
    body
}

/// One encrypted GameService connection and the salt counter its frames carry.
struct Session {
    stream: TcpStream,
    key: u8,
    salt: u8,
    /// The GameService connection/object id from this socket's full `0x0044` reply.
    connection_id: Option<u32>,
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
            connection_id: None,
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
        // Bootstrap frames are asynchronous, so locate this socket's full handover structurally
        // rather than by frame number. Leaving subsequent frames queued is important: a person
        // can advance the room while this instrument is between phases.
        loop {
            let (opcode, body) = self.receive("bootstrap handover").await?;
            if opcode != server_opcode::HANDOVER_REPLY {
                continue;
            }
            if let Some(connection_id) =
                bootstrap_connection_id(&body).map_err(|_| ClientError::BootstrapIdentity)?
            {
                self.connection_id = Some(connection_id);
                tracing::info!(connection_id, "bootstrap identity received");
                return Ok(());
            }
        }
    }

    async fn enter_channel(&mut self, channel: u8) -> Result<(), ClientError> {
        self.send(client_opcode::SELECT_CHANNEL, &[channel]).await?;
        self.wait_for(server_opcode::CHANNEL_JOINED, "channel entry")
            .await?;
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
    /// Requested holes and a capacity of two. Pressing Start is also what says the master is
    /// ready — a real client's button reads Start for the master and Ready for everyone else.
    async fn host_room(&mut self, name: &str, holes: u8) -> Result<(), ClientError> {
        let mut writer = PacketWriter::default();
        writer.u8(0); // room kind: versus
        writer.u32_le(30_000); // shot timer
        writer.u32_le(600_000); // game timer
        writer.u8(2); // capacity
        writer.u8(0); // hole progression
        writer.u8(holes); // holes
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
        // Paced by a person driving the other seat, so the whole wait is bounded rather than
        // the gap between frames: an empty room is silent for as long as it takes.
        let deadline = tokio::time::Instant::now() + ROOM_WAIT;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(ClientError::NobodyJoined);
            }
            let (opcode, census) = self.receive_within("the room to fill", remaining).await?;
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
        // The only wait here that is paced by a person: a real client in the other seat has to
        // be driven to the room and its host has to press Ready and then Start. Everything
        // after this is server-paced and keeps the ordinary frame timeout.
        self.idle_until(server_opcode::MATCH_INFO, "the match plan", ROOM_WAIT)
            .await?;
        // Do not drain here: the server can already have sent the actionable `0x0053` opening
        // turn alongside match conditions. `idle_until` consumes only the match plan, and the
        // following reader preserves every body it sees.
        self.send(client_opcode::HOLE_LOAD_FINISHED, &[]).await?;
        tracing::info!("loaded; waiting for a turn");

        let connection_id = self.connection_id.ok_or(ClientError::BootstrapIdentity)?;
        let mut script = HoleScript::new(strokes);
        loop {
            // A person may take minutes on the other seat, so this intentionally has no timeout
            // on its first byte. Unlike the old opcode-only reader, it retains the turn body.
            let (opcode, body) = self.next_frame("a turn").await?;
            match opcode {
                server_opcode::TURN_START | server_opcode::PLAYER_START_HOLE => {
                    let Some(announced) = turn_connection_id(&body) else {
                        tracing::warn!(opcode = format!("{opcode:#06x}"), "malformed turn body");
                        continue;
                    };
                    let writes = script.on_turn(opcode, announced == connection_id);
                    self.send_script_writes(&writes, connection_id, &script)
                        .await?;
                }
                server_opcode::SHOT_SYNC => {
                    let writes = script.on_shot_sync(is_own_shot_sync(&body, connection_id));
                    self.send_script_writes(&writes, connection_id, &script)
                        .await?;
                }
                server_opcode::FINISH_HOLE => tracing::info!("the hole finished"),
                server_opcode::MATCH_FINISH => return Ok(()),
                _ => {}
            }
        }
    }

    /// Emits a phase transition in wire order. `0x001c` follows only the matching `0x0064`.
    async fn send_script_writes(
        &mut self,
        writes: &[ScriptWrite],
        connection_id: u32,
        script: &HoleScript,
    ) -> Result<(), ClientError> {
        for write in writes {
            match write {
                ScriptWrite::ShotCommit => {
                    self.send(client_opcode::SHOT_COMMIT, &normal_shot_commit())
                        .await?;
                }
                ScriptWrite::ShotSync => {
                    self.send(client_opcode::SHOT_SYNC, &shot_sync(connection_id))
                        .await?;
                }
                ScriptWrite::ShotEnd => self.send(client_opcode::SHOT_END, &SHOT_END_BODY).await?,
                ScriptWrite::HoleFinish => {
                    self.send(client_opcode::HOLE_FINISH, &script.hole_result())
                        .await?;
                    tracing::info!(played = script.played, "holed out");
                }
            }
        }
        Ok(())
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
        self.receive_within(what, FRAME_TIMEOUT).await
    }

    async fn receive_within(
        &mut self,
        what: &'static str,
        first_byte: Duration,
    ) -> Result<(u16, Vec<u8>), ClientError> {
        let mut header = [0_u8; 3];
        timeout(first_byte, self.stream.read_exact(&mut header))
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

    async fn next_frame(&mut self, what: &'static str) -> Result<(u16, Vec<u8>), ClientError> {
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

    async fn wait_for(&mut self, opcode: u16, what: &'static str) -> Result<Vec<u8>, ClientError> {
        loop {
            let (seen, body) = self.receive(what).await?;
            if seen == opcode {
                return Ok(body);
            }
        }
    }

    /// Waits for one opcode across a whole budget rather than per frame.
    ///
    /// A room this seat is waiting in is silent between roster changes, so the ordinary
    /// per-frame timeout ends the wait long before the budget does — which is the wrong bound
    /// when what is being waited on is a person driving the other client. The budget covers the
    /// silence; once a frame starts arriving the ordinary timeout governs the rest of it.
    async fn idle_until(
        &mut self,
        opcode: u16,
        what: &'static str,
        budget: Duration,
    ) -> Result<Vec<u8>, ClientError> {
        let deadline = tokio::time::Instant::now() + budget;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(ClientError::Timeout(what));
            }
            let (seen, body) = self.receive_within(what, remaining).await?;
            if seen == opcode {
                return Ok(body);
            }
        }
    }
}
