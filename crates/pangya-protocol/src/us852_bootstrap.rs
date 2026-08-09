//! Reference-derived U.S. 852 retail GameService bootstrap packets.
//!
//! Every layout here is derived from the vendored `pangbox--packetdoc` definitions and
//! corroborated against a GB.852-targeting reference server's observable protocol
//! behavior. **None has been accepted by a real client.** These types supersede the
//! synthetic `0x7f**` families for the bootstrap path; see
//! `docs/protocol/US852_RETAIL_BOOTSTRAP.md` for the full contract and its provenance.

use crate::{
    CompatibilityProfile, DecodePacket, EncodePacket, PacketDecodeError, PacketEncodeError,
    PacketReader, PacketWriter,
};
use zeroize::Zeroizing;

/// Retail server version string the client requires in the handover reply.
pub const US852_SERVER_VERSION: &[u8] = b"852.00";
/// Maximum bytes accepted for any bootstrap PString field.
pub const MAX_BOOTSTRAP_STRING_BYTES: usize = 128;
/// Entries per chunk of a rostered container packet.
pub const IFF_CONTAINER_CHUNK_ENTRIES: usize = 50;
/// Equipped item slots carried by the retail equipment block.
pub const EQUIPPED_ITEM_SLOTS: usize = 10;
/// Trailing zeroed equipment slots after the equipped item ids.
const EQUIPMENT_TRAILING_SLOTS: usize = 11;
/// Maximum channels the retail server channel list may advertise.
pub const MAX_SERVER_CHANNELS: usize = 255;
/// Fixed byte width of a retail channel name.
pub const CHANNEL_NAME_BYTES: usize = 64;

/// Client-visible handover rejection codes.
///
/// These are the client's own reactions, so they are the primary diagnostic during
/// client bring-up: a wrong code sends the player somewhere confusing rather than
/// showing a useful message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum HandoverRejection {
    /// Returns the client to LoginService to reconnect.
    ReconnectLoginServer = 1,
    /// Client reports it cannot reach LoginService.
    CannotConnectLoginServer = 3,
    /// Permanent account block.
    IdPermanentlyBlocked = 5,
    /// Temporary account block.
    IdBlocked = 7,
    /// Client and server disagree on protocol version.
    ServerVersionMismatch = 11,
    /// Server admits only allowlisted users.
    NonWhitelistedUser = 14,
    /// Region-blocked.
    GeoBlocked = 16,
    /// Account moved to another service.
    AccountTransferred = 19,
}

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

/// Retail GameService authentication, client opcode `0x0002`.
///
/// The client sends this immediately after LoginService hands it over. Identity is only
/// ever established by consuming the handover; every field here is untrusted input.
#[derive(Clone, Eq, PartialEq)]
pub struct RetailGameAuth {
    /// Claimed username; not authoritative.
    pub username: Vec<u8>,
    /// Claimed numeric user id; not authoritative.
    pub user_id: u32,
    /// Secret login-to-game bearer.
    pub login_key: Zeroizing<Vec<u8>>,
    /// Client-reported version string, expected to match [`US852_SERVER_VERSION`].
    pub client_version: Vec<u8>,
    /// Secret session bearer.
    pub session_key: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for RetailGameAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RetailGameAuth")
            .field("user_id", &self.user_id)
            .field("login_key", &"<redacted>")
            .field("session_key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl DecodePacket for RetailGameAuth {
    const OPCODE: u16 = 0x0002;

    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError> {
        check_decode_profile(profile, reader)?;
        let username = reader.pstring(MAX_BOOTSTRAP_STRING_BYTES)?.to_vec();
        let user_id = reader.u32_le()?;
        let _padding = reader.array::<4>()?;
        let _unknown = reader.array::<2>()?;
        let login_key = Zeroizing::new(reader.pstring(MAX_BOOTSTRAP_STRING_BYTES)?.to_vec());
        let client_version = reader.pstring(MAX_BOOTSTRAP_STRING_BYTES)?.to_vec();
        let _unknown_c = reader.u32_le()?;
        let _unknown_d = reader.u32_le()?;
        let session_key = Zeroizing::new(reader.pstring(MAX_BOOTSTRAP_STRING_BYTES)?.to_vec());
        // Retail clients append further unread bytes here; the reference server ignores
        // them, so trailing content is tolerated rather than treated as malformed.
        Ok(Self {
            username,
            user_id,
            login_key,
            client_version,
            session_key,
        })
    }
}

impl EncodePacket for RetailGameAuth {
    const OPCODE: u16 = 0x0002;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.pstring(&self.username, MAX_BOOTSTRAP_STRING_BYTES)?;
        writer.u32_le(self.user_id);
        writer.bytes(&[0; 4]);
        writer.bytes(&[0; 2]);
        writer.pstring(&self.login_key, MAX_BOOTSTRAP_STRING_BYTES)?;
        writer.pstring(&self.client_version, MAX_BOOTSTRAP_STRING_BYTES)?;
        writer.u32_le(0);
        writer.u32_le(0);
        writer.pstring(&self.session_key, MAX_BOOTSTRAP_STRING_BYTES)?;
        Ok(())
    }
}

/// Short-form handover control replies, server opcode `0x0044`.
///
/// The full success reply is a separate, much larger packet; these are the control
/// frames the client consumes while the server loads player state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandoverControl {
    /// Loading progress, `0..=15`.
    Progress(u8),
    /// Authentication accepted.
    Ok,
    /// Authentication refused with a client-visible reason.
    Rejected(HandoverRejection),
}

impl HandoverControl {
    /// Highest progress step the client accepts.
    pub const MAX_PROGRESS: u8 = 15;

    /// Builds a bounded progress update.
    ///
    /// # Errors
    /// Rejects a step above [`Self::MAX_PROGRESS`].
    pub const fn progress(value: u8) -> Result<Self, PacketEncodeError> {
        if value > Self::MAX_PROGRESS {
            return Err(PacketEncodeError::Limit {
                field: "handover progress",
                actual: value as usize,
                maximum: Self::MAX_PROGRESS as usize,
            });
        }
        Ok(Self::Progress(value))
    }
}

impl EncodePacket for HandoverControl {
    const OPCODE: u16 = 0x0044;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        match self {
            Self::Progress(value) => {
                if *value > Self::MAX_PROGRESS {
                    return Err(PacketEncodeError::Limit {
                        field: "handover progress",
                        actual: usize::from(*value),
                        maximum: usize::from(Self::MAX_PROGRESS),
                    });
                }
                writer.u8(0xd2);
                writer.u8(*value);
            }
            Self::Ok => {
                writer.u16_le(0x00d3);
                writer.u8(0);
            }
            Self::Rejected(reason) => writer.u16_le(*reason as u16),
        }
        Ok(())
    }
}

/// One retail equipment block.
///
/// Emitted standalone as server opcode `0x0072` and again inside the full handover
/// reply, so it is modelled once and encoded in both places.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailEquipment {
    /// Equipped caddie inventory id.
    pub caddie_uid: u32,
    /// Equipped character inventory id.
    pub character_uid: u32,
    /// Equipped club-set inventory id.
    pub club_set_uid: u32,
    /// Equipped ball catalog id.
    pub comet_iff_id: u32,
    /// Equipped consumable catalog ids.
    pub item_iff_ids: [u32; EQUIPPED_ITEM_SLOTS],
}

impl RetailEquipment {
    fn encode_body(&self, writer: &mut PacketWriter) {
        writer.u32_le(self.caddie_uid);
        writer.u32_le(self.character_uid);
        writer.u32_le(self.club_set_uid);
        writer.u32_le(self.comet_iff_id);
        for id in self.item_iff_ids {
            writer.u32_le(id);
        }
        // Background, frame, sticker, slot, unknown, title, and the four skin variants
        // plus one further unknown. All are cosmetic and unset by this server.
        for _ in 0..EQUIPMENT_TRAILING_SLOTS {
            writer.u32_le(0);
        }
    }
}

impl EncodePacket for RetailEquipment {
    const OPCODE: u16 = 0x0072;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        self.encode_body(writer);
        Ok(())
    }
}

/// One advertised in-server channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailChannel {
    /// Display name, zero-padded to [`CHANNEL_NAME_BYTES`].
    pub name: Vec<u8>,
    /// Maximum concurrent players.
    pub capacity: u16,
    /// Current occupancy.
    pub player_count: u16,
    /// Channel identifier.
    pub id: u16,
    /// Packed entry-restriction flags.
    pub restrictions: u16,
}

/// Retail channel list, server opcode `0x004d`.
///
/// This is the packet the current synthetic build mislabels as equipment selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerChannelList {
    /// Advertised channels.
    pub channels: Vec<RetailChannel>,
}

impl EncodePacket for ServerChannelList {
    const OPCODE: u16 = 0x004d;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        let count = u8::try_from(self.channels.len()).map_err(|_| PacketEncodeError::Limit {
            field: "server channels",
            actual: self.channels.len(),
            maximum: MAX_SERVER_CHANNELS,
        })?;
        writer.u8(count);
        for channel in &self.channels {
            writer.fixed_nul(&channel.name, CHANNEL_NAME_BYTES)?;
            writer.u16_le(channel.capacity);
            writer.u16_le(channel.player_count);
            writer.u16_le(channel.id);
            writer.u16_le(channel.restrictions);
            writer.bytes(&[0; 5]);
        }
        Ok(())
    }
}

/// Which rostered container a chunk belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum IffContainerKind {
    /// Owned characters. Mislabelled as a profile blob by the synthetic build.
    CharacterRoster = 0x0070,
    /// Owned caddies. Absent from the synthetic build entirely.
    CaddieRoster = 0x0071,
    /// Owned inventory items.
    Inventory = 0x0073,
}

/// Header of one chunk of a rostered container.
///
/// The client reassembles chunks using `total_entries`, so every chunk of a container
/// must repeat the same total while carrying only its own slice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IffContainerChunk {
    /// Container this chunk belongs to.
    pub kind: IffContainerKind,
    /// Entry count across every chunk of this container.
    pub total_entries: u16,
    /// Opaque pre-encoded entry bodies carried by this chunk.
    pub entries: Vec<Vec<u8>>,
}

impl IffContainerChunk {
    /// Splits pre-encoded entries into wire-sized chunks.
    ///
    /// An empty container still yields exactly one chunk, because the client waits for a
    /// container packet rather than inferring emptiness from silence.
    ///
    /// # Errors
    /// Rejects a container with more entries than the wire count can express.
    pub fn split(
        kind: IffContainerKind,
        entries: Vec<Vec<u8>>,
    ) -> Result<Vec<Self>, PacketEncodeError> {
        let total_entries = u16::try_from(entries.len()).map_err(|_| PacketEncodeError::Limit {
            field: "container entries",
            actual: entries.len(),
            maximum: usize::from(u16::MAX),
        })?;
        if entries.is_empty() {
            return Ok(vec![Self {
                kind,
                total_entries: 0,
                entries: Vec::new(),
            }]);
        }
        Ok(entries
            .chunks(IFF_CONTAINER_CHUNK_ENTRIES)
            .map(|chunk| Self {
                kind,
                total_entries,
                entries: chunk.to_vec(),
            })
            .collect())
    }

    /// Returns the opcode this chunk must be framed with.
    #[must_use]
    pub const fn opcode(&self) -> u16 {
        self.kind as u16
    }

    /// Encodes this chunk's body.
    ///
    /// The opcode is carried by [`Self::opcode`] rather than a const, because one type
    /// serves three container opcodes.
    ///
    /// # Errors
    /// Rejects a chunk carrying more entries than the wire count can express.
    pub fn encode_body(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        let chunk_entries =
            u16::try_from(self.entries.len()).map_err(|_| PacketEncodeError::Limit {
                field: "chunk entries",
                actual: self.entries.len(),
                maximum: usize::from(u16::MAX),
            })?;
        writer.u16_le(self.total_entries);
        writer.u16_le(chunk_entries);
        for entry in &self.entries {
            writer.bytes(entry);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ServiceKind, decode_packet_payload, encode_packet_payload};

    fn profile() -> CompatibilityProfile {
        CompatibilityProfile::US_852
    }

    #[test]
    fn retail_auth_round_trips_and_redacts_secrets() {
        let auth = RetailGameAuth {
            username: b"player".to_vec(),
            user_id: 4242,
            login_key: Zeroizing::new(b"login-secret".to_vec()),
            client_version: US852_SERVER_VERSION.to_vec(),
            session_key: Zeroizing::new(b"session-secret".to_vec()),
        };
        let payload = encode_packet_payload(&auth, &profile()).expect("encode");
        let decoded =
            decode_packet_payload::<RetailGameAuth>(&payload, &profile(), ServiceKind::Game)
                .expect("decode");
        assert_eq!(decoded, auth);
        let debug = format!("{decoded:?}");
        assert!(!debug.contains("login-secret"));
        assert!(!debug.contains("session-secret"));
    }

    #[test]
    fn retail_auth_tolerates_trailing_client_bytes() {
        let auth = RetailGameAuth {
            username: b"player".to_vec(),
            user_id: 1,
            login_key: Zeroizing::new(b"k".to_vec()),
            client_version: US852_SERVER_VERSION.to_vec(),
            session_key: Zeroizing::new(b"s".to_vec()),
        };
        let mut payload = encode_packet_payload(&auth, &profile()).expect("encode");
        payload.extend_from_slice(&[0xaa; 12]);
        let decoded =
            decode_packet_payload::<RetailGameAuth>(&payload, &profile(), ServiceKind::Game)
                .expect("decode");
        assert_eq!(decoded.user_id, 1);
    }

    #[test]
    fn handover_control_forms_are_exact() {
        let progress =
            encode_packet_payload(&HandoverControl::Progress(7), &profile()).expect("progress");
        assert_eq!(progress.as_slice(), &[0xd2, 7]);
        let ok = encode_packet_payload(&HandoverControl::Ok, &profile()).expect("ok");
        assert_eq!(ok.as_slice(), &[0xd3, 0x00, 0]);
        let rejected = encode_packet_payload(
            &HandoverControl::Rejected(HandoverRejection::ServerVersionMismatch),
            &profile(),
        )
        .expect("rejected");
        assert_eq!(rejected.as_slice(), &[11, 0]);
    }

    #[test]
    fn handover_progress_is_bounded() {
        assert!(HandoverControl::progress(15).is_ok());
        assert!(HandoverControl::progress(16).is_err());
        assert!(encode_packet_payload(&HandoverControl::Progress(200), &profile()).is_err());
    }

    #[test]
    fn equipment_block_is_fixed_width() {
        let equipment = RetailEquipment {
            caddie_uid: 1,
            character_uid: 2,
            club_set_uid: 3,
            comet_iff_id: 0x1400_0000,
            item_iff_ids: [0; EQUIPPED_ITEM_SLOTS],
        };
        let payload = encode_packet_payload(&equipment, &profile()).expect("encode");
        // Four ids, ten item slots, eleven cosmetic slots, all u32.
        assert_eq!(payload.len(), (4 + EQUIPPED_ITEM_SLOTS + 11) * 4);
        assert_eq!(&payload[12..16], &0x1400_0000_u32.to_le_bytes());
    }

    #[test]
    fn channel_list_pads_names_and_bounds_count() {
        let list = ServerChannelList {
            channels: vec![RetailChannel {
                name: b"Lolo".to_vec(),
                capacity: 200,
                player_count: 3,
                id: 1,
                restrictions: 0,
            }],
        };
        let payload = encode_packet_payload(&list, &profile()).expect("encode");
        assert_eq!(payload.len(), 1 + CHANNEL_NAME_BYTES + 2 + 2 + 2 + 2 + 5);
        assert_eq!(payload[0], 1);
        assert_eq!(&payload[1..5], b"Lolo");
        assert!(payload[5..1 + CHANNEL_NAME_BYTES].iter().all(|b| *b == 0));
    }

    #[test]
    fn container_splits_at_fifty_and_repeats_the_total() {
        let entries = (0..120_u32).map(|i| i.to_le_bytes().to_vec()).collect();
        let chunks = IffContainerChunk::split(IffContainerKind::Inventory, entries).expect("split");
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].entries.len(), 50);
        assert_eq!(chunks[2].entries.len(), 20);
        for chunk in &chunks {
            assert_eq!(chunk.total_entries, 120);
            assert_eq!(chunk.opcode(), 0x0073);
        }
    }

    #[test]
    fn empty_container_still_emits_one_chunk() {
        let chunks =
            IffContainerChunk::split(IffContainerKind::CaddieRoster, Vec::new()).expect("split");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].total_entries, 0);
        assert_eq!(chunks[0].opcode(), 0x0071);
    }

    fn sample_reply() -> HandoverReply {
        HandoverReply {
            server_name: b"pangya-rs".to_vec(),
            player: sample_player_data(),
            server_time: [0; 16],
            disabled_features: HandoverReply::DEFAULT_DISABLED_FEATURES,
        }
    }

    fn sample_player_data() -> RetailPlayerData {
        RetailPlayerData {
            identity: RetailPlayerIdentity {
                username: b"player".to_vec(),
                nickname: b"Nick".to_vec(),
                connection_id: 7,
                user_id: 42,
            },
            statistics: RetailPlayerStatistics::default(),
            equipment: RetailEquipment {
                caddie_uid: 0,
                character_uid: 1,
                club_set_uid: 2,
                comet_iff_id: 0x1400_0000,
                item_iff_ids: [0; EQUIPPED_ITEM_SLOTS],
            },
            character: RetailCharacter {
                iff_id: 0x0400_0000,
                uid: 1,
                hair_color: 0,
                part_iff_ids: [0; CHARACTER_PARTS],
                part_uids: [0; CHARACTER_PARTS],
                stats: [0; CHARACTER_STATS],
                mastery: 0,
            },
            caddie: RetailCaddie::default(),
            club_set_iff_id: 0x1000_0000,
        }
    }

    /// The record the client reads is identical in the lobby and in a match roster, so the
    /// roster entry is exactly the reply's player block plus its room number, time and card
    /// count. The lobby reply writes `0xffff` in that leading field because the player is in
    /// no room; a roster entry writes the room the match is in.
    #[test]
    fn a_match_roster_entry_carries_the_same_player_record_as_the_lobby() {
        let reply = encode_packet_payload(&sample_reply(), &profile()).expect("reply");
        let roster = encode_packet_payload(
            &crate::RetailMatchStart::Roster(vec![crate::RetailMatchPlayer {
                room_number: 1,
                player: sample_player_data(),
                start_time: [0; 16],
            }]),
            &profile(),
        )
        .expect("roster");
        // Subtype and count, then the room number, then the record.
        assert_eq!(roster[0], 0x00);
        assert_eq!(roster[1], 1);
        assert_eq!(u16::from_le_bytes([roster[2], roster[3]]), 1);
        // In the reply the record follows the subtype, both pstrings and the room id.
        let record = 1 + 2 + 6 + 2 + 9 + 2;
        let width = roster.len() - 4 - 16 - 1;
        assert_eq!(roster[4..4 + width], reply[record..record + width]);
    }

    #[test]
    fn statistics_block_is_exactly_the_reference_width() {
        let mut writer = PacketWriter::default();
        RetailPlayerStatistics::default().encode_body(&mut writer);
        assert_eq!(writer.as_slice().len(), PLAYER_STATISTICS_BYTES);
    }

    #[test]
    fn course_statistics_block_is_forty_three_bytes() {
        let mut writer = PacketWriter::default();
        RetailCourseStatistics { course: 3 }.encode_body(&mut writer);
        assert_eq!(writer.as_slice().len(), 43);
        assert_eq!(writer.as_slice()[0], 3);
    }

    #[test]
    fn character_block_is_five_hundred_thirteen_bytes() {
        let mut writer = PacketWriter::default();
        sample_reply().player.character.encode_body(&mut writer);
        assert_eq!(writer.as_slice().len(), 513);
    }

    #[test]
    fn caddie_block_is_twenty_five_bytes() {
        let mut writer = PacketWriter::default();
        RetailCaddie::default().encode_body(&mut writer);
        assert_eq!(writer.as_slice().len(), 25);
    }

    #[test]
    fn handover_reply_announces_852_and_carries_the_full_history_block() {
        let payload = encode_packet_payload(&sample_reply(), &profile()).expect("encode");
        // Subtype byte, then the version PString the client checks. PStrings carry a
        // little-endian u16 length, so the text itself starts at offset 3.
        assert_eq!(payload[0], 0x00);
        assert_eq!(
            u16::from_le_bytes([payload[1], payload[2]]),
            US852_SERVER_VERSION.len() as u16
        );
        assert_eq!(
            &payload[3..3 + US852_SERVER_VERSION.len()],
            US852_SERVER_VERSION
        );
        // The 12x21 history block dominates the packet; a reply that omits it would be
        // more than ten kilobytes short and would strand the client on its loading screen.
        assert!(payload.len() > HISTORY_SEASONS * HISTORY_COURSES * 43);
    }

    #[test]
    fn container_chunk_body_is_exact() {
        let chunks =
            IffContainerChunk::split(IffContainerKind::CharacterRoster, vec![vec![1, 2, 3, 4]])
                .expect("split");
        let mut writer = PacketWriter::default();
        chunks[0]
            .encode_body(&mut writer, &profile())
            .expect("encode");
        assert_eq!(writer.as_slice(), &[1, 0, 1, 0, 1, 2, 3, 4]);
        assert_eq!(chunks[0].opcode(), 0x0070);
    }

    /// A zeroed block is what this replaces, and month zero is the tell: no date has one.
    #[test]
    fn a_packed_system_time_names_a_real_date() {
        // 2026-08-10 00:07:04.250 UTC, a Monday.
        let packed = packed_system_time(1_786_320_424, 250);
        let field = |index: usize| u16::from_le_bytes([packed[index * 2], packed[index * 2 + 1]]);
        assert_eq!(field(0), 2026);
        assert_eq!(field(1), 8);
        assert_eq!(field(2), 1);
        assert_eq!(field(3), 10);
        assert_eq!(field(4), 0);
        assert_eq!(field(5), 7);
        assert_eq!(field(6), 4);
        assert_eq!(field(7), 250);
        assert_eq!(packed_system_time(0, 0)[0..2], 1970_u16.to_le_bytes());
    }
}

/// Exact wire width of the retail player-statistics block.
pub const PLAYER_STATISTICS_BYTES: usize = 239;
/// Exact wire width of the retail trophy block.
pub const PLAYER_TROPHIES_BYTES: usize = 78;
/// Seasons carried by the historical statistics block.
pub const HISTORY_SEASONS: usize = 12;
/// Courses carried per season by the historical statistics block.
pub const HISTORY_COURSES: usize = 21;
/// Character part slots.
pub const CHARACTER_PARTS: usize = 24;
/// Character auxiliary part slots.
pub const CHARACTER_AUX_PARTS: usize = 5;
/// Character stat slots: power, control, accuracy, spin, curve.
pub const CHARACTER_STATS: usize = 5;
/// Character card slots.
pub const CHARACTER_CARDS: usize = 12;
/// Exact wire width of the retail equipped-character block.
pub const CHARACTER_BLOCK_BYTES: usize = 513;
/// Fixed guild-info tail width.
const GUILD_INFO_BYTES: usize = 277;

/// Retail cumulative player statistics.
///
/// The client renders several of these directly, so the named fields are the ones worth
/// getting right; every remaining byte in the block is deliberately zero.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetailPlayerStatistics {
    /// Lifetime shots.
    pub total_shots: u32,
    /// Lifetime putts.
    pub total_putts: u32,
    /// Lifetime play time in seconds.
    pub play_time_seconds: u32,
    /// Lifetime holes.
    pub total_holes: u32,
    /// Lifetime hole-in-ones.
    pub hole_in_ones: u32,
    /// Accumulated experience.
    pub experience: u32,
    /// Current level.
    pub level: u8,
    /// Sum of final scores; under par is negative.
    pub total_score: i32,
    /// Completed games, used by the client's quit-rate display.
    pub games_played: u32,
    /// Abandoned games, the other half of the quit rate.
    pub games_quit: u32,
    /// Current pang balance.
    pub pang: u64,
}

impl RetailPlayerStatistics {
    fn encode_body(&self, writer: &mut PacketWriter) {
        let start = writer.as_slice().len();
        writer.u32_le(self.total_shots);
        writer.u32_le(self.total_putts);
        writer.u32_le(self.play_time_seconds);
        writer.u32_le(0); // shot time seconds
        writer.f32_le(0.0); // longest drive
        writer.u32_le(0); // pangya shots
        writer.u32_le(0); // timeouts
        writer.u32_le(0); // out-of-bounds shots
        writer.u32_le(0); // total distance
        writer.u32_le(self.total_holes);
        writer.u32_le(0); // unfinished holes
        writer.u32_le(self.hole_in_ones);
        writer.u16_le(0); // bunker shots
        writer.u32_le(0); // fairway shots
        writer.u32_le(0); // albatross
        writer.u32_le(0);
        writer.u32_le(0); // successful putts
        writer.f32_le(0.0); // longest putt
        writer.f32_le(0.0); // longest chip-in
        writer.u32_le(self.experience);
        writer.u8(self.level);
        writer.u64_le(self.pang);
        writer.u32_le(self.total_score as u32);
        writer.bytes(&[0; 5]);
        writer.u8(0);
        for _ in 0..6 {
            writer.u64_le(0);
        }
        writer.u32_le(self.games_played);
        writer.u32_le(0); // team holes
        writer.u32_le(0); // team wins
        writer.u32_le(0); // team games
        for _ in 0..5 {
            writer.u32_le(0); // ladder point/hole/win/lose/draw
        }
        writer.u32_le(0); // combo current streak
        writer.u32_le(0); // combo best streak
        writer.u32_le(self.games_quit);
        writer.u32_le(0); // Pang won in battle
        for _ in 0..5 {
            writer.u32_le(0);
        }
        writer.bytes(&[0; 10]);
        writer.u32_le(0);
        writer.bytes(&[0; 8]);
        debug_assert_eq!(writer.as_slice().len() - start, PLAYER_STATISTICS_BYTES);
    }
}

/// Course-record slots the statistics frame always carries.
pub const STATISTICS_COURSE_SLOTS: usize = 12;

/// Player statistics, server opcode `0x0045`.
///
/// Sent to each player as part of starting a hole, between the roster and the match plan. The
/// client answers `0x000E` expecting it: without it the per-player record it builds the hole
/// from is never completed.
///
/// The twelve course slots are the "no record" form. Each is one signed byte, and `-1` means
/// the slot is empty, so the whole tail is `0xff` twelve times and no record bodies follow.
///
/// # Provenance
///
/// `pangbox/packetdoc` (`gameservice/server/0045.ksy`: `user_statistic_data`, then
/// `user_statistic_data_ext`, then twelve `course_stat_slot`s whose body is present only when
/// the leading `s1` is not `-1`; documented there as a response to "GameService Client 0x000E
/// Game Start"), and `Acrisio-Filho/SuperSS-Dev`
/// (`Server Lib/Game Server/GAME/game.cpp` `Game::sendUpdateInfoAndMapStatistics`, which for
/// its `-1` option writes `UserInfo`, `TrofelInfo`, then `addInt64(-1)` and `addInt32(-1)` —
/// the same twelve `0xff` bytes).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailPlayerStatisticsReport {
    /// The statistics themselves.
    pub statistics: RetailPlayerStatistics,
}

impl EncodePacket for RetailPlayerStatisticsReport {
    const OPCODE: u16 = 0x0045;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        self.statistics.encode_body(writer);
        // The trophy block, written the same way the handover record writes it.
        writer.u16_le(1);
        writer.bytes(&[0; PLAYER_TROPHIES_BYTES - 4]);
        writer.u16_le(1);
        writer.bytes(&[0xff; STATISTICS_COURSE_SLOTS]);
        Ok(())
    }
}

/// Pang balance update, server opcode `0x0095`.
///
/// The lobby header reads its pang from this, not from the bootstrap statistics block, so an
/// account funded out of band shows zero until this is sent.
///
/// # Provenance
///
/// Type discriminant `273` selecting a `u4` status and a `u8` amount, from `pangbox/server`
/// (`game/packet/server.go` `ServerMoneyUpdate`, `UpdatePangBalanceData`), ISC licensed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailPangBalance {
    /// Current pang balance.
    pub pang: u64,
}

impl EncodePacket for RetailPangBalance {
    const OPCODE: u16 = 0x0095;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u16_le(273);
        writer.u32_le(0);
        writer.u64_le(self.pang);
        Ok(())
    }
}

/// Point ("cookie") balance, server opcode `0x0096`.
///
/// # Provenance
///
/// A single `u8`, from `pangbox/server` (`game/packet/server.go` `ServerPointsBalance`), ISC
/// licensed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailPointBalance {
    /// Current point balance.
    pub points: u64,
}

impl EncodePacket for RetailPointBalance {
    const OPCODE: u16 = 0x0096;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u64_le(self.points);
        Ok(())
    }
}

/// Retail per-course historical statistics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetailCourseStatistics {
    /// Course ordinal within the fixed 21-course table.
    pub course: u8,
}

impl RetailCourseStatistics {
    fn encode_body(&self, writer: &mut PacketWriter) {
        writer.u8(self.course);
        for _ in 0..6 {
            writer.u32_le(0); // strokes, putts, holes, fairway, hole-in-ones, unknown
        }
        writer.u32_le(0); // total score
        writer.u8(0); // best score
        writer.u64_le(0); // best Pang earned
        writer.u32_le(0); // character used for the best score
        writer.u8(0);
    }
}

/// Retail equipped-character block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetailCharacter {
    /// Catalog id.
    pub iff_id: u32,
    /// Inventory id.
    pub uid: u32,
    /// Selected hair colour.
    pub hair_color: u32,
    /// Catalog ids of fitted parts.
    pub part_iff_ids: [u32; CHARACTER_PARTS],
    /// Inventory ids of fitted parts.
    pub part_uids: [u32; CHARACTER_PARTS],
    /// Stat points: power, control, accuracy, spin, curve.
    pub stats: [u8; CHARACTER_STATS],
    /// Accumulated character mastery.
    pub mastery: u32,
}

impl RetailCharacter {
    pub(crate) fn encode_body(&self, writer: &mut PacketWriter) {
        let start = writer.as_slice().len();
        writer.u32_le(self.iff_id);
        writer.u32_le(self.uid);
        writer.u32_le(self.hair_color);
        for id in self.part_iff_ids {
            writer.u32_le(id);
        }
        for id in self.part_uids {
            writer.u32_le(id);
        }
        writer.bytes(&[0; 216]);
        for _ in 0..CHARACTER_AUX_PARTS {
            writer.u32_le(0);
        }
        writer.u32_le(0); // cut-in catalog id
        writer.bytes(&[0; 12]);
        for stat in self.stats {
            writer.u8(stat);
        }
        writer.u32_le(self.mastery);
        for _ in 0..CHARACTER_CARDS {
            writer.u32_le(0);
        }
        debug_assert_eq!(writer.as_slice().len() - start, CHARACTER_BLOCK_BYTES);
    }
}

/// Retail equipped-caddie block. A zeroed value encodes "no caddie".
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetailCaddie {
    /// Inventory id.
    pub uid: u32,
    /// Catalog id.
    pub iff_id: u32,
    /// Current level.
    pub level: u8,
    /// Accumulated experience.
    pub experience: u32,
}

impl RetailCaddie {
    fn encode_body(&self, writer: &mut PacketWriter) {
        writer.u32_le(self.uid);
        writer.u32_le(self.iff_id);
        writer.u32_le(0);
        writer.u8(self.level);
        writer.u32_le(self.experience);
        writer.u64_le(0);
    }
}

/// Identity fields the client shows in the lobby.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailPlayerIdentity {
    /// Authentication name.
    pub username: Vec<u8>,
    /// Display nickname.
    pub nickname: Vec<u8>,
    /// Per-connection identifier.
    pub connection_id: u32,
    /// Durable numeric account identifier.
    pub user_id: u32,
}

impl RetailPlayerIdentity {
    fn encode_body(&self, writer: &mut PacketWriter) -> Result<(), PacketEncodeError> {
        writer.fixed_nul(&self.username, 22)?;
        writer.fixed_nul(&self.nickname, 22)?;
        writer.fixed_nul(&[], 17)?; // guild name
        writer.fixed_nul(&[], 24)?; // guild image
        writer.u32_le(self.connection_id);
        writer.bytes(&[0; 12]);
        writer.u32_le(0);
        writer.u32_le(0);
        writer.u16_le(0);
        writer.bytes(&[0; 6]);
        writer.bytes(&[0; 16]);
        writer.fixed_nul(&[], 128)?; // global id
        writer.u32_le(self.user_id);
        Ok(())
    }
}

/// One player, whole.
///
/// The client reads this same record in two places: the handover reply that admits it to the
/// lobby, and the match roster that opens a versus hole. Upstream reuses one structure for
/// both (`pangbox/server` `pangya/player.go` `PlayerData`, carried by `PlayerMainData` in
/// `game/packet/server.go` and by `GamePlayer` in the same file), so this does too — a match
/// roster that describes players any less completely than the lobby does is one the client
/// cannot build its models from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetailPlayerData {
    /// Identity fields.
    pub identity: RetailPlayerIdentity,
    /// Cumulative statistics.
    pub statistics: RetailPlayerStatistics,
    /// Equipment selections.
    pub equipment: RetailEquipment,
    /// Equipped character.
    pub character: RetailCharacter,
    /// Equipped caddie.
    pub caddie: RetailCaddie,
    /// Catalog id of the equipped club set.
    ///
    /// The client resolves the club models from this, not from the inventory id beside it. A
    /// zero here leaves it without a club set to draw from and it dies building the hole.
    pub club_set_iff_id: u32,
}

impl RetailPlayerData {
    pub(crate) fn encode_body(&self, writer: &mut PacketWriter) -> Result<(), PacketEncodeError> {
        self.identity.encode_body(writer)?;
        self.statistics.encode_body(writer);
        // Trophies: amateur 6..1 then pro 1..7, each gold/silver/bronze.
        writer.u16_le(1);
        writer.bytes(&[0; PLAYER_TROPHIES_BYTES - 4]);
        writer.u16_le(1);
        self.equipment.encode_body(writer);
        for _ in 0..HISTORY_SEASONS {
            for course in 0..HISTORY_COURSES {
                RetailCourseStatistics {
                    course: u8::try_from(course).unwrap_or(0),
                }
                .encode_body(writer);
            }
        }
        self.character.encode_body(writer);
        self.caddie.encode_body(writer);
        // Sixteen bytes the equipment block above is short of `UserEquip`
        // (`Acrisio-Filho/SuperSS-Dev`, `Server Lib/Game Server/TYPE/pangya_game_st.h`, whose
        // `skin_id[6]`, `skin_typeid[6]`, `mascot_id` and `poster[2]` total sixty against the
        // eleven zeroed words written there). They are carried here rather than in the
        // equipment block so the roster entry's total width, which matches the client's own
        // per-player record stride exactly, does not change.
        //
        // Their effect is to put the club set where the client reads it. The client takes its
        // hole-load lookup key from entry offset `0x2f2e` — measured, by stamping every zero
        // word of a roster entry with its own offset and reading back the word the client
        // keys on — and with these sixteen bytes in front, `0x2f2e` is exactly
        // `ClubSetInfo::_typeid`, the catalog id the club models are resolved from. A catalog
        // lookup that misses is the null the client then dereferences.
        writer.bytes(&[0; 16]);
        writer.u32_le(self.equipment.club_set_uid);
        writer.u32_le(self.club_set_iff_id);
        writer.bytes(&[0; 4]);
        // Active mascot.
        writer.u32_le(0);
        writer.u32_le(0);
        writer.u8(0);
        writer.u32_le(0);
        writer.fixed_nul(b"0", 16)?;
        writer.bytes(&[0; 33]);
        Ok(())
    }
}

/// Full retail handover reply, server opcode `0x0044` subtype `0x00`.
///
/// This is the packet that releases the client from its loading screen into the lobby.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandoverReply {
    /// Server display name.
    pub server_name: Vec<u8>,
    /// The player being admitted.
    pub player: RetailPlayerData,
    /// Packed server time.
    pub server_time: [u8; 16],
    /// Feature-disable flags; `1 << 18` is the known-good baseline.
    pub disabled_features: u64,
}

impl HandoverReply {
    /// Feature-flag value the reference server uses.
    pub const DEFAULT_DISABLED_FEATURES: u64 = 1 << 18;
}

impl EncodePacket for HandoverReply {
    const OPCODE: u16 = 0x0044;

    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError> {
        check_encode_profile(profile)?;
        writer.u8(0x00); // full-reply subtype
        writer.pstring(US852_SERVER_VERSION, MAX_BOOTSTRAP_STRING_BYTES)?;
        writer.pstring(&self.server_name, MAX_BOOTSTRAP_STRING_BYTES)?;
        // The room this player is in, ahead of the record itself: 0xffff means "none". It is
        // not part of the player record, which is why the match roster does not repeat it.
        writer.u16_le(0xffff);
        self.player.encode_body(writer)?;
        writer.bytes(&self.server_time);
        writer.u16_le(0);
        for _ in 0..3 {
            writer.u16_le(0); // papel shop
        }
        writer.u32_le(0);
        writer.u64_le(self.disabled_features);
        writer.u32_le(0); // login count
        writer.u32_le(0); // server flags
        writer.bytes(&[0; GUILD_INFO_BYTES]);
        Ok(())
    }
}

/// Packs a wall-clock instant into the sixteen-byte `SYSTEMTIME` the client reads.
///
/// Eight little-endian `u16` fields: year, month, day of week, day, hour, minute, second,
/// millisecond. A zeroed block is not a valid time — month zero and day zero name no date —
/// so anything the client converts back into a date from one gets nothing.
///
/// # Provenance
///
/// From `pangbox/server` (`pangya/systemtime.go`), ISC licensed.
#[must_use]
pub fn packed_system_time(unix_seconds: i64, millisecond: u16) -> [u8; 16] {
    let days = unix_seconds.div_euclid(86_400);
    let seconds_of_day = unix_seconds.rem_euclid(86_400);
    // 1970-01-01 was a Thursday, and day zero of the week is Sunday.
    let day_of_week = u16::try_from((days + 4).rem_euclid(7)).unwrap_or(0);
    let (year, month, day) = civil_from_days(days);
    let mut packed = [0_u8; 16];
    let fields = [
        year,
        month,
        day_of_week,
        day,
        u16::try_from(seconds_of_day / 3_600).unwrap_or(0),
        u16::try_from((seconds_of_day / 60) % 60).unwrap_or(0),
        u16::try_from(seconds_of_day % 60).unwrap_or(0),
        millisecond,
    ];
    for (slot, field) in packed.chunks_exact_mut(2).zip(fields) {
        slot.copy_from_slice(&field.to_le_bytes());
    }
    packed
}

/// Converts a count of days since the Unix epoch into a civil year, month and day.
///
/// Howard Hinnant's `civil_from_days`, shifted to an era beginning on 0000-03-01 so that the
/// leap day falls at the end of a year and needs no special case.
fn civil_from_days(days: i64) -> (u16, u16, u16) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (
        u16::try_from(year).unwrap_or(0),
        u16::try_from(month).unwrap_or(0),
        u16::try_from(day).unwrap_or(0),
    )
}
