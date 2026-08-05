//! Bounded lobby registry actor for process-local room discovery and admission.

use std::{
    collections::{BTreeMap, HashMap},
    num::NonZeroUsize,
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

use crate::room::{
    RoomActorEvent, RoomActorLimits, RoomEvent, RoomHandle, RoomIdentity, spawn_room_with_events,
};

/// Hard bounds for the lobby registry and each room it creates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LobbyLimits {
    max_rooms: NonZeroUsize,
    command_capacity: NonZeroUsize,
    event_capacity: NonZeroUsize,
    shutdown_timeout: Duration,
    room: RoomActorLimits,
}

impl LobbyLimits {
    /// Creates a fully bounded policy.
    #[must_use]
    pub const fn new(
        max_rooms: NonZeroUsize,
        command_capacity: NonZeroUsize,
        event_capacity: NonZeroUsize,
        shutdown_timeout: Duration,
        room: RoomActorLimits,
    ) -> Self {
        Self {
            max_rooms,
            command_capacity,
            event_capacity,
            shutdown_timeout,
            room,
        }
    }

    /// Returns whether nested capacities and deadlines are within production bounds.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.max_rooms.get() <= 65_536
            && self.command_capacity.get() <= 65_536
            && self.event_capacity.get() <= 65_536
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
            NonZeroUsize::new(256).unwrap_or(NonZeroUsize::MIN),
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

enum LobbyCommand {
    Create {
        name: RoomName,
        password: Option<RoomPassword>,
        settings: RoomSettings,
        owner: RoomIdentity,
        outbound: mpsc::Sender<RoomEvent>,
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
        reply: oneshot::Sender<Result<RoomSnapshot, RoomError>>,
    },
    Leave {
        connection_id: PlayerConnectionId,
        reply: oneshot::Sender<Result<Option<RoomSnapshot>, RoomError>>,
    },
    Disconnect {
        connection_id: PlayerConnectionId,
        reply: oneshot::Sender<Result<Option<RoomSnapshot>, RoomError>>,
    },
    Route {
        connection_id: PlayerConnectionId,
        command: LobbyRoomCommand,
        reply: oneshot::Sender<Result<LobbyRouteResult, RoomError>>,
    },
    Shutdown {
        reply: oneshot::Sender<Result<(), RoomError>>,
    },
    #[cfg(test)]
    AbortRoom {
        room_id: RoomId,
        reply: oneshot::Sender<Result<(), RoomError>>,
    },
}

/// Cloneable endpoint for the sole lobby registry task.
#[derive(Clone)]
pub struct LobbyHandle {
    commands: mpsc::Sender<LobbyCommand>,
    shutdown_timeout: Duration,
    queue_drops: broadcast::Sender<PlayerConnectionId>,
}

impl std::fmt::Debug for LobbyHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LobbyHandle")
            .finish_non_exhaustive()
    }
}

impl LobbyHandle {
    pub(crate) fn subscribe_queue_drops(&self) -> broadcast::Receiver<PlayerConnectionId> {
        self.queue_drops.subscribe()
    }

    fn send(&self, command: LobbyCommand) -> Result<(), RoomError> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => RoomError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => RoomError::Closed,
            })
    }

    /// Creates a room and atomically registers its owner connection.
    pub async fn create(
        &self,
        name: RoomName,
        password: Option<RoomPassword>,
        settings: RoomSettings,
        owner: RoomIdentity,
        outbound: mpsc::Sender<RoomEvent>,
    ) -> Result<RoomSummary, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send(LobbyCommand::Create {
            name,
            password,
            settings,
            owner,
            outbound,
            reply,
        })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Lists immutable summaries in ascending room-ID order.
    pub async fn list(&self) -> Result<Vec<RoomSummary>, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send(LobbyCommand::List { reply })?;
        receive.await.map_err(|_| RoomError::Closed)
    }

    /// Atomically admits a connection to one room only.
    pub async fn join(
        &self,
        room_id: RoomId,
        identity: RoomIdentity,
        password: Option<RoomPassword>,
        outbound: mpsc::Sender<RoomEvent>,
    ) -> Result<RoomSnapshot, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send(LobbyCommand::Join {
            room_id,
            identity,
            password,
            outbound,
            reply,
        })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Voluntarily leaves the connection's registered room.
    pub async fn leave(
        &self,
        connection_id: PlayerConnectionId,
    ) -> Result<Option<RoomSnapshot>, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send(LobbyCommand::Leave {
            connection_id,
            reply,
        })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Routes a dropped connection through the room's priority control queue.
    pub async fn disconnect(
        &self,
        connection_id: PlayerConnectionId,
    ) -> Result<Option<RoomSnapshot>, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send(LobbyCommand::Disconnect {
            connection_id,
            reply,
        })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Routes a command only to the caller's registered room.
    pub async fn route(
        &self,
        connection_id: PlayerConnectionId,
        command: LobbyRoomCommand,
    ) -> Result<LobbyRouteResult, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send(LobbyCommand::Route {
            connection_id,
            command,
            reply,
        })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    #[cfg(test)]
    async fn abort_room_for_test(&self, room_id: RoomId) -> Result<(), RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send(LobbyCommand::AbortRoom { room_id, reply })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Drains all rooms within one bounded registry shutdown deadline.
    pub async fn shutdown(&self) -> Result<(), RoomError> {
        let (reply, receive) = oneshot::channel();
        timeout(
            self.shutdown_timeout,
            self.commands.send(LobbyCommand::Shutdown { reply }),
        )
        .await
        .map_err(|_| RoomError::Timeout)?
        .map_err(|_| RoomError::Closed)?;
        timeout(self.shutdown_timeout, receive)
            .await
            .map_err(|_| RoomError::Timeout)?
            .map_err(|_| RoomError::Closed)?
    }
}

struct LobbyRegistry {
    limits: LobbyLimits,
    rooms: BTreeMap<RoomId, RoomRecord>,
    connections: HashMap<PlayerConnectionId, RoomId>,
    next_room_id: u32,
    events: mpsc::Sender<RoomActorEvent>,
    queue_drops: broadcast::Sender<PlayerConnectionId>,
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

    fn remove_room(&mut self, room_id: RoomId) {
        self.rooms.remove(&room_id);
        self.connections.retain(|_, assigned| *assigned != room_id);
    }

    fn update_snapshot(&mut self, room_id: RoomId, snapshot: &RoomSnapshot) {
        if let Some(record) = self.rooms.get_mut(&room_id) {
            record.summary = snapshot.summary().clone();
        }
    }

    fn room_for(&self, connection_id: PlayerConnectionId) -> Result<RoomId, RoomError> {
        self.connections
            .get(&connection_id)
            .copied()
            .ok_or(RoomError::NotMember)
    }

    async fn create(
        &mut self,
        name: RoomName,
        password: Option<RoomPassword>,
        settings: RoomSettings,
        owner: RoomIdentity,
        outbound: mpsc::Sender<RoomEvent>,
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
            self.limits.room,
            Some(self.events.clone()),
            Some(self.queue_drops.clone()),
        );
        self.rooms.insert(
            id,
            RoomRecord {
                handle,
                summary: summary.clone(),
            },
        );
        self.connections.insert(connection_id, id);
        Ok(summary)
    }

    async fn join(
        &mut self,
        room_id: RoomId,
        identity: RoomIdentity,
        password: Option<RoomPassword>,
        outbound: mpsc::Sender<RoomEvent>,
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
        match handle.join(identity, password, outbound).await {
            Ok(snapshot) => {
                self.connections.insert(connection_id, room_id);
                self.update_snapshot(room_id, &snapshot);
                Ok(snapshot)
            }
            Err(RoomError::Closed) => {
                self.remove_room(room_id);
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
                    self.remove_room(room_id);
                }
                Ok(snapshot)
            }
            Err(RoomError::Closed) => {
                self.remove_room(room_id);
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
                self.remove_room(room_id);
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
            RoomActorEvent::Closed(room_id) => self.remove_room(room_id),
        }
    }

    async fn shutdown(&mut self) -> Result<(), RoomError> {
        let handles: Vec<_> = self
            .rooms
            .values()
            .map(|record| record.handle.clone())
            .collect();
        let drain = join_all(handles.iter().map(RoomHandle::shutdown));
        let result = timeout(self.limits.shutdown_timeout, drain)
            .await
            .map_err(|_| RoomError::Timeout)?;
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
    let (events, event_rx) = mpsc::channel(limits.event_capacity.get());
    let (queue_drops, _) = broadcast::channel(limits.event_capacity.get());
    let handle = LobbyHandle {
        commands,
        shutdown_timeout: limits.shutdown_timeout,
        queue_drops: queue_drops.clone(),
    };
    let registry = LobbyRegistry {
        limits,
        rooms: BTreeMap::new(),
        connections: HashMap::new(),
        next_room_id: 1,
        events,
        queue_drops,
    };
    tokio::spawn(run_lobby(registry, command_rx, event_rx));
    handle
}

async fn run_lobby(
    mut registry: LobbyRegistry,
    mut commands: mpsc::Receiver<LobbyCommand>,
    mut events: mpsc::Receiver<RoomActorEvent>,
) {
    let mut open = true;
    while open {
        tokio::select! {
            command = commands.recv() => match command {
                Some(LobbyCommand::Create { name, password, settings, owner, outbound, reply }) => {
                    let result = registry.create(name, password, settings, owner, outbound).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::List { reply }) => {
                    let summaries = registry.rooms.values().map(|record| record.summary.clone()).collect();
                    let _ignored = reply.send(summaries);
                }
                Some(LobbyCommand::Join { room_id, identity, password, outbound, reply }) => {
                    let result = registry.join(room_id, identity, password, outbound).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::Leave { connection_id, reply }) => {
                    let result = registry.remove_connection(connection_id, false).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::Disconnect { connection_id, reply }) => {
                    let result = registry.remove_connection(connection_id, true).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::Route { connection_id, command, reply }) => {
                    let result = registry.route(connection_id, command).await;
                    let _ignored = reply.send(result);
                }
                Some(LobbyCommand::Shutdown { reply }) => {
                    let result = registry.shutdown().await;
                    let _ignored = reply.send(result);
                    open = false;
                }
                #[cfg(test)]
                Some(LobbyCommand::AbortRoom { room_id, reply }) => {
                    let result = registry.rooms.get(&room_id).map_or(Err(RoomError::RoomNotFound), |record| {
                        record.handle.abort_actor_for_test();
                        Ok(())
                    });
                    let _ignored = reply.send(result);
                }
                None => {
                    let _ignored = registry.shutdown().await;
                    open = false;
                }
            },
            event = events.recv() => {
                if let Some(event) = event { registry.process_event(event); }
            }
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
            nonzero(128),
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
                )
                .await,
            Err(RoomError::MaxRooms)
        );
        assert_eq!(
            lobby.join(second.id(), identity(1), None, tx).await,
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
                lobby.join(room_id, identity(connection), None, tx).await
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
        assert!(lobby.join(first.id(), identity(2), None, tx).await.is_ok());
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
    async fn get_state_routes_and_full_outbound_queue_reports_then_cleans_up() {
        let lobby = spawn_lobby(limits(2));
        let mut queue_drops = lobby.subscribe_queue_drops();
        let (tx, _rx) = mpsc::channel(1);
        let room = lobby
            .create(
                RoomName::parse("bounded").unwrap_or_else(|_| unreachable!()),
                None,
                RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
                identity(1),
                tx,
            )
            .await
            .unwrap_or_else(|_| unreachable!());
        let state = lobby.route(id(1), LobbyRoomCommand::GetState).await;
        assert!(matches!(state, Ok(LobbyRouteResult::Snapshot(_))));
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
        assert_eq!(queue_drops.recv().await, Ok(id(1)));
        assert!(lobby.disconnect(id(1)).await.is_ok());
        assert_eq!(lobby.list().await.unwrap_or_default().len(), 0);
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
                .join(room.id(), identity(3), None, tx.clone())
                .await
                .is_ok()
        );
        assert!(lobby.join(room.id(), identity(2), None, tx).await.is_ok());
        assert!(lobby.leave(id(1)).await.is_ok());
        let listed = lobby.list().await.unwrap_or_default();
        assert_eq!(
            listed.first().map(RoomSummary::owner_nickname),
            Some("Player3")
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
