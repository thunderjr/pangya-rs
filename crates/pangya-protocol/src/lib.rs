#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Bounded U.S. 852 packet primitives, state registry, and Tokio codec.

mod codec;
mod error;
mod game;
mod login;
mod m4_room;
mod m5_solo;
mod m6_stroke;
mod m7_economy;
mod profile;
mod reader;
mod registry;
mod us852_bootstrap;
mod us852_match;
mod us852_room;
mod writer;

pub use codec::{CodecLimits, FrameCodec, FrameMetadata, InboundFrame, OutboundFrame};
pub use error::{ErrorClass, PacketDecodeError, PacketEncodeError};
pub use game::{
    ChannelJoined, CharacterBootstrap, CharacterInfo, EquipmentInfo, GAME_INVENTORY_SEGMENT_ITEMS,
    GameAuth, InventoryBootstrap, InventorySegment, MAX_GAME_HANDOVER_BYTES, PlayerInfo,
    RETAIL_ACCEPTED_SESSION_OPCODES, RETAIL_RECENT_PLAYER_BYTES, RETAIL_RECENT_PLAYERS,
    RetailChannelJoinNotice, RetailChannelJoined, RetailLoginBonusRequest, RetailLoginBonusStatus,
    RetailPlayerHistory, RetailPlayerHistoryRequest, RetailSelectChannel, SelectChannel,
    is_retail_accepted_session_opcode, synthetic_game_hello, us852_game_hello,
};
pub use login::{
    ChatMacros, CheckNickname, EmptyMessageServerList, GameServerEntry, GameServerList,
    LOGIN_ERROR_DUPLICATE_CONNECTION, LOGIN_ERROR_INVALID_CREDENTIALS, LOGIN_STATUS_SET_CHARACTER,
    LOGIN_STATUS_SET_NICKNAME, LoginKey, LoginRequest, LoginResult, LoginSuccess,
    MAX_LOGIN_SERVER_CHANNELS, NicknameCheckResult, SelectCharacter, SelectServer,
    ServerChannelEntry, SessionKey, SetNickname, us852_login_hello,
};
pub use m4_room::{
    MAX_ROOM_MEMBERS, MAX_ROOM_SUMMARIES, RoomChatEvent, RoomChatRequest, RoomCommand,
    RoomCommandResult, RoomCommandResultResponse, RoomCreateRequest, RoomJoinRequest,
    RoomKickRequest, RoomLeaveRequest, RoomListRequest, RoomListResponse, RoomMembershipEvent,
    RoomMembershipKind, RoomReadyRequest, RoomSettingsRequest, RoomStateRequest, RoomStateResponse,
    SYNTHETIC_M4_C2S_CHAT, SYNTHETIC_M4_C2S_CREATE, SYNTHETIC_M4_C2S_JOIN, SYNTHETIC_M4_C2S_KICK,
    SYNTHETIC_M4_C2S_LEAVE, SYNTHETIC_M4_C2S_LIST, SYNTHETIC_M4_C2S_READY,
    SYNTHETIC_M4_C2S_SETTINGS, SYNTHETIC_M4_C2S_STATE, SYNTHETIC_M4_S2C_CHAT,
    SYNTHETIC_M4_S2C_COMMAND_RESULT, SYNTHETIC_M4_S2C_LIST, SYNTHETIC_M4_S2C_MEMBERSHIP_EVENT,
    SYNTHETIC_M4_S2C_STATE, synthetic_m4_registry,
};
pub use m5_solo::{
    BalanceUpdate, FinishHole, HoleResult, Lie, LoadingComplete, MatchAbortReason, MatchAborted,
    MatchPhase, MatchStarted, SYNTHETIC_M5_C2S_FINISH_HOLE, SYNTHETIC_M5_C2S_LOADING_COMPLETE,
    SYNTHETIC_M5_C2S_SHOT_ACTION, SYNTHETIC_M5_C2S_SHOT_RESULT, SYNTHETIC_M5_C2S_START_SOLO,
    SYNTHETIC_M5_S2C_BALANCE_UPDATE, SYNTHETIC_M5_S2C_COMMAND_RESULT, SYNTHETIC_M5_S2C_HOLE_RESULT,
    SYNTHETIC_M5_S2C_MATCH_ABORTED, SYNTHETIC_M5_S2C_MATCH_PHASE, SYNTHETIC_M5_S2C_MATCH_STARTED,
    SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY, SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY, ShotAction,
    ShotActionRelay, ShotResult, ShotResultRelay, SoloCommand, SoloCommandOutcome,
    SoloCommandResult, SoloPhase, StartSolo, Weather, Wind, synthetic_m5_registry,
};
pub use m6_stroke::{
    SYNTHETIC_M6_C2S_GIVE_UP, SYNTHETIC_M6_C2S_LOADING_COMPLETE, SYNTHETIC_M6_C2S_SHOT_ACTION,
    SYNTHETIC_M6_C2S_SHOT_RESULT, SYNTHETIC_M6_C2S_START_STROKE_TWO, SYNTHETIC_M6_S2C_ACTION_RELAY,
    SYNTHETIC_M6_S2C_BALANCE_UPDATE, SYNTHETIC_M6_S2C_COMMAND_RESULT,
    SYNTHETIC_M6_S2C_MATCH_ABORTED, SYNTHETIC_M6_S2C_MATCH_STARTED, SYNTHETIC_M6_S2C_PHASE,
    SYNTHETIC_M6_S2C_RESULT_RELAY, SYNTHETIC_M6_S2C_STANDINGS, SYNTHETIC_M6_S2C_TURN_STARTED,
    StartStrokeTwo, StrokeAbortReason, StrokeActionRelay, StrokeBalanceUpdate, StrokeCommand,
    StrokeCommandOutcome, StrokeCommandResult, StrokeCompletion, StrokeGiveUp,
    StrokeLoadingComplete, StrokeMatchAborted, StrokeMatchStarted, StrokePhase, StrokePhaseKind,
    StrokeResultRelay, StrokeShotAction, StrokeShotResult, StrokeStandingEntry, StrokeStandings,
    StrokeTurnStarted, synthetic_m6_registry,
};
pub use m7_economy::{
    ConsumeOneRequest, EconomyCommand, EconomyCommandResult, EconomyItemKind, EconomyOutcome,
    EquipRequest, EquipmentChanged, InventoryChanged, MAX_PURCHASE_QUANTITY, MAX_SHOP_PAGE_ENTRIES,
    PurchaseCommitted, PurchaseRequestPacket, RepairCommitted, RepairRequest,
    SYNTHETIC_M7_C2S_CONSUME, SYNTHETIC_M7_C2S_EQUIP, SYNTHETIC_M7_C2S_PURCHASE,
    SYNTHETIC_M7_C2S_REPAIR, SYNTHETIC_M7_C2S_SHOP_PAGE, SYNTHETIC_M7_S2C_COMMAND_RESULT,
    SYNTHETIC_M7_S2C_EQUIPMENT_CHANGED, SYNTHETIC_M7_S2C_INVENTORY_CHANGED,
    SYNTHETIC_M7_S2C_PURCHASE_COMMITTED, SYNTHETIC_M7_S2C_REPAIR_COMMITTED,
    SYNTHETIC_M7_S2C_SHOP_PAGE, ShopOffer, ShopPage, ShopPageRequest, synthetic_m7_registry,
};
pub use profile::{
    ClientVersion, CompatibilityProfile, ConnectionState, Direction, ProfileError, Region,
    ServiceKind,
};
pub use reader::PacketReader;
pub use registry::{PacketRegistry, RegistryKey, RegistryLookup};
pub use us852_bootstrap::{
    CHANNEL_NAME_BYTES, CHARACTER_AUX_PARTS, CHARACTER_CARDS, CHARACTER_PARTS, CHARACTER_STATS,
    EQUIPPED_ITEM_SLOTS, HISTORY_COURSES, HISTORY_SEASONS, HandoverControl, HandoverRejection,
    HandoverReply, IFF_CONTAINER_CHUNK_ENTRIES, IffContainerChunk, IffContainerKind,
    MAX_BOOTSTRAP_STRING_BYTES, MAX_SERVER_CHANNELS, PLAYER_STATISTICS_BYTES,
    PLAYER_TROPHIES_BYTES, RetailCaddie, RetailChannel, RetailCharacter, RetailCourseStatistics,
    RetailEquipment, RetailGameAuth, RetailPangBalance, RetailPlayerIdentity,
    RetailPlayerStatistics, RetailPointBalance, ServerChannelList, US852_SERVER_VERSION,
};
pub use us852_match::{
    MAX_MATCH_HOLES, RetailAimRotate, RetailFinishHole, RetailHole, RetailHoleWeather,
    RetailHoleWind, RetailMatchInfo, RetailMatchStart, RetailPlayerStartHole,
    RetailShotCommitRelay, RetailTurnEnd, RetailTurnStart, RetailWeather,
};
pub use us852_room::{
    MAX_RETAIL_PURCHASE_ITEMS, MAX_ROOM_PLAYERS, MAX_ROOMS_PER_LIST, RETAIL_CONSUMABLE_SLOTS,
    ROOM_NAME_BYTES, ROOM_PLAYER_RECORD_BYTES, ROOM_RECORD_BYTES, RetailEquipmentSlot,
    RetailEquipmentUpdate, RetailEquipmentUpdated, RetailHoleProgression,
    RetailLockerCombinationAttempt, RetailLockerCombinationResponse, RetailLockerInventoryRequest,
    RetailLockerInventoryResponse, RetailMultiplayerJoined, RetailMultiplayerLeft,
    RetailMyRoomEnter, RetailMyRoomEntered, RetailMyRoomInventoryRequest, RetailMyRoomLayout,
    RetailPangSpent, RetailPlayerInfo, RetailPurchaseItem, RetailPurchaseRequest,
    RetailPurchaseResponse, RetailRoom, RetailRoomCensus, RetailRoomCreate, RetailRoomJoin,
    RetailRoomJoinResult, RetailRoomLeave, RetailRoomList, RetailRoomPlayer, RetailRoomState,
    RetailRoomType, RetailShopJoin, RetailShopJoined, RoomCensusKind, RoomJoinRejection,
    RoomListKind, RoomPlayerFlags,
};
pub use writer::PacketWriter;

/// Explicit storage for fields whose meaning is not established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownBytes<const N: usize>(pub [u8; N]);

/// Decodes one typed packet payload.
pub trait DecodePacket: Sized {
    /// Wire opcode.
    const OPCODE: u16;
    /// Decode using a selected compatibility profile.
    ///
    /// # Errors
    /// Returns a contextual error for malformed or unsupported fields.
    fn decode(
        reader: &mut PacketReader<'_>,
        profile: &CompatibilityProfile,
    ) -> Result<Self, PacketDecodeError>;
}

/// Encodes one typed packet payload.
pub trait EncodePacket {
    /// Wire opcode.
    const OPCODE: u16;
    /// Encode using a selected compatibility profile.
    ///
    /// # Errors
    /// Returns an error when a value cannot fit its wire representation.
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError>;
}

/// Encodes a typed packet body without its opcode.
///
/// This is the safe composition boundary used by the runtime before the bounded
/// transport encoder adds the opcode, compression, and encryption.
///
/// # Errors
/// Returns the packet model's checked encoding error.
pub fn encode_packet_payload<T: EncodePacket>(
    packet: &T,
    profile: &CompatibilityProfile,
) -> Result<zeroize::Zeroizing<Vec<u8>>, PacketEncodeError> {
    let mut writer = PacketWriter::new();
    packet.encode(&mut writer, profile)?;
    Ok(zeroize::Zeroizing::new(writer.into_inner()))
}

/// Decodes a typed packet body using redacted contextual errors.
///
/// # Errors
/// Returns a checked packet decoding error. Packet implementations decide
/// whether an unknown tail is part of their provisional layout.
pub fn decode_packet_payload<T: DecodePacket>(
    payload: &[u8],
    profile: &CompatibilityProfile,
    service: ServiceKind,
) -> Result<T, PacketDecodeError> {
    let mut reader =
        PacketReader::new(payload, Direction::ClientToServer, service, Some(T::OPCODE));
    T::decode(&mut reader, profile)
}
