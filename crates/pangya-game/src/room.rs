//! Runtime-neutral room rules wrapped by one bounded Tokio owner task.

use std::{num::NonZeroUsize, time::Duration};

use pangya_domain::{
    AccountId, ChatText, MemberSnapshot, Nickname, PlayerConnectionId, RoomError, RoomId, RoomName,
    RoomPassword, RoomSettings, RoomSnapshot, RoomSummary,
};
use rand::{RngCore as _, rngs::OsRng};
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;
use tokio::{
    sync::{mpsc, oneshot},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

/// Identity established by the connection/authentication boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomIdentity {
    /// Process-local connection identity.
    pub connection_id: PlayerConnectionId,
    /// Durable authenticated account identity.
    pub account_id: AccountId,
    /// Validated display nickname.
    pub nickname: Nickname,
}

/// Bounded event delivered to a room member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoomEvent {
    /// Membership/settings/ready state changed.
    Snapshot(RoomSnapshot),
    /// A validated chat message was broadcast.
    Chat {
        /// Authoritative sender projection.
        from: MemberSnapshot,
        /// Validated text.
        text: ChatText,
    },
    /// The owner removed this connection.
    Kicked {
        /// Authoritative owner projection.
        by: MemberSnapshot,
    },
    /// The room shut down.
    Closed,
}

/// Hard bounds for one room actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomActorLimits {
    normal_capacity: NonZeroUsize,
    control_capacity: NonZeroUsize,
    control_timeout: Duration,
}

impl RoomActorLimits {
    /// Creates actor queue bounds. Nonzero types make unbounded/zero queues unrepresentable.
    #[must_use]
    pub const fn new(
        normal_capacity: NonZeroUsize,
        control_capacity: NonZeroUsize,
        control_timeout: Duration,
    ) -> Self {
        Self {
            normal_capacity,
            control_capacity,
            control_timeout,
        }
    }

    /// Returns whether allocation and timeout bounds are suitable for production composition.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.normal_capacity.get() <= 65_536
            && self.control_capacity.get() <= 65_536
            && !self.control_timeout.is_zero()
            && self.control_timeout.as_secs() <= 300
    }
}

impl Default for RoomActorLimits {
    fn default() -> Self {
        Self::new(
            NonZeroUsize::new(64).unwrap_or(NonZeroUsize::MIN),
            NonZeroUsize::new(16).unwrap_or(NonZeroUsize::MIN),
            Duration::from_secs(2),
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) enum RoomActorEvent {
    Summary(RoomSummary),
    Closed(RoomId),
}

#[derive(Clone)]
struct PasswordDigest {
    salt: [u8; 32],
    digest: [u8; 32],
}

impl PasswordDigest {
    fn new(password: &RoomPassword) -> Self {
        let mut salt = [0_u8; 32];
        OsRng.fill_bytes(&mut salt);
        Self {
            salt,
            digest: digest_password(&salt, password.expose_bytes()),
        }
    }

    fn verifies(&self, password: Option<&RoomPassword>) -> bool {
        password.is_some_and(|password| {
            let candidate = digest_password(&self.salt, password.expose_bytes());
            bool::from(self.digest.ct_eq(&candidate))
        })
    }
}

fn digest_password(salt: &[u8; 32], password: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(password);
    hasher.finalize().into()
}

struct Member {
    identity: RoomIdentity,
    owner: bool,
    ready: bool,
    joined_order: u64,
    outbound: mpsc::Sender<RoomEvent>,
    cancellation: CancellationToken,
}

impl Member {
    fn snapshot(&self) -> MemberSnapshot {
        MemberSnapshot::new(
            self.identity.connection_id,
            self.identity.account_id,
            self.identity.nickname.display().to_owned(),
            self.owner,
            self.ready,
        )
    }
}

struct RoomState {
    id: RoomId,
    name: RoomName,
    password: Option<PasswordDigest>,
    settings: RoomSettings,
    members: Vec<Member>,
    next_join_order: u64,
}

impl RoomState {
    fn new(
        id: RoomId,
        name: RoomName,
        password: Option<RoomPassword>,
        settings: RoomSettings,
        owner: RoomIdentity,
        outbound: mpsc::Sender<RoomEvent>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            id,
            name,
            password: password.as_ref().map(PasswordDigest::new),
            settings,
            members: vec![Member {
                identity: owner,
                owner: true,
                ready: false,
                joined_order: 0,
                outbound,
                cancellation,
            }],
            next_join_order: 1,
        }
    }

    fn deliver(&self, member: &Member, event: RoomEvent) {
        if member.outbound.try_send(event).is_err() {
            member.cancellation.cancel();
        }
    }

    fn member_index(&self, connection_id: PlayerConnectionId) -> Option<usize> {
        self.members
            .iter()
            .position(|member| member.identity.connection_id == connection_id)
    }

    fn summary(&self) -> RoomSummary {
        let owner_nickname = self
            .members
            .iter()
            .find(|member| member.owner)
            .map_or_else(String::new, |member| {
                member.identity.nickname.display().to_owned()
            });
        RoomSummary::new(
            self.id,
            self.name.clone(),
            owner_nickname,
            u8::try_from(self.members.len()).unwrap_or(u8::MAX),
            self.settings.max_members(),
            self.password.is_some(),
        )
    }

    fn snapshot(&self) -> RoomSnapshot {
        RoomSnapshot::new(
            self.summary(),
            self.members.iter().map(Member::snapshot).collect(),
        )
    }

    #[cfg(test)]
    fn join(
        &mut self,
        identity: RoomIdentity,
        password: Option<&RoomPassword>,
        outbound: mpsc::Sender<RoomEvent>,
    ) -> Result<RoomSnapshot, RoomError> {
        self.join_with_cancellation(identity, password, outbound, CancellationToken::new())
    }

    fn join_with_cancellation(
        &mut self,
        identity: RoomIdentity,
        password: Option<&RoomPassword>,
        outbound: mpsc::Sender<RoomEvent>,
        cancellation: CancellationToken,
    ) -> Result<RoomSnapshot, RoomError> {
        if self.member_index(identity.connection_id).is_some() {
            return Err(RoomError::AlreadyMember);
        }
        if self.members.len() >= usize::from(self.settings.max_members()) {
            return Err(RoomError::Full);
        }
        if self
            .password
            .as_ref()
            .is_some_and(|digest| !digest.verifies(password))
        {
            return Err(RoomError::InvalidPassword);
        }
        let joined_order = self.next_join_order;
        self.next_join_order = self.next_join_order.saturating_add(1);
        self.members.push(Member {
            identity,
            owner: false,
            ready: false,
            joined_order,
            outbound,
            cancellation,
        });
        Ok(self.snapshot())
    }

    fn remove(
        &mut self,
        connection_id: PlayerConnectionId,
    ) -> Result<Option<RoomSnapshot>, RoomError> {
        let index = self
            .member_index(connection_id)
            .ok_or(RoomError::NotMember)?;
        let removed_owner = self.members[index].owner;
        self.members.remove(index);
        if self.members.is_empty() {
            return Ok(None);
        }
        if removed_owner {
            let next_owner = self
                .members
                .iter()
                .enumerate()
                .min_by_key(|(_, member)| (member.joined_order, member.identity.connection_id))
                .map(|(index, _)| index);
            if let Some(index) = next_owner {
                self.members[index].owner = true;
            }
        }
        Ok(Some(self.snapshot()))
    }

    fn update_settings(
        &mut self,
        caller: PlayerConnectionId,
        settings: RoomSettings,
    ) -> Result<RoomSnapshot, RoomError> {
        let caller = self.member_index(caller).ok_or(RoomError::NotMember)?;
        if !self.members[caller].owner {
            return Err(RoomError::NotOwner);
        }
        if usize::from(settings.max_members()) < self.members.len() {
            return Err(RoomError::CapacityBelowOccupancy);
        }
        self.settings = settings;
        Ok(self.snapshot())
    }

    fn set_ready(
        &mut self,
        caller: PlayerConnectionId,
        ready: bool,
    ) -> Result<RoomSnapshot, RoomError> {
        let caller = self.member_index(caller).ok_or(RoomError::NotMember)?;
        self.members[caller].ready = ready;
        Ok(self.snapshot())
    }

    fn chat(&self, caller: PlayerConnectionId, text: ChatText) -> Result<(), RoomError> {
        let caller = self.member_index(caller).ok_or(RoomError::NotMember)?;
        let event = RoomEvent::Chat {
            from: self.members[caller].snapshot(),
            text,
        };
        for member in &self.members {
            self.deliver(member, event.clone());
        }
        Ok(())
    }

    fn kick(
        &mut self,
        caller: PlayerConnectionId,
        target: PlayerConnectionId,
    ) -> Result<RoomSnapshot, RoomError> {
        let caller_index = self.member_index(caller).ok_or(RoomError::NotMember)?;
        if !self.members[caller_index].owner {
            return Err(RoomError::NotOwner);
        }
        if caller == target {
            return Err(RoomError::CannotKickSelf);
        }
        let target_index = self.member_index(target).ok_or(RoomError::MemberNotFound)?;
        let owner = self.members[caller_index].snapshot();
        let target = self.members.remove(target_index);
        self.deliver(&target, RoomEvent::Kicked { by: owner });
        Ok(self.snapshot())
    }

    fn broadcast_snapshot(&self, snapshot: &RoomSnapshot) {
        for member in &self.members {
            self.deliver(member, RoomEvent::Snapshot(snapshot.clone()));
        }
    }
}

enum RoomCommand {
    Join {
        identity: RoomIdentity,
        password: Option<RoomPassword>,
        outbound: mpsc::Sender<RoomEvent>,
        cancellation: CancellationToken,
        reply: oneshot::Sender<Result<RoomSnapshot, RoomError>>,
    },
    Leave {
        caller: PlayerConnectionId,
        reply: oneshot::Sender<Result<Option<RoomSnapshot>, RoomError>>,
    },
    Settings {
        caller: PlayerConnectionId,
        settings: RoomSettings,
        reply: oneshot::Sender<Result<RoomSnapshot, RoomError>>,
    },
    Ready {
        caller: PlayerConnectionId,
        ready: bool,
        reply: oneshot::Sender<Result<RoomSnapshot, RoomError>>,
    },
    Chat {
        caller: PlayerConnectionId,
        text: ChatText,
        reply: oneshot::Sender<Result<(), RoomError>>,
    },
    Kick {
        caller: PlayerConnectionId,
        target: PlayerConnectionId,
        reply: oneshot::Sender<Result<RoomSnapshot, RoomError>>,
    },
    State {
        reply: oneshot::Sender<RoomSnapshot>,
    },
}

enum ControlCommand {
    Disconnect {
        caller: PlayerConnectionId,
        reply: oneshot::Sender<Result<Option<RoomSnapshot>, RoomError>>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

/// Cloneable command endpoint for one room actor.
#[derive(Clone)]
pub struct RoomHandle {
    id: RoomId,
    normal: mpsc::Sender<RoomCommand>,
    control: mpsc::Sender<ControlCommand>,
    control_timeout: Duration,
    _actor_abort: tokio::task::AbortHandle,
}

impl std::fmt::Debug for RoomHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RoomHandle")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl RoomHandle {
    /// Room identifier.
    #[must_use]
    pub const fn id(&self) -> RoomId {
        self.id
    }

    fn send_normal(&self, command: RoomCommand) -> Result<(), RoomError> {
        self.normal.try_send(command).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => RoomError::QueueFull,
            mpsc::error::TrySendError::Closed(_) => RoomError::Closed,
        })
    }

    /// Joins using only the authoritative passed identity.
    pub async fn join(
        &self,
        identity: RoomIdentity,
        password: Option<RoomPassword>,
        outbound: mpsc::Sender<RoomEvent>,
    ) -> Result<RoomSnapshot, RoomError> {
        self.join_with_cancellation(identity, password, outbound, CancellationToken::new())
            .await
    }

    pub(crate) async fn join_with_cancellation(
        &self,
        identity: RoomIdentity,
        password: Option<RoomPassword>,
        outbound: mpsc::Sender<RoomEvent>,
        cancellation: CancellationToken,
    ) -> Result<RoomSnapshot, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send_normal(RoomCommand::Join {
            identity,
            password,
            outbound,
            cancellation,
            reply,
        })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Leaves voluntarily.
    pub async fn leave(
        &self,
        caller: PlayerConnectionId,
    ) -> Result<Option<RoomSnapshot>, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send_normal(RoomCommand::Leave { caller, reply })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Applies owner-only room settings.
    pub async fn update_settings(
        &self,
        caller: PlayerConnectionId,
        settings: RoomSettings,
    ) -> Result<RoomSnapshot, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send_normal(RoomCommand::Settings {
            caller,
            settings,
            reply,
        })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Changes the authoritative caller's ready state.
    pub async fn set_ready(
        &self,
        caller: PlayerConnectionId,
        ready: bool,
    ) -> Result<RoomSnapshot, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send_normal(RoomCommand::Ready {
            caller,
            ready,
            reply,
        })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Broadcasts validated chat using the authoritative caller identity.
    pub async fn chat(&self, caller: PlayerConnectionId, text: ChatText) -> Result<(), RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send_normal(RoomCommand::Chat {
            caller,
            text,
            reply,
        })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Removes a member when called by the owner.
    pub async fn kick(
        &self,
        caller: PlayerConnectionId,
        target: PlayerConnectionId,
    ) -> Result<RoomSnapshot, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send_normal(RoomCommand::Kick {
            caller,
            target,
            reply,
        })?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Returns an immutable point-in-time state projection.
    pub async fn state(&self) -> Result<RoomSnapshot, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send_normal(RoomCommand::State { reply })?;
        receive.await.map_err(|_| RoomError::Closed)
    }

    async fn send_control(&self, command: ControlCommand) -> Result<(), RoomError> {
        timeout(self.control_timeout, self.control.send(command))
            .await
            .map_err(|_| RoomError::Timeout)?
            .map_err(|_| RoomError::Closed)
    }

    /// Removes a dropped connection through the priority control queue.
    pub async fn disconnect(
        &self,
        caller: PlayerConnectionId,
    ) -> Result<Option<RoomSnapshot>, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send_control(ControlCommand::Disconnect { caller, reply })
            .await?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Shuts down through the priority queue with a bounded deadline.
    pub async fn shutdown(&self) -> Result<(), RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send_control(ControlCommand::Shutdown { reply })
            .await?;
        receive.await.map_err(|_| RoomError::Closed)
    }

    #[cfg(test)]
    pub(crate) fn abort_actor_for_test(&self) {
        self._actor_abort.abort();
    }
}

/// Starts one task that solely owns all mutable room state.
pub fn spawn_room(
    id: RoomId,
    name: RoomName,
    password: Option<RoomPassword>,
    settings: RoomSettings,
    owner: RoomIdentity,
    owner_outbound: mpsc::Sender<RoomEvent>,
    limits: RoomActorLimits,
) -> (RoomHandle, RoomSummary) {
    spawn_room_with_events(
        id,
        name,
        password,
        settings,
        owner,
        owner_outbound,
        CancellationToken::new(),
        limits,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_room_with_events(
    id: RoomId,
    name: RoomName,
    password: Option<RoomPassword>,
    settings: RoomSettings,
    owner: RoomIdentity,
    owner_outbound: mpsc::Sender<RoomEvent>,
    owner_cancellation: CancellationToken,
    limits: RoomActorLimits,
    events: Option<mpsc::Sender<RoomActorEvent>>,
) -> (RoomHandle, RoomSummary) {
    let state = RoomState::new(
        id,
        name,
        password,
        settings,
        owner,
        owner_outbound,
        owner_cancellation,
    );
    let summary = state.summary();
    let (normal, normal_rx) = mpsc::channel(limits.normal_capacity.get());
    let (control, control_rx) = mpsc::channel(limits.control_capacity.get());
    let actor = tokio::spawn(run_room(state, normal_rx, control_rx, events.clone()));
    let actor_abort = actor.abort_handle();
    tokio::spawn(async move {
        let _outcome = actor.await;
        if let Some(events) = events {
            let _ignored = events.send(RoomActorEvent::Closed(id)).await;
        }
    });
    let handle = RoomHandle {
        id,
        normal,
        control,
        control_timeout: limits.control_timeout,
        _actor_abort: actor_abort,
    };
    (handle, summary)
}

async fn run_room(
    mut state: RoomState,
    mut normal: mpsc::Receiver<RoomCommand>,
    mut control: mpsc::Receiver<ControlCommand>,
    events: Option<mpsc::Sender<RoomActorEvent>>,
) {
    let mut open = true;
    while open {
        tokio::select! {
            biased;
            command = control.recv() => match command {
                Some(ControlCommand::Disconnect { caller, reply }) => {
                    let result = state.remove(caller);
                    if let Ok(snapshot) = &result {
                        open = snapshot.is_some();
                        after_mutation(&state, snapshot.as_ref(), events.as_ref());
                    }
                    let _ignored = reply.send(result);
                }
                Some(ControlCommand::Shutdown { reply }) => {
                    for member in &state.members {
                        state.deliver(member, RoomEvent::Closed);
                    }
                    let _ignored = reply.send(());
                    open = false;
                }
                None => {
                    if normal.is_closed() { open = false; }
                }
            },
            command = normal.recv() => match command {
                Some(command) => open = handle_normal(&mut state, command, events.as_ref()),
                None => {
                    if control.is_closed() { open = false; }
                }
            },
        }
    }
}

fn handle_normal(
    state: &mut RoomState,
    command: RoomCommand,
    events: Option<&mpsc::Sender<RoomActorEvent>>,
) -> bool {
    match command {
        RoomCommand::Join {
            identity,
            password,
            outbound,
            cancellation,
            reply,
        } => {
            let result =
                state.join_with_cancellation(identity, password.as_ref(), outbound, cancellation);
            if let Ok(snapshot) = &result {
                after_mutation(state, Some(snapshot), events);
            }
            let _ignored = reply.send(result);
            true
        }
        RoomCommand::Leave { caller, reply } => {
            let result = state.remove(caller);
            let mut open = true;
            if let Ok(snapshot) = &result {
                open = snapshot.is_some();
                after_mutation(state, snapshot.as_ref(), events);
            }
            let _ignored = reply.send(result);
            open
        }
        RoomCommand::Settings {
            caller,
            settings,
            reply,
        } => {
            let result = state.update_settings(caller, settings);
            if let Ok(snapshot) = &result {
                after_mutation(state, Some(snapshot), events);
            }
            let _ignored = reply.send(result);
            true
        }
        RoomCommand::Ready {
            caller,
            ready,
            reply,
        } => {
            let result = state.set_ready(caller, ready);
            if let Ok(snapshot) = &result {
                after_mutation(state, Some(snapshot), events);
            }
            let _ignored = reply.send(result);
            true
        }
        RoomCommand::Chat {
            caller,
            text,
            reply,
        } => {
            let _ignored = reply.send(state.chat(caller, text));
            true
        }
        RoomCommand::Kick {
            caller,
            target,
            reply,
        } => {
            let result = state.kick(caller, target);
            if let Ok(snapshot) = &result {
                after_mutation(state, Some(snapshot), events);
            }
            let _ignored = reply.send(result);
            true
        }
        RoomCommand::State { reply } => {
            let _ignored = reply.send(state.snapshot());
            true
        }
    }
}

fn after_mutation(
    state: &RoomState,
    snapshot: Option<&RoomSnapshot>,
    events: Option<&mpsc::Sender<RoomActorEvent>>,
) {
    if let Some(snapshot) = snapshot {
        state.broadcast_snapshot(snapshot);
        if let Some(events) = events {
            let _ignored = events.try_send(RoomActorEvent::Summary(snapshot.summary().clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use proptest::prelude::*;

    use super::*;

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

    fn state(
        capacity: u8,
        password: Option<RoomPassword>,
    ) -> (RoomState, mpsc::Receiver<RoomEvent>) {
        let (tx, rx) = mpsc::channel(64);
        (
            RoomState::new(
                RoomId::new(1).unwrap_or_else(|_| unreachable!()),
                RoomName::parse("room").unwrap_or_else(|_| unreachable!()),
                password,
                RoomSettings::new(capacity).unwrap_or_else(|_| unreachable!()),
                identity(1),
                tx,
                CancellationToken::new(),
            ),
            rx,
        )
    }

    #[test]
    fn pure_rejections_do_not_mutate_and_invariants_hold() {
        let password = RoomPassword::parse("secret").ok();
        let (mut state, _rx) = state(2, password);
        let before = state.snapshot();
        let (tx, _rx2) = mpsc::channel(1);
        let wrong = RoomPassword::parse("wrong").ok();
        assert_eq!(
            state.join(identity(2), wrong.as_ref(), tx.clone()),
            Err(RoomError::InvalidPassword)
        );
        assert_eq!(state.snapshot(), before);
        let correct = RoomPassword::parse("secret").ok();
        assert!(
            state
                .join(identity(2), correct.as_ref(), tx.clone())
                .is_ok()
        );
        let full = state.snapshot();
        assert_eq!(
            state.join(identity(3), correct.as_ref(), tx),
            Err(RoomError::Full)
        );
        assert_eq!(state.snapshot(), full);
        assert_eq!(full.members().len(), 2);
        assert_eq!(
            full.members()
                .iter()
                .filter(|member| member.is_owner())
                .count(),
            1
        );
    }

    #[test]
    fn pure_owner_transfer_is_longest_present_and_authz_is_stable() {
        let (mut state, _rx) = state(4, None);
        let (tx, _receiver) = mpsc::channel(8);
        assert!(state.join(identity(3), None, tx.clone()).is_ok());
        assert!(state.join(identity(2), None, tx).is_ok());
        let before = state.snapshot();
        assert_eq!(
            state.update_settings(
                id(3),
                RoomSettings::new(3).unwrap_or_else(|_| unreachable!())
            ),
            Err(RoomError::NotOwner)
        );
        assert_eq!(state.snapshot(), before);
        let snapshot = state
            .remove(id(1))
            .ok()
            .flatten()
            .unwrap_or_else(|| unreachable!());
        let owner = snapshot
            .members()
            .iter()
            .find(|member| member.is_owner())
            .map(MemberSnapshot::connection_id);
        assert_eq!(owner, Some(id(3)));
        assert_eq!(
            snapshot
                .members()
                .iter()
                .map(MemberSnapshot::connection_id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            snapshot.members().len()
        );
    }

    #[test]
    fn pure_owner_transfer_tie_uses_connection_id() {
        let (mut state, _rx) = state(4, None);
        let (tx, _receiver) = mpsc::channel(8);
        assert!(state.join(identity(3), None, tx.clone()).is_ok());
        assert!(state.join(identity(2), None, tx).is_ok());
        for member in &mut state.members[1..] {
            member.joined_order = 1;
        }
        let snapshot = state
            .remove(id(1))
            .ok()
            .flatten()
            .unwrap_or_else(|| unreachable!());
        let owner = snapshot
            .members()
            .iter()
            .find(|member| member.is_owner())
            .map(MemberSnapshot::connection_id);
        assert_eq!(owner, Some(id(2)));
    }

    proptest! {
        #[test]
        fn pure_command_sequences_preserve_room_invariants(
            operations in proptest::collection::vec((0_u8..5, 1_u8..12), 1..200)
        ) {
            let (mut state, _rx) = state(6, None);
            let (tx, _receiver) = mpsc::channel(256);
            for (operation, raw_connection) in operations {
                let connection = id(u64::from(raw_connection));
                let before = state.snapshot();
                let result = match operation {
                    0 => state.join(identity(u64::from(raw_connection)), None, tx.clone()).map(Some),
                    1 => state.remove(connection),
                    2 => state.set_ready(connection, true).map(Some),
                    3 => state.update_settings(connection, RoomSettings::new(4).unwrap_or_else(|_| unreachable!())).map(Some),
                    _ => state.kick(id(1), connection).map(Some),
                };
                if result.is_err() {
                    prop_assert_eq!(state.snapshot(), before);
                }
                let snapshot = state.snapshot();
                prop_assert!(snapshot.members().len() <= usize::from(snapshot.summary().max_members()));
                let unique: HashSet<_> = snapshot.members().iter().map(MemberSnapshot::connection_id).collect();
                prop_assert_eq!(unique.len(), snapshot.members().len());
                prop_assert_eq!(snapshot.members().iter().filter(|member| member.is_owner()).count(), usize::from(!snapshot.members().is_empty()));
                if snapshot.members().is_empty() {
                    break;
                }
            }
        }
    }

    #[tokio::test]
    async fn normal_queue_saturation_returns_queue_full() {
        let (normal, _normal_rx) = mpsc::channel(1);
        let (control, _control_rx) = mpsc::channel(1);
        let sleeper = tokio::spawn(std::future::pending::<()>());
        let handle = RoomHandle {
            id: RoomId::new(1).unwrap_or_else(|_| unreachable!()),
            normal,
            control,
            control_timeout: Duration::from_millis(10),
            _actor_abort: sleeper.abort_handle(),
        };
        let (first_reply, _first_receive) = oneshot::channel();
        assert!(
            handle
                .send_normal(RoomCommand::State { reply: first_reply })
                .is_ok()
        );
        let (second_reply, _second_receive) = oneshot::channel();
        assert_eq!(
            handle.send_normal(RoomCommand::State {
                reply: second_reply
            }),
            Err(RoomError::QueueFull)
        );
        sleeper.abort();
    }

    #[tokio::test]
    async fn actor_password_ready_chat_kick_transfer_disconnect_and_shutdown() {
        let (owner_tx, mut owner_rx) = mpsc::channel(32);
        let (handle, _) = spawn_room(
            RoomId::new(1).unwrap_or_else(|_| unreachable!()),
            RoomName::parse("room").unwrap_or_else(|_| unreachable!()),
            RoomPassword::parse("secret").ok(),
            RoomSettings::new(3).unwrap_or_else(|_| unreachable!()),
            identity(1),
            owner_tx,
            RoomActorLimits::default(),
        );
        let (member_tx, mut member_rx) = mpsc::channel(32);
        assert_eq!(
            handle
                .join(
                    identity(2),
                    RoomPassword::parse("wrong").ok(),
                    member_tx.clone()
                )
                .await,
            Err(RoomError::InvalidPassword)
        );
        assert!(
            handle
                .join(identity(2), RoomPassword::parse("secret").ok(), member_tx)
                .await
                .is_ok()
        );
        assert_eq!(
            handle.set_ready(id(2), true).await.map(|snapshot| snapshot
                .members()
                .iter()
                .find(|member| member.connection_id() == id(2))
                .map(MemberSnapshot::is_ready)),
            Ok(Some(true))
        );
        assert_eq!(
            handle
                .update_settings(
                    id(2),
                    RoomSettings::new(3).unwrap_or_else(|_| unreachable!())
                )
                .await,
            Err(RoomError::NotOwner)
        );
        assert!(
            handle
                .chat(
                    id(2),
                    ChatText::parse("hello").unwrap_or_else(|_| unreachable!())
                )
                .await
                .is_ok()
        );
        let mut saw_chat = false;
        while let Ok(event) = owner_rx.try_recv() {
            if matches!(event, RoomEvent::Chat { .. }) {
                saw_chat = true;
            }
        }
        assert!(saw_chat);
        assert_eq!(handle.kick(id(2), id(1)).await, Err(RoomError::NotOwner));
        assert!(handle.leave(id(1)).await.is_ok());
        let state = handle.state().await.unwrap_or_else(|_| unreachable!());
        assert!(state.members()[0].is_owner());
        assert_eq!(state.members()[0].connection_id(), id(2));
        assert_eq!(handle.disconnect(id(2)).await, Ok(None));
        assert_eq!(handle.state().await, Err(RoomError::Closed));
        assert!(member_rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn actor_full_self_kick_and_explicit_shutdown() {
        let (tx, _rx) = mpsc::channel(8);
        let (handle, _) = spawn_room(
            RoomId::new(2).unwrap_or_else(|_| unreachable!()),
            RoomName::parse("room").unwrap_or_else(|_| unreachable!()),
            None,
            RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
            identity(1),
            tx.clone(),
            RoomActorLimits::default(),
        );
        assert!(handle.join(identity(2), None, tx.clone()).await.is_ok());
        assert_eq!(
            handle.join(identity(3), None, tx).await,
            Err(RoomError::Full)
        );
        assert_eq!(
            handle.kick(id(1), id(1)).await,
            Err(RoomError::CannotKickSelf)
        );
        assert!(handle.shutdown().await.is_ok());
        assert_eq!(handle.state().await, Err(RoomError::Closed));
    }
}
