//! Bounded lobby registry actor for process-local room discovery and admission.

use std::{
    collections::{BTreeMap, HashMap},
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};

use futures_util::{StreamExt as _, stream::FuturesOrdered};
use pangya_domain::{
    AbortMatch, AbortStrokeMatch, BeginSoloMatch, BeginStrokeMatch, ChatText, CommitSoloHole,
    CommitStrokeMatch, MarkSoloInGame, MarkStrokeInGame, MatchAbortReason, MatchId, MatchResultKey,
    PlayerConnectionId, RoomError, RoomId, RoomName, RoomPassword, RoomSettings, RoomSnapshot,
    RoomSummary, SoloMatchResult, StrokeMatchResult,
};
use pangya_protocol::{
    LoadingComplete, ShotAction, ShotResult, StrokeLoadingComplete, StrokeShotAction,
    StrokeShotResult,
};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

use crate::{
    match_state::{RelayDisposition, SoloMatchError, SoloStartPlan},
    room::{
        RoomActorEvent, RoomActorLimits, RoomCloseOutcome, RoomEvent, RoomHandle, RoomIdentity,
        spawn_room_with_events,
    },
    stroke_state::{StrokeLoadingOutcome, StrokeMatchError, StrokeRelayOutcome, StrokeStartPlan},
};

/// Hard bounds for the lobby registry and each room it creates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LobbyLimits {
    max_rooms: NonZeroUsize,
    command_capacity: NonZeroUsize,
    cleanup_capacity: NonZeroUsize,
    event_capacity: NonZeroUsize,
    command_timeout: Duration,
    shutdown_timeout: Duration,
    room: RoomActorLimits,
}

impl LobbyLimits {
    /// Creates a fully bounded policy.
    #[must_use]
    pub const fn new(
        max_rooms: NonZeroUsize,
        command_capacity: NonZeroUsize,
        cleanup_capacity: NonZeroUsize,
        event_capacity: NonZeroUsize,
        command_timeout: Duration,
        shutdown_timeout: Duration,
        room: RoomActorLimits,
    ) -> Self {
        Self {
            max_rooms,
            command_capacity,
            cleanup_capacity,
            event_capacity,
            command_timeout,
            shutdown_timeout,
            room,
        }
    }

    /// Priority cleanup queue capacity.
    #[must_use]
    pub const fn cleanup_capacity(self) -> usize {
        self.cleanup_capacity.get()
    }

    /// Returns whether nested capacities and deadlines are within production bounds.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.max_rooms.get() <= 65_536
            && self.command_capacity.get() <= 65_536
            && self.cleanup_capacity.get() <= 10_001
            && self.event_capacity.get() <= 65_536
            && !self.command_timeout.is_zero()
            && self.command_timeout.as_secs() <= 300
            && !self.shutdown_timeout.is_zero()
            && self.shutdown_timeout.as_secs() <= 300
            && self.room.is_valid()
    }
}

impl Default for LobbyLimits {
    fn default() -> Self {
        Self::new(
            NonZeroUsize::new(1_024).unwrap_or(NonZeroUsize::MIN),
            NonZeroUsize::new(256).unwrap_or(NonZeroUsize::MIN),
            NonZeroUsize::new(257).unwrap_or(NonZeroUsize::MIN),
            NonZeroUsize::new(256).unwrap_or(NonZeroUsize::MIN),
            Duration::from_secs(3),
            Duration::from_secs(5),
            RoomActorLimits::default(),
        )
    }
}

/// Ordered room lifecycle kind published by the sole lobby registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RoomLifecycleEvent {
    Created,
    Closed,
}

/// Ordered room lifecycle state published without room identifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RoomLifecycle {
    pub(crate) event: RoomLifecycleEvent,
    pub(crate) active_count: usize,
}

/// Ordered match cardinality transition published by the sole lobby registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MatchLifecycleEvent {
    Activated,
    Deactivated,
}

/// Exact process-local match count after one authoritative transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MatchLifecycle {
    pub(crate) event: MatchLifecycleEvent,
    pub(crate) active_count: usize,
}

/// A room operation routed by the registry using the caller's registered connection identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LobbyRoomCommand {
    /// Owner-only capacity update.
    UpdateSettings(RoomSettings),
    /// Set the caller's ready state.
    SetReady(bool),
    /// Broadcast validated chat.
    Chat(ChatText),
    /// Owner-only removal of another authoritative connection ID.
    Kick(PlayerConnectionId),
    /// Fetch the caller's current authoritative room state.
    GetState,
}

/// Result of a routed operation. Chat has no state mutation projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LobbyRouteResult {
    /// An immutable post-mutation state.
    Snapshot(RoomSnapshot),
    /// Chat was accepted for bounded broadcast.
    ChatAccepted,
}

/// Typed solo gameplay operation routed from an authoritative connection mapping.
#[derive(Clone, Debug, PartialEq)]
pub enum LobbySoloCommand {
    /// Reserve an immutable begin before repository persistence.
    PrepareStart(SoloStartPlan),
    /// Confirm exact begin persistence and enter Loading.
    ConfirmBegin {
        /// Match ID returned by PrepareStart.
        match_id: MatchId,
        /// Result key returned by PrepareStart.
        result_key: MatchResultKey,
    },
    /// Cancel exact reservation after begin failure/cancellation.
    CancelBegin {
        /// Reserved match ID.
        match_id: MatchId,
        /// Reserved result key.
        result_key: MatchResultKey,
    },
    /// Canonical validated loading completion.
    LoadingComplete(LoadingComplete),
    /// Validated sequenced action.
    ShotAction(ShotAction),
    /// Validated result for the pending action.
    ShotResult(ShotResult),
    /// Prepare the server-owned commit request.
    PrepareFinish,
    /// Apply an exact trusted repository result.
    ApplyCommit(SoloMatchResult),
    /// Abort a commit failure/cancellation without reward.
    Abort(MatchAbortReason),
    /// Clear a retained abort after repository acknowledgement.
    AcknowledgeAbort(AbortMatch),
}

/// Typed output from a routed solo operation.
#[derive(Clone, Debug, PartialEq)]
pub enum LobbySoloRouteResult {
    /// Immutable begin request for persistence.
    Begin(BeginSoloMatch),
    /// Authoritative in-game transition request for persistence.
    InGame(MarkSoloInGame),
    /// Server-owned commit request for persistence.
    Commit(CommitSoloHole),
    /// Trusted committed result emitted and match cleared.
    Committed(SoloMatchResult),
    /// Accepted or exact duplicate relay status.
    Relay(RelayDisposition),
    /// Idempotent no-reward abort, if a match existed.
    Abort(Option<AbortMatch>),
    /// Command completed without a data payload.
    Applied,
}

/// Typed stroke gameplay operation routed from authoritative room membership.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)] // The bounded plan stays typed and allocation-free at routing.
pub enum LobbyStrokeCommand {
    /// Reserve immutable start input.
    PrepareStart(StrokeStartPlan),
    /// Confirm durable begin.
    ConfirmBegin {
        /// Durable aggregate identity.
        match_id: MatchId,
        /// Aggregate result idempotency key.
        result_key: MatchResultKey,
    },
    /// Cancel unpersisted begin.
    CancelBegin {
        /// Reserved aggregate identity.
        match_id: MatchId,
        /// Reserved aggregate result key.
        result_key: MatchResultKey,
    },
    /// Per-member load barrier signal.
    LoadingComplete(StrokeLoadingComplete),
    /// Confirm durable loading-to-in-game transition.
    ConfirmInGame(MarkStrokeInGame),
    /// Validated action.
    ShotAction(StrokeShotAction),
    /// Validated result.
    ShotResult(StrokeShotResult),
    /// Participant give-up.
    GiveUp,
    /// Explicit no-reward abort.
    Abort(MatchAbortReason),
}

/// Typed stroke route output.
#[derive(Clone, Debug, PartialEq)]
pub enum LobbyStrokeRouteResult {
    /// Immutable begin request.
    Begin(BeginStrokeMatch),
    /// Load barrier result.
    Loading(StrokeLoadingOutcome),
    /// Relay disposition.
    Relay(RelayDisposition),
    /// Result relay plus optional terminal settlement.
    Result(StrokeRelayOutcome),
    /// Automatic terminal settlement.
    Settlement(CommitStrokeMatch),
    /// Exact abort, if active state existed.
    Abort(Option<AbortStrokeMatch>),
    /// Command applied.
    Applied,
}

/// Typed M6 persistence work retained independently of connection mappings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LobbyStrokePersistence {
    /// Aggregate no-reward abort.
    Abort {
        /// Authoritative room identity.
        room_id: RoomId,
        /// Exact durable abort request.
        request: AbortStrokeMatch,
    },
    /// Aggregate settlement.
    Settlement {
        /// Authoritative room identity.
        room_id: RoomId,
        /// Exact aggregate settlement request.
        request: CommitStrokeMatch,
    },
}

/// Bounded retained persistence work produced while gracefully shutting down the lobby.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LobbyShutdownOutcome {
    aborts: Vec<AbortMatch>,
    stroke: Vec<LobbyStrokePersistence>,
}

impl LobbyShutdownOutcome {
    /// Retained no-reward aborts, in ascending room-ID shutdown order.
    #[must_use]
    pub fn aborts(&self) -> &[AbortMatch] {
        &self.aborts
    }

    /// Consumes the outcome into its bounded abort list.
    #[must_use]
    pub fn into_aborts(self) -> Vec<AbortMatch> {
        self.aborts
    }

    /// Retained M6 work in ascending room-ID shutdown order.
    #[must_use]
    pub fn stroke(&self) -> &[LobbyStrokePersistence] {
        &self.stroke
    }
}

/// Typed lobby shutdown failure.
#[derive(Clone, Copy, Debug, Eq, thiserror::Error, PartialEq)]
pub enum LobbyShutdownError {
    /// A lobby or room control operation failed.
    #[error(transparent)]
    Room(#[from] RoomError),
    /// Registry cardinality exceeded its configured hard room cap.
    #[error("lobby shutdown room cardinality exceeded its configured bound")]
    CapacityInvariant,
    /// The bounded abort output could not reserve its configured maximum capacity.
    #[error("lobby shutdown could not reserve its bounded abort output")]
    Allocation,
}

struct RoomRecord {
    handle: RoomHandle,
    summary: RoomSummary,
}

struct ConnectionRecord {
    room_id: RoomId,
    cancellation: CancellationToken,
}

const COMMAND_PENDING: u8 = 0;
const COMMAND_EXECUTING: u8 = 1;
const COMMAND_CANCELLED: u8 = 2;

type CommandGate = Arc<AtomicU8>;

struct GatedCommand<T> {
    gate: CommandGate,
    command: T,
}

enum LobbyCommand {
    Create {
        name: RoomName,
        password: Option<RoomPassword>,
        settings: RoomSettings,
        owner: RoomIdentity,
        outbound: mpsc::Sender<RoomEvent>,
        cancellation: CancellationToken,
        reply: oneshot::Sender<Result<RoomSummary, RoomError>>,
    },
    List {
        reply: oneshot::Sender<Vec<RoomSummary>>,
    },
    Join {
        room_id: RoomId,
        identity: RoomIdentity,
        password: Option<RoomPassword>,
        outbound: mpsc::Sender<RoomEvent>,
        cancellation: CancellationToken,
        reply: oneshot::Sender<Result<RoomSnapshot, RoomError>>,
    },
    Leave {
        connection_id: PlayerConnectionId,
        reply: oneshot::Sender<Result<Option<RoomSnapshot>, RoomError>>,
    },
    Route {
        connection_id: PlayerConnectionId,
        command: LobbyRoomCommand,
        reply: oneshot::Sender<Result<LobbyRouteResult, RoomError>>,
    },
    RouteSolo {
        connection_id: PlayerConnectionId,
        command: LobbySoloCommand,
        reply: oneshot::Sender<Result<LobbySoloRouteResult, SoloMatchError>>,
    },
    RouteStroke {
        connection_id: PlayerConnectionId,
        command: LobbyStrokeCommand,
        reply: oneshot::Sender<Result<LobbyStrokeRouteResult, StrokeMatchError>>,
    },
    ApplyStrokeInGame {
        room_id: RoomId,
        mark: MarkStrokeInGame,
        reply: oneshot::Sender<Result<(), StrokeMatchError>>,
    },
    ApplyStrokeCommit {
        room_id: RoomId,
        result: StrokeMatchResult,
        reply: oneshot::Sender<Result<StrokeMatchResult, StrokeMatchError>>,
    },
    AcknowledgeStrokeAbort {
        room_id: RoomId,
        abort: AbortStrokeMatch,
        reply: oneshot::Sender<Result<(), StrokeMatchError>>,
    },
    #[cfg(test)]
    AbortRoom {
        room_id: RoomId,
        reply: oneshot::Sender<Result<(), RoomError>>,
    },
    #[cfg(test)]
    CreateAfterRelease {
        name: RoomName,
        settings: RoomSettings,
        owner: RoomIdentity,
        outbound: mpsc::Sender<RoomEvent>,
        cancellation: CancellationToken,
        started: oneshot::Sender<()>,
        release: oneshot::Receiver<()>,
        reply: oneshot::Sender<Result<RoomSummary, RoomError>>,
    },
}

enum LobbyControl {
    Disconnect {
        connection_id: PlayerConnectionId,
        reason: MatchAbortReason,
        reply: oneshot::Sender<Result<RoomCloseOutcome, RoomError>>,
    },
    Shutdown {
        reply: oneshot::Sender<Result<LobbyShutdownOutcome, LobbyShutdownError>>,
    },
}

/// Cloneable endpoint for the sole lobby registry task.
#[derive(Clone)]
pub struct LobbyHandle {
    commands: mpsc::Sender<GatedCommand<LobbyCommand>>,
    controls: mpsc::Sender<GatedCommand<LobbyControl>>,
    command_timeout: Duration,
    shutdown_timeout: Duration,
    room_lifecycle: broadcast::Sender<RoomLifecycle>,
    match_lifecycle: broadcast::Sender<MatchLifecycle>,
}

impl std::fmt::Debug for LobbyHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LobbyHandle")
            .finish_non_exhaustive()
    }
}

impl LobbyHandle {
    pub(crate) fn subscribe_room_lifecycle(&self) -> broadcast::Receiver<RoomLifecycle> {
        self.room_lifecycle.subscribe()
    }

    pub(crate) fn subscribe_match_lifecycle(&self) -> broadcast::Receiver<MatchLifecycle> {
        self.match_lifecycle.subscribe()
    }

    fn send(&self, command: LobbyCommand, gate: CommandGate) -> Result<(), RoomError> {
        self.commands
            .try_send(GatedCommand { gate, command })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => RoomError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => RoomError::Closed,
            })
    }

    async fn send_control(
        &self,
        command: LobbyControl,
        gate: CommandGate,
        deadline: Duration,
    ) -> Result<(), RoomError> {
        timeout(deadline, self.controls.send(GatedCommand { gate, command }))
            .await
            .map_err(|_| RoomError::Timeout)?
            .map_err(|_| RoomError::Closed)
    }

    async fn await_reply<T>(
        gate: &CommandGate,
        mut receive: oneshot::Receiver<T>,
        deadline: Duration,
    ) -> Result<T, RoomError> {
        match timeout(deadline, &mut receive).await {
            Ok(reply) => reply.map_err(|_| RoomError::Closed),
            Err(_) => match gate.compare_exchange(
                COMMAND_PENDING,
                COMMAND_CANCELLED,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => Err(RoomError::Timeout),
                Err(COMMAND_EXECUTING) => receive.await.map_err(|_| RoomError::Closed),
                Err(_) => Err(RoomError::Closed),
            },
        }
    }

    fn new_gate() -> CommandGate {
        Arc::new(AtomicU8::new(COMMAND_PENDING))
    }

    /// Creates a room and atomically registers its owner connection.
    pub async fn create(
        &self,
        name: RoomName,
        password: Option<RoomPassword>,
        settings: RoomSettings,
        owner: RoomIdentity,
        outbound: mpsc::Sender<RoomEvent>,
        cancellation: CancellationToken,
    ) -> Result<RoomSummary, RoomError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send(
            LobbyCommand::Create {
                name,
                password,
                settings,
                owner,
                outbound,
                cancellation,
                reply,
            },
            Arc::clone(&gate),
        )?;
        Self::await_reply(&gate, receive, self.command_timeout).await?
    }

    /// Lists immutable summaries in ascending room-ID order.
    pub async fn list(&self) -> Result<Vec<RoomSummary>, RoomError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send(LobbyCommand::List { reply }, Arc::clone(&gate))?;
        Self::await_reply(&gate, receive, self.command_timeout).await
    }

    /// Atomically admits a connection to one room only.
    pub async fn join(
        &self,
        room_id: RoomId,
        identity: RoomIdentity,
        password: Option<RoomPassword>,
        outbound: mpsc::Sender<RoomEvent>,
        cancellation: CancellationToken,
    ) -> Result<RoomSnapshot, RoomError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send(
            LobbyCommand::Join {
                room_id,
                identity,
                password,
                outbound,
                cancellation,
                reply,
            },
            Arc::clone(&gate),
        )?;
        Self::await_reply(&gate, receive, self.command_timeout).await?
    }

    /// Voluntarily leaves the connection's registered room.
    pub async fn leave(
        &self,
        connection_id: PlayerConnectionId,
    ) -> Result<Option<RoomSnapshot>, RoomError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send(
            LobbyCommand::Leave {
                connection_id,
                reply,
            },
            Arc::clone(&gate),
        )?;
        Self::await_reply(&gate, receive, self.command_timeout).await?
    }

    /// Routes a dropped connection through the priority control queue.
    pub async fn disconnect(
        &self,
        connection_id: PlayerConnectionId,
    ) -> Result<Option<AbortMatch>, RoomError> {
        self.disconnect_with_reason(connection_id, MatchAbortReason::Disconnect)
            .await
    }

    /// Routes cleanup through the priority queue with an authoritative abort reason.
    pub async fn disconnect_with_reason(
        &self,
        connection_id: PlayerConnectionId,
        reason: MatchAbortReason,
    ) -> Result<Option<AbortMatch>, RoomError> {
        self.disconnect_with_work(connection_id, reason)
            .await
            .map(|outcome| match outcome {
                RoomCloseOutcome::M5Abort { request, .. } => Some(request),
                _ => None,
            })
    }

    /// Removes a mapping while retaining typed M5/M6 persistence work by room/match authority.
    pub async fn disconnect_with_work(
        &self,
        connection_id: PlayerConnectionId,
        reason: MatchAbortReason,
    ) -> Result<RoomCloseOutcome, RoomError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send_control(
            LobbyControl::Disconnect {
                connection_id,
                reason,
                reply,
            },
            Arc::clone(&gate),
            self.command_timeout,
        )
        .await?;
        Self::await_reply(&gate, receive, self.command_timeout).await?
    }

    /// Routes a command only to the caller's registered room.
    pub async fn route(
        &self,
        connection_id: PlayerConnectionId,
        command: LobbyRoomCommand,
    ) -> Result<LobbyRouteResult, RoomError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send(
            LobbyCommand::Route {
                connection_id,
                command,
                reply,
            },
            Arc::clone(&gate),
        )?;
        Self::await_reply(&gate, receive, self.command_timeout).await?
    }

    /// Routes a typed solo command using only the lobby's connection-to-room mapping.
    pub async fn route_solo(
        &self,
        connection_id: PlayerConnectionId,
        command: LobbySoloCommand,
    ) -> Result<LobbySoloRouteResult, SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send(
            LobbyCommand::RouteSolo {
                connection_id,
                command,
                reply,
            },
            Arc::clone(&gate),
        )
        .map_err(map_room_to_solo)?;
        Self::await_reply(&gate, receive, self.command_timeout)
            .await
            .map_err(map_room_to_solo)?
    }

    /// Routes a typed stroke command through the caller's authoritative mapping.
    pub async fn route_stroke(
        &self,
        connection_id: PlayerConnectionId,
        command: LobbyStrokeCommand,
    ) -> Result<LobbyStrokeRouteResult, StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send(
            LobbyCommand::RouteStroke {
                connection_id,
                command,
                reply,
            },
            Arc::clone(&gate),
        )
        .map_err(map_room_to_stroke)?;
        Self::await_reply(&gate, receive, self.command_timeout)
            .await
            .map_err(map_room_to_stroke)?
    }

    /// Applies durable in-game confirmation using room authority after mapping removal.
    pub async fn apply_stroke_in_game_by_room(
        &self,
        room_id: RoomId,
        mark: MarkStrokeInGame,
    ) -> Result<(), StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send(
            LobbyCommand::ApplyStrokeInGame {
                room_id,
                mark,
                reply,
            },
            Arc::clone(&gate),
        )
        .map_err(map_room_to_stroke)?;
        Self::await_reply(&gate, receive, self.command_timeout)
            .await
            .map_err(map_room_to_stroke)?
    }

    /// Applies trusted aggregate commit using room authority after mapping removal.
    pub async fn apply_stroke_commit_by_room(
        &self,
        room_id: RoomId,
        result: StrokeMatchResult,
    ) -> Result<StrokeMatchResult, StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send(
            LobbyCommand::ApplyStrokeCommit {
                room_id,
                result,
                reply,
            },
            Arc::clone(&gate),
        )
        .map_err(map_room_to_stroke)?;
        Self::await_reply(&gate, receive, self.command_timeout)
            .await
            .map_err(map_room_to_stroke)?
    }

    /// Acknowledges a durable abort using room authority after mapping removal.
    pub async fn acknowledge_stroke_abort_by_room(
        &self,
        room_id: RoomId,
        abort: AbortStrokeMatch,
    ) -> Result<(), StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send(
            LobbyCommand::AcknowledgeStrokeAbort {
                room_id,
                abort,
                reply,
            },
            Arc::clone(&gate),
        )
        .map_err(map_room_to_stroke)?;
        Self::await_reply(&gate, receive, self.command_timeout)
            .await
            .map_err(map_room_to_stroke)?
    }

    #[cfg(test)]
    async fn abort_room_for_test(&self, room_id: RoomId) -> Result<(), RoomError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send(
            LobbyCommand::AbortRoom { room_id, reply },
            Arc::clone(&gate),
        )?;
        Self::await_reply(&gate, receive, self.command_timeout).await?
    }

    /// Drains all rooms and returns their bounded retained aborts.
    pub async fn shutdown(&self) -> Result<LobbyShutdownOutcome, LobbyShutdownError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send_control(
            LobbyControl::Shutdown { reply },
            Arc::clone(&gate),
            self.shutdown_timeout,
        )
        .await
        .map_err(LobbyShutdownError::from)?;
        Self::await_reply(&gate, receive, self.shutdown_timeout)
            .await
            .map_err(LobbyShutdownError::from)?
    }
}

struct LobbyRegistry {
    limits: LobbyLimits,
    rooms: BTreeMap<RoomId, RoomRecord>,
    connections: HashMap<PlayerConnectionId, ConnectionRecord>,
    next_room_id: u32,
    events: mpsc::Sender<RoomActorEvent>,
    room_lifecycle: broadcast::Sender<RoomLifecycle>,
    match_lifecycle: broadcast::Sender<MatchLifecycle>,
    prepared_matches: BTreeMap<RoomId, BeginSoloMatch>,
    active_matches: BTreeMap<RoomId, AbortMatch>,
    prepared_stroke: BTreeMap<RoomId, BeginStrokeMatch>,
    active_stroke: BTreeMap<RoomId, (MatchId, MatchResultKey)>,
}

impl LobbyRegistry {
    fn allocate_room_id(&mut self) -> Result<RoomId, RoomError> {
        let attempts = self.rooms.len().saturating_add(1);
        for _ in 0..attempts {
            let candidate = if self.next_room_id == 0 {
                1
            } else {
                self.next_room_id
            };
            self.next_room_id = candidate.checked_add(1).unwrap_or(1);
            let id = RoomId::new(candidate).map_err(|_| RoomError::IdExhausted)?;
            if !self.rooms.contains_key(&id) {
                return Ok(id);
            }
        }
        Err(RoomError::IdExhausted)
    }

    fn publish_lifecycle(&self, event: RoomLifecycleEvent) {
        let lifecycle = RoomLifecycle {
            event,
            active_count: self.rooms.len(),
        };
        let _no_receivers = self.room_lifecycle.send(lifecycle);
    }

    fn publish_match_lifecycle(&self, event: MatchLifecycleEvent) {
        let lifecycle = MatchLifecycle {
            event,
            active_count: self
                .active_matches
                .len()
                .saturating_add(self.active_stroke.len()),
        };
        let _no_receivers = self.match_lifecycle.send(lifecycle);
    }

    fn start_match(&mut self, room_id: RoomId, begin: BeginSoloMatch) {
        let abort = AbortMatch::new(
            begin.match_id(),
            begin.result_key(),
            begin.account_id(),
            MatchAbortReason::PersistenceFailure,
        );
        if self.active_matches.insert(room_id, abort).is_none() {
            self.publish_match_lifecycle(MatchLifecycleEvent::Activated);
        }
    }

    fn start_stroke_match(&mut self, room_id: RoomId, begin: &BeginStrokeMatch) {
        if self
            .active_stroke
            .insert(room_id, (begin.match_id(), begin.result_key()))
            .is_none()
        {
            self.publish_match_lifecycle(MatchLifecycleEvent::Activated);
        }
    }

    fn deactivate_stroke(
        &mut self,
        room_id: RoomId,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) {
        let exact = self
            .active_stroke
            .get(&room_id)
            .is_some_and(|current| *current == (match_id, result_key));
        if exact {
            self.active_stroke.remove(&room_id);
            self.publish_match_lifecycle(MatchLifecycleEvent::Deactivated);
        }
    }

    fn deactivate_committed_match(&mut self, room_id: RoomId, result: SoloMatchResult) {
        let exact = self.active_matches.get(&room_id).is_some_and(|active| {
            active.match_id() == result.match_id()
                && active.result_key() == result.result_key()
                && active.account_id() == result.account_id()
        });
        if exact {
            self.active_matches.remove(&room_id);
            self.publish_match_lifecycle(MatchLifecycleEvent::Deactivated);
        }
    }

    fn deactivate_aborted_match(&mut self, room_id: RoomId, abort: AbortMatch) {
        let exact = self.active_matches.get(&room_id).is_some_and(|active| {
            active.match_id() == abort.match_id()
                && active.result_key() == abort.result_key()
                && active.account_id() == abort.account_id()
        });
        if exact {
            self.active_matches.remove(&room_id);
            self.publish_match_lifecycle(MatchLifecycleEvent::Deactivated);
        }
    }

    fn remove_room(&mut self, room_id: RoomId, cancel_members: bool) {
        self.prepared_matches.remove(&room_id);
        self.prepared_stroke.remove(&room_id);
        let removed_match = self.active_matches.remove(&room_id).is_some();
        let removed_stroke = self.active_stroke.remove(&room_id).is_some();
        if removed_match || removed_stroke {
            self.publish_match_lifecycle(MatchLifecycleEvent::Deactivated);
        }
        let removed = self.rooms.remove(&room_id).is_some();
        self.connections.retain(|_, connection| {
            if connection.room_id != room_id {
                return true;
            }
            if cancel_members {
                connection.cancellation.cancel();
            }
            false
        });
        if removed {
            self.publish_lifecycle(RoomLifecycleEvent::Closed);
        }
    }

    fn update_snapshot(&mut self, room_id: RoomId, snapshot: &RoomSnapshot) {
        if let Some(record) = self.rooms.get_mut(&room_id) {
            record.summary = snapshot.summary().clone();
        }
    }

    fn room_for(&self, connection_id: PlayerConnectionId) -> Result<RoomId, RoomError> {
        self.connections
            .get(&connection_id)
            .map(|connection| connection.room_id)
            .ok_or(RoomError::NotMember)
    }

    async fn create(
        &mut self,
        name: RoomName,
        password: Option<RoomPassword>,
        settings: RoomSettings,
        owner: RoomIdentity,
        outbound: mpsc::Sender<RoomEvent>,
        cancellation: CancellationToken,
    ) -> Result<RoomSummary, RoomError> {
        if self.connections.contains_key(&owner.connection_id) {
            return Err(RoomError::AlreadyMember);
        }
        if self.rooms.len() >= self.limits.max_rooms.get() {
            return Err(RoomError::MaxRooms);
        }
        let id = self.allocate_room_id()?;
        let connection_id = owner.connection_id;
        let (handle, summary) = spawn_room_with_events(
            id,
            name,
            password,
            settings,
            owner,
            outbound,
            cancellation.clone(),
            self.limits.room,
            Some(self.events.clone()),
        );
        self.rooms.insert(
            id,
            RoomRecord {
                handle,
                summary: summary.clone(),
            },
        );
        self.publish_lifecycle(RoomLifecycleEvent::Created);
        self.connections.insert(
            connection_id,
            ConnectionRecord {
                room_id: id,
                cancellation,
            },
        );
        Ok(summary)
    }

    async fn join(
        &mut self,
        room_id: RoomId,
        identity: RoomIdentity,
        password: Option<RoomPassword>,
        outbound: mpsc::Sender<RoomEvent>,
        cancellation: CancellationToken,
    ) -> Result<RoomSnapshot, RoomError> {
        if self.connections.contains_key(&identity.connection_id) {
            return Err(RoomError::AlreadyMember);
        }
        let handle = self
            .rooms
            .get(&room_id)
            .map(|record| record.handle.clone())
            .ok_or(RoomError::RoomNotFound)?;
        let connection_id = identity.connection_id;
        match handle
            .join_with_cancellation(identity, password, outbound, cancellation.clone())
            .await
        {
            Ok(snapshot) => {
                self.connections.insert(
                    connection_id,
                    ConnectionRecord {
                        room_id,
                        cancellation,
                    },
                );
                self.update_snapshot(room_id, &snapshot);
                Ok(snapshot)
            }
            Err(RoomError::Closed) => {
                self.remove_room(room_id, true);
                Err(RoomError::RoomNotFound)
            }
            Err(error) => Err(error),
        }
    }

    async fn leave_connection(
        &mut self,
        connection_id: PlayerConnectionId,
    ) -> Result<Option<RoomSnapshot>, RoomError> {
        let room_id = self.room_for(connection_id)?;
        let handle = self
            .rooms
            .get(&room_id)
            .map(|record| record.handle.clone())
            .ok_or(RoomError::RoomNotFound)?;
        match handle.leave(connection_id).await {
            Ok(snapshot) => {
                self.connections.remove(&connection_id);
                if let Some(snapshot) = &snapshot {
                    self.update_snapshot(room_id, snapshot);
                } else {
                    self.remove_room(room_id, false);
                }
                Ok(snapshot)
            }
            Err(RoomError::Closed) => {
                self.remove_room(room_id, true);
                Err(RoomError::RoomNotFound)
            }
            Err(error) => Err(error),
        }
    }

    async fn disconnect_connection(
        &mut self,
        connection_id: PlayerConnectionId,
        reason: MatchAbortReason,
    ) -> Result<RoomCloseOutcome, RoomError> {
        let room_id = self.room_for(connection_id)?;
        let handle = self
            .rooms
            .get(&room_id)
            .map(|record| record.handle.clone())
            .ok_or(RoomError::RoomNotFound)?;
        match handle.disconnect_with_abort(connection_id, reason).await {
            Ok(outcome) => {
                self.connections.remove(&connection_id);
                if let Some(abort) = outcome.abort {
                    self.prepared_matches.remove(&room_id);
                    self.deactivate_aborted_match(room_id, abort);
                }
                match outcome.outcome {
                    RoomCloseOutcome::M6Abort { request, .. } => {
                        self.prepared_stroke.remove(&room_id);
                        // Retain active authority until persistence acknowledgement.
                        let _request = request;
                    }
                    RoomCloseOutcome::M6Settlement { request, .. } => {
                        self.prepared_stroke.remove(&room_id);
                        let _request = request;
                    }
                    RoomCloseOutcome::None | RoomCloseOutcome::M5Abort { .. } => {}
                }
                if let Some(snapshot) = &outcome.snapshot {
                    self.update_snapshot(room_id, snapshot);
                } else {
                    self.remove_room(room_id, false);
                }
                Ok(outcome.outcome)
            }
            Err(RoomError::Closed) => {
                self.remove_room(room_id, true);
                Err(RoomError::RoomNotFound)
            }
            Err(error) => Err(error),
        }
    }

    async fn route(
        &mut self,
        connection_id: PlayerConnectionId,
        command: LobbyRoomCommand,
    ) -> Result<LobbyRouteResult, RoomError> {
        let room_id = self.room_for(connection_id)?;
        let handle = self
            .rooms
            .get(&room_id)
            .map(|record| record.handle.clone())
            .ok_or(RoomError::RoomNotFound)?;
        let result = match command {
            LobbyRoomCommand::UpdateSettings(settings) => handle
                .update_settings(connection_id, settings)
                .await
                .map(LobbyRouteResult::Snapshot),
            LobbyRoomCommand::SetReady(ready) => handle
                .set_ready(connection_id, ready)
                .await
                .map(LobbyRouteResult::Snapshot),
            LobbyRoomCommand::Chat(text) => handle
                .chat(connection_id, text)
                .await
                .map(|()| LobbyRouteResult::ChatAccepted),
            LobbyRoomCommand::Kick(target) => {
                let result = handle.kick(connection_id, target).await;
                if result.is_ok() {
                    self.connections.remove(&target);
                }
                result.map(LobbyRouteResult::Snapshot)
            }
            LobbyRoomCommand::GetState => handle.state().await.map(LobbyRouteResult::Snapshot),
        };
        match result {
            Ok(LobbyRouteResult::Snapshot(snapshot)) => {
                self.update_snapshot(room_id, &snapshot);
                Ok(LobbyRouteResult::Snapshot(snapshot))
            }
            Ok(result) => Ok(result),
            Err(RoomError::Closed) => {
                self.remove_room(room_id, true);
                Err(RoomError::RoomNotFound)
            }
            Err(error) => Err(error),
        }
    }

    async fn route_solo(
        &mut self,
        connection_id: PlayerConnectionId,
        command: LobbySoloCommand,
    ) -> Result<LobbySoloRouteResult, SoloMatchError> {
        let room_id = self.room_for(connection_id).map_err(map_room_to_solo)?;
        let handle = self
            .rooms
            .get(&room_id)
            .map(|record| record.handle.clone())
            .ok_or(SoloMatchError::Closed)?;
        let result = match command {
            LobbySoloCommand::PrepareStart(plan) => {
                let result = handle.prepare_solo_start(connection_id, plan).await;
                if let Ok(begin) = &result {
                    self.prepared_matches.insert(room_id, begin.clone());
                }
                result.map(LobbySoloRouteResult::Begin)
            }
            LobbySoloCommand::ConfirmBegin {
                match_id,
                result_key,
            } => {
                let result = handle
                    .confirm_solo_begin(connection_id, match_id, result_key)
                    .await;
                if result.is_ok()
                    && let Some(begin) = self.prepared_matches.remove(&room_id)
                {
                    self.start_match(room_id, begin);
                }
                result.map(|()| LobbySoloRouteResult::Applied)
            }
            LobbySoloCommand::CancelBegin {
                match_id,
                result_key,
            } => {
                let result = handle
                    .cancel_solo_begin(connection_id, match_id, result_key)
                    .await;
                if result.is_ok() {
                    self.prepared_matches.remove(&room_id);
                }
                result.map(|()| LobbySoloRouteResult::Applied)
            }
            LobbySoloCommand::LoadingComplete(loading) => handle
                .solo_loading_complete(connection_id, loading)
                .await
                .map(LobbySoloRouteResult::InGame),
            LobbySoloCommand::ShotAction(action) => handle
                .solo_action(connection_id, action)
                .await
                .map(LobbySoloRouteResult::Relay),
            LobbySoloCommand::ShotResult(result) => handle
                .solo_result(connection_id, result)
                .await
                .map(LobbySoloRouteResult::Relay),
            LobbySoloCommand::PrepareFinish => handle
                .prepare_solo_finish(connection_id)
                .await
                .map(LobbySoloRouteResult::Commit),
            LobbySoloCommand::ApplyCommit(result) => {
                let applied = handle.apply_solo_commit(connection_id, result).await;
                if let Ok(committed) = applied {
                    self.prepared_matches.remove(&room_id);
                    self.deactivate_committed_match(room_id, committed);
                    Ok(LobbySoloRouteResult::Committed(committed))
                } else {
                    applied.map(LobbySoloRouteResult::Committed)
                }
            }
            LobbySoloCommand::Abort(reason) => {
                let aborted = handle.abort_solo(connection_id, reason).await;
                if aborted.is_ok() {
                    self.prepared_matches.remove(&room_id);
                }
                aborted.map(LobbySoloRouteResult::Abort)
            }
            LobbySoloCommand::AcknowledgeAbort(abort) => {
                let acknowledged = handle.acknowledge_solo_abort(connection_id, abort).await;
                if acknowledged.is_ok() {
                    self.prepared_matches.remove(&room_id);
                    self.deactivate_aborted_match(room_id, abort);
                }
                acknowledged.map(|()| LobbySoloRouteResult::Applied)
            }
        };
        if result == Err(SoloMatchError::Closed) {
            self.remove_room(room_id, true);
        }
        result
    }

    async fn route_stroke(
        &mut self,
        connection_id: PlayerConnectionId,
        command: LobbyStrokeCommand,
    ) -> Result<LobbyStrokeRouteResult, StrokeMatchError> {
        let room_id = self.room_for(connection_id).map_err(map_room_to_stroke)?;
        let handle = self
            .rooms
            .get(&room_id)
            .map(|record| record.handle.clone())
            .ok_or(StrokeMatchError::Closed)?;
        let result = match command {
            LobbyStrokeCommand::PrepareStart(plan) => {
                let result = handle.prepare_stroke_start(connection_id, plan).await;
                if let Ok(begin) = &result {
                    self.prepared_stroke.insert(room_id, begin.clone());
                }
                result.map(LobbyStrokeRouteResult::Begin)
            }
            LobbyStrokeCommand::ConfirmBegin {
                match_id,
                result_key,
            } => {
                let result = handle
                    .confirm_stroke_begin(connection_id, match_id, result_key)
                    .await;
                if result.is_ok()
                    && let Some(begin) = self.prepared_stroke.remove(&room_id)
                {
                    self.start_stroke_match(room_id, &begin);
                }
                result.map(|()| LobbyStrokeRouteResult::Applied)
            }
            LobbyStrokeCommand::CancelBegin {
                match_id,
                result_key,
            } => {
                let result = handle
                    .cancel_stroke_begin(connection_id, match_id, result_key)
                    .await;
                if result.is_ok() {
                    self.prepared_stroke.remove(&room_id);
                }
                result.map(|()| LobbyStrokeRouteResult::Applied)
            }
            LobbyStrokeCommand::LoadingComplete(loading) => handle
                .stroke_loading_complete(connection_id, loading)
                .await
                .map(LobbyStrokeRouteResult::Loading),
            LobbyStrokeCommand::ConfirmInGame(mark) => handle
                .confirm_stroke_in_game(mark)
                .await
                .map(|()| LobbyStrokeRouteResult::Applied),
            LobbyStrokeCommand::ShotAction(action) => handle
                .stroke_action(connection_id, action)
                .await
                .map(LobbyStrokeRouteResult::Relay),
            LobbyStrokeCommand::ShotResult(result) => handle
                .stroke_result(connection_id, result)
                .await
                .map(LobbyStrokeRouteResult::Result),
            LobbyStrokeCommand::GiveUp => handle
                .stroke_give_up(connection_id)
                .await
                .map(LobbyStrokeRouteResult::Settlement),
            LobbyStrokeCommand::Abort(reason) => handle
                .abort_stroke(reason)
                .await
                .map(LobbyStrokeRouteResult::Abort),
        };
        if result == Err(StrokeMatchError::Closed) {
            self.remove_room(room_id, true);
        }
        result
    }

    async fn apply_stroke_in_game(
        &mut self,
        room_id: RoomId,
        mark: MarkStrokeInGame,
    ) -> Result<(), StrokeMatchError> {
        let exact = self
            .active_stroke
            .get(&room_id)
            .is_some_and(|current| *current == (mark.match_id(), mark.result_key()));
        if !exact {
            return Err(StrokeMatchError::IdentityMismatch);
        }
        let handle = self
            .rooms
            .get(&room_id)
            .map(|record| record.handle.clone())
            .ok_or(StrokeMatchError::Closed)?;
        handle.confirm_stroke_in_game(mark).await
    }

    async fn apply_stroke_commit(
        &mut self,
        room_id: RoomId,
        result: StrokeMatchResult,
    ) -> Result<StrokeMatchResult, StrokeMatchError> {
        let exact = self
            .active_stroke
            .get(&room_id)
            .is_some_and(|current| *current == (result.match_id(), result.result_key()));
        if !exact {
            return Err(StrokeMatchError::IdentityMismatch);
        }
        let handle = self
            .rooms
            .get(&room_id)
            .map(|record| record.handle.clone())
            .ok_or(StrokeMatchError::Closed)?;
        let committed = handle.apply_stroke_commit(result).await?;
        self.deactivate_stroke(room_id, committed.match_id(), committed.result_key());
        Ok(committed)
    }

    async fn acknowledge_stroke_abort(
        &mut self,
        room_id: RoomId,
        abort: AbortStrokeMatch,
    ) -> Result<(), StrokeMatchError> {
        let exact = self
            .active_stroke
            .get(&room_id)
            .is_some_and(|current| *current == (abort.match_id(), abort.result_key()));
        if !exact {
            return Err(StrokeMatchError::IdentityMismatch);
        }
        let handle = self
            .rooms
            .get(&room_id)
            .map(|record| record.handle.clone())
            .ok_or(StrokeMatchError::Closed)?;
        handle.acknowledge_stroke_abort(abort).await?;
        self.deactivate_stroke(room_id, abort.match_id(), abort.result_key());
        Ok(())
    }

    fn process_event(&mut self, event: RoomActorEvent) {
        match event {
            RoomActorEvent::Summary(summary) => {
                if let Some(record) = self.rooms.get_mut(&summary.id()) {
                    record.summary = summary;
                }
            }
            RoomActorEvent::Closed(room_id) => {
                self.remove_room(room_id, true);
            }
        }
    }

    async fn shutdown(&mut self) -> Result<LobbyShutdownOutcome, LobbyShutdownError> {
        let max_rooms = self.limits.max_rooms.get();
        if self.rooms.len() > max_rooms {
            return Err(LobbyShutdownError::CapacityInvariant);
        }
        let mut aborts = Vec::new();
        aborts
            .try_reserve_exact(max_rooms)
            .map_err(|_| LobbyShutdownError::Allocation)?;
        let mut stroke = Vec::new();
        stroke
            .try_reserve_exact(max_rooms)
            .map_err(|_| LobbyShutdownError::Allocation)?;

        // Futures are inserted in ascending room-ID order and polled concurrently. Ordered output
        // keeps the persisted handoff deterministic without an unbounded intermediate Vec.
        let mut shutdowns = FuturesOrdered::new();
        for (room_id, record) in &self.rooms {
            let room_id = *room_id;
            let handle = record.handle.clone();
            shutdowns.push_back(async move { (room_id, handle.shutdown().await) });
        }
        let mut first_error = None;
        while let Some((room_id, result)) = shutdowns.next().await {
            match result {
                Ok(RoomCloseOutcome::M5Abort { request: abort, .. })
                    if aborts.len() < max_rooms =>
                {
                    self.prepared_matches.remove(&room_id);
                    self.deactivate_aborted_match(room_id, abort);
                    aborts.push(abort);
                }
                Ok(RoomCloseOutcome::M6Abort { request, .. }) if stroke.len() < max_rooms => {
                    self.prepared_stroke.remove(&room_id);
                    self.deactivate_stroke(room_id, request.match_id(), request.result_key());
                    stroke.push(LobbyStrokePersistence::Abort { room_id, request });
                }
                Ok(RoomCloseOutcome::M6Settlement { request, .. }) if stroke.len() < max_rooms => {
                    self.prepared_stroke.remove(&room_id);
                    self.deactivate_stroke(room_id, request.match_id(), request.result_key());
                    stroke.push(LobbyStrokePersistence::Settlement { room_id, request });
                }
                Ok(
                    RoomCloseOutcome::M5Abort { .. }
                    | RoomCloseOutcome::M6Abort { .. }
                    | RoomCloseOutcome::M6Settlement { .. },
                ) => {
                    first_error = Some(LobbyShutdownError::CapacityInvariant);
                }
                Ok(RoomCloseOutcome::None) => {}
                Err(error) if first_error.is_none() => {
                    first_error = Some(LobbyShutdownError::Room(error));
                }
                Err(_) => {}
            }
        }
        while let Some(room_id) = self.rooms.keys().next().copied() {
            self.remove_room(room_id, true);
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(LobbyShutdownOutcome { aborts, stroke })
    }
}

fn map_room_to_stroke(error: RoomError) -> StrokeMatchError {
    match error {
        RoomError::NotMember => StrokeMatchError::NotMember,
        RoomError::QueueFull => StrokeMatchError::QueueFull,
        RoomError::Timeout => StrokeMatchError::Timeout,
        _ => StrokeMatchError::Closed,
    }
}

fn map_room_to_solo(error: RoomError) -> SoloMatchError {
    match error {
        RoomError::NotMember => SoloMatchError::NotMember,
        RoomError::QueueFull => SoloMatchError::QueueFull,
        RoomError::Timeout => SoloMatchError::Timeout,
        _ => SoloMatchError::Closed,
    }
}

/// Starts the sole bounded lobby registry actor.
#[must_use]
pub fn spawn_lobby(limits: LobbyLimits) -> LobbyHandle {
    let (commands, command_rx) = mpsc::channel(limits.command_capacity.get());
    let (controls, control_rx) = mpsc::channel(limits.cleanup_capacity.get());
    let (events, event_rx) = mpsc::channel(limits.event_capacity.get());
    let (room_lifecycle, _) = broadcast::channel(limits.max_rooms.get());
    let (match_lifecycle, _) = broadcast::channel(limits.max_rooms.get());
    let handle = LobbyHandle {
        commands,
        controls,
        command_timeout: limits.command_timeout,
        shutdown_timeout: limits.shutdown_timeout,
        room_lifecycle: room_lifecycle.clone(),
        match_lifecycle: match_lifecycle.clone(),
    };
    let registry = LobbyRegistry {
        limits,
        rooms: BTreeMap::new(),
        connections: HashMap::new(),
        next_room_id: 1,
        events,
        room_lifecycle,
        match_lifecycle,
        prepared_matches: BTreeMap::new(),
        active_matches: BTreeMap::new(),
        prepared_stroke: BTreeMap::new(),
        active_stroke: BTreeMap::new(),
    };
    tokio::spawn(run_lobby(registry, command_rx, control_rx, event_rx));
    handle
}

fn begin<T>(gated: GatedCommand<T>) -> Option<T> {
    gated
        .gate
        .compare_exchange(
            COMMAND_PENDING,
            COMMAND_EXECUTING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .ok()
        .map(|_| gated.command)
}

async fn run_lobby(
    mut registry: LobbyRegistry,
    mut commands: mpsc::Receiver<GatedCommand<LobbyCommand>>,
    mut controls: mpsc::Receiver<GatedCommand<LobbyControl>>,
    mut events: mpsc::Receiver<RoomActorEvent>,
) {
    let mut open = true;
    while open {
        tokio::select! {
            biased;
            control = controls.recv() => match control.and_then(begin) {
                Some(LobbyControl::Disconnect { connection_id, reason, reply }) => {
                    let result = registry.disconnect_connection(connection_id, reason).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyControl::Shutdown { reply }) => {
                    let result = registry.shutdown().await;
                    let _ignored = reply.send(result);
                    open = false;
                }
                None if controls.is_closed() => {
                    let _ignored = registry.shutdown().await;
                    open = false;
                }
                None => {}
            },
            event = events.recv() => {
                if let Some(event) = event {
                    registry.process_event(event);
                }
            }
            command = commands.recv() => match command.and_then(begin) {
                Some(LobbyCommand::Create { name, password, settings, owner, outbound, cancellation, reply }) => {
                    let result = registry.create(name, password, settings, owner, outbound, cancellation).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::List { reply }) => {
                    let summaries = registry.rooms.values().map(|record| record.summary.clone()).collect();
                    let _ignored = reply.send(summaries);
                }
                Some(LobbyCommand::Join { room_id, identity, password, outbound, cancellation, reply }) => {
                    let result = registry.join(room_id, identity, password, outbound, cancellation).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::Leave { connection_id, reply }) => {
                    let result = registry.leave_connection(connection_id).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::Route { connection_id, command, reply }) => {
                    let result = registry.route(connection_id, command).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::RouteSolo { connection_id, command, reply }) => {
                    let result = registry.route_solo(connection_id, command).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::RouteStroke { connection_id, command, reply }) => {
                    let result = registry.route_stroke(connection_id, command).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::ApplyStrokeInGame { room_id, mark, reply }) => {
                    let result = registry.apply_stroke_in_game(room_id, mark).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::ApplyStrokeCommit { room_id, result, reply }) => {
                    let result = registry.apply_stroke_commit(room_id, result).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::AcknowledgeStrokeAbort { room_id, abort, reply }) => {
                    let result = registry.acknowledge_stroke_abort(room_id, abort).await;
                    let _ignored = reply.send(result);
                }
                #[cfg(test)]
                Some(LobbyCommand::AbortRoom { room_id, reply }) => {
                    let result = registry.rooms.get(&room_id).map_or(Err(RoomError::RoomNotFound), |record| {
                        record.handle.abort_actor_for_test();
                        Ok(())
                    });
                    let _ignored = reply.send(result);
                }
                #[cfg(test)]
                Some(LobbyCommand::CreateAfterRelease { name, settings, owner, outbound, cancellation, started, release, reply }) => {
                    let _ignored = started.send(());
                    let result = if release.await.is_ok() {
                        registry.create(name, None, settings, owner, outbound, cancellation).await
                    } else {
                        Err(RoomError::Closed)
                    };
                    let _ignored = reply.send(result);
                }
                None if commands.is_closed() => {
                    let _ignored = registry.shutdown().await;
                    open = false;
                }
                None => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::future::join_all;

    use super::*;
    use pangya_domain::{
        AccountId, CatalogFingerprint, CourseId, MatchSeed, MemberSnapshot, Nickname,
        OneHoleConfig, ServerBalances, StrokeParticipant, StrokePlayerResult, StrokeRosterOrder,
        synthetic_stroke_reward_v1,
    };
    use uuid::Uuid;

    use crate::match_state::deterministic_conditions;

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).unwrap_or(NonZeroUsize::MIN)
    }

    fn id(value: u64) -> PlayerConnectionId {
        PlayerConnectionId::new(value).unwrap_or_else(|_| unreachable!())
    }

    fn identity(value: u64) -> RoomIdentity {
        RoomIdentity {
            connection_id: id(value),
            account_id: AccountId::new(i64::try_from(value).unwrap_or(1))
                .unwrap_or_else(|_| unreachable!()),
            nickname: Nickname::parse(&format!("Player{value}")).unwrap_or_else(|_| unreachable!()),
        }
    }

    fn solo_plan(account_id: AccountId, timeout: Duration) -> SoloStartPlan {
        let seed = MatchSeed::new([0; 32]);
        let (weather, wind) = deterministic_conditions(seed).unwrap_or_else(|_| unreachable!());
        SoloStartPlan::new(
            BeginSoloMatch::new(
                MatchId::new(Uuid::from_u128(201)),
                MatchResultKey::new(Uuid::from_u128(202)),
                account_id,
                OneHoleConfig::new(CourseId::new(1).unwrap_or_else(|_| unreachable!()), 4)
                    .unwrap_or_else(|_| unreachable!()),
                CatalogFingerprint::new([3; 32]),
                seed,
                weather,
                wind,
            ),
            timeout,
            3,
        )
        .unwrap_or_else(|_| unreachable!())
    }

    fn stroke_plan(first: &RoomIdentity, second: &RoomIdentity) -> StrokeStartPlan {
        let seed = MatchSeed::new([0; 32]);
        let (weather, wind) = deterministic_conditions(seed).unwrap_or_else(|_| unreachable!());
        let begin = BeginStrokeMatch::new(
            MatchId::new(Uuid::from_u128(301)),
            MatchResultKey::new(Uuid::from_u128(302)),
            [
                StrokeParticipant::new(
                    first.account_id,
                    StrokeRosterOrder::First,
                    MatchResultKey::new(Uuid::from_u128(303)),
                ),
                StrokeParticipant::new(
                    second.account_id,
                    StrokeRosterOrder::Second,
                    MatchResultKey::new(Uuid::from_u128(304)),
                ),
            ],
            OneHoleConfig::new(CourseId::new(1).unwrap_or_else(|_| unreachable!()), 4)
                .unwrap_or_else(|_| unreachable!()),
            CatalogFingerprint::new([3; 32]),
            seed,
            weather,
            wind,
        )
        .unwrap_or_else(|_| unreachable!());
        StrokeStartPlan::new(
            begin,
            [first.connection_id, second.connection_id],
            Duration::from_secs(5),
            Duration::from_secs(5),
            Duration::from_secs(30),
            3,
        )
        .unwrap_or_else(|_| unreachable!())
    }

    fn limits(max_rooms: usize) -> LobbyLimits {
        LobbyLimits::new(
            nonzero(max_rooms),
            nonzero(128),
            nonzero(129),
            nonzero(128),
            Duration::from_secs(2),
            Duration::from_secs(2),
            RoomActorLimits::default(),
        )
    }

    async fn create(lobby: &LobbyHandle, owner: u64, capacity: u8) -> RoomSummary {
        let (tx, _rx) = mpsc::channel(32);
        lobby
            .create(
                RoomName::parse(&format!("room{owner}")).unwrap_or_else(|_| unreachable!()),
                None,
                RoomSettings::new(capacity).unwrap_or_else(|_| unreachable!()),
                identity(owner),
                tx,
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|_| unreachable!())
    }

    #[tokio::test]
    async fn registry_enforces_room_cap_unique_ids_and_one_room_per_connection() {
        let lobby = spawn_lobby(limits(2));
        let first = create(&lobby, 1, 3).await;
        let second = create(&lobby, 2, 3).await;
        assert_ne!(first.id(), second.id());
        let (tx, _rx) = mpsc::channel(8);
        assert_eq!(
            lobby
                .create(
                    RoomName::parse("third").unwrap_or_else(|_| unreachable!()),
                    None,
                    RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                    identity(3),
                    tx.clone(),
                    CancellationToken::new(),
                )
                .await,
            Err(RoomError::MaxRooms)
        );
        assert_eq!(
            lobby
                .join(second.id(), identity(1), None, tx, CancellationToken::new(),)
                .await,
            Err(RoomError::AlreadyMember)
        );
        let listed = lobby.list().await.unwrap_or_default();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id(), first.id());
        assert!(lobby.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn concurrent_joins_never_exceed_capacity_and_summary_tracks_mutations() {
        let lobby = spawn_lobby(limits(4));
        let room = create(&lobby, 1, 2).await;
        let room_id = room.id();
        let mut tasks = Vec::new();
        for connection in 2..=12 {
            let lobby = lobby.clone();
            tasks.push(tokio::spawn(async move {
                let (tx, _rx) = mpsc::channel(8);
                lobby
                    .join(
                        room_id,
                        identity(connection),
                        None,
                        tx,
                        CancellationToken::new(),
                    )
                    .await
            }));
        }
        let results = join_all(tasks).await;
        let admitted = results
            .into_iter()
            .filter(|result| matches!(result, Ok(Ok(_))))
            .count();
        assert_eq!(admitted, 1);
        let listed = lobby.list().await.unwrap_or_default();
        assert_eq!(listed.first().map(RoomSummary::members), Some(2));
        assert!(
            listed
                .first()
                .is_some_and(|summary| summary.members() <= summary.max_members())
        );
        assert!(lobby.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn routing_kick_transfer_disconnect_and_failed_room_isolation() {
        let lobby = spawn_lobby(limits(4));
        let first = create(&lobby, 1, 3).await;
        let second = create(&lobby, 10, 2).await;
        let (tx, _rx) = mpsc::channel(32);
        assert!(
            lobby
                .join(first.id(), identity(2), None, tx, CancellationToken::new(),)
                .await
                .is_ok()
        );
        assert_eq!(
            lobby.route(id(2), LobbyRoomCommand::Kick(id(1))).await,
            Err(RoomError::NotOwner)
        );
        let kicked = lobby.route(id(1), LobbyRoomCommand::Kick(id(2))).await;
        assert!(matches!(kicked, Ok(LobbyRouteResult::Snapshot(_))));
        assert_eq!(lobby.leave(id(2)).await, Err(RoomError::NotMember));
        assert!(lobby.abort_room_for_test(first.id()).await.is_ok());
        let mut listed = lobby.list().await.unwrap_or_default();
        for _ in 0..16 {
            if listed.len() == 1 {
                break;
            }
            tokio::task::yield_now().await;
            listed = lobby.list().await.unwrap_or_default();
        }
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id(), second.id());
        assert!(
            lobby
                .route(id(10), LobbyRoomCommand::SetReady(true))
                .await
                .is_ok()
        );
        assert!(lobby.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn noisy_room_saturation_cancels_only_the_affected_connection() {
        let lobby = spawn_lobby(limits(2));
        let affected = CancellationToken::new();
        let unrelated = CancellationToken::new();
        let (affected_tx, _affected_rx) = mpsc::channel(1);
        let room = lobby
            .create(
                RoomName::parse("bounded").unwrap_or_else(|_| unreachable!()),
                None,
                RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                identity(1),
                affected_tx,
                affected.clone(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let (unrelated_tx, _unrelated_rx) = mpsc::channel(8);
        let _other = lobby
            .create(
                RoomName::parse("quiet").unwrap_or_else(|_| unreachable!()),
                None,
                RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                identity(2),
                unrelated_tx,
                unrelated.clone(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(
            lobby
                .route(id(1), LobbyRoomCommand::SetReady(true))
                .await
                .is_ok()
        );
        assert!(
            lobby
                .route(id(1), LobbyRoomCommand::SetReady(false))
                .await
                .is_ok()
        );
        assert!(affected.is_cancelled());
        assert!(!unrelated.is_cancelled());
        assert!(lobby.disconnect(id(1)).await.is_ok());
        assert_eq!(room.members(), 1);
        assert!(lobby.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn owner_transfer_is_reflected_in_registry_summary() {
        let lobby = spawn_lobby(limits(2));
        let room = create(&lobby, 1, 3).await;
        let (tx, _rx) = mpsc::channel(32);
        assert!(
            lobby
                .join(
                    room.id(),
                    identity(3),
                    None,
                    tx.clone(),
                    CancellationToken::new(),
                )
                .await
                .is_ok()
        );
        assert!(
            lobby
                .join(room.id(), identity(2), None, tx, CancellationToken::new(),)
                .await
                .is_ok()
        );
        assert!(lobby.leave(id(1)).await.is_ok());
        let listed = lobby.list().await.unwrap_or_default();
        assert_eq!(
            listed.first().map(RoomSummary::owner_nickname),
            Some("Player3")
        );
        assert!(lobby.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn queued_command_cancels_without_mutation_and_begun_command_commits() {
        let mut policy = limits(4);
        policy.command_timeout = Duration::from_millis(20);
        let lobby = spawn_lobby(policy);
        let (outbound, _events) = mpsc::channel(8);
        let cancellation = CancellationToken::new();
        let (started, began) = oneshot::channel();
        let (release, continue_execution) = oneshot::channel();
        let (reply, receive) = oneshot::channel();
        let gate = LobbyHandle::new_gate();
        assert!(
            lobby
                .send(
                    LobbyCommand::CreateAfterRelease {
                        name: RoomName::parse("begun").unwrap_or_else(|_| unreachable!()),
                        settings: RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                        owner: identity(1),
                        outbound,
                        cancellation,
                        started,
                        release: continue_execution,
                        reply,
                    },
                    Arc::clone(&gate),
                )
                .is_ok()
        );
        let waiting = tokio::spawn(async move {
            LobbyHandle::await_reply(&gate, receive, Duration::from_millis(20)).await
        });
        assert!(began.await.is_ok());

        let (cancelled_outbound, _cancelled_events) = mpsc::channel(8);
        assert_eq!(
            lobby
                .create(
                    RoomName::parse("cancelled").unwrap_or_else(|_| unreachable!()),
                    None,
                    RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                    identity(2),
                    cancelled_outbound,
                    CancellationToken::new(),
                )
                .await,
            Err(RoomError::Timeout)
        );
        assert!(release.send(()).is_ok());
        let committed = waiting
            .await
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(committed.members(), 1);
        let listed = lobby.list().await.unwrap_or_default();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id(), committed.id());
        assert!(lobby.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn priority_disconnect_reason_survives_saturated_normal_queue() {
        let mut policy = limits(4);
        policy.command_capacity = nonzero(2);
        policy.command_timeout = Duration::from_millis(100);
        let lobby = spawn_lobby(policy);
        let room = create(&lobby, 1, 2).await;
        let plan = solo_plan(identity(1).account_id, Duration::from_secs(1));
        assert!(matches!(
            lobby
                .route_solo(id(1), LobbySoloCommand::PrepareStart(plan.clone()))
                .await,
            Ok(LobbySoloRouteResult::Begin(_))
        ));
        assert_eq!(
            lobby
                .route_solo(
                    id(1),
                    LobbySoloCommand::ConfirmBegin {
                        match_id: plan.begin().match_id(),
                        result_key: plan.begin().result_key(),
                    },
                )
                .await,
            Ok(LobbySoloRouteResult::Applied)
        );
        let (outbound, _events) = mpsc::channel(8);
        let (started, began) = oneshot::channel();
        let (release, continue_execution) = oneshot::channel();
        let (reply, _receive) = oneshot::channel();
        let gate = LobbyHandle::new_gate();
        assert!(
            lobby
                .send(
                    LobbyCommand::CreateAfterRelease {
                        name: RoomName::parse("blocker").unwrap_or_else(|_| unreachable!()),
                        settings: RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                        owner: identity(2),
                        outbound,
                        cancellation: CancellationToken::new(),
                        started,
                        release: continue_execution,
                        reply,
                    },
                    gate,
                )
                .is_ok()
        );
        assert!(began.await.is_ok());
        for _ in 0..2 {
            let (reply, _receive) = oneshot::channel();
            assert!(
                lobby
                    .send(LobbyCommand::List { reply }, LobbyHandle::new_gate())
                    .is_ok()
            );
        }
        let (reply, _receive) = oneshot::channel();
        assert_eq!(
            lobby.send(LobbyCommand::List { reply }, LobbyHandle::new_gate()),
            Err(RoomError::QueueFull)
        );
        let disconnect = {
            let lobby = lobby.clone();
            tokio::spawn(async move {
                lobby
                    .disconnect_with_reason(id(1), MatchAbortReason::Shutdown)
                    .await
            })
        };
        tokio::task::yield_now().await;
        assert!(release.send(()).is_ok());
        let abort = disconnect
            .await
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|| unreachable!());
        assert_eq!(abort.match_id(), plan.begin().match_id());
        assert_eq!(abort.reason(), MatchAbortReason::Shutdown);
        let listed = lobby.list().await.unwrap_or_default();
        assert!(listed.iter().all(|summary| summary.id() != room.id()));
        assert!(lobby.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn lifecycle_is_ordered_and_aborted_actor_is_isolated() {
        let lobby = spawn_lobby(limits(4));
        let owner = CancellationToken::new();
        let member = CancellationToken::new();
        let unrelated = CancellationToken::new();
        let (tx, _rx) = mpsc::channel(16);
        let other = lobby
            .create(
                RoomName::parse("survivor").unwrap_or_else(|_| unreachable!()),
                None,
                RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                identity(3),
                tx.clone(),
                unrelated.clone(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let mut lifecycle = lobby.subscribe_room_lifecycle();
        let room = lobby
            .create(
                RoomName::parse("failed").unwrap_or_else(|_| unreachable!()),
                None,
                RoomSettings::new(3).unwrap_or_else(|_| unreachable!()),
                identity(1),
                tx.clone(),
                owner.clone(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            lifecycle.recv().await,
            Ok(RoomLifecycle {
                event: RoomLifecycleEvent::Created,
                active_count: 2,
            })
        );
        assert!(
            lobby
                .join(room.id(), identity(2), None, tx, member.clone())
                .await
                .is_ok()
        );
        assert!(lobby.abort_room_for_test(room.id()).await.is_ok());
        assert_eq!(
            timeout(Duration::from_secs(1), lifecycle.recv()).await,
            Ok(Ok(RoomLifecycle {
                event: RoomLifecycleEvent::Closed,
                active_count: 1,
            }))
        );
        assert!(owner.is_cancelled());
        assert!(member.is_cancelled());
        assert!(!unrelated.is_cancelled());
        assert_eq!(lobby.list().await.unwrap_or_default(), vec![other]);
        assert!(lobby.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn active_match_isolated_to_its_room_and_shutdown_returns_saturated_abort() {
        let lobby = spawn_lobby(limits(2));
        let saturated = CancellationToken::new();
        let unrelated = CancellationToken::new();
        let (active_tx, _active_rx) = mpsc::channel(1);
        let active = lobby
            .create(
                RoomName::parse("active").unwrap_or_else(|_| unreachable!()),
                None,
                RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                identity(1),
                active_tx,
                saturated.clone(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let (other_tx, _other_rx) = mpsc::channel(8);
        let other = lobby
            .create(
                RoomName::parse("other").unwrap_or_else(|_| unreachable!()),
                None,
                RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                identity(2),
                other_tx,
                unrelated.clone(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let mut match_lifecycle = lobby.subscribe_match_lifecycle();
        let plan = solo_plan(identity(1).account_id, Duration::from_secs(5));
        assert!(matches!(
            lobby
                .route_solo(id(1), LobbySoloCommand::PrepareStart(plan.clone()))
                .await,
            Ok(LobbySoloRouteResult::Begin(_))
        ));
        assert_eq!(
            lobby
                .route_solo(
                    id(1),
                    LobbySoloCommand::ConfirmBegin {
                        match_id: plan.begin().match_id(),
                        result_key: plan.begin().result_key(),
                    },
                )
                .await,
            Ok(LobbySoloRouteResult::Applied)
        );
        assert_eq!(
            timeout(Duration::from_secs(1), match_lifecycle.recv()).await,
            Ok(Ok(MatchLifecycle {
                event: MatchLifecycleEvent::Activated,
                active_count: 1,
            }))
        );
        assert!(saturated.is_cancelled());
        assert!(!unrelated.is_cancelled());
        assert_eq!(
            lobby.route(id(1), LobbyRoomCommand::SetReady(true)).await,
            Err(RoomError::MatchActive)
        );
        assert!(matches!(
            lobby.route(id(2), LobbyRoomCommand::SetReady(true)).await,
            Ok(LobbyRouteResult::Snapshot(_))
        ));
        assert_ne!(active.id(), other.id());

        let outcome = lobby.shutdown().await.unwrap_or_else(|_| unreachable!());
        assert_eq!(
            outcome.aborts(),
            &[AbortMatch::new(
                plan.begin().match_id(),
                plan.begin().result_key(),
                plan.begin().account_id(),
                MatchAbortReason::Shutdown,
            )]
        );
        assert!(outcome.aborts().len() <= 2);
        assert_eq!(
            timeout(Duration::from_secs(1), match_lifecycle.recv()).await,
            Ok(Ok(MatchLifecycle {
                event: MatchLifecycleEvent::Deactivated,
                active_count: 0,
            }))
        );
        assert!(matches!(
            match_lifecycle.try_recv(),
            Err(broadcast::error::TryRecvError::Closed | broadcast::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn stroke_disconnect_returns_settlement_and_apply_by_room_survives_mapping_removal() {
        let lobby = spawn_lobby(limits(3));
        let first = identity(1);
        let second = identity(2);
        let (first_tx, _first_rx) = mpsc::channel(64);
        let (second_tx, _second_rx) = mpsc::channel(64);
        let room = lobby
            .create(
                RoomName::parse("stroke-room").unwrap_or_else(|_| unreachable!()),
                None,
                RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                first.clone(),
                first_tx,
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        assert!(
            lobby
                .join(
                    room.id(),
                    second.clone(),
                    None,
                    second_tx,
                    CancellationToken::new()
                )
                .await
                .is_ok()
        );
        assert!(
            lobby
                .route(first.connection_id, LobbyRoomCommand::SetReady(true))
                .await
                .is_ok()
        );
        assert!(
            lobby
                .route(second.connection_id, LobbyRoomCommand::SetReady(true))
                .await
                .is_ok()
        );
        let plan = stroke_plan(&first, &second);
        assert!(matches!(
            lobby
                .route_stroke(
                    first.connection_id,
                    LobbyStrokeCommand::PrepareStart(plan.clone())
                )
                .await,
            Ok(LobbyStrokeRouteResult::Begin(_))
        ));
        let mut lifecycle = lobby.subscribe_match_lifecycle();
        assert_eq!(
            lobby
                .route_stroke(
                    first.connection_id,
                    LobbyStrokeCommand::ConfirmBegin {
                        match_id: plan.begin().match_id(),
                        result_key: plan.begin().result_key(),
                    },
                )
                .await,
            Ok(LobbyStrokeRouteResult::Applied)
        );
        assert_eq!(
            lifecycle.recv().await.map(|value| value.active_count),
            Ok(1)
        );
        let loading = StrokeLoadingComplete::new(100).unwrap_or_else(|_| unreachable!());
        assert_eq!(
            lobby
                .route_stroke(
                    second.connection_id,
                    LobbyStrokeCommand::LoadingComplete(loading)
                )
                .await,
            Ok(LobbyStrokeRouteResult::Loading(
                StrokeLoadingOutcome::Waiting
            ))
        );
        let mark = match lobby
            .route_stroke(
                first.connection_id,
                LobbyStrokeCommand::LoadingComplete(loading),
            )
            .await
        {
            Ok(LobbyStrokeRouteResult::Loading(StrokeLoadingOutcome::PersistenceRequired(
                mark,
            ))) => mark,
            _ => unreachable!(),
        };
        assert!(
            lobby
                .apply_stroke_in_game_by_room(room.id(), mark)
                .await
                .is_ok()
        );
        let outcome = lobby
            .disconnect_with_work(first.connection_id, MatchAbortReason::Disconnect)
            .await
            .unwrap_or_else(|_| unreachable!());
        let RoomCloseOutcome::M6Settlement {
            room_id,
            request: commit,
        } = outcome
        else {
            unreachable!()
        };
        assert_eq!(room_id, room.id());
        assert_eq!(
            lobby
                .route(first.connection_id, LobbyRoomCommand::GetState)
                .await,
            Err(RoomError::NotMember)
        );
        let results = [0_usize, 1_usize].map(|index| {
            let input = commit.players()[index];
            let reward =
                synthetic_stroke_reward_v1(commit.config(), input.strokes(), input.completion())
                    .unwrap_or_else(|_| unreachable!());
            StrokePlayerResult::new(input, reward, ServerBalances::from_persisted(100, 100))
        });
        let result = StrokeMatchResult::new(commit.match_id(), commit.result_key(), results);
        assert_eq!(
            lobby.apply_stroke_commit_by_room(room.id(), result).await,
            Ok(result)
        );
        assert_eq!(
            lifecycle.recv().await.map(|value| value.active_count),
            Ok(0)
        );
        assert!(
            lobby
                .route(second.connection_id, LobbyRoomCommand::GetState)
                .await
                .is_ok()
        );
        assert!(lobby.shutdown().await.is_ok());
    }

    #[test]
    fn snapshots_have_exactly_one_owner_and_unique_connections() {
        let members = [
            MemberSnapshot::new(
                id(1),
                AccountId::new(1).unwrap_or_else(|_| unreachable!()),
                "one".into(),
                true,
                false,
            ),
            MemberSnapshot::new(
                id(2),
                AccountId::new(2).unwrap_or_else(|_| unreachable!()),
                "two".into(),
                false,
                true,
            ),
        ];
        assert_eq!(members.iter().filter(|member| member.is_owner()).count(), 1);
        assert_ne!(members[0].connection_id(), members[1].connection_id());
    }
}
