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

use futures_util::future::join_all;
use pangya_domain::{
    ChatText, PlayerConnectionId, RoomError, RoomId, RoomName, RoomPassword, RoomSettings,
    RoomSnapshot, RoomSummary,
};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

use crate::room::{
    RoomActorEvent, RoomActorLimits, RoomEvent, RoomHandle, RoomIdentity, spawn_room_with_events,
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
        reply: oneshot::Sender<Result<Option<RoomSnapshot>, RoomError>>,
    },
    Shutdown {
        reply: oneshot::Sender<Result<(), RoomError>>,
    },
}

/// Cloneable endpoint for the sole lobby registry task.
#[derive(Clone)]
pub struct LobbyHandle {
    commands: mpsc::Sender<GatedCommand<LobbyCommand>>,
    controls: mpsc::Sender<GatedCommand<LobbyControl>>,
    command_timeout: Duration,
    shutdown_timeout: Duration,
    room_closures: broadcast::Sender<RoomId>,
}

impl std::fmt::Debug for LobbyHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LobbyHandle")
            .finish_non_exhaustive()
    }
}

impl LobbyHandle {
    pub(crate) fn subscribe_room_closures(&self) -> broadcast::Receiver<RoomId> {
        self.room_closures.subscribe()
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
    ) -> Result<Option<RoomSnapshot>, RoomError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send_control(
            LobbyControl::Disconnect {
                connection_id,
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

    /// Drains all rooms within one bounded registry shutdown deadline.
    pub async fn shutdown(&self) -> Result<(), RoomError> {
        let (reply, receive) = oneshot::channel();
        let gate = Self::new_gate();
        self.send_control(
            LobbyControl::Shutdown { reply },
            Arc::clone(&gate),
            self.shutdown_timeout,
        )
        .await?;
        Self::await_reply(&gate, receive, self.shutdown_timeout).await?
    }
}

struct LobbyRegistry {
    limits: LobbyLimits,
    rooms: BTreeMap<RoomId, RoomRecord>,
    connections: HashMap<PlayerConnectionId, ConnectionRecord>,
    next_room_id: u32,
    events: mpsc::Sender<RoomActorEvent>,
    room_closures: broadcast::Sender<RoomId>,
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

    fn remove_room(&mut self, room_id: RoomId, cancel_members: bool) {
        self.rooms.remove(&room_id);
        self.connections.retain(|_, connection| {
            if connection.room_id != room_id {
                return true;
            }
            if cancel_members {
                connection.cancellation.cancel();
            }
            false
        });
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

    async fn remove_connection(
        &mut self,
        connection_id: PlayerConnectionId,
        disconnect: bool,
    ) -> Result<Option<RoomSnapshot>, RoomError> {
        let room_id = self.room_for(connection_id)?;
        let handle = self
            .rooms
            .get(&room_id)
            .map(|record| record.handle.clone())
            .ok_or(RoomError::RoomNotFound)?;
        let result = if disconnect {
            handle.disconnect(connection_id).await
        } else {
            handle.leave(connection_id).await
        };
        match result {
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

    fn process_event(&mut self, event: RoomActorEvent) {
        match event {
            RoomActorEvent::Summary(summary) => {
                if let Some(record) = self.rooms.get_mut(&summary.id()) {
                    record.summary = summary;
                }
            }
            RoomActorEvent::Closed(room_id) => {
                self.remove_room(room_id, true);
                let _no_receivers = self.room_closures.send(room_id);
            }
        }
    }

    async fn shutdown(&mut self) -> Result<(), RoomError> {
        let handles: Vec<_> = self
            .rooms
            .values()
            .map(|record| record.handle.clone())
            .collect();
        let result = join_all(handles.iter().map(RoomHandle::shutdown)).await;
        self.rooms.clear();
        self.connections.clear();
        if result.into_iter().any(|result| result.is_err()) {
            return Err(RoomError::Closed);
        }
        Ok(())
    }
}

/// Starts the sole bounded lobby registry actor.
#[must_use]
pub fn spawn_lobby(limits: LobbyLimits) -> LobbyHandle {
    let (commands, command_rx) = mpsc::channel(limits.command_capacity.get());
    let (controls, control_rx) = mpsc::channel(limits.cleanup_capacity.get());
    let (events, event_rx) = mpsc::channel(limits.event_capacity.get());
    let (room_closures, _) = broadcast::channel(limits.max_rooms.get());
    let handle = LobbyHandle {
        commands,
        controls,
        command_timeout: limits.command_timeout,
        shutdown_timeout: limits.shutdown_timeout,
        room_closures: room_closures.clone(),
    };
    let registry = LobbyRegistry {
        limits,
        rooms: BTreeMap::new(),
        connections: HashMap::new(),
        next_room_id: 1,
        events,
        room_closures,
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
                Some(LobbyControl::Disconnect { connection_id, reply }) => {
                    let result = registry.remove_connection(connection_id, true).await;
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
                    let result = registry.remove_connection(connection_id, false).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::Route { connection_id, command, reply }) => {
                    let result = registry.route(connection_id, command).await;
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
    use super::*;
    use pangya_domain::{AccountId, MemberSnapshot, Nickname};

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
    async fn priority_disconnect_survives_saturated_normal_queue() {
        let mut policy = limits(4);
        policy.command_capacity = nonzero(2);
        policy.command_timeout = Duration::from_millis(100);
        let lobby = spawn_lobby(policy);
        let room = create(&lobby, 1, 2).await;
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
            tokio::spawn(async move { lobby.disconnect(id(1)).await })
        };
        tokio::task::yield_now().await;
        assert!(release.send(()).is_ok());
        assert_eq!(
            disconnect.await.unwrap_or_else(|_| unreachable!()),
            Ok(None)
        );
        let listed = lobby.list().await.unwrap_or_default();
        assert!(listed.iter().all(|summary| summary.id() != room.id()));
        assert!(lobby.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn aborted_actor_cancels_only_its_members_and_publishes_one_closure() {
        let lobby = spawn_lobby(limits(4));
        let owner = CancellationToken::new();
        let member = CancellationToken::new();
        let unrelated = CancellationToken::new();
        let (tx, _rx) = mpsc::channel(16);
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
        assert!(
            lobby
                .join(room.id(), identity(2), None, tx.clone(), member.clone())
                .await
                .is_ok()
        );
        let other = lobby
            .create(
                RoomName::parse("survivor").unwrap_or_else(|_| unreachable!()),
                None,
                RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                identity(3),
                tx,
                unrelated.clone(),
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let mut closures = lobby.subscribe_room_closures();
        assert!(lobby.abort_room_for_test(room.id()).await.is_ok());
        assert_eq!(
            timeout(Duration::from_secs(1), closures.recv()).await,
            Ok(Ok(room.id()))
        );
        assert!(owner.is_cancelled());
        assert!(member.is_cancelled());
        assert!(!unrelated.is_cancelled());
        assert_eq!(lobby.list().await.unwrap_or_default(), vec![other]);
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
