use crate::{
    CompatibilityProfile, DecodePacket, EncodePacket, PacketDecodeError, PacketEncodeError,
    PacketReader, PacketWriter,
};
use zeroize::Zeroizing;

/// Maximum handover bearer bytes accepted by the synthetic GameService auth packet.
pub const MAX_GAME_HANDOVER_BYTES: usize = 128;
/// Maximum inventory records in one synthetic bootstrap segment.
pub const GAME_INVENTORY_SEGMENT_ITEMS: usize = 50;

fn check_decode_profile(
    profile: &CompatibilityProfile,
    reader: &PacketReader<'_>,
) -> Result<(), PacketDecodeError> {
    profile
        .require_us852()
        .map_err(|error| reader.invalid(error.to_string()))
}

fn check_encode_profile(profile: &CompatibilityProfile) -> Result<(), PacketEncodeError> {
    profile.require_us852().map_err(Into::into)
}

fn require_end(reader: &mut PacketReader<'_>) -> Result<(), PacketDecodeError> {
    if reader.remaining() == 0 {
        Ok(())
    } else {
        Err(reader.invalid("synthetic packet has trailing bytes"))
    }
}

/// Builds the four-byte synthetic M3 GameService hello.
///
/// Only the behavioral property that the negotiated key is the final byte is treated as
/// locally attested. The preceding bytes and complete layout remain synthetic.
///
/// # Errors
/// Rejects transport keys outside `0x00..=0x0f`.
pub fn synthetic_game_hello(key: u8) -> Result<[u8; 4], PacketEncodeError> {
    if key > 0x0f {
        return Err(PacketEncodeError::Limit {
            field: "transport key",
            actual: usize::from(key),
            maximum: 15,
        });
    }
    Ok([0, 0, 0, key])
}

/// Builds the nine-byte U.S. 852 retail GameService hello.
///
/// Unlike the synthetic four-byte hello this is the layout a real client parses: eight
/// source-observed constant bytes followed by the negotiated transport key. A real U.S. 852
/// client that receives the shorter synthetic hello instead reads the following frame at the
/// wrong offset and drops the connection, which it reports on its server list as
/// "Server is full".
///
/// # Provenance
///
/// Constants adapted from `pangbox/server` (`game/server/auth.go`, `game/packet/server.go`
/// `ConnectMessage`), ISC licensed. The hello is written unframed, exactly these bytes.
///
/// # Errors
/// Rejects transport keys outside `0x00..=0x0f`.
pub fn us852_game_hello(key: u8) -> Result<[u8; 9], PacketEncodeError> {
    if key > 0x0f {
        return Err(PacketEncodeError::Limit {
            field: "transport key",
            actual: usize::from(key),
            maximum: 15,
        });
    }
    Ok([0x00, 0x06, 0x00, 0x00, 0x3f, 0x00, 0x01, 0x01, key])
}

/// Synthetic minimal GameService auth packet, client opcode `0x0002`.
#[derive(Clone, Eq, PartialEq)]
pub struct GameAuth {
    /// Untrusted claimed account ID; authoritative identity comes only from handover consume.
    pub claimed_account_id: u64,
    /// Secret login-to-game bearer.
    pub handover: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for GameAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GameAuth")
            .field("claimed_account_id", &self.claimed_account_id)
            .field("handover", &"<redacted>")
            .finish()
    }
}

impl DecodePacket for GameAuth {
    const OPCODE: u16 = 0x0002;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let claimed_account_id = reader.u64_le()?;
        let handover = Zeroizing::new(reader.pstring(MAX_GAME_HANDOVER_BYTES)?.to_vec());
        require_end(reader)?;
        Ok(Self {
            claimed_account_id,
            handover,
        })
    }
}

/// Synthetic minimal channel selection packet, client opcode `0x0004`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectChannel {
    /// Locally configured synthetic channel ID.
    pub channel_id: u32,
}

impl DecodePacket for SelectChannel {
    const OPCODE: u16 = 0x0004;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let channel_id = reader.u32_le()?;
        require_end(reader)?;
        Ok(Self { channel_id })
    }
}

/// U.S. 852 retail sub-server (channel) selection, client opcode `0x0004`.
///
/// # Provenance
///
/// Layout from the vendored PacketDoc `gameservice/client/0004.ksy`: a single `u1` sub-server ID,
/// where the synthetic [`SelectChannel`] carries a `u32`. A real client sends the one-byte form.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailSelectChannel {
    /// Sub-server identifier from the advertised channel list.
    pub sub_server_id: u8,
}

impl DecodePacket for RetailSelectChannel {
    const OPCODE: u16 = 0x0004;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let sub_server_id = reader.u8()?;
        require_end(reader)?;
        Ok(Self { sub_server_id })
    }
}

/// U.S. 852 retail sub-server connect response, server opcode `0x004e`.
///
/// # Provenance
///
/// Layout from the vendored PacketDoc `gameservice/server/004e.ksy`: a single `u1`, documented as
/// only ever witnessed as `0x01`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailChannelJoined;

impl EncodePacket for RetailChannelJoined {
    const OPCODE: u16 = 0x004e;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u8(0x01);
        Ok(())
    }
}

/// U.S. 852 retail login-bonus status request, client opcode `0x016e`.
///
/// # Provenance
///
/// Layout from the vendored PacketDoc `gameservice/client/016e.ksy`: no payload. Documented as
/// always following the sub-server connect, which is where a real client sends it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailLoginBonusRequest;

impl DecodePacket for RetailLoginBonusRequest {
    const OPCODE: u16 = 0x016e;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        require_end(reader)?;
        Ok(Self)
    }
}

/// U.S. 852 retail login-bonus status response, server opcode `0x0248`.
///
/// This server has no login-bonus schedule, so it answers with the "already collected" form and a
/// zeroed preview: nothing is offered and nothing is claimable. Reporting the uncollected form
/// would advertise a reward the client could then try to claim.
///
/// # Provenance
///
/// Layout from the vendored PacketDoc `gameservice/server/0248.ksy`. The trailing block is a union
/// selected by `bonus_collected`; the collected branch is three `u4` preview fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailLoginBonusStatus;

impl EncodePacket for RetailLoginBonusStatus {
    const OPCODE: u16 = 0x0248;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.bytes(&[0; 4]);
        writer.u8(0x01);
        writer.u32_le(0);
        writer.u32_le(0);
        writer.u32_le(0);
        writer.u32_le(0);
        writer.u32_le(0);
        Ok(())
    }
}

/// U.S. 852 retail post-channel-join notice, server opcode `0x01f6`.
///
/// # Provenance
///
/// Four zero bytes, from `pangbox/server` (`game/server/conn.go`, the `ClientJoinChannel` case),
/// ISC licensed, which sends it immediately after the sub-server connect response. Its meaning is
/// not established, so the bytes are carried verbatim rather than modelled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailChannelJoinNotice;

impl EncodePacket for RetailChannelJoinNotice {
    const OPCODE: u16 = 0x01f6;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.bytes(&[0; 4]);
        Ok(())
    }
}

/// Session-level client opcodes a real U.S. 852 client sends that this server accepts and answers
/// with nothing.
///
/// These are documented in `pangbox/server` (`game/packet/client.go`) and handled there with an
/// empty case — the client neither expects nor waits for a reply. Accepting them explicitly keeps
/// the shipped `unknown_opcode_policy` meaningful: genuinely unrecognized opcodes still hit it,
/// rather than the whole lobby depending on a permissive policy.
///
/// Room and match opcodes are deliberately absent. Those have real state handlers, and silently
/// accepting one would hide a gap in them instead of surfacing it.
pub const RETAIL_ACCEPTED_SESSION_OPCODES: &[u16] = &[
    0x0007, // online status of another user
    0x0018, // typing indicator
    0x0032, // idle status
    0x004f, // unclassified
    0x0069, // chat macro set
    0x0088, // unclassified
    0x008b, // messenger list request
    0x00c1, // unclassified
    0x00fe, // unclassified
];

/// Returns whether `opcode` is a session-level opcode this server accepts without replying.
#[must_use]
pub fn is_retail_accepted_session_opcode(opcode: u16) -> bool {
    RETAIL_ACCEPTED_SESSION_OPCODES.contains(&opcode)
}

/// In-match client opcodes a real U.S. 852 client sends that this server accepts and answers
/// with nothing.
///
/// These are the cosmetic half of a hole: where a player is aiming, how far the power meter has
/// travelled, which club is in hand, where a relief drop went, and what the client believes the
/// hole's par and pin are. Upstream relays each to the other participants
/// (`pangbox/server`, `game/room/room.go` `handleRoomGameShotRotate` and its neighbours, ISC),
/// and this server does not yet, so an opponent's aim does not animate. None of them changes a
/// stroke, a turn, or a score, which is why not answering is safe — and why they are listed
/// here rather than left to the unknown-opcode policy, which would drop the connection
/// mid-hole.
///
/// The authoritative half — start, load, shot commit, shot sync, turn end, hole finish — is
/// deliberately absent: those have real handlers.
pub const RETAIL_ACCEPTED_MATCH_OPCODES: &[u16] = &[
    0x0006, // game end
    0x000b, // channel equipment sync sent immediately after a Practice start
    0x000c, // the equipment this player is taking into the hole
    0x0013, // aim rotation
    0x0014, // power meter input
    0x0015, // power level
    0x0016, // club change
    0x0017, // item use
    0x0019, // comet relief drop
    0x001a, // the client's own hole info: par, tee and pin
    0x0022, // acknowledgement of the active-player announcement
    0x0030, // pause
    0x0037, // last player leaving the room
    0x0042, // aiming arrow
];

/// Returns whether `opcode` is an in-match opcode this server accepts without replying.
#[must_use]
pub fn is_retail_accepted_match_opcode(opcode: u16) -> bool {
    RETAIL_ACCEPTED_MATCH_OPCODES.contains(&opcode)
}

/// U.S. 852 retail first-shot acknowledgement, server opcode `0x0090`.
///
/// The client announces it is ready for the first shot of a hole and waits to be told to go.
/// The reply carries nothing; it is the fact of it that the client reads.
///
/// # Provenance
///
/// Opcode, empty body, and its use as the direct answer to client `0x0034` from
/// `pangbox/server` (`game/packet/server.go` `ServerPlayerFirstShotReady`,
/// `game/server/conn.go` `ClientFirstShotReady`), ISC licensed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetailFirstShotReady;

impl EncodePacket for RetailFirstShotReady {
    const OPCODE: u16 = 0x0090;

    fn encode(
        &self,
        _writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        profile.require_us852()?;
        Ok(())
    }
}

/// The client opcode [`RetailFirstShotReady`] answers.
pub const RETAIL_C2S_FIRST_SHOT_READY: u16 = 0x0034;

/// Recent-player slots in a [`RetailPlayerHistory`].
pub const RETAIL_RECENT_PLAYERS: usize = 5;
/// Bytes in one recent-player record: `u32`, two 22-byte names, `u32`.
pub const RETAIL_RECENT_PLAYER_BYTES: usize = 52;

/// Client-reported exception, client opcode `0x0033`.
///
/// The client sends this when its own error handler fires, carrying the message it would
/// otherwise only write to its crash log. It is the one channel through which a closed-source
/// client explains itself, so the server logs it rather than discarding it.
///
/// # Provenance
///
/// `pangbox/server` (`game/packet/client.go`, `0x0033: &ClientException{}`, whose body is a
/// single filler byte followed by a `PString` message) and `Acrisio-Filho/SuperSS-Dev`
/// (`Server Lib/Game Server/PACKET/packet_func_sv.cpp` `packet_func::packet033`, which routes
/// it to `requestExceptionClientMessage`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailClientException {
    /// The message, as the client wrote it.
    pub message: Vec<u8>,
}

/// Longest client exception message accepted.
pub const MAX_CLIENT_EXCEPTION_BYTES: usize = 512;

impl DecodePacket for RetailClientException {
    const OPCODE: u16 = 0x0033;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let _empty = reader.u8()?;
        Ok(Self {
            message: reader.pstring(MAX_CLIENT_EXCEPTION_BYTES)?.to_vec(),
        })
    }
}

impl RetailClientException {
    /// Renders the message for a log line: printable ASCII only, and bounded.
    ///
    /// The client controls every byte here, so it is sanitised rather than trusted. Control
    /// characters would let it forge log structure, and an unbounded string would let it
    /// flood the log.
    #[must_use]
    pub fn sanitized(&self) -> String {
        self.message
            .iter()
            .take(256)
            .map(|&byte| {
                if (0x20..0x7f).contains(&byte) {
                    char::from(byte)
                } else {
                    '.'
                }
            })
            .collect()
    }
}

/// U.S. 852 retail recent-player history request, client opcode `0x009c`.
///
/// # Provenance
///
/// No payload. Opcode and empty body from `pangbox/server` (`game/packet/client.go`
/// `ClientRequestPlayerHistory`), ISC licensed; the vendored PacketDoc `gameservice/client/009c.ksy`
/// documents the same empty request. A real client sends it right after entering a channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailPlayerHistoryRequest;

impl DecodePacket for RetailPlayerHistoryRequest {
    const OPCODE: u16 = 0x009c;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        require_end(reader)?;
        Ok(Self)
    }
}

/// U.S. 852 retail recent-player history, server opcode `0x010e`.
///
/// Five fixed slots, all zeroed: this server keeps no recent-opponent history, and an empty list is
/// the honest answer rather than inventing players.
///
/// # Provenance
///
/// Record shape from `pangbox/server` (`game/packet/server.go` `ServerPlayerHistory`, `RecentPlayer`),
/// ISC licensed, which sends the same zero-valued response. The vendored PacketDoc
/// `gameservice/server/010e.ksy` describes the identical 260 bytes as one record plus padding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailPlayerHistory;

impl EncodePacket for RetailPlayerHistory {
    const OPCODE: u16 = 0x010e;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.bytes(&[0; RETAIL_RECENT_PLAYERS * RETAIL_RECENT_PLAYER_BYTES]);
        Ok(())
    }
}

/// Synthetic minimal player/profile bootstrap, server opcode `0x0070`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerInfo {
    /// Authoritative account ID.
    pub account_id: u64,
    /// Persisted nickname bytes.
    pub nickname: Vec<u8>,
    /// Pang balance.
    pub pang: u64,
    /// Point balance.
    pub points: u64,
    /// Experience balance.
    pub experience: u64,
}

impl EncodePacket for PlayerInfo {
    const OPCODE: u16 = 0x0070;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u64_le(self.account_id);
        writer.pstring(&self.nickname, 64)?;
        writer.u64_le(self.pang);
        writer.u64_le(self.points);
        writer.u64_le(self.experience);
        Ok(())
    }
}

/// One synthetic character bootstrap record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterBootstrap {
    /// Durable character ID.
    pub id: u64,
    /// Catalog type ID.
    pub type_id: u32,
}

/// Synthetic character list, server opcode `0x0072`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterInfo {
    /// Bounded owned characters.
    pub characters: Vec<CharacterBootstrap>,
}

impl EncodePacket for CharacterInfo {
    const OPCODE: u16 = 0x0072;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.count_u16(self.characters.len(), 64)?;
        for character in &self.characters {
            writer.u64_le(character.id);
            writer.u32_le(character.type_id);
        }
        Ok(())
    }
}

/// One synthetic inventory bootstrap record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InventoryBootstrap {
    /// Durable inventory ID.
    pub id: u64,
    /// Catalog type ID.
    pub type_id: u32,
    /// Positive quantity.
    pub quantity: u32,
}

/// One segmented synthetic inventory packet, server opcode `0x0073`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventorySegment {
    /// Zero-based segment index.
    pub segment_index: u16,
    /// Total segment count.
    pub segment_count: u16,
    /// At most 50 inventory records.
    pub items: Vec<InventoryBootstrap>,
}

impl EncodePacket for InventorySegment {
    const OPCODE: u16 = 0x0073;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        if self.segment_count == 0 || self.segment_index >= self.segment_count {
            return Err(PacketEncodeError::Limit {
                field: "inventory segment",
                actual: usize::from(self.segment_index),
                maximum: usize::from(self.segment_count.saturating_sub(1)),
            });
        }
        writer.u16_le(self.segment_index);
        writer.u16_le(self.segment_count);
        writer.count_u16(self.items.len(), GAME_INVENTORY_SEGMENT_ITEMS)?;
        for item in &self.items {
            writer.u64_le(item.id);
            writer.u32_le(item.type_id);
            writer.u32_le(item.quantity);
        }
        Ok(())
    }
}

/// Synthetic equipment selection, server opcode `0x004d`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EquipmentInfo {
    /// Selected character ID.
    pub character_id: u64,
    /// Equipped club inventory ID, or zero.
    pub club_item_id: u64,
    /// Equipped ball inventory ID, or zero.
    pub ball_item_id: u64,
    /// Optimistic equipment version.
    pub version: u32,
}

impl EncodePacket for EquipmentInfo {
    const OPCODE: u16 = 0x004d;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u64_le(self.character_id);
        writer.u64_le(self.club_item_id);
        writer.u64_le(self.ball_item_id);
        writer.u32_le(self.version);
        Ok(())
    }
}

/// Synthetic channel entry acknowledgement, server opcode `0x004e`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelJoined {
    /// Entered synthetic channel ID.
    pub channel_id: u32,
}

impl EncodePacket for ChannelJoined {
    const OPCODE: u16 = 0x004e;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u32_le(self.channel_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_hello_key_is_final_byte_and_range_checked() {
        let hello = synthetic_game_hello(9).expect("hello");
        assert_eq!(hello.last(), Some(&9));
        assert!(synthetic_game_hello(16).is_err());
    }

    /// The retail hello's exact bytes and length are a compatibility surface: a real client that
    /// gets the shorter synthetic one reads the next frame at the wrong offset and disconnects.
    #[test]
    fn retail_game_hello_is_nine_source_observed_bytes_ending_in_the_key() {
        for key in 0..=0x0f_u8 {
            let hello = us852_game_hello(key).expect("hello");
            assert_eq!(
                hello,
                [0x00, 0x06, 0x00, 0x00, 0x3f, 0x00, 0x01, 0x01, key],
                "key {key}"
            );
        }
        assert_eq!(us852_game_hello(0).expect("hello").len(), 9);
        assert_ne!(
            us852_game_hello(0).expect("retail").len(),
            synthetic_game_hello(0).expect("synthetic").len(),
            "the two hellos must stay distinguishable by length"
        );
        assert!(us852_game_hello(0x10).is_err());
    }

    /// The client sends the request the moment it enters a channel, so the answer is on the
    /// critical path to the lobby. `bonus_collected = 1` selects the preview branch, which is
    /// three `u4`s rather than the uncollected branch's eight padding bytes plus one `u4`.
    #[test]
    fn retail_login_bonus_status_reports_nothing_claimable() {
        let mut writer = PacketWriter::new();
        RetailLoginBonusStatus
            .encode(&mut writer, &CompatibilityProfile::US_852)
            .expect("encode");
        let bytes = writer.into_inner();
        assert_eq!(bytes.len(), 4 + 1 + 4 * 5);
        assert_eq!(&bytes[..4], &[0; 4]);
        assert_eq!(bytes[4], 0x01, "already collected, so nothing is claimable");
        assert!(bytes[5..].iter().all(|byte| *byte == 0));
    }

    /// An empty history is five zeroed slots, not an empty packet: the client reads a fixed
    /// number of fixed-width records.
    /// The allowlist exists so the unknown-opcode policy still means something. If a room or
    /// match opcode ever lands in it, a genuine gap in those handlers would be silently accepted.
    #[test]
    fn accepted_session_opcodes_exclude_room_and_match_opcodes() {
        for opcode in RETAIL_ACCEPTED_SESSION_OPCODES {
            assert!(is_retail_accepted_session_opcode(*opcode));
        }
        for room_or_match in [
            0x0008_u16, 0x0009, 0x000a, 0x000d, 0x000f, 0x0011, 0x0012, 0x0031,
        ] {
            assert!(
                !is_retail_accepted_session_opcode(room_or_match),
                "{room_or_match:#06x} has a state handler and must not be silently accepted"
            );
        }
        assert!(!is_retail_accepted_session_opcode(GameAuth::OPCODE));
        assert!(!is_retail_accepted_session_opcode(SelectChannel::OPCODE));
    }

    #[test]
    fn retail_player_history_is_five_empty_slots() {
        let mut writer = PacketWriter::new();
        RetailPlayerHistory
            .encode(&mut writer, &CompatibilityProfile::US_852)
            .expect("encode");
        let bytes = writer.into_inner();
        assert_eq!(bytes.len(), 260);
        assert_eq!(
            bytes.len(),
            RETAIL_RECENT_PLAYERS * RETAIL_RECENT_PLAYER_BYTES
        );
        assert!(bytes.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn retail_login_bonus_request_has_no_payload() {
        let mut reader = PacketReader::new(
            &[],
            crate::Direction::ClientToServer,
            crate::ServiceKind::Game,
            Some(RetailLoginBonusRequest::OPCODE),
        );
        assert!(
            RetailLoginBonusRequest::decode(&mut reader, &CompatibilityProfile::US_852).is_ok()
        );
        let mut trailing = PacketReader::new(
            &[0],
            crate::Direction::ClientToServer,
            crate::ServiceKind::Game,
            Some(RetailLoginBonusRequest::OPCODE),
        );
        assert!(
            RetailLoginBonusRequest::decode(&mut trailing, &CompatibilityProfile::US_852).is_err()
        );
    }

    #[test]
    fn inventory_segment_enforces_fifty_item_cap() {
        let packet = InventorySegment {
            segment_index: 0,
            segment_count: 1,
            items: vec![
                InventoryBootstrap {
                    id: 1,
                    type_id: 2,
                    quantity: 3,
                };
                51
            ],
        };
        let mut writer = PacketWriter::new();
        assert!(
            packet
                .encode(&mut writer, &CompatibilityProfile::US_852)
                .is_err()
        );
    }
}
