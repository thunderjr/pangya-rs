//! Runtime-neutral room rules wrapped by one bounded Tokio owner task.

use std::{num::NonZeroUsize, time::Duration};

use pangya_domain::{
    AbortMatch, AbortStrokeMatch, AccountId, BeginSoloMatch, BeginStrokeMatch, CharacterId,
    ChatText, CommitSoloHole, CommitStrokeMatch, MarkSoloInGame, MarkStrokeInGame,
    MatchAbortReason, MatchId, MatchResultKey, MemberCard, MemberSnapshot, Nickname,
    PlayerConnectionId, RoomError, RoomId, RoomName, RoomPassword, RoomSettings, RoomSnapshot,
    RoomSummary, SoloMatchResult, StrokeMatchResult,
};
use pangya_protocol::{
    LoadingComplete, ShotAction, ShotResult, StrokeLoadingComplete, StrokeShotAction,
    StrokeShotResult,
};
use rand::{RngCore as _, rngs::OsRng};
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;
use tokio::{
    sync::{mpsc, oneshot},
    time::{Instant, sleep_until, timeout},
};
use tokio_util::sync::CancellationToken;

use crate::{
    match_state::{
        RelayDisposition, SoloMatchError, SoloMatchPhase, SoloMatchState, SoloStartPlan,
    },
    stroke_state::{
        StrokeDeadline, StrokeDeadlineOutcome, StrokeHoleOutOutcome, StrokeLoadingOutcome,
        StrokeMatchError, StrokeMatchPhase, StrokeMatchState, StrokeRelayOutcome, StrokeStartPlan,
    },
};

/// The participant a stroke phase is waiting on, if any.
const fn active_participant(phase: StrokeMatchPhase) -> Option<PlayerConnectionId> {
    match phase {
        StrokeMatchPhase::AwaitAction { active, .. }
        | StrokeMatchPhase::AwaitResult { active, .. } => Some(active),
        _ => None,
    }
}

/// Identity established by the connection/authentication boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomIdentity {
    /// Process-local connection identity.
    pub connection_id: PlayerConnectionId,
    /// Durable authenticated account identity.
    pub account_id: AccountId,
    /// Validated display nickname.
    pub nickname: Nickname,
    /// The account's selected character, so a room roster can render it.
    pub character_id: Option<CharacterId>,
    /// That character's catalog id, which is what the client resolves the model by.
    pub character_iff_id: Option<u32>,
    /// What the rest of the room sees of this player, including in a match roster.
    pub card: MemberCard,
}

/// The largest client-authored in-match payload this server will relay.
pub const MAX_RETAIL_RELAY_BYTES: usize = 256;

/// A client-authored in-match frame relayed to the participants without interpretation.
///
/// The client computes trajectory and ball state, so these carry its own bytes. This server's
/// authority is the stroke count and turn arbitration around them, never their content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetailMatchRelay {
    /// The committed shot, announced to the participants.
    Shot(Vec<u8>),
    /// Post-shot ball state, echoed to the participants.
    Sync(Vec<u8>),
    /// Aim rotation, represented as the original IEEE-754 wire bits.
    Aim(u32),
    /// Extra-power selection.
    Power(u8),
    /// Active-club selection.
    Club(u8),
    /// In-match item type selection.
    Item(u32),
    /// Comet-relief coordinates, represented as their original IEEE-754 wire bits.
    CometRelief([u32; 3]),
}

impl RetailMatchRelay {
    /// The relayed bytes.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        match self {
            Self::Shot(body) | Self::Sync(body) => body,
            Self::Aim(_)
            | Self::Power(_)
            | Self::Club(_)
            | Self::Item(_)
            | Self::CometRelief(_) => &[],
        }
    }

    /// Whether the payload is within the relay bound.
    #[must_use]
    pub fn is_bounded(&self) -> bool {
        match self {
            Self::Shot(body) | Self::Sync(body) => body.len() <= MAX_RETAIL_RELAY_BYTES,
            Self::Aim(_)
            | Self::Power(_)
            | Self::Club(_)
            | Self::Item(_)
            | Self::CometRelief(_) => true,
        }
    }
}

/// Bounded event delivered to a room member.
#[derive(Clone, Debug, PartialEq)]
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
    /// A persisted checked solo plan was confirmed and loading started.
    SoloStarted(SoloStartPlan),
    /// Authoritative solo phase changed for an exact match.
    SoloPhase {
        /// Durable match identity.
        match_id: MatchId,
        /// Authoritative phase projection.
        phase: SoloMatchPhase,
    },
    /// Validated action relayed with authoritative connection identity.
    SoloActionRelay {
        /// Sole authoritative sender.
        from: PlayerConnectionId,
        /// Validated action.
        action: ShotAction,
    },
    /// Validated result relayed with authoritative connection identity.
    SoloResultRelay {
        /// Sole authoritative sender.
        from: PlayerConnectionId,
        /// Validated result.
        result: ShotResult,
    },
    /// A durable idempotent abort must be applied without reward.
    AbortRequested(AbortMatch),
    /// Trusted persisted result committed and the room returned to open.
    SoloCommitted(SoloMatchResult),
    /// Persisted checked stroke plan confirmed for both captured participants.
    StrokeStarted(StrokeStartPlan),
    /// Authoritative stroke phase changed.
    StrokePhase {
        /// Durable match identity.
        match_id: MatchId,
        /// Pure authoritative phase.
        phase: StrokeMatchPhase,
    },
    /// A new turn started; broadcast identically to both participants.
    StrokeTurn(StrokeMatchPhase),
    /// Newly accepted validated stroke action.
    StrokeActionRelay {
        /// Captured sender connection.
        from: PlayerConnectionId,
        /// Validated protocol value.
        action: StrokeShotAction,
    },
    /// Newly accepted validated stroke result.
    StrokeResultRelay {
        /// Captured sender connection.
        from: PlayerConnectionId,
        /// Validated protocol value.
        result: StrokeShotResult,
    },
    /// One participant's hole-loading progress, for everyone else's loading bar.
    RetailLoadProgress {
        /// Captured sender connection.
        from: PlayerConnectionId,
        /// How far along the client says it is.
        progress: u8,
    },
    /// A participant's own in-match frame, relayed unchanged to the captured roster.
    RetailRelay {
        /// Captured sender connection.
        from: PlayerConnectionId,
        /// The client's payload and what it is.
        relay: RetailMatchRelay,
    },
    /// Exactly one connected coordinator must persist this aggregate settlement.
    StrokeSettlementRequested(CommitStrokeMatch),
    /// Exactly one connected coordinator must persist this no-reward abort.
    StrokeAbortRequested(AbortStrokeMatch),
    /// Trusted persisted aggregate committed and room returned open.
    StrokeCommitted(StrokeMatchResult),
    /// Exact durable stroke abort was acknowledged.
    StrokeAborted(AbortStrokeMatch),
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
            self.identity.character_id,
            self.identity.character_iff_id,
        )
        .with_card(self.identity.card.clone())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MatchDeadlineKind {
    SoloLoading,
    StrokeLoading,
    StrokeTurn(u64),
    StrokeGame(u64),
}

#[derive(Clone, Copy, Debug)]
struct ScheduledDeadline {
    at: Instant,
    kind: MatchDeadlineKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingStrokePersistence {
    Abort(AbortStrokeMatch),
    Settlement(CommitStrokeMatch),
}

impl PendingStrokePersistence {
    fn outcome(self, room_id: RoomId) -> RoomCloseOutcome {
        match self {
            Self::Abort(request) => RoomCloseOutcome::M6Abort { room_id, request },
            Self::Settlement(request) => RoomCloseOutcome::M6Settlement { room_id, request },
        }
    }

    fn event(self) -> RoomEvent {
        match self {
            Self::Abort(request) => RoomEvent::StrokeAbortRequested(request),
            Self::Settlement(request) => RoomEvent::StrokeSettlementRequested(request),
        }
    }
}

#[derive(Default)]
struct DeadlineScheduler {
    solo_loading: Option<Instant>,
    stroke_loading: Option<Instant>,
    stroke_turn: Option<(Instant, u64)>,
    stroke_game: Option<(Instant, u64)>,
}

impl DeadlineScheduler {
    fn next(&self) -> Option<ScheduledDeadline> {
        let candidates = [
            self.solo_loading.map(|at| ScheduledDeadline {
                at,
                kind: MatchDeadlineKind::SoloLoading,
            }),
            self.stroke_loading.map(|at| ScheduledDeadline {
                at,
                kind: MatchDeadlineKind::StrokeLoading,
            }),
            // The whole-game cap wins an exact tie with a turn cap.
            self.stroke_game.map(|(at, generation)| ScheduledDeadline {
                at,
                kind: MatchDeadlineKind::StrokeGame(generation),
            }),
            self.stroke_turn.map(|(at, generation)| ScheduledDeadline {
                at,
                kind: MatchDeadlineKind::StrokeTurn(generation),
            }),
        ];
        candidates
            .into_iter()
            .flatten()
            .min_by_key(|deadline| deadline.at)
    }

    fn clear_kind(&mut self, kind: MatchDeadlineKind) {
        match kind {
            MatchDeadlineKind::SoloLoading => self.solo_loading = None,
            MatchDeadlineKind::StrokeLoading => self.stroke_loading = None,
            MatchDeadlineKind::StrokeTurn(generation) => {
                if self
                    .stroke_turn
                    .is_some_and(|(_, current)| current == generation)
                {
                    self.stroke_turn = None;
                }
            }
            MatchDeadlineKind::StrokeGame(generation) => {
                if self
                    .stroke_game
                    .is_some_and(|(_, current)| current == generation)
                {
                    self.stroke_game = None;
                }
            }
        }
    }

    fn clear_solo(&mut self) {
        self.solo_loading = None;
    }
    fn clear_stroke(&mut self) {
        self.stroke_loading = None;
        self.stroke_turn = None;
        self.stroke_game = None;
    }
}

struct RoomState {
    id: RoomId,
    name: RoomName,
    password: Option<PasswordDigest>,
    settings: RoomSettings,
    members: Vec<Member>,
    next_join_order: u64,
    solo: SoloMatchState,
    stroke: StrokeMatchState,
    pending_stroke_persistence: Option<PendingStrokePersistence>,
    stroke_persistence_event_delivered: bool,
    stroke_persistence_control_delivered: bool,
    deadlines: DeadlineScheduler,
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
            solo: SoloMatchState::new(),
            stroke: StrokeMatchState::new(),
            pending_stroke_persistence: None,
            stroke_persistence_event_delivered: false,
            stroke_persistence_control_delivered: false,
            deadlines: DeadlineScheduler::default(),
        }
    }

    fn deliver(&self, member: &Member, event: RoomEvent) -> bool {
        if member.cancellation.is_cancelled() {
            return false;
        }
        if member.outbound.try_send(event).is_ok() {
            true
        } else {
            member.cancellation.cancel();
            false
        }
    }

    fn member_index(&self, connection_id: PlayerConnectionId) -> Option<usize> {
        self.members
            .iter()
            .position(|member| member.identity.connection_id == connection_id)
    }

    fn match_active(&self) -> bool {
        self.solo.is_active() || self.stroke.is_active()
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
            self.settings.profile(),
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
        if self.match_active() {
            return Err(RoomError::MatchActive);
        }
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
        if self.match_active() {
            return Err(RoomError::MatchActive);
        }
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
        if self.match_active() {
            return Err(RoomError::MatchActive);
        }
        let caller = self.member_index(caller).ok_or(RoomError::NotMember)?;
        self.members[caller].ready = ready;
        Ok(self.snapshot())
    }

    fn chat(&self, caller: PlayerConnectionId, text: ChatText) -> Result<(), RoomError> {
        if self.match_active() {
            return Err(RoomError::MatchActive);
        }
        let caller = self.member_index(caller).ok_or(RoomError::NotMember)?;
        let event = RoomEvent::Chat {
            from: self.members[caller].snapshot(),
            text,
        };
        for member in &self.members {
            let _delivered = self.deliver(member, event.clone());
        }
        Ok(())
    }

    fn kick(
        &mut self,
        caller: PlayerConnectionId,
        target: PlayerConnectionId,
    ) -> Result<RoomSnapshot, RoomError> {
        if self.match_active() {
            return Err(RoomError::MatchActive);
        }
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
        let _delivered = self.deliver(&target, RoomEvent::Kicked { by: owner });
        Ok(self.snapshot())
    }

    fn solo_owner(&self, caller: PlayerConnectionId) -> Result<&Member, SoloMatchError> {
        let member = self
            .member_index(caller)
            .and_then(|index| self.members.get(index))
            .ok_or(SoloMatchError::NotMember)?;
        if !member.owner {
            return Err(SoloMatchError::NotOwner);
        }
        Ok(member)
    }

    fn prepare_solo_start(
        &mut self,
        caller: PlayerConnectionId,
        plan: SoloStartPlan,
    ) -> Result<BeginSoloMatch, SoloMatchError> {
        let owner = self.solo_owner(caller)?;
        if self.stroke.is_active() {
            return Err(SoloMatchError::InvalidPhase);
        }
        if self.members.len() != 1 {
            return Err(SoloMatchError::NotSolo);
        }
        if plan.begin().account_id() != owner.identity.account_id {
            return Err(SoloMatchError::AccountMismatch);
        }
        self.solo.prepare_start(plan)
    }

    fn confirm_solo_begin(
        &mut self,
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), SoloMatchError> {
        self.solo_owner(caller)?;
        self.solo.confirm_begin(match_id, result_key)?;
        let timeout = self
            .solo
            .loading_timeout()
            .ok_or(SoloMatchError::InvalidPhase)?;
        self.deadlines.solo_loading = Some(Instant::now() + timeout);
        let plan = self
            .solo
            .start_plan()
            .cloned()
            .ok_or(SoloMatchError::InvalidPhase)?;
        self.deliver_solo(RoomEvent::SoloStarted(plan));
        self.deliver_solo(RoomEvent::SoloPhase {
            match_id,
            phase: SoloMatchPhase::Loading,
        });
        Ok(())
    }

    fn cancel_solo_begin(
        &mut self,
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), SoloMatchError> {
        self.solo_owner(caller)?;
        self.solo.cancel_begin(match_id, result_key)
    }

    fn solo_loading_complete(
        &mut self,
        caller: PlayerConnectionId,
        loading: LoadingComplete,
    ) -> Result<MarkSoloInGame, SoloMatchError> {
        self.solo_owner(caller)?;
        let begin = self.solo.begin().ok_or(SoloMatchError::InvalidPhase)?;
        let mark = MarkSoloInGame::new(begin.match_id(), begin.result_key(), begin.account_id());
        self.solo.loading_complete(loading.progress())?;
        self.deadlines.clear_solo();
        self.deliver_solo(RoomEvent::SoloPhase {
            match_id: mark.match_id(),
            phase: self.solo.phase(),
        });
        Ok(mark)
    }

    fn solo_action(
        &mut self,
        caller: PlayerConnectionId,
        action: ShotAction,
    ) -> Result<RelayDisposition, SoloMatchError> {
        self.solo_owner(caller)?;
        let match_id = self
            .solo
            .start_plan()
            .map(|plan| plan.begin().match_id())
            .ok_or(SoloMatchError::InvalidPhase)?;
        let disposition = self.solo.accept_action(action)?;
        if disposition == RelayDisposition::Accepted {
            self.deliver_solo(RoomEvent::SoloActionRelay {
                from: caller,
                action,
            });
            self.deliver_solo(RoomEvent::SoloPhase {
                match_id,
                phase: self.solo.phase(),
            });
        }
        Ok(disposition)
    }

    fn solo_result(
        &mut self,
        caller: PlayerConnectionId,
        result: ShotResult,
    ) -> Result<RelayDisposition, SoloMatchError> {
        self.solo_owner(caller)?;
        let match_id = self
            .solo
            .start_plan()
            .map(|plan| plan.begin().match_id())
            .ok_or(SoloMatchError::InvalidPhase)?;
        let disposition = self.solo.accept_result(result)?;
        if disposition == RelayDisposition::Accepted {
            self.deliver_solo(RoomEvent::SoloResultRelay {
                from: caller,
                result,
            });
            self.deliver_solo(RoomEvent::SoloPhase {
                match_id,
                phase: self.solo.phase(),
            });
        }
        Ok(disposition)
    }

    fn solo_hole_out(
        &mut self,
        caller: PlayerConnectionId,
    ) -> Result<RelayDisposition, SoloMatchError> {
        self.solo_owner(caller)?;
        let match_id = self
            .solo
            .start_plan()
            .map(|plan| plan.begin().match_id())
            .ok_or(SoloMatchError::InvalidPhase)?;
        let disposition = self.solo.hole_out()?;
        if disposition == RelayDisposition::Accepted {
            self.deliver_solo(RoomEvent::SoloPhase {
                match_id,
                phase: self.solo.phase(),
            });
        }
        Ok(disposition)
    }

    fn prepare_solo_finish(
        &mut self,
        caller: PlayerConnectionId,
    ) -> Result<CommitSoloHole, SoloMatchError> {
        self.solo_owner(caller)?;
        let commit = self.solo.prepare_finish()?;
        self.deliver_solo(RoomEvent::SoloPhase {
            match_id: commit.match_id(),
            phase: self.solo.phase(),
        });
        Ok(commit)
    }

    fn apply_solo_commit(
        &mut self,
        caller: PlayerConnectionId,
        result: SoloMatchResult,
    ) -> Result<SoloMatchResult, SoloMatchError> {
        self.solo_owner(caller)?;
        let committed = self.solo.apply_commit(result)?;
        self.deliver_solo(RoomEvent::SoloCommitted(committed));
        Ok(committed)
    }

    fn abort_solo(
        &mut self,
        caller: PlayerConnectionId,
        reason: MatchAbortReason,
    ) -> Result<Option<AbortMatch>, SoloMatchError> {
        self.solo_owner(caller)?;
        self.deadlines.clear_solo();
        Ok(self.solo.abort(reason))
    }

    fn acknowledge_solo_abort(
        &mut self,
        caller: PlayerConnectionId,
        abort: AbortMatch,
    ) -> Result<(), SoloMatchError> {
        self.solo_owner(caller)?;
        self.solo.acknowledge_abort(abort)?;
        self.deliver_solo(RoomEvent::SoloPhase {
            match_id: abort.match_id(),
            phase: SoloMatchPhase::Open,
        });
        Ok(())
    }

    fn stroke_participant(&self, caller: PlayerConnectionId) -> Result<&Member, StrokeMatchError> {
        let member = self
            .member_index(caller)
            .and_then(|index| self.members.get(index))
            .ok_or(StrokeMatchError::NotMember)?;
        let rostered = self
            .stroke
            .start_plan()
            .is_some_and(|plan| plan.roster().contains(&caller));
        if self.stroke.is_active() && !rostered {
            return Err(StrokeMatchError::NotParticipant);
        }
        Ok(member)
    }

    fn prepare_stroke_start(
        &mut self,
        caller: PlayerConnectionId,
        plan: StrokeStartPlan,
    ) -> Result<BeginStrokeMatch, StrokeMatchError> {
        if self.solo.is_active() || self.stroke.is_active() {
            return Err(StrokeMatchError::InvalidPhase);
        }
        let caller_index = self
            .member_index(caller)
            .ok_or(StrokeMatchError::NotMember)?;
        if !self.members[caller_index].owner {
            return Err(StrokeMatchError::NotOwner);
        }
        if self.members.len() != 2 {
            return Err(StrokeMatchError::NotExactlyTwo);
        }
        if self.members.iter().any(|member| !member.ready) {
            return Err(StrokeMatchError::NotReady);
        }
        let mut order = [0_usize, 1_usize];
        order.sort_by_key(|index| {
            let member = &self.members[*index];
            (member.joined_order, member.identity.connection_id)
        });
        let roster = [
            self.members[order[0]].identity.connection_id,
            self.members[order[1]].identity.connection_id,
        ];
        let accounts = [
            self.members[order[0]].identity.account_id,
            self.members[order[1]].identity.account_id,
        ];
        let participants = plan.begin().participants();
        if accounts[0] == accounts[1]
            || *plan.roster() != roster
            || participants[0].account_id() != accounts[0]
            || participants[1].account_id() != accounts[1]
        {
            return Err(StrokeMatchError::RosterMismatch);
        }
        let begin = self.stroke.prepare_start(plan)?;
        self.pending_stroke_persistence = None;
        self.stroke_persistence_event_delivered = false;
        self.stroke_persistence_control_delivered = false;
        Ok(begin)
    }

    fn confirm_stroke_begin(
        &mut self,
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), StrokeMatchError> {
        self.stroke_participant(caller)?;
        self.stroke.confirm_begin(match_id, result_key)?;
        let plan = self
            .stroke
            .start_plan()
            .cloned()
            .ok_or(StrokeMatchError::InvalidPhase)?;
        self.deadlines.stroke_loading = Some(Instant::now() + plan.loading_timeout());
        self.broadcast_match(RoomEvent::StrokeStarted(plan));
        self.broadcast_match(RoomEvent::StrokePhase {
            match_id,
            phase: self.stroke.phase(),
        });
        Ok(())
    }

    fn cancel_stroke_begin(
        &mut self,
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), StrokeMatchError> {
        self.stroke_participant(caller)?;
        self.stroke.cancel_begin(match_id, result_key)
    }

    fn stroke_loading_complete(
        &mut self,
        caller: PlayerConnectionId,
        loading: StrokeLoadingComplete,
    ) -> Result<StrokeLoadingOutcome, StrokeMatchError> {
        self.stroke_participant(caller)?;
        let outcome = self.stroke.loading_complete(caller, loading.progress())?;
        if matches!(outcome, StrokeLoadingOutcome::PersistenceRequired(_)) {
            self.deadlines.stroke_loading = None;
        }
        if matches!(outcome, StrokeLoadingOutcome::PersistenceRequired(_)) {
            let match_id = self
                .stroke
                .start_plan()
                .map(|plan| plan.begin().match_id())
                .ok_or(StrokeMatchError::InvalidPhase)?;
            self.broadcast_match(RoomEvent::StrokePhase {
                match_id,
                phase: self.stroke.phase(),
            });
        }
        Ok(outcome)
    }

    fn confirm_stroke_in_game(&mut self, mark: MarkStrokeInGame) -> Result<(), StrokeMatchError> {
        self.stroke.confirm_in_game(mark)?;
        let plan = self
            .stroke
            .start_plan()
            .ok_or(StrokeMatchError::InvalidPhase)?;
        let now = Instant::now();
        let turn_generation = self
            .stroke
            .turn_generation()
            .ok_or(StrokeMatchError::Invariant)?;
        let game_generation = self
            .stroke
            .game_generation()
            .ok_or(StrokeMatchError::Invariant)?;
        self.deadlines.stroke_turn = Some((now + plan.turn_timeout(), turn_generation));
        self.deadlines.stroke_game = Some((now + plan.game_timeout(), game_generation));
        self.broadcast_match(RoomEvent::StrokePhase {
            match_id: mark.match_id(),
            phase: self.stroke.phase(),
        });
        self.broadcast_match(RoomEvent::StrokeTurn(self.stroke.phase()));
        Ok(())
    }

    fn stroke_action(
        &mut self,
        caller: PlayerConnectionId,
        action: StrokeShotAction,
    ) -> Result<RelayDisposition, StrokeMatchError> {
        self.stroke_participant(caller)?;
        let disposition = self.stroke.accept_action(caller, action)?;
        if disposition == RelayDisposition::Accepted {
            self.broadcast_match(RoomEvent::StrokeActionRelay {
                from: caller,
                action,
            });
        }
        Ok(disposition)
    }

    fn stroke_result(
        &mut self,
        caller: PlayerConnectionId,
        result: StrokeShotResult,
    ) -> Result<StrokeRelayOutcome, StrokeMatchError> {
        self.stroke_participant(caller)?;
        let outcome = self.stroke.accept_result(caller, result)?;
        if outcome.disposition() == RelayDisposition::Accepted {
            self.broadcast_match(RoomEvent::StrokeResultRelay {
                from: caller,
                result,
            });
            if let Some(commit) = outcome.settlement() {
                self.deadlines.clear_stroke();
                self.broadcast_match(RoomEvent::StrokePhase {
                    match_id: commit.match_id(),
                    phase: self.stroke.phase(),
                });
                let _delivered = self.request_stroke_settlement(commit, None);
            } else {
                let plan = self
                    .stroke
                    .start_plan()
                    .ok_or(StrokeMatchError::InvalidPhase)?;
                let generation = self
                    .stroke
                    .turn_generation()
                    .ok_or(StrokeMatchError::Invariant)?;
                self.deadlines.stroke_turn =
                    Some((Instant::now() + plan.turn_timeout(), generation));
                self.broadcast_match(RoomEvent::StrokeTurn(self.stroke.phase()));
            }
        }
        Ok(outcome)
    }

    fn retail_relay(
        &mut self,
        caller: PlayerConnectionId,
        relay: RetailMatchRelay,
    ) -> Result<(), StrokeMatchError> {
        self.stroke_participant(caller)?;
        if !relay.is_bounded() {
            return Err(StrokeMatchError::Invariant);
        }
        if !matches!(
            self.stroke.phase(),
            StrokeMatchPhase::AwaitAction { .. } | StrokeMatchPhase::AwaitResult { .. }
        ) {
            return Err(StrokeMatchError::InvalidPhase);
        }
        self.broadcast_match(RoomEvent::RetailRelay {
            from: caller,
            relay,
        });
        Ok(())
    }

    /// Publishes one participant's loading progress to the whole match.
    ///
    /// Unlike a shot relay this belongs to the loading phase, which is the only time the
    /// client sends it and the only time anyone is waiting to see it.
    fn retail_load_progress(
        &mut self,
        caller: PlayerConnectionId,
        progress: u8,
    ) -> Result<(), StrokeMatchError> {
        self.stroke_participant(caller)?;
        self.broadcast_match(RoomEvent::RetailLoadProgress {
            from: caller,
            progress,
        });
        Ok(())
    }

    fn stroke_hole_out(
        &mut self,
        caller: PlayerConnectionId,
    ) -> Result<StrokeHoleOutOutcome, StrokeMatchError> {
        self.stroke_participant(caller)?;
        let active_before = active_participant(self.stroke.phase());
        let outcome = self.stroke.hole_out(caller)?;
        match outcome {
            StrokeHoleOutOutcome::Settlement(commit) => {
                self.deadlines.clear_stroke();
                self.broadcast_match(RoomEvent::StrokePhase {
                    match_id: commit.match_id(),
                    phase: self.stroke.phase(),
                });
                let _delivered = self.request_stroke_settlement(commit, None);
            }
            // Only a changed turn is announced. Finishing out of turn leaves the other
            // participant mid-shot, and re-announcing their turn would restart it.
            StrokeHoleOutOutcome::Waiting => {
                if active_participant(self.stroke.phase()) != active_before {
                    let plan = self
                        .stroke
                        .start_plan()
                        .ok_or(StrokeMatchError::InvalidPhase)?;
                    let generation = self
                        .stroke
                        .turn_generation()
                        .ok_or(StrokeMatchError::Invariant)?;
                    self.deadlines.stroke_turn =
                        Some((Instant::now() + plan.turn_timeout(), generation));
                    self.broadcast_match(RoomEvent::StrokeTurn(self.stroke.phase()));
                }
            }
            StrokeHoleOutOutcome::Duplicate => {}
        }
        Ok(outcome)
    }

    fn stroke_give_up(
        &mut self,
        caller: PlayerConnectionId,
    ) -> Result<CommitStrokeMatch, StrokeMatchError> {
        self.stroke_participant(caller)?;
        let commit = self.stroke.give_up(caller)?;
        self.deadlines.clear_stroke();
        self.broadcast_match(RoomEvent::StrokePhase {
            match_id: commit.match_id(),
            phase: self.stroke.phase(),
        });
        let _delivered = self.request_stroke_settlement(commit, None);
        Ok(commit)
    }

    fn apply_stroke_commit(
        &mut self,
        result: StrokeMatchResult,
    ) -> Result<StrokeMatchResult, StrokeMatchError> {
        let roster = self
            .stroke
            .roster()
            .copied()
            .ok_or(StrokeMatchError::InvalidPhase)?;
        let committed = self.stroke.apply_commit(result)?;
        self.pending_stroke_persistence = None;
        self.stroke_persistence_event_delivered = false;
        self.stroke_persistence_control_delivered = false;
        self.deadlines.clear_stroke();
        self.broadcast_connections(&roster, RoomEvent::StrokeCommitted(committed));
        Ok(committed)
    }

    fn abort_stroke(&mut self, reason: MatchAbortReason) -> Option<AbortStrokeMatch> {
        self.deadlines.clear_stroke();
        if let Some(pending) = self.pending_stroke_persistence {
            match pending {
                PendingStrokePersistence::Abort(abort) => return Some(abort),
                PendingStrokePersistence::Settlement(_) => {
                    self.pending_stroke_persistence = None;
                    self.stroke_persistence_event_delivered = false;
                    self.stroke_persistence_control_delivered = false;
                }
            }
        }
        let abort = self.stroke.abort(reason);
        if let Some(abort) = abort {
            let _delivered = self.request_stroke_abort(abort, None);
        }
        abort
    }

    fn acknowledge_stroke_abort(
        &mut self,
        abort: AbortStrokeMatch,
    ) -> Result<(), StrokeMatchError> {
        let roster = self
            .stroke
            .roster()
            .copied()
            .ok_or(StrokeMatchError::InvalidPhase)?;
        if self.stroke.pending_abort().is_none() {
            let prepared = self
                .stroke
                .abort(abort.reason())
                .ok_or(StrokeMatchError::InvalidPhase)?;
            if prepared != abort {
                return Err(StrokeMatchError::IdentityMismatch);
            }
        }
        self.stroke.acknowledge_abort(abort)?;
        self.pending_stroke_persistence = None;
        self.stroke_persistence_event_delivered = false;
        self.stroke_persistence_control_delivered = false;
        self.broadcast_connections(&roster, RoomEvent::StrokeAborted(abort));
        Ok(())
    }

    fn request_stroke_settlement(
        &mut self,
        commit: CommitStrokeMatch,
        excluded: Option<PlayerConnectionId>,
    ) -> bool {
        self.request_stroke_persistence(PendingStrokePersistence::Settlement(commit), excluded)
    }

    fn request_stroke_abort(
        &mut self,
        abort: AbortStrokeMatch,
        excluded: Option<PlayerConnectionId>,
    ) -> bool {
        self.request_stroke_persistence(PendingStrokePersistence::Abort(abort), excluded)
    }

    fn request_stroke_persistence(
        &mut self,
        work: PendingStrokePersistence,
        excluded: Option<PlayerConnectionId>,
    ) -> bool {
        if !self.retain_stroke_persistence(work) {
            return false;
        }
        if self.stroke_persistence_event_delivered {
            return true;
        }
        // Persistence authority is exclusive: once priority cleanup claimed the work, no
        // connection may also receive a coordinator request for the same repository operation.
        if self.stroke_persistence_control_delivered {
            return false;
        }

        let mut candidates: Vec<_> = self
            .members
            .iter()
            .enumerate()
            .filter(|(_, member)| {
                Some(member.identity.connection_id) != excluded
                    && !member.cancellation.is_cancelled()
            })
            .map(|(index, _)| index)
            .collect();
        candidates.sort_by_key(|index| {
            let member = &self.members[*index];
            (
                !member.owner,
                member.joined_order,
                member.identity.connection_id,
            )
        });
        for index in candidates {
            let Some(member) = self.members.get(index) else {
                continue;
            };
            if self.deliver(member, work.event()) {
                self.stroke_persistence_event_delivered = true;
                return true;
            }
        }
        false
    }

    fn retain_stroke_persistence(&mut self, work: PendingStrokePersistence) -> bool {
        match self.pending_stroke_persistence {
            Some(current) => current == work,
            None => {
                self.pending_stroke_persistence = Some(work);
                self.stroke_persistence_event_delivered = false;
                self.stroke_persistence_control_delivered = false;
                true
            }
        }
    }

    fn claim_stroke_persistence(&mut self) -> RoomCloseOutcome {
        // Automatic deadline/give-up work is owned by the one connection that received the
        // coordinator event. Priority cleanup may claim only work that has not been enqueued.
        if self.stroke_persistence_control_delivered || self.stroke_persistence_event_delivered {
            return RoomCloseOutcome::None;
        }
        let Some(work) = self.pending_stroke_persistence else {
            return RoomCloseOutcome::None;
        };
        self.stroke_persistence_control_delivered = true;
        work.outcome(self.id)
    }

    fn disconnect_outcome(
        &mut self,
        caller: PlayerConnectionId,
        solo_reason: MatchAbortReason,
    ) -> RoomCloseOutcome {
        if self.stroke.is_active() {
            if solo_reason == MatchAbortReason::Shutdown {
                if self.stroke_persistence_control_delivered {
                    return RoomCloseOutcome::None;
                }
                self.prepare_priority_stroke_abort(MatchAbortReason::Shutdown);
                return self.claim_stroke_persistence();
            }
            if self.pending_stroke_persistence.is_some() {
                return self.claim_stroke_persistence();
            }
            match self.stroke.disconnect(caller) {
                Ok(StrokeDeadlineOutcome::Aborted(abort)) => {
                    self.deadlines.clear_stroke();
                    let _retained =
                        self.retain_stroke_persistence(PendingStrokePersistence::Abort(abort));
                    self.claim_stroke_persistence()
                }
                Ok(StrokeDeadlineOutcome::Settlement(commit)) => {
                    self.deadlines.clear_stroke();
                    self.broadcast_match(RoomEvent::StrokePhase {
                        match_id: commit.match_id(),
                        phase: self.stroke.phase(),
                    });
                    let _retained = self
                        .retain_stroke_persistence(PendingStrokePersistence::Settlement(commit));
                    self.claim_stroke_persistence()
                }
                Ok(StrokeDeadlineOutcome::Stale) | Err(_) => RoomCloseOutcome::None,
            }
        } else {
            self.mark_aborted(solo_reason)
                .map_or(RoomCloseOutcome::None, |abort| RoomCloseOutcome::M5Abort {
                    room_id: self.id,
                    request: abort,
                })
        }
    }

    fn prepare_priority_stroke_abort(&mut self, reason: MatchAbortReason) {
        self.deadlines.clear_stroke();
        if matches!(
            self.pending_stroke_persistence,
            Some(PendingStrokePersistence::Abort(abort)) if abort.reason() == reason
        ) {
            return;
        }
        self.pending_stroke_persistence = None;
        self.stroke_persistence_event_delivered = false;
        self.stroke_persistence_control_delivered = false;
        if let Some(abort) = self.stroke.prioritize_abort(reason) {
            self.pending_stroke_persistence = Some(PendingStrokePersistence::Abort(abort));
        }
    }

    fn prioritize_and_claim_stroke_abort(
        &mut self,
        reason: MatchAbortReason,
    ) -> Option<AbortStrokeMatch> {
        self.prepare_priority_stroke_abort(reason);
        let Some(PendingStrokePersistence::Abort(abort)) = self.pending_stroke_persistence else {
            return None;
        };
        // Replacement transfers the existing event/control claim to this control caller. Keep the
        // claim marked until durable acknowledgement; concurrent cleanup must never retry it.
        self.stroke_persistence_event_delivered = false;
        self.stroke_persistence_control_delivered = true;
        Some(abort)
    }

    fn shutdown_outcome(&mut self) -> RoomCloseOutcome {
        if self.stroke.is_active() {
            if self.stroke_persistence_control_delivered {
                return RoomCloseOutcome::None;
            }
            self.prepare_priority_stroke_abort(MatchAbortReason::Shutdown);
            self.claim_stroke_persistence()
        } else {
            self.mark_aborted(MatchAbortReason::Shutdown)
                .map_or(RoomCloseOutcome::None, |abort| RoomCloseOutcome::M5Abort {
                    room_id: self.id,
                    request: abort,
                })
        }
    }

    fn handle_deadline(&mut self, kind: MatchDeadlineKind) -> RoomCloseOutcome {
        self.deadlines.clear_kind(kind);
        match kind {
            MatchDeadlineKind::SoloLoading => self
                .mark_aborted(MatchAbortReason::LoadingTimeout)
                .map_or(RoomCloseOutcome::None, |abort| RoomCloseOutcome::M5Abort {
                    room_id: self.id,
                    request: abort,
                }),
            MatchDeadlineKind::StrokeLoading => {
                match self.stroke.deadline_expired(StrokeDeadline::Loading) {
                    Ok(StrokeDeadlineOutcome::Aborted(abort)) => {
                        let _delivered = self.request_stroke_abort(abort, None);
                        RoomCloseOutcome::M6Abort {
                            room_id: self.id,
                            request: abort,
                        }
                    }
                    _ => RoomCloseOutcome::None,
                }
            }
            MatchDeadlineKind::StrokeTurn(generation) => {
                self.handle_stroke_deadline(StrokeDeadline::Turn { generation })
            }
            MatchDeadlineKind::StrokeGame(generation) => {
                self.handle_stroke_deadline(StrokeDeadline::Game { generation })
            }
        }
    }

    fn handle_stroke_deadline(&mut self, deadline: StrokeDeadline) -> RoomCloseOutcome {
        match self.stroke.deadline_expired(deadline) {
            Ok(StrokeDeadlineOutcome::Settlement(commit)) => {
                self.deadlines.clear_stroke();
                self.broadcast_match(RoomEvent::StrokePhase {
                    match_id: commit.match_id(),
                    phase: self.stroke.phase(),
                });
                let _delivered = self.request_stroke_settlement(commit, None);
                RoomCloseOutcome::M6Settlement {
                    room_id: self.id,
                    request: commit,
                }
            }
            Ok(StrokeDeadlineOutcome::Aborted(abort)) => {
                self.deadlines.clear_stroke();
                let _delivered = self.request_stroke_abort(abort, None);
                RoomCloseOutcome::M6Abort {
                    room_id: self.id,
                    request: abort,
                }
            }
            Ok(StrokeDeadlineOutcome::Stale) | Err(_) => RoomCloseOutcome::None,
        }
    }

    fn mark_aborted(&mut self, reason: MatchAbortReason) -> Option<AbortMatch> {
        self.deadlines.clear_solo();
        let abort = self.solo.abort(reason);
        if let Some(abort) = abort {
            self.deliver_solo(RoomEvent::AbortRequested(abort));
        }
        abort
    }

    fn deliver_solo(&self, event: RoomEvent) {
        if let Some(member) = self.members.first() {
            let _delivered = self.deliver(member, event);
        }
    }

    fn broadcast_match(&self, event: RoomEvent) {
        if let Some(roster) = self.stroke.roster() {
            self.broadcast_connections(roster, event);
        }
    }

    fn broadcast_connections(&self, roster: &[PlayerConnectionId; 2], event: RoomEvent) {
        for connection_id in roster {
            if let Some(index) = self.member_index(*connection_id)
                && let Some(member) = self.members.get(index)
            {
                let _delivered = self.deliver(member, event.clone());
            }
        }
    }

    fn broadcast_snapshot(&self, snapshot: &RoomSnapshot) {
        for member in &self.members {
            let _delivered = self.deliver(member, RoomEvent::Snapshot(snapshot.clone()));
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
    PrepareSoloStart {
        caller: PlayerConnectionId,
        plan: SoloStartPlan,
        reply: oneshot::Sender<Result<BeginSoloMatch, SoloMatchError>>,
    },
    ConfirmSoloBegin {
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
        reply: oneshot::Sender<Result<(), SoloMatchError>>,
    },
    CancelSoloBegin {
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
        reply: oneshot::Sender<Result<(), SoloMatchError>>,
    },
    SoloLoading {
        caller: PlayerConnectionId,
        loading: LoadingComplete,
        reply: oneshot::Sender<Result<MarkSoloInGame, SoloMatchError>>,
    },
    SoloAction {
        caller: PlayerConnectionId,
        action: ShotAction,
        reply: oneshot::Sender<Result<RelayDisposition, SoloMatchError>>,
    },
    SoloResult {
        caller: PlayerConnectionId,
        result: ShotResult,
        reply: oneshot::Sender<Result<RelayDisposition, SoloMatchError>>,
    },
    SoloHoleOut {
        caller: PlayerConnectionId,
        reply: oneshot::Sender<Result<RelayDisposition, SoloMatchError>>,
    },
    PrepareSoloFinish {
        caller: PlayerConnectionId,
        reply: oneshot::Sender<Result<CommitSoloHole, SoloMatchError>>,
    },
    ApplySoloCommit {
        caller: PlayerConnectionId,
        result: SoloMatchResult,
        reply: oneshot::Sender<Result<SoloMatchResult, SoloMatchError>>,
    },
    AbortSolo {
        caller: PlayerConnectionId,
        reason: MatchAbortReason,
        reply: oneshot::Sender<Result<Option<AbortMatch>, SoloMatchError>>,
    },
    AcknowledgeSoloAbort {
        caller: PlayerConnectionId,
        abort: AbortMatch,
        reply: oneshot::Sender<Result<(), SoloMatchError>>,
    },
    PrepareStrokeStart {
        caller: PlayerConnectionId,
        plan: StrokeStartPlan,
        reply: oneshot::Sender<Result<BeginStrokeMatch, StrokeMatchError>>,
    },
    ConfirmStrokeBegin {
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
        reply: oneshot::Sender<Result<(), StrokeMatchError>>,
    },
    CancelStrokeBegin {
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
        reply: oneshot::Sender<Result<(), StrokeMatchError>>,
    },
    StrokeLoading {
        caller: PlayerConnectionId,
        loading: StrokeLoadingComplete,
        reply: oneshot::Sender<Result<StrokeLoadingOutcome, StrokeMatchError>>,
    },
    ConfirmStrokeInGame {
        mark: MarkStrokeInGame,
        reply: oneshot::Sender<Result<(), StrokeMatchError>>,
    },
    StrokeAction {
        caller: PlayerConnectionId,
        action: StrokeShotAction,
        reply: oneshot::Sender<Result<RelayDisposition, StrokeMatchError>>,
    },
    StrokeResult {
        caller: PlayerConnectionId,
        result: StrokeShotResult,
        reply: oneshot::Sender<Result<StrokeRelayOutcome, StrokeMatchError>>,
    },
    StrokeHoleOut {
        caller: PlayerConnectionId,
        reply: oneshot::Sender<Result<StrokeHoleOutOutcome, StrokeMatchError>>,
    },
    RetailRelay {
        caller: PlayerConnectionId,
        relay: RetailMatchRelay,
        reply: oneshot::Sender<Result<(), StrokeMatchError>>,
    },
    RetailLoadProgress {
        caller: PlayerConnectionId,
        progress: u8,
        reply: oneshot::Sender<Result<(), StrokeMatchError>>,
    },
    StrokeGiveUp {
        caller: PlayerConnectionId,
        reply: oneshot::Sender<Result<CommitStrokeMatch, StrokeMatchError>>,
    },
    ApplyStrokeCommit {
        result: StrokeMatchResult,
        reply: oneshot::Sender<Result<StrokeMatchResult, StrokeMatchError>>,
    },
    PrioritizeStrokeAbort {
        reason: MatchAbortReason,
        reply: oneshot::Sender<Option<AbortStrokeMatch>>,
    },
    AbortStroke {
        reason: MatchAbortReason,
        reply: oneshot::Sender<Option<AbortStrokeMatch>>,
    },
    AcknowledgeStrokeAbort {
        abort: AbortStrokeMatch,
        reply: oneshot::Sender<Result<(), StrokeMatchError>>,
    },
}

/// Persistence work retained by a room closure/control transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoomCloseOutcome {
    /// No active persistence work.
    None,
    /// M5 no-reward abort.
    M5Abort {
        /// Authoritative room identity.
        room_id: RoomId,
        /// Exact solo abort request.
        request: AbortMatch,
    },
    /// M6 no-reward aggregate abort.
    M6Abort {
        /// Authoritative room identity.
        room_id: RoomId,
        /// Exact aggregate abort request.
        request: AbortStrokeMatch,
    },
    /// M6 aggregate settlement after an in-game forfeit/deadline.
    M6Settlement {
        /// Authoritative room identity.
        room_id: RoomId,
        /// Exact aggregate settlement request.
        request: CommitStrokeMatch,
    },
}

impl RoomCloseOutcome {
    /// Authoritative room identity when work exists.
    #[must_use]
    pub const fn room_id(self) -> Option<RoomId> {
        match self {
            Self::None => None,
            Self::M5Abort { room_id, .. }
            | Self::M6Abort { room_id, .. }
            | Self::M6Settlement { room_id, .. } => Some(room_id),
        }
    }

    /// Durable match identity when work exists.
    #[must_use]
    pub const fn match_id(self) -> Option<MatchId> {
        match self {
            Self::None => None,
            Self::M5Abort { request, .. } => Some(request.match_id()),
            Self::M6Abort { request, .. } => Some(request.match_id()),
            Self::M6Settlement { request, .. } => Some(request.match_id()),
        }
    }
}

/// Atomic priority-disconnect output used by the lobby registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomDisconnect {
    /// Post-removal room projection, or `None` when no members remain.
    pub snapshot: Option<RoomSnapshot>,
    /// Whether the empty actor must remain alive for claimed, unacknowledged M6 persistence.
    pub retain_for_persistence: bool,
    /// Closed persistence work with room/match authority.
    pub outcome: RoomCloseOutcome,
    /// Compatibility M5 abort projection.
    pub abort: Option<AbortMatch>,
}

enum ControlCommand {
    Disconnect {
        caller: PlayerConnectionId,
        reason: MatchAbortReason,
        reply: oneshot::Sender<Result<RoomDisconnect, RoomError>>,
    },
    Shutdown {
        reply: oneshot::Sender<RoomCloseOutcome>,
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

    fn send_solo(&self, command: RoomCommand) -> Result<(), SoloMatchError> {
        self.send_normal(command).map_err(map_room_to_solo)
    }

    /// Reserves a checked start plan on the normal gameplay queue.
    pub async fn prepare_solo_start(
        &self,
        caller: PlayerConnectionId,
        plan: SoloStartPlan,
    ) -> Result<BeginSoloMatch, SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_solo(RoomCommand::PrepareSoloStart {
            caller,
            plan,
            reply,
        })?;
        receive.await.map_err(|_| SoloMatchError::Closed)?
    }

    /// Confirms exact begin persistence and starts the actor-owned loading deadline.
    pub async fn confirm_solo_begin(
        &self,
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_solo(RoomCommand::ConfirmSoloBegin {
            caller,
            match_id,
            result_key,
            reply,
        })?;
        receive.await.map_err(|_| SoloMatchError::Closed)?
    }

    /// Cancels an exact unpersisted reservation after begin persistence failed/cancelled.
    pub async fn cancel_solo_begin(
        &self,
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_solo(RoomCommand::CancelSoloBegin {
            caller,
            match_id,
            result_key,
            reply,
        })?;
        receive.await.map_err(|_| SoloMatchError::Closed)?
    }

    /// Applies validated canonical loading completion.
    pub async fn solo_loading_complete(
        &self,
        caller: PlayerConnectionId,
        loading: LoadingComplete,
    ) -> Result<MarkSoloInGame, SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_solo(RoomCommand::SoloLoading {
            caller,
            loading,
            reply,
        })?;
        receive.await.map_err(|_| SoloMatchError::Closed)?
    }

    /// Applies a validated sequenced action.
    pub async fn solo_action(
        &self,
        caller: PlayerConnectionId,
        action: ShotAction,
    ) -> Result<RelayDisposition, SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_solo(RoomCommand::SoloAction {
            caller,
            action,
            reply,
        })?;
        receive.await.map_err(|_| SoloMatchError::Closed)?
    }

    /// Applies a validated result matching the pending action.
    pub async fn solo_result(
        &self,
        caller: PlayerConnectionId,
        result: ShotResult,
    ) -> Result<RelayDisposition, SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_solo(RoomCommand::SoloResult {
            caller,
            result,
            reply,
        })?;
        receive.await.map_err(|_| SoloMatchError::Closed)?
    }

    /// Marks the already-counted retail shot as holed.
    pub async fn solo_hole_out(
        &self,
        caller: PlayerConnectionId,
    ) -> Result<RelayDisposition, SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_solo(RoomCommand::SoloHoleOut { caller, reply })?;
        receive.await.map_err(|_| SoloMatchError::Closed)?
    }

    /// Builds the server-owned commit request after hole completion.
    pub async fn prepare_solo_finish(
        &self,
        caller: PlayerConnectionId,
    ) -> Result<CommitSoloHole, SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_solo(RoomCommand::PrepareSoloFinish { caller, reply })?;
        receive.await.map_err(|_| SoloMatchError::Closed)?
    }

    /// Applies an exact trusted repository result and clears the match.
    pub async fn apply_solo_commit(
        &self,
        caller: PlayerConnectionId,
        result: SoloMatchResult,
    ) -> Result<SoloMatchResult, SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_solo(RoomCommand::ApplySoloCommit {
            caller,
            result,
            reply,
        })?;
        receive.await.map_err(|_| SoloMatchError::Closed)?
    }

    /// Converts an active noncommitted match to a retained no-reward abort.
    pub async fn abort_solo(
        &self,
        caller: PlayerConnectionId,
        reason: MatchAbortReason,
    ) -> Result<Option<AbortMatch>, SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_solo(RoomCommand::AbortSolo {
            caller,
            reason,
            reply,
        })?;
        receive.await.map_err(|_| SoloMatchError::Closed)?
    }

    /// Clears an exact retained abort after persistence acknowledgement.
    pub async fn acknowledge_solo_abort(
        &self,
        caller: PlayerConnectionId,
        abort: AbortMatch,
    ) -> Result<(), SoloMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_solo(RoomCommand::AcknowledgeSoloAbort {
            caller,
            abort,
            reply,
        })?;
        receive.await.map_err(|_| SoloMatchError::Closed)?
    }

    fn send_stroke(&self, command: RoomCommand) -> Result<(), StrokeMatchError> {
        self.send_normal(command).map_err(map_room_to_stroke)
    }

    /// Reserves an exact checked two-player stroke plan.
    pub async fn prepare_stroke_start(
        &self,
        caller: PlayerConnectionId,
        plan: StrokeStartPlan,
    ) -> Result<BeginStrokeMatch, StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::PrepareStrokeStart {
            caller,
            plan,
            reply,
        })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Confirms begin persistence and starts loading deadline.
    pub async fn confirm_stroke_begin(
        &self,
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::ConfirmStrokeBegin {
            caller,
            match_id,
            result_key,
            reply,
        })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Cancels an exact unpersisted stroke reservation.
    pub async fn cancel_stroke_begin(
        &self,
        caller: PlayerConnectionId,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::CancelStrokeBegin {
            caller,
            match_id,
            result_key,
            reply,
        })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Applies one canonical per-member loading completion.
    pub async fn stroke_loading_complete(
        &self,
        caller: PlayerConnectionId,
        loading: StrokeLoadingComplete,
    ) -> Result<StrokeLoadingOutcome, StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::StrokeLoading {
            caller,
            loading,
            reply,
        })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Applies the durable loading-to-in-game confirmation by room/match authority.
    pub async fn confirm_stroke_in_game(
        &self,
        mark: MarkStrokeInGame,
    ) -> Result<(), StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::ConfirmStrokeInGame { mark, reply })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Applies one validated stroke action.
    pub async fn stroke_action(
        &self,
        caller: PlayerConnectionId,
        action: StrokeShotAction,
    ) -> Result<RelayDisposition, StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::StrokeAction {
            caller,
            action,
            reply,
        })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Applies one validated stroke result.
    pub async fn stroke_result(
        &self,
        caller: PlayerConnectionId,
        result: StrokeShotResult,
    ) -> Result<StrokeRelayOutcome, StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::StrokeResult {
            caller,
            result,
            reply,
        })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Relays one participant's own in-match frame to the captured roster.
    ///
    /// # Errors
    ///
    /// Returns an error when the room is closed, the caller is not a participant, the payload
    /// exceeds [`MAX_RETAIL_RELAY_BYTES`], or no hole is being played.
    pub async fn retail_relay(
        &self,
        caller: PlayerConnectionId,
        relay: RetailMatchRelay,
    ) -> Result<(), StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::RetailRelay {
            caller,
            relay,
            reply,
        })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Publishes one participant's hole-loading progress to the whole match.
    ///
    /// # Errors
    ///
    /// Returns an error when the room is closed or the caller is not a participant.
    pub async fn retail_load_progress(
        &self,
        caller: PlayerConnectionId,
        progress: u8,
    ) -> Result<(), StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::RetailLoadProgress {
            caller,
            progress,
            reply,
        })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Records that a participant holed out, and returns automatic settlement once both have.
    ///
    /// # Errors
    ///
    /// Returns an error when the room is closed or the aggregate rejects the completion.
    pub async fn stroke_hole_out(
        &self,
        caller: PlayerConnectionId,
    ) -> Result<StrokeHoleOutOutcome, StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::StrokeHoleOut { caller, reply })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Applies any participant's give-up and returns automatic settlement.
    pub async fn stroke_give_up(
        &self,
        caller: PlayerConnectionId,
    ) -> Result<CommitStrokeMatch, StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::StrokeGiveUp { caller, reply })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Applies trusted aggregate persistence output without requiring a connection mapping.
    pub async fn apply_stroke_commit(
        &self,
        result: StrokeMatchResult,
    ) -> Result<StrokeMatchResult, StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::ApplyStrokeCommit { result, reply })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    /// Replaces any unacknowledged aggregate outcome with a priority no-reward abort.
    pub async fn prioritize_stroke_abort(
        &self,
        reason: MatchAbortReason,
    ) -> Result<Option<AbortStrokeMatch>, StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::PrioritizeStrokeAbort { reason, reply })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)
    }

    /// Converts any active stroke state to an exact no-reward abort.
    pub async fn abort_stroke(
        &self,
        reason: MatchAbortReason,
    ) -> Result<Option<AbortStrokeMatch>, StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::AbortStroke { reason, reply })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)
    }

    /// Acknowledges an exact stroke abort by room/match authority.
    pub async fn acknowledge_stroke_abort(
        &self,
        abort: AbortStrokeMatch,
    ) -> Result<(), StrokeMatchError> {
        let (reply, receive) = oneshot::channel();
        self.send_stroke(RoomCommand::AcknowledgeStrokeAbort { abort, reply })?;
        receive.await.map_err(|_| StrokeMatchError::Closed)?
    }

    async fn send_control(&self, command: ControlCommand) -> Result<(), RoomError> {
        timeout(self.control_timeout, self.control.send(command))
            .await
            .map_err(|_| RoomError::Timeout)?
            .map_err(|_| RoomError::Closed)
    }

    /// Removes a dropped connection through the priority control queue.
    ///
    /// This compatibility projection discards the abort output. Lobby cleanup uses
    /// [`Self::disconnect_with_abort`] so persistence callers cannot lose it.
    pub async fn disconnect(
        &self,
        caller: PlayerConnectionId,
    ) -> Result<Option<RoomSnapshot>, RoomError> {
        self.disconnect_with_abort(caller, MatchAbortReason::Disconnect)
            .await
            .map(|outcome| outcome.snapshot)
    }

    /// Atomically produces an idempotent abort before removing the dropped member.
    pub async fn disconnect_with_abort(
        &self,
        caller: PlayerConnectionId,
        reason: MatchAbortReason,
    ) -> Result<RoomDisconnect, RoomError> {
        let (reply, receive) = oneshot::channel();
        self.send_control(ControlCommand::Disconnect {
            caller,
            reason,
            reply,
        })
        .await?;
        receive.await.map_err(|_| RoomError::Closed)?
    }

    /// Shuts down through the priority queue with a bounded deadline.
    ///
    /// Returns the retained idempotent abort for any noncommitted match even when member event
    /// delivery is saturated.
    pub async fn shutdown(&self) -> Result<RoomCloseOutcome, RoomError> {
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
        RoomError::QueueFull => SoloMatchError::QueueFull,
        RoomError::Timeout => SoloMatchError::Timeout,
        _ => SoloMatchError::Closed,
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
        let scheduled = state.deadlines.next();
        let deadline = scheduled.map_or_else(Instant::now, |deadline| deadline.at);
        let sleeper = sleep_until(deadline);
        tokio::pin!(sleeper);
        tokio::select! {
            biased;
            () = &mut sleeper, if scheduled.is_some() => {
                if let Some(deadline) = scheduled {
                    let _work = state.handle_deadline(deadline.kind);
                }
            }
            command = control.recv() => match command {
                Some(ControlCommand::Disconnect { caller, reason, reply }) => {
                    let result = if state.member_index(caller).is_none() {
                        Err(RoomError::NotMember)
                    } else {
                        let outcome = state.disconnect_outcome(caller, reason);
                        let abort = match outcome {
                            RoomCloseOutcome::M5Abort { request, .. } => Some(request),
                            _ => None,
                        };
                        state.remove(caller).map(|snapshot| {
                            let retain_for_persistence = snapshot.is_none()
                                && state.pending_stroke_persistence.is_some();
                            RoomDisconnect {
                                snapshot,
                                retain_for_persistence,
                                outcome,
                                abort,
                            }
                        })
                    };
                    if let Ok(outcome) = &result {
                        open = outcome.snapshot.is_some() || outcome.retain_for_persistence;
                        after_mutation(&state, outcome.snapshot.as_ref(), events.as_ref());
                    }
                    let _ignored = reply.send(result);
                }
                Some(ControlCommand::Shutdown { reply }) => {
                    let outcome = state.shutdown_outcome();
                    for member in &state.members {
                        let _delivered = state.deliver(member, RoomEvent::Closed);
                    }
                    let _ignored = reply.send(outcome);
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
            let result = if state.match_active() {
                Err(RoomError::MatchActive)
            } else {
                state.remove(caller)
            };
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
        RoomCommand::PrepareSoloStart {
            caller,
            plan,
            reply,
        } => {
            let _ignored = reply.send(state.prepare_solo_start(caller, plan));
            true
        }
        RoomCommand::ConfirmSoloBegin {
            caller,
            match_id,
            result_key,
            reply,
        } => {
            let _ignored = reply.send(state.confirm_solo_begin(caller, match_id, result_key));
            true
        }
        RoomCommand::CancelSoloBegin {
            caller,
            match_id,
            result_key,
            reply,
        } => {
            let _ignored = reply.send(state.cancel_solo_begin(caller, match_id, result_key));
            true
        }
        RoomCommand::SoloLoading {
            caller,
            loading,
            reply,
        } => {
            let _ignored = reply.send(state.solo_loading_complete(caller, loading));
            true
        }
        RoomCommand::SoloAction {
            caller,
            action,
            reply,
        } => {
            let _ignored = reply.send(state.solo_action(caller, action));
            true
        }
        RoomCommand::SoloResult {
            caller,
            result,
            reply,
        } => {
            let _ignored = reply.send(state.solo_result(caller, result));
            true
        }
        RoomCommand::SoloHoleOut { caller, reply } => {
            let _ignored = reply.send(state.solo_hole_out(caller));
            true
        }
        RoomCommand::PrepareSoloFinish { caller, reply } => {
            let _ignored = reply.send(state.prepare_solo_finish(caller));
            true
        }
        RoomCommand::ApplySoloCommit {
            caller,
            result,
            reply,
        } => {
            let _ignored = reply.send(state.apply_solo_commit(caller, result));
            true
        }
        RoomCommand::AbortSolo {
            caller,
            reason,
            reply,
        } => {
            let _ignored = reply.send(state.abort_solo(caller, reason));
            true
        }
        RoomCommand::AcknowledgeSoloAbort {
            caller,
            abort,
            reply,
        } => {
            let _ignored = reply.send(state.acknowledge_solo_abort(caller, abort));
            true
        }
        RoomCommand::PrepareStrokeStart {
            caller,
            plan,
            reply,
        } => {
            let _ignored = reply.send(state.prepare_stroke_start(caller, plan));
            true
        }
        RoomCommand::ConfirmStrokeBegin {
            caller,
            match_id,
            result_key,
            reply,
        } => {
            let _ignored = reply.send(state.confirm_stroke_begin(caller, match_id, result_key));
            true
        }
        RoomCommand::CancelStrokeBegin {
            caller,
            match_id,
            result_key,
            reply,
        } => {
            let _ignored = reply.send(state.cancel_stroke_begin(caller, match_id, result_key));
            true
        }
        RoomCommand::StrokeLoading {
            caller,
            loading,
            reply,
        } => {
            let _ignored = reply.send(state.stroke_loading_complete(caller, loading));
            true
        }
        RoomCommand::ConfirmStrokeInGame { mark, reply } => {
            let _ignored = reply.send(state.confirm_stroke_in_game(mark));
            true
        }
        RoomCommand::StrokeAction {
            caller,
            action,
            reply,
        } => {
            let _ignored = reply.send(state.stroke_action(caller, action));
            true
        }
        RoomCommand::StrokeResult {
            caller,
            result,
            reply,
        } => {
            let _ignored = reply.send(state.stroke_result(caller, result));
            true
        }
        RoomCommand::StrokeHoleOut { caller, reply } => {
            let _ignored = reply.send(state.stroke_hole_out(caller));
            true
        }
        RoomCommand::RetailRelay {
            caller,
            relay,
            reply,
        } => {
            let _ignored = reply.send(state.retail_relay(caller, relay));
            true
        }
        RoomCommand::RetailLoadProgress {
            caller,
            progress,
            reply,
        } => {
            let _ignored = reply.send(state.retail_load_progress(caller, progress));
            true
        }
        RoomCommand::StrokeGiveUp { caller, reply } => {
            let _ignored = reply.send(state.stroke_give_up(caller));
            true
        }
        RoomCommand::ApplyStrokeCommit { result, reply } => {
            let result = state.apply_stroke_commit(result);
            let open = result.is_err() || !state.members.is_empty();
            let _ignored = reply.send(result);
            open
        }
        RoomCommand::PrioritizeStrokeAbort { reason, reply } => {
            let abort = state.prioritize_and_claim_stroke_abort(reason);
            let _ignored = reply.send(abort);
            true
        }
        RoomCommand::AbortStroke { reason, reply } => {
            let _ignored = reply.send(state.abort_stroke(reason));
            true
        }
        RoomCommand::AcknowledgeStrokeAbort { abort, reply } => {
            let result = state.acknowledge_stroke_abort(abort);
            let open = result.is_err() || !state.members.is_empty();
            let _ignored = reply.send(result);
            open
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

    use pangya_domain::{
        CatalogFingerprint, CourseId, MatchSeed, OneHoleConfig, ServerBalances, SoloReward,
        StrokeCount, StrokeParticipant, StrokePlayerResult, StrokeRosterOrder,
        synthetic_stroke_reward_v1,
    };
    use pangya_protocol::Lie;
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::match_state::deterministic_conditions;

    fn id(value: u64) -> PlayerConnectionId {
        PlayerConnectionId::new(value).unwrap_or_else(|_| unreachable!())
    }

    fn identity(value: u64) -> RoomIdentity {
        RoomIdentity {
            connection_id: id(value),
            account_id: AccountId::new(i64::try_from(value).unwrap_or(1))
                .unwrap_or_else(|_| unreachable!()),
            nickname: Nickname::parse(&format!("Player{value}")).unwrap_or_else(|_| unreachable!()),
            character_id: None,
            character_iff_id: None,
            card: pangya_domain::MemberCard::default(),
        }
    }

    fn solo_plan(account_id: AccountId, timeout: Duration, max_strokes: u8) -> SoloStartPlan {
        let seed = MatchSeed::new([0; 32]);
        let (weather, wind) = deterministic_conditions(seed).unwrap_or_else(|_| unreachable!());
        SoloStartPlan::new(
            BeginSoloMatch::new(
                MatchId::new(Uuid::from_u128(101)),
                MatchResultKey::new(Uuid::from_u128(102)),
                account_id,
                OneHoleConfig::new(CourseId::new(1).unwrap_or_else(|_| unreachable!()), 4)
                    .unwrap_or_else(|_| unreachable!()),
                CatalogFingerprint::new([3; 32]),
                seed,
                weather,
                wind,
            ),
            timeout,
            max_strokes,
        )
        .unwrap_or_else(|_| unreachable!())
    }

    fn stroke_plan(
        first: &RoomIdentity,
        second: &RoomIdentity,
        loading: Duration,
        turn: Duration,
        game: Duration,
        max_strokes: u8,
    ) -> StrokeStartPlan {
        let seed = MatchSeed::new([0; 32]);
        let (weather, wind) = deterministic_conditions(seed).unwrap_or_else(|_| unreachable!());
        let begin = BeginStrokeMatch::new(
            MatchId::new(Uuid::from_u128(201)),
            MatchResultKey::new(Uuid::from_u128(202)),
            [
                StrokeParticipant::new(
                    first.account_id,
                    StrokeRosterOrder::First,
                    MatchResultKey::new(Uuid::from_u128(203)),
                ),
                StrokeParticipant::new(
                    second.account_id,
                    StrokeRosterOrder::Second,
                    MatchResultKey::new(Uuid::from_u128(204)),
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
            loading,
            turn,
            game,
            max_strokes,
        )
        .unwrap_or_else(|_| unreachable!())
    }

    fn persisted_stroke(commit: CommitStrokeMatch) -> StrokeMatchResult {
        let players = [0_usize, 1_usize].map(|index| {
            let input = commit.players()[index];
            let reward =
                synthetic_stroke_reward_v1(commit.config(), input.strokes(), input.completion())
                    .expect("reward");
            StrokePlayerResult::new(input, reward, ServerBalances::from_persisted(100, 100))
        });
        StrokeMatchResult::new(commit.match_id(), commit.result_key(), players)
    }

    fn playing_stroke_room(state: &mut RoomState, plan: &StrokeStartPlan) {
        let roster = *plan.roster();
        state
            .prepare_stroke_start(roster[0], plan.clone())
            .expect("prepare stroke");
        state
            .confirm_stroke_begin(
                roster[0],
                plan.begin().match_id(),
                plan.begin().result_key(),
            )
            .expect("confirm stroke");
        let loading = StrokeLoadingComplete::new(100).expect("loading");
        state
            .stroke_loading_complete(roster[0], loading)
            .expect("first load");
        let StrokeLoadingOutcome::PersistenceRequired(mark) = state
            .stroke_loading_complete(roster[1], loading)
            .expect("second load")
        else {
            unreachable!()
        };
        state.confirm_stroke_in_game(mark).expect("in game");
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
    async fn solo_requires_authenticated_owner_and_exactly_one_member() {
        let (tx, _rx) = mpsc::channel(32);
        let (handle, _) = spawn_room(
            RoomId::new(9).unwrap_or_else(|_| unreachable!()),
            RoomName::parse("solo-auth").unwrap_or_else(|_| unreachable!()),
            None,
            RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
            identity(1),
            tx.clone(),
            RoomActorLimits::default(),
        );
        assert!(handle.join(identity(2), None, tx).await.is_ok());
        let plan = solo_plan(identity(1).account_id, Duration::from_secs(1), 3);
        assert_eq!(
            handle.prepare_solo_start(id(2), plan.clone()).await,
            Err(SoloMatchError::NotOwner)
        );
        assert_eq!(
            handle.prepare_solo_start(id(1), plan).await,
            Err(SoloMatchError::NotSolo)
        );
        assert!(handle.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn active_solo_blocks_room_mutations_and_commit_clears_room() {
        let (tx, _rx) = mpsc::channel(64);
        let (handle, _) = spawn_room(
            RoomId::new(10).unwrap_or_else(|_| unreachable!()),
            RoomName::parse("solo-block").unwrap_or_else(|_| unreachable!()),
            None,
            RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
            identity(1),
            tx,
            RoomActorLimits::default(),
        );
        let plan = solo_plan(identity(1).account_id, Duration::from_secs(1), 1);
        assert!(handle.prepare_solo_start(id(1), plan.clone()).await.is_ok());
        let (join_tx, _join_rx) = mpsc::channel(8);
        assert_eq!(
            handle.join(identity(2), None, join_tx).await,
            Err(RoomError::MatchActive)
        );
        assert_eq!(
            handle.set_ready(id(1), true).await,
            Err(RoomError::MatchActive)
        );
        assert_eq!(handle.kick(id(1), id(2)).await, Err(RoomError::MatchActive));
        assert_eq!(
            handle
                .update_settings(
                    id(1),
                    RoomSettings::new(3).unwrap_or_else(|_| unreachable!())
                )
                .await,
            Err(RoomError::MatchActive)
        );
        assert_eq!(
            handle
                .chat(
                    id(1),
                    ChatText::parse("blocked").unwrap_or_else(|_| unreachable!())
                )
                .await,
            Err(RoomError::MatchActive)
        );
        assert_eq!(handle.leave(id(1)).await, Err(RoomError::MatchActive));
        assert!(
            handle
                .confirm_solo_begin(id(1), plan.begin().match_id(), plan.begin().result_key())
                .await
                .is_ok()
        );
        assert!(
            handle
                .solo_loading_complete(
                    id(1),
                    LoadingComplete::new(100).unwrap_or_else(|_| unreachable!())
                )
                .await
                .is_ok()
        );
        let action = ShotAction::new(1, 1, 10.0, 0.0, 0.0, 0.0).unwrap_or_else(|_| unreachable!());
        assert_eq!(
            handle.solo_action(id(1), action).await,
            Ok(RelayDisposition::Accepted)
        );
        let result =
            ShotResult::new(1, 0.0, 0.0, 0.0, Lie::Green, true).unwrap_or_else(|_| unreachable!());
        assert_eq!(
            handle.solo_result(id(1), result).await,
            Ok(RelayDisposition::Accepted)
        );
        let commit = handle
            .prepare_solo_finish(id(1))
            .await
            .unwrap_or_else(|_| unreachable!());
        let committed = SoloMatchResult::new(
            commit.match_id(),
            commit.result_key(),
            commit.account_id(),
            StrokeCount::new(1).unwrap_or_else(|_| unreachable!()),
            SoloReward::from_persisted(-3, 16, 5),
            ServerBalances::from_persisted(16, 5),
        );
        assert_eq!(
            handle.apply_solo_commit(id(1), committed).await,
            Ok(committed)
        );
        assert!(
            handle
                .chat(
                    id(1),
                    ChatText::parse("open").unwrap_or_else(|_| unreachable!())
                )
                .await
                .is_ok()
        );
        assert!(handle.shutdown().await.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn loading_deadline_wins_expiry_race_but_predate_disconnect_wins_normal_work() {
        // Build the actor's state and queues before spawning it so the expired timer and queued
        // disconnect are both ready on its first biased select poll.
        let (mut expired_state, _expired_events) = state(2, None);
        let expired_plan = solo_plan(identity(1).account_id, Duration::from_secs(5), 3);
        assert!(
            expired_state
                .prepare_solo_start(id(1), expired_plan.clone())
                .is_ok()
        );
        assert!(
            expired_state
                .confirm_solo_begin(
                    id(1),
                    expired_plan.begin().match_id(),
                    expired_plan.begin().result_key(),
                )
                .is_ok()
        );
        tokio::time::advance(Duration::from_secs(5)).await;
        let (_normal_tx, normal_rx) = mpsc::channel(1);
        let (control_tx, control_rx) = mpsc::channel(1);
        let (reply, receive) = oneshot::channel();
        assert!(
            control_tx
                .try_send(ControlCommand::Disconnect {
                    caller: id(1),
                    reason: MatchAbortReason::Disconnect,
                    reply,
                })
                .is_ok()
        );
        let actor = tokio::spawn(run_room(expired_state, normal_rx, control_rx, None));
        let expired_outcome = receive
            .await
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|_| unreachable!());
        assert!(actor.await.is_ok());
        let expired_abort = expired_outcome.abort.unwrap_or_else(|| unreachable!());
        assert_eq!(expired_abort.reason(), MatchAbortReason::LoadingTimeout);
        assert_eq!(expired_abort.match_id(), expired_plan.begin().match_id());

        // With an unexpired timer, prequeue both paths and prove control wins over normal.
        let (mut predate_state, _predate_events) = state(2, None);
        let predate_plan = solo_plan(identity(1).account_id, Duration::from_secs(5), 3);
        assert!(
            predate_state
                .prepare_solo_start(id(1), predate_plan.clone())
                .is_ok()
        );
        assert!(
            predate_state
                .confirm_solo_begin(
                    id(1),
                    predate_plan.begin().match_id(),
                    predate_plan.begin().result_key(),
                )
                .is_ok()
        );
        let (normal_tx, normal_rx) = mpsc::channel(1);
        let (state_reply, state_receive) = oneshot::channel();
        assert!(
            normal_tx
                .try_send(RoomCommand::State { reply: state_reply })
                .is_ok()
        );
        let (control_tx, control_rx) = mpsc::channel(1);
        let (disconnect_reply, disconnect_receive) = oneshot::channel();
        assert!(
            control_tx
                .try_send(ControlCommand::Disconnect {
                    caller: id(1),
                    reason: MatchAbortReason::Disconnect,
                    reply: disconnect_reply,
                })
                .is_ok()
        );
        let actor = tokio::spawn(run_room(predate_state, normal_rx, control_rx, None));
        let predate_outcome = disconnect_receive
            .await
            .unwrap_or_else(|_| unreachable!())
            .unwrap_or_else(|_| unreachable!());
        assert!(actor.await.is_ok());
        assert!(state_receive.await.is_err());
        let predate_abort = predate_outcome.abort.unwrap_or_else(|| unreachable!());
        assert_eq!(predate_abort.reason(), MatchAbortReason::Disconnect);
        assert_eq!(predate_abort.match_id(), predate_plan.begin().match_id());
    }

    #[tokio::test(start_paused = true)]
    async fn active_match_shutdown_returns_exact_abort_with_full_events_and_cancels_timer() {
        let cancellation = CancellationToken::new();
        let (tx, _rx) = mpsc::channel(1);
        let (handle, _) = spawn_room_with_events(
            RoomId::new(13).unwrap_or_else(|_| unreachable!()),
            RoomName::parse("solo-shutdown").unwrap_or_else(|_| unreachable!()),
            None,
            RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
            identity(1),
            tx,
            cancellation.clone(),
            RoomActorLimits::default(),
            None,
        );
        let plan = solo_plan(identity(1).account_id, Duration::from_secs(5), 3);
        assert!(handle.prepare_solo_start(id(1), plan.clone()).await.is_ok());
        assert!(
            handle
                .confirm_solo_begin(id(1), plan.begin().match_id(), plan.begin().result_key())
                .await
                .is_ok()
        );
        assert!(cancellation.is_cancelled());
        let RoomCloseOutcome::M5Abort { request: abort, .. } =
            handle.shutdown().await.unwrap_or_else(|_| unreachable!())
        else {
            unreachable!()
        };
        assert_eq!(
            abort,
            AbortMatch::new(
                plan.begin().match_id(),
                plan.begin().result_key(),
                plan.begin().account_id(),
                MatchAbortReason::Shutdown,
            )
        );
        tokio::time::advance(Duration::from_secs(10)).await;
        assert_eq!(handle.state().await, Err(RoomError::Closed));
    }

    #[test]
    fn stroke_start_is_exact_two_ready_owner_and_mutually_exclusive() {
        let first = identity(1);
        let second = identity(2);
        let plan = stroke_plan(
            &first,
            &second,
            Duration::from_secs(5),
            Duration::from_secs(5),
            Duration::from_secs(30),
            3,
        );
        let (mut state, _owner_rx) = state(3, None);
        assert_eq!(
            state.prepare_stroke_start(first.connection_id, plan.clone()),
            Err(StrokeMatchError::NotExactlyTwo)
        );
        let (second_tx, _second_rx) = mpsc::channel(16);
        assert!(state.join(second.clone(), None, second_tx).is_ok());
        assert_eq!(
            state.prepare_stroke_start(first.connection_id, plan.clone()),
            Err(StrokeMatchError::NotReady)
        );
        assert!(state.set_ready(first.connection_id, true).is_ok());
        assert!(state.set_ready(second.connection_id, true).is_ok());
        assert_eq!(
            state.prepare_stroke_start(second.connection_id, plan.clone()),
            Err(StrokeMatchError::NotOwner)
        );
        assert!(
            state
                .prepare_stroke_start(first.connection_id, plan)
                .is_ok()
        );
        assert_eq!(
            state.set_ready(first.connection_id, false),
            Err(RoomError::MatchActive)
        );
        assert_eq!(
            state.chat(
                first.connection_id,
                ChatText::parse("x").unwrap_or_else(|_| unreachable!())
            ),
            Err(RoomError::MatchActive)
        );
        let solo = solo_plan(first.account_id, Duration::from_secs(5), 3);
        assert_eq!(
            state.prepare_solo_start(first.connection_id, solo),
            Err(SoloMatchError::InvalidPhase)
        );
    }

    #[tokio::test]
    async fn stroke_common_events_broadcast_and_settlement_has_one_owner_coordinator() {
        let first = identity(1);
        let second = identity(2);
        let (owner_tx, mut owner_rx) = mpsc::channel(64);
        let (second_tx, mut second_rx) = mpsc::channel(64);
        let (handle, _) = spawn_room(
            RoomId::new(20).unwrap_or_else(|_| unreachable!()),
            RoomName::parse("stroke").unwrap_or_else(|_| unreachable!()),
            None,
            RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
            first.clone(),
            owner_tx,
            RoomActorLimits::default(),
        );
        assert!(handle.join(second.clone(), None, second_tx).await.is_ok());
        assert!(handle.set_ready(first.connection_id, true).await.is_ok());
        assert!(handle.set_ready(second.connection_id, true).await.is_ok());
        while owner_rx.try_recv().is_ok() {}
        while second_rx.try_recv().is_ok() {}
        let plan = stroke_plan(
            &first,
            &second,
            Duration::from_secs(5),
            Duration::from_secs(5),
            Duration::from_secs(30),
            3,
        );
        assert!(
            handle
                .prepare_stroke_start(first.connection_id, plan.clone())
                .await
                .is_ok()
        );
        assert!(
            handle
                .confirm_stroke_begin(
                    first.connection_id,
                    plan.begin().match_id(),
                    plan.begin().result_key()
                )
                .await
                .is_ok()
        );
        assert!(matches!(
            owner_rx.recv().await,
            Some(RoomEvent::StrokeStarted(_))
        ));
        assert!(matches!(
            second_rx.recv().await,
            Some(RoomEvent::StrokeStarted(_))
        ));
        assert!(matches!(
            owner_rx.recv().await,
            Some(RoomEvent::StrokePhase { .. })
        ));
        assert!(matches!(
            second_rx.recv().await,
            Some(RoomEvent::StrokePhase { .. })
        ));
        let loading = StrokeLoadingComplete::new(100).unwrap_or_else(|_| unreachable!());
        assert_eq!(
            handle
                .stroke_loading_complete(second.connection_id, loading)
                .await,
            Ok(StrokeLoadingOutcome::Waiting)
        );
        let mark = match handle
            .stroke_loading_complete(first.connection_id, loading)
            .await
        {
            Ok(StrokeLoadingOutcome::PersistenceRequired(mark)) => mark,
            _ => unreachable!(),
        };
        assert!(handle.confirm_stroke_in_game(mark).await.is_ok());
        while owner_rx.try_recv().is_ok() {}
        while second_rx.try_recv().is_ok() {}
        let commit = handle
            .stroke_give_up(first.connection_id)
            .await
            .unwrap_or_else(|_| unreachable!());
        let owner_events: Vec<_> = std::iter::from_fn(|| owner_rx.try_recv().ok()).collect();
        let second_events: Vec<_> = std::iter::from_fn(|| second_rx.try_recv().ok()).collect();
        assert!(
            owner_events
                .iter()
                .any(|event| matches!(event, RoomEvent::StrokePhase { .. }))
        );
        assert!(
            second_events
                .iter()
                .any(|event| matches!(event, RoomEvent::StrokePhase { .. }))
        );
        assert_eq!(owner_events.iter().filter(|event| matches!(event, RoomEvent::StrokeSettlementRequested(value) if *value == commit)).count(), 1);
        assert!(
            !second_events
                .iter()
                .any(|event| matches!(event, RoomEvent::StrokeSettlementRequested(_)))
        );
        let results = [0_usize, 1_usize].map(|index| {
            let input = commit.players()[index];
            let reward =
                synthetic_stroke_reward_v1(commit.config(), input.strokes(), input.completion())
                    .unwrap_or_else(|_| unreachable!());
            StrokePlayerResult::new(input, reward, ServerBalances::from_persisted(100, 100))
        });
        let persisted = StrokeMatchResult::new(commit.match_id(), commit.result_key(), results);
        assert_eq!(handle.apply_stroke_commit(persisted).await, Ok(persisted));
        assert!(handle.shutdown().await.is_ok());
    }

    #[test]
    fn stroke_persistence_falls_back_and_retains_work_when_all_outbounds_are_full() {
        let first = identity(1);
        let second = identity(2);
        let plan = stroke_plan(
            &first,
            &second,
            Duration::from_secs(5),
            Duration::from_secs(5),
            Duration::from_secs(30),
            3,
        );

        let owner_cancel = CancellationToken::new();
        let second_cancel = CancellationToken::new();
        let (owner_tx, mut owner_rx) = mpsc::channel(32);
        let (second_tx, mut second_rx) = mpsc::channel(32);
        let mut fallback = RoomState::new(
            RoomId::new(30).expect("room"),
            RoomName::parse("fallback").expect("name"),
            None,
            RoomSettings::new(2).expect("settings"),
            first.clone(),
            owner_tx.clone(),
            owner_cancel.clone(),
        );
        fallback
            .join_with_cancellation(second.clone(), None, second_tx, second_cancel.clone())
            .expect("join");
        fallback
            .set_ready(first.connection_id, true)
            .expect("ready");
        fallback
            .set_ready(second.connection_id, true)
            .expect("ready");
        playing_stroke_room(&mut fallback, &plan);
        while owner_rx.try_recv().is_ok() {}
        while second_rx.try_recv().is_ok() {}
        while owner_tx.try_send(RoomEvent::Closed).is_ok() {}
        let commit = fallback
            .stroke_give_up(first.connection_id)
            .expect("give-up settlement");
        assert!(owner_cancel.is_cancelled());
        assert!(!second_cancel.is_cancelled());
        let second_events: Vec<_> = std::iter::from_fn(|| second_rx.try_recv().ok()).collect();
        assert_eq!(
            second_events
                .iter()
                .filter(|event| matches!(event, RoomEvent::StrokeSettlementRequested(value) if *value == commit))
                .count(),
            1,
            "only the next deterministic noncancelled coordinator receives work"
        );

        let owner_cancel = CancellationToken::new();
        let second_cancel = CancellationToken::new();
        let (owner_tx, mut owner_rx) = mpsc::channel(32);
        let (second_tx, mut second_rx) = mpsc::channel(32);
        let mut retained = RoomState::new(
            RoomId::new(31).expect("room"),
            RoomName::parse("retained").expect("name"),
            None,
            RoomSettings::new(2).expect("settings"),
            first.clone(),
            owner_tx.clone(),
            owner_cancel.clone(),
        );
        retained
            .join_with_cancellation(
                second.clone(),
                None,
                second_tx.clone(),
                second_cancel.clone(),
            )
            .expect("join");
        retained
            .set_ready(first.connection_id, true)
            .expect("ready");
        retained
            .set_ready(second.connection_id, true)
            .expect("ready");
        playing_stroke_room(&mut retained, &plan);
        while owner_rx.try_recv().is_ok() {}
        while second_rx.try_recv().is_ok() {}
        while owner_tx.try_send(RoomEvent::Closed).is_ok() {}
        while second_tx.try_send(RoomEvent::Closed).is_ok() {}
        let commit = retained
            .stroke_give_up(first.connection_id)
            .expect("give-up settlement");
        assert!(owner_cancel.is_cancelled());
        assert!(second_cancel.is_cancelled());
        assert!(
            std::iter::from_fn(|| owner_rx.try_recv().ok())
                .chain(std::iter::from_fn(|| second_rx.try_recv().ok()))
                .all(|event| !matches!(event, RoomEvent::StrokeSettlementRequested(_)))
        );
        assert_eq!(
            retained.disconnect_outcome(first.connection_id, MatchAbortReason::Disconnect),
            RoomCloseOutcome::M6Settlement {
                room_id: retained.id,
                request: commit,
            }
        );
        assert_eq!(
            retained.disconnect_outcome(second.connection_id, MatchAbortReason::Disconnect),
            RoomCloseOutcome::None,
            "room authority hands retained work to priority cancellation exactly once"
        );
        let persisted = persisted_stroke(commit);
        assert_eq!(retained.apply_stroke_commit(persisted), Ok(persisted));
    }

    #[test]
    fn priority_abort_replacement_transfers_and_retains_the_control_claim() {
        let first = identity(1);
        let second = identity(2);
        let plan = stroke_plan(
            &first,
            &second,
            Duration::from_secs(5),
            Duration::from_secs(5),
            Duration::from_secs(30),
            3,
        );
        let (owner_tx, _owner_rx) = mpsc::channel(32);
        let (second_tx, _second_rx) = mpsc::channel(32);
        let mut state = RoomState::new(
            RoomId::new(32).expect("room"),
            RoomName::parse("claim-transfer").expect("name"),
            None,
            RoomSettings::new(2).expect("settings"),
            first.clone(),
            owner_tx,
            CancellationToken::new(),
        );
        state
            .join_with_cancellation(second.clone(), None, second_tx, CancellationToken::new())
            .expect("join");
        state
            .set_ready(first.connection_id, true)
            .expect("owner ready");
        state
            .set_ready(second.connection_id, true)
            .expect("peer ready");
        playing_stroke_room(&mut state, &plan);

        let RoomCloseOutcome::M6Settlement {
            request: settlement,
            ..
        } = state.disconnect_outcome(first.connection_id, MatchAbortReason::Disconnect)
        else {
            unreachable!()
        };
        assert!(state.stroke_persistence_control_delivered);
        assert_eq!(
            state.disconnect_outcome(second.connection_id, MatchAbortReason::Shutdown),
            RoomCloseOutcome::None,
            "shutdown cleanup cannot steal an existing control claim"
        );
        assert_eq!(
            state.pending_stroke_persistence,
            Some(PendingStrokePersistence::Settlement(settlement))
        );
        assert!(state.stroke_persistence_control_delivered);
        let abort = state
            .prioritize_and_claim_stroke_abort(MatchAbortReason::Shutdown)
            .expect("priority abort");
        assert_eq!(abort.reason(), MatchAbortReason::Shutdown);
        assert_eq!(
            state.pending_stroke_persistence,
            Some(PendingStrokePersistence::Abort(abort))
        );
        assert!(!state.stroke_persistence_event_delivered);
        assert!(state.stroke_persistence_control_delivered);
        assert_eq!(
            state.disconnect_outcome(second.connection_id, MatchAbortReason::Shutdown),
            RoomCloseOutcome::None,
            "the transferred claim is not available to concurrent cleanup"
        );

        assert_eq!(state.acknowledge_stroke_abort(abort), Ok(()));
        assert_eq!(state.pending_stroke_persistence, None);
        assert!(!state.stroke_persistence_event_delivered);
        assert!(!state.stroke_persistence_control_delivered);
        assert_eq!(
            state.acknowledge_stroke_abort(abort),
            Err(StrokeMatchError::InvalidPhase),
            "the exact abort is acknowledged only once"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn stroke_turn_timer_fires_without_action_extension_and_shutdown_converts_to_abort() {
        let first = identity(1);
        let second = identity(2);
        let (owner_tx, mut owner_rx) = mpsc::channel(64);
        let (second_tx, _second_rx) = mpsc::channel(64);
        let (handle, _) = spawn_room(
            RoomId::new(21).unwrap_or_else(|_| unreachable!()),
            RoomName::parse("stroke-timer").unwrap_or_else(|_| unreachable!()),
            None,
            RoomSettings::new(2).unwrap_or_else(|_| unreachable!()),
            first.clone(),
            owner_tx,
            RoomActorLimits::default(),
        );
        assert!(handle.join(second.clone(), None, second_tx).await.is_ok());
        assert!(handle.set_ready(first.connection_id, true).await.is_ok());
        assert!(handle.set_ready(second.connection_id, true).await.is_ok());
        let plan = stroke_plan(
            &first,
            &second,
            Duration::from_secs(4),
            Duration::from_secs(2),
            Duration::from_secs(20),
            3,
        );
        assert!(
            handle
                .prepare_stroke_start(first.connection_id, plan.clone())
                .await
                .is_ok()
        );
        assert!(
            handle
                .confirm_stroke_begin(
                    first.connection_id,
                    plan.begin().match_id(),
                    plan.begin().result_key()
                )
                .await
                .is_ok()
        );
        let loading = StrokeLoadingComplete::new(100).unwrap_or_else(|_| unreachable!());
        assert!(
            handle
                .stroke_loading_complete(first.connection_id, loading)
                .await
                .is_ok()
        );
        let mark = match handle
            .stroke_loading_complete(second.connection_id, loading)
            .await
        {
            Ok(StrokeLoadingOutcome::PersistenceRequired(mark)) => mark,
            _ => unreachable!(),
        };
        assert!(handle.confirm_stroke_in_game(mark).await.is_ok());
        let action =
            StrokeShotAction::new(1, 1, 10.0, 0.0, 0.0, 0.0).unwrap_or_else(|_| unreachable!());
        assert_eq!(
            handle.stroke_action(first.connection_id, action).await,
            Ok(RelayDisposition::Accepted)
        );
        while owner_rx.try_recv().is_ok() {}
        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::task::yield_now().await;
        let commit = std::iter::from_fn(|| owner_rx.try_recv().ok())
            .find_map(|event| match event {
                RoomEvent::StrokeSettlementRequested(commit) => Some(commit),
                _ => None,
            })
            .expect("automatic settlement event");
        let outcome = handle.shutdown().await.unwrap_or_else(|_| unreachable!());
        assert_eq!(
            outcome,
            RoomCloseOutcome::M6Abort {
                room_id: handle.id(),
                request: AbortStrokeMatch::new(
                    commit.match_id(),
                    commit.result_key(),
                    MatchAbortReason::Shutdown,
                ),
            }
        );
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
