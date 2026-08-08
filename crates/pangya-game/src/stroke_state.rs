//! Pure deterministic state for one local synthetic exactly-two stroke match.
//!
//! The room actor owns this aggregate. It has no clock, I/O, repository, mutex, or
//! client-authoritative scoring/reward state.

use std::{cmp::Ordering, time::Duration};

use pangya_domain::{
    AbortStrokeMatch, AccountId, BeginStrokeMatch, CommitStrokeMatch, MarkStrokeInGame,
    MatchAbortReason, MatchId, MatchResultKey, PlayerConnectionId,
    StrokeCompletion as DomainCompletion, StrokeMatchResult, StrokePlace, StrokePlayerCommit,
};
use pangya_protocol::{StrokeShotAction, StrokeShotResult};
use thiserror::Error;

use crate::match_state::{
    LOADING_TIMEOUT_HARD_CAP, MAX_SOLO_STROKES, RelayDisposition, deterministic_conditions,
};

/// Turn and whole-game durations may not reserve an actor beyond this hard cap.
pub const STROKE_GAME_TIMEOUT_HARD_CAP: Duration = Duration::from_secs(3_600);
/// The generated stroke protocol shares the authoritative thirty-stroke hard cap.
pub const MAX_STROKE_STROKES: u8 = MAX_SOLO_STROKES;

/// Immutable checked plan captured before begin persistence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrokeStartPlan {
    begin: BeginStrokeMatch,
    roster: [PlayerConnectionId; 2],
    loading_timeout: Duration,
    turn_timeout: Duration,
    game_timeout: Duration,
    max_strokes: u8,
}

impl StrokeStartPlan {
    /// Validates deterministic conditions, exact distinct runtime roster, and duration/stroke caps.
    ///
    /// # Errors
    /// Returns [`StrokeMatchError::InvalidPlan`] for any invalid bound or identity drift.
    pub fn new(
        begin: BeginStrokeMatch,
        roster: [PlayerConnectionId; 2],
        loading_timeout: Duration,
        turn_timeout: Duration,
        game_timeout: Duration,
        max_strokes: u8,
    ) -> Result<Self, StrokeMatchError> {
        let duration_valid = |duration: Duration, cap: Duration| {
            !duration.is_zero()
                && duration <= cap
                && duration.as_millis() != 0
                && duration.as_millis() <= u128::from(u32::MAX)
        };
        let expected = deterministic_conditions(begin.seed())
            .map_err(|_| StrokeMatchError::DeterministicConditionsInvariant)?;
        if roster[0] == roster[1]
            || !duration_valid(loading_timeout, LOADING_TIMEOUT_HARD_CAP)
            || !duration_valid(turn_timeout, STROKE_GAME_TIMEOUT_HARD_CAP)
            || !duration_valid(game_timeout, STROKE_GAME_TIMEOUT_HARD_CAP)
            || !(1..=MAX_STROKE_STROKES).contains(&max_strokes)
            || expected != (begin.weather(), begin.wind())
        {
            return Err(StrokeMatchError::InvalidPlan);
        }
        Ok(Self {
            begin,
            roster,
            loading_timeout,
            turn_timeout,
            game_timeout,
            max_strokes,
        })
    }

    /// Immutable repository begin request.
    #[must_use]
    pub const fn begin(&self) -> &BeginStrokeMatch {
        &self.begin
    }
    /// Captured connection roster in stable room join order.
    #[must_use]
    pub const fn roster(&self) -> &[PlayerConnectionId; 2] {
        &self.roster
    }
    /// Loading barrier timeout.
    #[must_use]
    pub const fn loading_timeout(&self) -> Duration {
        self.loading_timeout
    }
    /// Per-turn timeout.
    #[must_use]
    pub const fn turn_timeout(&self) -> Duration {
        self.turn_timeout
    }
    /// Whole-game timeout.
    #[must_use]
    pub const fn game_timeout(&self) -> Duration {
        self.game_timeout
    }
    /// Authoritative per-player stroke cap.
    #[must_use]
    pub const fn max_strokes(&self) -> u8 {
        self.max_strokes
    }
}

/// Pure completion projection used before persistence DTO construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrokeCompletion {
    /// Ball was holed.
    Holed,
    /// Authoritative stroke cap was reached.
    StrokeCap,
    /// Participant forfeited.
    Forfeit(ForfeitReason),
    /// Opponent forfeited; no score or course record is fabricated.
    WinnerByForfeit,
}

/// Closed in-game forfeit reasons.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForfeitReason {
    /// Voluntary give-up.
    GiveUp,
    /// Participant disconnected.
    Disconnect,
    /// Active turn expired.
    TurnTimeout,
    /// Whole game expired while unfinished.
    GameTimeout,
}

/// Public aggregate phase projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrokeMatchPhase {
    /// No active or retained aborted match.
    Open,
    /// Begin persistence is outstanding.
    Starting,
    /// Per-player load barrier.
    Loading {
        /// Roster-ordered completion flags.
        loaded: [bool; 2],
    },
    /// Both loaded; durable in-game transition is outstanding.
    LoadingPersistencePending,
    /// Awaiting a new action from the active participant.
    AwaitAction {
        /// Active captured connection.
        active: PlayerConnectionId,
        /// Global one-based turn number.
        turn: u32,
        /// Active player's required sequence.
        sequence: u32,
    },
    /// Awaiting the result matching the active participant's accepted action.
    AwaitResult {
        /// Active captured connection.
        active: PlayerConnectionId,
        /// Global one-based turn number.
        turn: u32,
        /// Sequence of the pending action.
        sequence: u32,
    },
    /// Automatic aggregate commit is pending.
    ResultsPending,
    /// Exact no-reward abort is retained until acknowledgement.
    Aborted,
}

/// Actor-owned deadline identity. Generations make stale sleepers harmless.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrokeDeadline {
    /// Global loading barrier deadline.
    Loading,
    /// Current turn deadline.
    Turn {
        /// Generation captured when the turn started.
        generation: u64,
    },
    /// Whole-game deadline.
    Game {
        /// Generation captured when durable in-game state was confirmed.
        generation: u64,
    },
}

/// Result of a pure deadline transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrokeDeadlineOutcome {
    /// Deadline was stale and did not mutate state.
    Stale,
    /// Loading was aborted without reward.
    Aborted(AbortStrokeMatch),
    /// In-game completion automatically prepared settlement.
    Settlement(CommitStrokeMatch),
}

/// Loading barrier result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrokeLoadingOutcome {
    /// This member loaded; the other remains outstanding.
    Waiting,
    /// Both loaded and the exact durable transition must be persisted.
    PersistenceRequired(MarkStrokeInGame),
    /// Exact repeat of a member's load completion.
    Duplicate,
}

/// Result of a participant announcing that their ball is in the hole.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrokeHoleOutOutcome {
    /// The caller finished; the other participant is still playing.
    Waiting,
    /// The caller had already finished this hole.
    Duplicate,
    /// Both participants are terminal and settlement is prepared.
    Settlement(CommitStrokeMatch),
}

/// Accepted relay and optional automatic terminal settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StrokeRelayOutcome {
    disposition: RelayDisposition,
    settlement: Option<CommitStrokeMatch>,
}

impl StrokeRelayOutcome {
    /// Whether the payload was new or an exact replay.
    #[must_use]
    pub const fn disposition(self) -> RelayDisposition {
        self.disposition
    }
    /// Automatic terminal settlement, if this result ended the match.
    #[must_use]
    pub const fn settlement(self) -> Option<CommitStrokeMatch> {
        self.settlement
    }
}

/// Stable pure aggregate rejection.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum StrokeMatchError {
    /// Fixed deterministic reductions no longer satisfy domain bounds.
    #[error("deterministic stroke conditions violate domain invariants")]
    DeterministicConditionsInvariant,
    /// Plan violates duration, roster, condition, or stroke bounds.
    #[error("stroke match start plan is invalid")]
    InvalidPlan,
    /// Command is invalid in this phase.
    #[error("stroke match command is invalid in this phase")]
    InvalidPhase,
    /// Match/result identity drifted.
    #[error("stroke match identity does not match")]
    IdentityMismatch,
    /// Caller is not in the captured roster.
    #[error("stroke command caller is not a participant")]
    NotParticipant,
    /// Caller is not the active participant.
    #[error("stroke command caller does not own the turn")]
    InvalidTurn,
    /// Sequence is stale, skipped, or mismatches the pending action.
    #[error("stroke shot sequence is invalid")]
    InvalidSequence,
    /// Repeated sequence carries different content.
    #[error("stroke shot replay conflicts with accepted content")]
    ConflictingReplay,
    /// Loading progress is not canonical.
    #[error("stroke loading progress must be exactly 100")]
    InvalidProgress,
    /// Checked turn/stroke/settlement construction failed.
    #[error("stroke match invariant failed")]
    Invariant,
    /// Caller is not an authenticated room member.
    #[error("stroke command caller is not a room member")]
    NotMember,
    /// Caller is not owner.
    #[error("stroke start requires room owner")]
    NotOwner,
    /// Start requires exactly two members.
    #[error("stroke match requires exactly two members")]
    NotExactlyTwo,
    /// Both members must be ready.
    #[error("stroke match requires both members ready")]
    NotReady,
    /// Account identities must be distinct and match the plan roster.
    #[error("stroke match roster does not match room authority")]
    RosterMismatch,
    /// Bounded command queue full.
    #[error("stroke match command queue is full")]
    QueueFull,
    /// Owning room closed.
    #[error("stroke match room is closed")]
    Closed,
    /// Bounded command deadline elapsed.
    #[error("stroke match command timed out")]
    Timeout,
}

#[derive(Clone, Debug, PartialEq)]
struct PlayerState {
    connection_id: PlayerConnectionId,
    account_id: AccountId,
    sequence: u32,
    strokes: u16,
    completion: Option<StrokeCompletion>,
    last_action: Option<StrokeShotAction>,
    last_result: Option<StrokeShotResult>,
}

#[derive(Clone, Debug, PartialEq)]
enum ActivePhase {
    Starting,
    Loading([bool; 2]),
    LoadingPersistencePending,
    AwaitAction,
    AwaitResult(StrokeShotAction),
    ResultsPending(CommitStrokeMatch),
}

#[derive(Clone, Debug, PartialEq)]
struct ActiveMatch {
    plan: StrokeStartPlan,
    players: [PlayerState; 2],
    phase: ActivePhase,
    active: usize,
    turn: u32,
    turn_generation: u64,
    game_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingAbort {
    request: AbortStrokeMatch,
    roster: [PlayerConnectionId; 2],
}

/// Pure exactly-two stroke aggregate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StrokeMatchState {
    active: Option<ActiveMatch>,
    pending_abort: Option<PendingAbort>,
}

impl StrokeMatchState {
    /// Constructs open state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: None,
            pending_abort: None,
        }
    }

    /// Stable public phase.
    #[must_use]
    pub fn phase(&self) -> StrokeMatchPhase {
        if self.pending_abort.is_some() {
            return StrokeMatchPhase::Aborted;
        }
        self.active
            .as_ref()
            .map_or(StrokeMatchPhase::Open, |active| {
                let player = &active.players[active.active];
                match active.phase {
                    ActivePhase::Starting => StrokeMatchPhase::Starting,
                    ActivePhase::Loading(loaded) => StrokeMatchPhase::Loading { loaded },
                    ActivePhase::LoadingPersistencePending => {
                        StrokeMatchPhase::LoadingPersistencePending
                    }
                    ActivePhase::AwaitAction => StrokeMatchPhase::AwaitAction {
                        active: player.connection_id,
                        turn: active.turn,
                        sequence: player.sequence,
                    },
                    ActivePhase::AwaitResult(action) => StrokeMatchPhase::AwaitResult {
                        active: player.connection_id,
                        turn: active.turn,
                        sequence: action.sequence(),
                    },
                    ActivePhase::ResultsPending(_) => StrokeMatchPhase::ResultsPending,
                }
            })
    }

    /// Whether room mutations must remain blocked.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.is_some() || self.pending_abort.is_some()
    }

    /// Checked plan while active.
    #[must_use]
    pub fn start_plan(&self) -> Option<&StrokeStartPlan> {
        self.active.as_ref().map(|active| &active.plan)
    }

    /// Captured connection roster while active or awaiting abort acknowledgement.
    #[must_use]
    pub fn roster(&self) -> Option<&[PlayerConnectionId; 2]> {
        self.active
            .as_ref()
            .map(|active| active.plan.roster())
            .or_else(|| self.pending_abort.as_ref().map(|pending| &pending.roster))
    }

    /// Current turn generation, only in an in-game turn phase.
    #[must_use]
    pub fn turn_generation(&self) -> Option<u64> {
        self.active.as_ref().and_then(|active| {
            matches!(
                active.phase,
                ActivePhase::AwaitAction | ActivePhase::AwaitResult(_)
            )
            .then_some(active.turn_generation)
        })
    }

    /// Current game generation after durable in-game confirmation.
    #[must_use]
    pub fn game_generation(&self) -> Option<u64> {
        self.active.as_ref().and_then(|active| {
            matches!(
                active.phase,
                ActivePhase::AwaitAction
                    | ActivePhase::AwaitResult(_)
                    | ActivePhase::ResultsPending(_)
            )
            .then_some(active.game_generation)
        })
    }

    /// Reserves an immutable plan.
    pub fn prepare_start(
        &mut self,
        plan: StrokeStartPlan,
    ) -> Result<BeginStrokeMatch, StrokeMatchError> {
        if self.is_active() {
            return Err(StrokeMatchError::InvalidPhase);
        }
        let begin = plan.begin().clone();
        let participants = begin.participants();
        self.active = Some(ActiveMatch {
            players: [
                PlayerState {
                    connection_id: plan.roster[0],
                    account_id: participants[0].account_id(),
                    sequence: 1,
                    strokes: 0,
                    completion: None,
                    last_action: None,
                    last_result: None,
                },
                PlayerState {
                    connection_id: plan.roster[1],
                    account_id: participants[1].account_id(),
                    sequence: 1,
                    strokes: 0,
                    completion: None,
                    last_action: None,
                    last_result: None,
                },
            ],
            plan,
            phase: ActivePhase::Starting,
            active: 0,
            turn: 1,
            turn_generation: 0,
            game_generation: 0,
        });
        Ok(begin)
    }

    /// Confirms exact begin persistence and opens the loading barrier.
    pub fn confirm_begin(
        &mut self,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), StrokeMatchError> {
        let active = self.active.as_mut().ok_or(StrokeMatchError::InvalidPhase)?;
        if active.phase != ActivePhase::Starting {
            return Err(StrokeMatchError::InvalidPhase);
        }
        verify_ids(active, match_id, result_key)?;
        active.phase = ActivePhase::Loading([false; 2]);
        Ok(())
    }

    /// Cancels only an exact unpersisted reservation.
    pub fn cancel_begin(
        &mut self,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), StrokeMatchError> {
        let active = self.active.as_ref().ok_or(StrokeMatchError::InvalidPhase)?;
        if active.phase != ActivePhase::Starting {
            return Err(StrokeMatchError::InvalidPhase);
        }
        verify_ids(active, match_id, result_key)?;
        self.active = None;
        Ok(())
    }

    /// Marks one participant loaded. Both orders are equivalent and exact repeats are idempotent.
    pub fn loading_complete(
        &mut self,
        caller: PlayerConnectionId,
        progress: u8,
    ) -> Result<StrokeLoadingOutcome, StrokeMatchError> {
        if progress != 100 {
            return Err(StrokeMatchError::InvalidProgress);
        }
        let active = self.active.as_mut().ok_or(StrokeMatchError::InvalidPhase)?;
        let index = player_index(active, caller)?;
        let ActivePhase::Loading(mut loaded) = active.phase else {
            return if active.phase == ActivePhase::LoadingPersistencePending {
                Ok(StrokeLoadingOutcome::Duplicate)
            } else {
                Err(StrokeMatchError::InvalidPhase)
            };
        };
        if loaded[index] {
            return Ok(StrokeLoadingOutcome::Duplicate);
        }
        loaded[index] = true;
        if loaded == [true; 2] {
            let begin = active.plan.begin();
            let mark = MarkStrokeInGame::new(begin.match_id(), begin.result_key());
            active.phase = ActivePhase::LoadingPersistencePending;
            Ok(StrokeLoadingOutcome::PersistenceRequired(mark))
        } else {
            active.phase = ActivePhase::Loading(loaded);
            Ok(StrokeLoadingOutcome::Waiting)
        }
    }

    /// Confirms the exact durable in-game transition and starts turn/game generation one.
    pub fn confirm_in_game(&mut self, mark: MarkStrokeInGame) -> Result<(), StrokeMatchError> {
        let active = self.active.as_mut().ok_or(StrokeMatchError::InvalidPhase)?;
        if active.phase != ActivePhase::LoadingPersistencePending {
            return Err(StrokeMatchError::InvalidPhase);
        }
        verify_ids(active, mark.match_id(), mark.result_key())?;
        active.turn_generation = 1;
        active.game_generation = 1;
        active.phase = ActivePhase::AwaitAction;
        Ok(())
    }

    /// Accepts only the active player's expected action, with exact replay recognition even later.
    pub fn accept_action(
        &mut self,
        caller: PlayerConnectionId,
        action: StrokeShotAction,
    ) -> Result<RelayDisposition, StrokeMatchError> {
        let active = self.active.as_mut().ok_or(StrokeMatchError::InvalidPhase)?;
        let index = player_index(active, caller)?;
        let player = &active.players[index];
        if player
            .last_action
            .is_some_and(|previous| previous.sequence() == action.sequence())
        {
            return if player
                .last_action
                .is_some_and(|previous| same_action(previous, action))
            {
                Ok(RelayDisposition::Duplicate)
            } else {
                Err(StrokeMatchError::ConflictingReplay)
            };
        }
        if active.phase != ActivePhase::AwaitAction {
            return Err(StrokeMatchError::InvalidPhase);
        }
        if index != active.active {
            return Err(StrokeMatchError::InvalidTurn);
        }
        if action.sequence() != player.sequence {
            return Err(StrokeMatchError::InvalidSequence);
        }
        active.players[index].last_action = Some(action);
        active.phase = ActivePhase::AwaitResult(action);
        Ok(RelayDisposition::Accepted)
    }

    /// Accepts one matching result, advances authoritative strokes/turn, and automatically
    /// prepares settlement once both participants are terminal.
    pub fn accept_result(
        &mut self,
        caller: PlayerConnectionId,
        result: StrokeShotResult,
    ) -> Result<StrokeRelayOutcome, StrokeMatchError> {
        let active = self.active.as_mut().ok_or(StrokeMatchError::InvalidPhase)?;
        let index = player_index(active, caller)?;
        let player = &active.players[index];
        if player
            .last_result
            .is_some_and(|previous| previous.sequence() == result.sequence())
        {
            return if player
                .last_result
                .is_some_and(|previous| same_result(previous, result))
            {
                Ok(StrokeRelayOutcome {
                    disposition: RelayDisposition::Duplicate,
                    settlement: None,
                })
            } else {
                Err(StrokeMatchError::ConflictingReplay)
            };
        }
        let ActivePhase::AwaitResult(action) = active.phase else {
            return Err(StrokeMatchError::InvalidPhase);
        };
        if index != active.active {
            return Err(StrokeMatchError::InvalidTurn);
        }
        if result.sequence() != action.sequence() {
            return Err(StrokeMatchError::InvalidSequence);
        }
        let next_strokes = active.players[index]
            .strokes
            .checked_add(1)
            .filter(|strokes| *strokes <= u16::from(active.plan.max_strokes))
            .ok_or(StrokeMatchError::Invariant)?;
        let next_sequence = active.players[index]
            .sequence
            .checked_add(1)
            .ok_or(StrokeMatchError::Invariant)?;
        let next_turn = active
            .turn
            .checked_add(1)
            .ok_or(StrokeMatchError::Invariant)?;
        let next_generation = active
            .turn_generation
            .checked_add(1)
            .ok_or(StrokeMatchError::Invariant)?;
        active.players[index].strokes = next_strokes;
        active.players[index].last_result = Some(result);
        active.players[index].sequence = next_sequence;
        if result.holed() {
            active.players[index].completion = Some(StrokeCompletion::Holed);
        } else if next_strokes == u16::from(active.plan.max_strokes) {
            active.players[index].completion = Some(StrokeCompletion::StrokeCap);
        }
        active.turn = next_turn;
        if active
            .players
            .iter()
            .all(|player| player.completion.is_some())
        {
            let commit = build_commit(active)?;
            active.phase = ActivePhase::ResultsPending(commit);
            return Ok(StrokeRelayOutcome {
                disposition: RelayDisposition::Accepted,
                settlement: Some(commit),
            });
        }
        active.active = next_unfinished(active, index);
        active.turn_generation = next_generation;
        active.phase = ActivePhase::AwaitAction;
        Ok(StrokeRelayOutcome {
            disposition: RelayDisposition::Accepted,
            settlement: None,
        })
    }

    /// Records that a participant's ball is in the hole, without charging a stroke.
    ///
    /// A retail client plays the holing shot through the ordinary action/result pair and only
    /// *then* announces that the hole is over, so counting the announcement as a stroke would
    /// score every hole one over. The announcement is therefore a completion, not a shot.
    ///
    /// # Errors
    ///
    /// Returns an error when the caller is not a participant, when a shot of theirs is still
    /// outstanding, or when no hole is being played.
    pub fn hole_out(
        &mut self,
        caller: PlayerConnectionId,
    ) -> Result<StrokeHoleOutOutcome, StrokeMatchError> {
        let active = self.active.as_mut().ok_or(StrokeMatchError::InvalidPhase)?;
        let index = player_index(active, caller)?;
        // In `AwaitResult` the caller's own shot has not landed yet, so nothing can have holed.
        if active.phase != ActivePhase::AwaitAction {
            return Err(StrokeMatchError::InvalidPhase);
        }
        if active.players[index].completion.is_some() {
            return Ok(StrokeHoleOutOutcome::Duplicate);
        }
        if active.players[index].strokes == 0 {
            return Err(StrokeMatchError::InvalidPhase);
        }
        active.players[index].completion = Some(StrokeCompletion::Holed);
        if active
            .players
            .iter()
            .all(|player| player.completion.is_some())
        {
            let commit = build_commit(active)?;
            active.phase = ActivePhase::ResultsPending(commit);
            return Ok(StrokeHoleOutOutcome::Settlement(commit));
        }
        if index == active.active {
            let next_turn = active
                .turn
                .checked_add(1)
                .ok_or(StrokeMatchError::Invariant)?;
            let next_generation = active
                .turn_generation
                .checked_add(1)
                .ok_or(StrokeMatchError::Invariant)?;
            active.active = next_unfinished(active, index);
            active.turn = next_turn;
            active.turn_generation = next_generation;
        }
        Ok(StrokeHoleOutOutcome::Waiting)
    }

    /// Voluntary participant forfeit; the other becomes winner-by-forfeit.
    pub fn give_up(
        &mut self,
        caller: PlayerConnectionId,
    ) -> Result<CommitStrokeMatch, StrokeMatchError> {
        self.forfeit(caller, ForfeitReason::GiveUp)
    }

    /// Disconnects a captured participant. Loading aborts; in-game disconnect settles a forfeit.
    pub fn disconnect(
        &mut self,
        caller: PlayerConnectionId,
    ) -> Result<StrokeDeadlineOutcome, StrokeMatchError> {
        if let Some(pending) = self.pending_abort {
            if !pending.roster.contains(&caller) {
                return Err(StrokeMatchError::NotParticipant);
            }
            return Ok(StrokeDeadlineOutcome::Aborted(pending.request));
        }
        let active = self.active.as_ref().ok_or(StrokeMatchError::InvalidPhase)?;
        let _index = player_index(active, caller)?;
        match active.phase {
            ActivePhase::Starting
            | ActivePhase::Loading(_)
            | ActivePhase::LoadingPersistencePending => {
                let abort = self
                    .abort(MatchAbortReason::Disconnect)
                    .ok_or(StrokeMatchError::Invariant)?;
                Ok(StrokeDeadlineOutcome::Aborted(abort))
            }
            ActivePhase::AwaitAction | ActivePhase::AwaitResult(_) => self
                .forfeit(caller, ForfeitReason::Disconnect)
                .map(StrokeDeadlineOutcome::Settlement),
            ActivePhase::ResultsPending(commit) => Ok(StrokeDeadlineOutcome::Settlement(commit)),
        }
    }

    /// Applies an actor timer only when its phase and generation are current.
    pub fn deadline_expired(
        &mut self,
        deadline: StrokeDeadline,
    ) -> Result<StrokeDeadlineOutcome, StrokeMatchError> {
        let Some(active) = self.active.as_ref() else {
            return Ok(StrokeDeadlineOutcome::Stale);
        };
        match deadline {
            StrokeDeadline::Loading => {
                if !matches!(active.phase, ActivePhase::Loading(_)) {
                    return Ok(StrokeDeadlineOutcome::Stale);
                }
                let abort = self
                    .abort(MatchAbortReason::LoadingTimeout)
                    .ok_or(StrokeMatchError::Invariant)?;
                Ok(StrokeDeadlineOutcome::Aborted(abort))
            }
            StrokeDeadline::Turn { generation } => {
                if generation != active.turn_generation
                    || !matches!(
                        active.phase,
                        ActivePhase::AwaitAction | ActivePhase::AwaitResult(_)
                    )
                {
                    return Ok(StrokeDeadlineOutcome::Stale);
                }
                let caller = active.players[active.active].connection_id;
                self.forfeit(caller, ForfeitReason::TurnTimeout)
                    .map(StrokeDeadlineOutcome::Settlement)
            }
            StrokeDeadline::Game { generation } => {
                if generation != active.game_generation
                    || !matches!(
                        active.phase,
                        ActivePhase::AwaitAction | ActivePhase::AwaitResult(_)
                    )
                {
                    return Ok(StrokeDeadlineOutcome::Stale);
                }
                let active = self.active.as_mut().ok_or(StrokeMatchError::Invariant)?;
                for player in &mut active.players {
                    if player.completion.is_none() {
                        player.completion =
                            Some(StrokeCompletion::Forfeit(ForfeitReason::GameTimeout));
                    }
                }
                let commit = build_commit(active)?;
                active.phase = ActivePhase::ResultsPending(commit);
                Ok(StrokeDeadlineOutcome::Settlement(commit))
            }
        }
    }

    /// Returns the automatically retained settlement in terminal state.
    pub fn prepare_settlement(&self) -> Result<CommitStrokeMatch, StrokeMatchError> {
        let active = self.active.as_ref().ok_or(StrokeMatchError::InvalidPhase)?;
        if let ActivePhase::ResultsPending(commit) = active.phase {
            Ok(commit)
        } else {
            Err(StrokeMatchError::InvalidPhase)
        }
    }

    /// Applies only an exact trusted aggregate result and clears to open. A matching committed
    /// result wins an abort race.
    pub fn apply_commit(
        &mut self,
        result: StrokeMatchResult,
    ) -> Result<StrokeMatchResult, StrokeMatchError> {
        if let Some(pending) = self.pending_abort {
            let abort = pending.request;
            if result.match_id() != abort.match_id() || result.result_key() != abort.result_key() {
                return Err(StrokeMatchError::IdentityMismatch);
            }
            self.pending_abort = None;
            return Ok(result);
        }
        let active = self.active.as_ref().ok_or(StrokeMatchError::InvalidPhase)?;
        let ActivePhase::ResultsPending(commit) = active.phase else {
            return Err(StrokeMatchError::InvalidPhase);
        };
        if result.match_id() != commit.match_id() || result.result_key() != commit.result_key() {
            return Err(StrokeMatchError::IdentityMismatch);
        }
        for (actual, expected) in result.players().iter().zip(commit.players()) {
            if actual.participant() != expected.participant()
                || actual.strokes() != expected.strokes()
                || actual.place() != expected.place()
                || actual.completion() != expected.completion()
            {
                return Err(StrokeMatchError::IdentityMismatch);
            }
        }
        self.active = None;
        Ok(result)
    }

    /// Moves any noncommitted state into an exact retained no-reward abort.
    pub fn abort(&mut self, reason: MatchAbortReason) -> Option<AbortStrokeMatch> {
        if let Some(pending) = self.pending_abort {
            return Some(pending.request);
        }
        let active = self.active.take()?;
        let begin = active.plan.begin();
        let abort = AbortStrokeMatch::new(begin.match_id(), begin.result_key(), reason);
        self.pending_abort = Some(PendingAbort {
            request: abort,
            roster: *active.plan.roster(),
        });
        Some(abort)
    }

    /// Replaces any noncommitted or pending terminal outcome with a higher-priority abort.
    ///
    /// Service shutdown uses this to ensure a concurrent disconnect/timeout cannot settle rewards.
    pub fn prioritize_abort(&mut self, reason: MatchAbortReason) -> Option<AbortStrokeMatch> {
        if let Some(pending) = self.pending_abort.as_mut() {
            let request = AbortStrokeMatch::new(
                pending.request.match_id(),
                pending.request.result_key(),
                reason,
            );
            pending.request = request;
            return Some(request);
        }
        self.abort(reason)
    }

    /// Retained exact abort.
    #[must_use]
    pub const fn pending_abort(&self) -> Option<AbortStrokeMatch> {
        match self.pending_abort {
            Some(pending) => Some(pending.request),
            None => None,
        }
    }

    /// Clears only the exact acknowledged abort.
    pub fn acknowledge_abort(
        &mut self,
        acknowledged: AbortStrokeMatch,
    ) -> Result<(), StrokeMatchError> {
        match self.pending_abort {
            Some(expected) if expected.request == acknowledged => {
                self.pending_abort = None;
                Ok(())
            }
            Some(_) => Err(StrokeMatchError::IdentityMismatch),
            None => Err(StrokeMatchError::InvalidPhase),
        }
    }

    fn forfeit(
        &mut self,
        caller: PlayerConnectionId,
        reason: ForfeitReason,
    ) -> Result<CommitStrokeMatch, StrokeMatchError> {
        let active = self.active.as_mut().ok_or(StrokeMatchError::InvalidPhase)?;
        if !matches!(
            active.phase,
            ActivePhase::AwaitAction | ActivePhase::AwaitResult(_)
        ) {
            return Err(StrokeMatchError::InvalidPhase);
        }
        let loser = player_index(active, caller)?;
        let winner = 1 - loser;
        active.players[loser].completion = Some(StrokeCompletion::Forfeit(reason));
        if active.players[winner].completion.is_none() {
            active.players[winner].completion = Some(StrokeCompletion::WinnerByForfeit);
        }
        let commit = build_commit(active)?;
        active.phase = ActivePhase::ResultsPending(commit);
        Ok(commit)
    }
}

fn player_index(
    active: &ActiveMatch,
    caller: PlayerConnectionId,
) -> Result<usize, StrokeMatchError> {
    active
        .players
        .iter()
        .position(|player| player.connection_id == caller)
        .ok_or(StrokeMatchError::NotParticipant)
}

fn next_unfinished(active: &ActiveMatch, previous: usize) -> usize {
    let other = 1 - previous;
    if active.players[other].completion.is_none() {
        other
    } else {
        previous
    }
}

fn verify_ids(
    active: &ActiveMatch,
    match_id: MatchId,
    result_key: MatchResultKey,
) -> Result<(), StrokeMatchError> {
    let begin = active.plan.begin();
    if begin.match_id() == match_id && begin.result_key() == result_key {
        Ok(())
    } else {
        Err(StrokeMatchError::IdentityMismatch)
    }
}

fn completion_order(completion: StrokeCompletion) -> u8 {
    match completion {
        StrokeCompletion::Holed => 0,
        StrokeCompletion::StrokeCap => 1,
        StrokeCompletion::WinnerByForfeit => 2,
        StrokeCompletion::Forfeit(_) => 3,
    }
}

fn compare_players(left: (usize, &PlayerState), right: (usize, &PlayerState)) -> Ordering {
    let left_completion = left
        .1
        .completion
        .unwrap_or(StrokeCompletion::Forfeit(ForfeitReason::GameTimeout));
    let right_completion = right
        .1
        .completion
        .unwrap_or(StrokeCompletion::Forfeit(ForfeitReason::GameTimeout));
    completion_order(left_completion)
        .cmp(&completion_order(right_completion))
        .then_with(|| left.1.strokes.cmp(&right.1.strokes))
        .then_with(|| left.0.cmp(&right.0))
}

fn domain_completion(completion: StrokeCompletion) -> DomainCompletion {
    match completion {
        StrokeCompletion::Holed => DomainCompletion::Holed,
        StrokeCompletion::StrokeCap => DomainCompletion::StrokeCap,
        StrokeCompletion::WinnerByForfeit => DomainCompletion::WinnerByForfeit,
        StrokeCompletion::Forfeit(ForfeitReason::GiveUp) => DomainCompletion::GiveUp,
        StrokeCompletion::Forfeit(ForfeitReason::Disconnect) => DomainCompletion::Disconnect,
        StrokeCompletion::Forfeit(ForfeitReason::TurnTimeout) => DomainCompletion::TurnTimeout,
        StrokeCompletion::Forfeit(ForfeitReason::GameTimeout) => DomainCompletion::GameTimeout,
    }
}

fn build_commit(active: &ActiveMatch) -> Result<CommitStrokeMatch, StrokeMatchError> {
    let first_wins =
        compare_players((0, &active.players[0]), (1, &active.players[1])) != Ordering::Greater;
    let places = if first_wins {
        [StrokePlace::First, StrokePlace::Second]
    } else {
        [StrokePlace::Second, StrokePlace::First]
    };
    let participants = active.plan.begin().participants();
    let make = |index: usize| {
        let completion = active.players[index]
            .completion
            .ok_or(StrokeMatchError::Invariant)?;
        StrokePlayerCommit::new(
            participants[index],
            active.players[index].strokes,
            places[index],
            domain_completion(completion),
        )
        .map_err(|_| StrokeMatchError::Invariant)
    };
    let players = [make(0)?, make(1)?];
    let begin = active.plan.begin();
    CommitStrokeMatch::new(
        begin.match_id(),
        begin.result_key(),
        begin.config(),
        players,
    )
    .map_err(|_| StrokeMatchError::Invariant)
}

fn same_action(left: StrokeShotAction, right: StrokeShotAction) -> bool {
    left.sequence() == right.sequence()
        && left.club() == right.club()
        && left.power().to_bits() == right.power().to_bits()
        && left.angle().to_bits() == right.angle().to_bits()
        && left.spin().to_bits() == right.spin().to_bits()
        && left.curve().to_bits() == right.curve().to_bits()
}

fn same_result(left: StrokeShotResult, right: StrokeShotResult) -> bool {
    left.sequence() == right.sequence()
        && left.x().to_bits() == right.x().to_bits()
        && left.y().to_bits() == right.y().to_bits()
        && left.z().to_bits() == right.z().to_bits()
        && left.lie() == right.lie()
        && left.holed() == right.holed()
}

#[cfg(test)]
mod tests {
    use pangya_domain::{
        AccountId, CatalogFingerprint, CourseId, MatchSeed, OneHoleConfig, ServerBalances,
        StrokeParticipant, StrokePlayerResult, StrokeRosterOrder, synthetic_stroke_reward_v1,
    };
    use pangya_protocol::Lie;
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;

    fn connection(value: u64) -> PlayerConnectionId {
        PlayerConnectionId::new(value).unwrap_or_else(|_| unreachable!())
    }

    fn plan(max_strokes: u8) -> StrokeStartPlan {
        let seed = MatchSeed::new([0; 32]);
        let (weather, wind) = deterministic_conditions(seed).unwrap_or_else(|_| unreachable!());
        let participants = [
            StrokeParticipant::new(
                AccountId::new(11).unwrap_or_else(|_| unreachable!()),
                StrokeRosterOrder::First,
                MatchResultKey::new(Uuid::from_u128(3)),
            ),
            StrokeParticipant::new(
                AccountId::new(22).unwrap_or_else(|_| unreachable!()),
                StrokeRosterOrder::Second,
                MatchResultKey::new(Uuid::from_u128(4)),
            ),
        ];
        let begin = BeginStrokeMatch::new(
            MatchId::new(Uuid::from_u128(1)),
            MatchResultKey::new(Uuid::from_u128(2)),
            participants,
            OneHoleConfig::new(CourseId::new(1).unwrap_or_else(|_| unreachable!()), 4)
                .unwrap_or_else(|_| unreachable!()),
            CatalogFingerprint::new([7; 32]),
            seed,
            weather,
            wind,
        )
        .unwrap_or_else(|_| unreachable!());
        StrokeStartPlan::new(
            begin,
            [connection(1), connection(2)],
            Duration::from_secs(5),
            Duration::from_secs(6),
            Duration::from_secs(30),
            max_strokes,
        )
        .unwrap_or_else(|_| unreachable!())
    }

    fn action(sequence: u32, power: f32) -> StrokeShotAction {
        StrokeShotAction::new(sequence, 1, power, 0.0, 0.0, 0.0).unwrap_or_else(|_| unreachable!())
    }

    fn result(sequence: u32, x: f32, holed: bool) -> StrokeShotResult {
        StrokeShotResult::new(sequence, x, 0.0, 0.0, Lie::Fairway, holed)
            .unwrap_or_else(|_| unreachable!())
    }

    fn persisted(commit: CommitStrokeMatch) -> StrokeMatchResult {
        let players = [0_usize, 1_usize].map(|index| {
            let input = commit.players()[index];
            let reward =
                synthetic_stroke_reward_v1(commit.config(), input.strokes(), input.completion())
                    .unwrap_or_else(|_| unreachable!());
            StrokePlayerResult::new(input, reward, ServerBalances::from_persisted(100, 100))
        });
        StrokeMatchResult::new(commit.match_id(), commit.result_key(), players)
    }

    fn loading(state: &mut StrokeMatchState, plan: &StrokeStartPlan) {
        assert!(state.prepare_start(plan.clone()).is_ok());
        assert!(
            state
                .confirm_begin(plan.begin().match_id(), plan.begin().result_key())
                .is_ok()
        );
    }

    fn playing(state: &mut StrokeMatchState, plan: &StrokeStartPlan, reverse: bool) {
        loading(state, plan);
        let roster = plan.roster();
        let order = if reverse {
            [roster[1], roster[0]]
        } else {
            *roster
        };
        assert_eq!(
            state.loading_complete(order[0], 100),
            Ok(StrokeLoadingOutcome::Waiting)
        );
        let mark = match state.loading_complete(order[1], 100) {
            Ok(StrokeLoadingOutcome::PersistenceRequired(mark)) => mark,
            _ => unreachable!(),
        };
        assert!(state.confirm_in_game(mark).is_ok());
    }

    /// A retail client plays the holing shot through the ordinary action/result pair and only
    /// then announces the hole is over. Charging that announcement as a stroke would score every
    /// hole one over, so it is a completion and nothing else.
    #[test]
    fn holing_out_completes_without_charging_a_stroke() {
        let plan = plan(10);
        let mut state = StrokeMatchState::new();
        playing(&mut state, &plan, false);
        let roster = *plan.roster();
        assert_eq!(
            state.accept_action(roster[0], action(1, 1.0)),
            Ok(RelayDisposition::Accepted)
        );
        assert_eq!(
            state
                .accept_result(roster[0], result(1, 1.0, false))
                .map(|outcome| outcome.disposition()),
            Ok(RelayDisposition::Accepted)
        );
        // The turn moved on, so the announcement arrives from the participant who is not active.
        assert_eq!(
            state.phase(),
            StrokeMatchPhase::AwaitAction {
                active: roster[1],
                turn: 2,
                sequence: 1,
            }
        );
        assert_eq!(state.hole_out(roster[0]), Ok(StrokeHoleOutOutcome::Waiting));
        assert_eq!(
            state.hole_out(roster[0]),
            Ok(StrokeHoleOutOutcome::Duplicate),
            "a repeated announcement changes nothing"
        );
        // The other participant keeps playing, and keeps the turn once the first is finished.
        assert_eq!(
            state.accept_action(roster[1], action(1, 1.0)),
            Ok(RelayDisposition::Accepted)
        );
        assert_eq!(
            state
                .accept_result(roster[1], result(1, 1.0, false))
                .map(|outcome| outcome.disposition()),
            Ok(RelayDisposition::Accepted)
        );
        assert_eq!(
            state.phase(),
            StrokeMatchPhase::AwaitAction {
                active: roster[1],
                turn: 3,
                sequence: 2,
            }
        );
        let commit = match state.hole_out(roster[1]) {
            Ok(StrokeHoleOutOutcome::Settlement(commit)) => commit,
            other => unreachable!("both finished settles the hole: {other:?}"),
        };
        // One shot each: exactly one stroke each, and both holed.
        for index in 0..2 {
            assert_eq!(commit.players()[index].strokes(), 1);
            assert_eq!(
                commit.players()[index].completion(),
                DomainCompletion::Holed
            );
        }
    }

    /// The hole cannot be over before it has been played, and it cannot be over while the
    /// caller's own shot is still outstanding.
    #[test]
    fn holing_out_is_refused_before_a_stroke_and_mid_shot() {
        let plan = plan(10);
        let mut state = StrokeMatchState::new();
        playing(&mut state, &plan, false);
        let roster = *plan.roster();
        assert_eq!(
            state.hole_out(roster[0]),
            Err(StrokeMatchError::InvalidPhase)
        );
        assert_eq!(
            state.hole_out(connection(9)),
            Err(StrokeMatchError::NotParticipant)
        );
        assert_eq!(
            state.accept_action(roster[0], action(1, 1.0)),
            Ok(RelayDisposition::Accepted)
        );
        assert_eq!(
            state.hole_out(roster[0]),
            Err(StrokeMatchError::InvalidPhase),
            "the shot has not landed yet"
        );
    }

    /// Finishing while holding the turn must hand it to the participant still playing, or the
    /// hole would wait on somebody who has nothing left to do.
    #[test]
    fn holing_out_in_turn_hands_the_turn_over() {
        let plan = plan(10);
        let mut state = StrokeMatchState::new();
        playing(&mut state, &plan, false);
        let roster = *plan.roster();
        // Both play one shot, which returns the turn to the first participant.
        for index in [0_usize, 1_usize] {
            assert_eq!(
                state.accept_action(roster[index], action(1, 1.0)),
                Ok(RelayDisposition::Accepted)
            );
            assert_eq!(
                state
                    .accept_result(roster[index], result(1, 1.0, false))
                    .map(|outcome| outcome.disposition()),
                Ok(RelayDisposition::Accepted)
            );
        }
        assert_eq!(
            state.phase(),
            StrokeMatchPhase::AwaitAction {
                active: roster[0],
                turn: 3,
                sequence: 2,
            }
        );
        assert_eq!(state.hole_out(roster[0]), Ok(StrokeHoleOutOutcome::Waiting));
        assert_eq!(
            state.phase(),
            StrokeMatchPhase::AwaitAction {
                active: roster[1],
                turn: 4,
                sequence: 2,
            }
        );
        assert_eq!(state.turn_generation(), Some(4));
    }

    #[test]
    fn both_loading_orders_produce_identical_playing_state() {
        let plan = plan(3);
        let mut left = StrokeMatchState::new();
        let mut right = StrokeMatchState::new();
        playing(&mut left, &plan, false);
        playing(&mut right, &plan, true);
        assert_eq!(left, right);
        assert_eq!(
            left.phase(),
            StrokeMatchPhase::AwaitAction {
                active: connection(1),
                turn: 1,
                sequence: 1,
            }
        );
        assert_eq!(left.turn_generation(), Some(1));
        assert_eq!(left.game_generation(), Some(1));
    }

    #[test]
    fn completed_loading_replays_are_exact_noops_before_and_during_persistence() {
        let plan = plan(3);
        let mut state = StrokeMatchState::new();
        loading(&mut state, &plan);
        assert_eq!(
            state.loading_complete(connection(1), 100),
            Ok(StrokeLoadingOutcome::Waiting)
        );
        let once_loaded = state.clone();
        assert_eq!(
            state.loading_complete(connection(1), 100),
            Ok(StrokeLoadingOutcome::Duplicate)
        );
        assert_eq!(state, once_loaded);
        assert!(matches!(
            state.loading_complete(connection(2), 100),
            Ok(StrokeLoadingOutcome::PersistenceRequired(_))
        ));
        for caller in [connection(1), connection(2)] {
            let pending = state.clone();
            assert_eq!(
                state.loading_complete(caller, 100),
                Ok(StrokeLoadingOutcome::Duplicate)
            );
            assert_eq!(state, pending);
            assert_eq!(
                state.loading_complete(caller, 99),
                Err(StrokeMatchError::InvalidProgress)
            );
            assert_eq!(state, pending);
        }
    }

    #[test]
    fn turns_alternate_and_exact_duplicates_survive_advancement() {
        let plan = plan(3);
        let mut state = StrokeMatchState::new();
        playing(&mut state, &plan, false);
        let first_action = action(1, 10.0);
        let first_result = result(1, 1.0, false);
        assert_eq!(
            state.accept_action(connection(2), first_action),
            Err(StrokeMatchError::InvalidTurn)
        );
        assert_eq!(
            state.accept_action(connection(1), first_action),
            Ok(RelayDisposition::Accepted)
        );
        assert_eq!(
            state
                .accept_result(connection(1), first_result)
                .map(StrokeRelayOutcome::disposition),
            Ok(RelayDisposition::Accepted)
        );
        let advanced = state.clone();
        assert_eq!(
            state.accept_action(connection(1), first_action),
            Ok(RelayDisposition::Duplicate)
        );
        assert_eq!(
            state
                .accept_result(connection(1), first_result)
                .map(StrokeRelayOutcome::disposition),
            Ok(RelayDisposition::Duplicate)
        );
        assert_eq!(state, advanced);
        assert_eq!(
            state.accept_action(connection(1), action(1, 11.0)),
            Err(StrokeMatchError::ConflictingReplay)
        );
        assert_eq!(state, advanced);
        assert_eq!(
            state.accept_result(connection(1), result(1, 2.0, false)),
            Err(StrokeMatchError::ConflictingReplay)
        );
        assert_eq!(state, advanced);
        assert_eq!(
            state.phase(),
            StrokeMatchPhase::AwaitAction {
                active: connection(2),
                turn: 2,
                sequence: 1,
            }
        );
    }

    #[test]
    fn one_finished_player_is_skipped_and_final_result_auto_prepares_unique_standings() {
        let plan = plan(3);
        let mut state = StrokeMatchState::new();
        playing(&mut state, &plan, false);
        assert!(state.accept_action(connection(1), action(1, 1.0)).is_ok());
        assert!(
            state
                .accept_result(connection(1), result(1, 0.0, true))
                .is_ok()
        );
        assert!(state.accept_action(connection(2), action(1, 1.0)).is_ok());
        let terminal = state
            .accept_result(connection(2), result(1, 0.0, false))
            .unwrap_or_else(|_| unreachable!());
        assert!(terminal.settlement().is_none());
        assert_eq!(
            state.phase(),
            StrokeMatchPhase::AwaitAction {
                active: connection(2),
                turn: 3,
                sequence: 2,
            }
        );
        assert!(state.accept_action(connection(2), action(2, 1.0)).is_ok());
        assert!(
            state
                .accept_result(connection(2), result(2, 0.0, false))
                .is_ok()
        );
        assert!(state.accept_action(connection(2), action(3, 1.0)).is_ok());
        let terminal = state
            .accept_result(connection(2), result(3, 0.0, false))
            .unwrap_or_else(|_| unreachable!());
        let commit = terminal.settlement().unwrap_or_else(|| unreachable!());
        assert_eq!(state.prepare_settlement(), Ok(commit));
        assert_eq!(commit.players()[0].place(), StrokePlace::First);
        assert_eq!(commit.players()[1].place(), StrokePlace::Second);
        assert_eq!(commit.players()[0].completion(), DomainCompletion::Holed);
        assert_eq!(
            commit.players()[1].completion(),
            DomainCompletion::StrokeCap
        );
    }

    #[test]
    fn initial_turn_giveup_makes_other_truthful_winner_with_zero_strokes() {
        let plan = plan(3);
        let mut state = StrokeMatchState::new();
        playing(&mut state, &plan, false);
        let commit = state
            .give_up(connection(1))
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(commit.players()[0].place(), StrokePlace::Second);
        assert_eq!(commit.players()[0].completion(), DomainCompletion::GiveUp);
        assert_eq!(commit.players()[0].strokes(), 0);
        assert_eq!(commit.players()[1].place(), StrokePlace::First);
        assert_eq!(
            commit.players()[1].completion(),
            DomainCompletion::WinnerByForfeit
        );
        assert_eq!(commit.players()[1].strokes(), 0);
    }

    #[test]
    fn nonactive_participant_may_give_up_and_disconnect_settles_in_game() {
        let plan = plan(3);
        let mut state = StrokeMatchState::new();
        playing(&mut state, &plan, false);
        assert!(state.accept_action(connection(1), action(1, 1.0)).is_ok());
        assert!(
            state
                .accept_result(connection(1), result(1, 0.0, false))
                .is_ok()
        );
        let commit = state.give_up(connection(1)).expect("nonactive give-up");
        assert_eq!(commit.players()[0].completion(), DomainCompletion::GiveUp);
        assert_eq!(commit.players()[0].place(), StrokePlace::Second);
        assert_eq!(
            commit.players()[1].completion(),
            DomainCompletion::WinnerByForfeit
        );

        let mut disconnected = StrokeMatchState::new();
        playing(&mut disconnected, &plan, false);
        let StrokeDeadlineOutcome::Settlement(commit) = disconnected
            .disconnect(connection(2))
            .expect("in-game disconnect")
        else {
            unreachable!()
        };
        assert_eq!(
            commit.players()[0].completion(),
            DomainCompletion::WinnerByForfeit
        );
        assert_eq!(
            commit.players()[1].completion(),
            DomainCompletion::Disconnect
        );
    }

    #[test]
    fn lower_strokes_win_and_roster_order_breaks_exact_ties() {
        let plan = plan(4);
        let mut lower = StrokeMatchState::new();
        playing(&mut lower, &plan, false);
        assert!(lower.accept_action(connection(1), action(1, 1.0)).is_ok());
        assert!(
            lower
                .accept_result(connection(1), result(1, 0.0, false))
                .is_ok()
        );
        assert!(lower.accept_action(connection(2), action(1, 1.0)).is_ok());
        assert!(
            lower
                .accept_result(connection(2), result(1, 0.0, true))
                .is_ok()
        );
        assert!(lower.accept_action(connection(1), action(2, 1.0)).is_ok());
        let commit = lower
            .accept_result(connection(1), result(2, 0.0, true))
            .expect("terminal result")
            .settlement()
            .expect("settlement");
        assert_eq!(commit.players()[0].place(), StrokePlace::Second);
        assert_eq!(commit.players()[1].place(), StrokePlace::First);

        let mut tied = StrokeMatchState::new();
        playing(&mut tied, &plan, false);
        for caller in [connection(1), connection(2)] {
            assert!(tied.accept_action(caller, action(1, 1.0)).is_ok());
            let outcome = tied
                .accept_result(caller, result(1, 0.0, true))
                .expect("result");
            if caller == connection(2) {
                let commit = outcome.settlement().expect("tie settlement");
                assert_eq!(commit.players()[0].place(), StrokePlace::First);
                assert_eq!(commit.players()[1].place(), StrokePlace::Second);
            }
        }
    }

    #[test]
    fn game_timeout_preserves_completed_participant_without_fabricating_winner() {
        let plan = plan(3);
        let mut state = StrokeMatchState::new();
        playing(&mut state, &plan, false);
        assert!(state.accept_action(connection(1), action(1, 1.0)).is_ok());
        assert!(
            state
                .accept_result(connection(1), result(1, 0.0, true))
                .is_ok()
        );
        let StrokeDeadlineOutcome::Settlement(commit) = state
            .deadline_expired(StrokeDeadline::Game { generation: 1 })
            .expect("game timeout")
        else {
            unreachable!()
        };
        assert_eq!(commit.players()[0].completion(), DomainCompletion::Holed);
        assert_eq!(
            commit.players()[1].completion(),
            DomainCompletion::GameTimeout
        );
        assert!(
            commit
                .players()
                .iter()
                .all(|player| player.completion() != DomainCompletion::WinnerByForfeit)
        );
    }

    #[test]
    fn commit_drift_rejects_without_mutation_and_commit_wins_abort_race() {
        let plan = plan(3);
        let mut state = StrokeMatchState::new();
        playing(&mut state, &plan, false);
        let commit = state.give_up(connection(1)).expect("settlement");
        let persisted = persisted(commit);
        let expected_player = commit.players()[0];
        let drifted_input = StrokePlayerCommit::new(
            expected_player.participant(),
            expected_player.strokes() + 1,
            expected_player.place(),
            expected_player.completion(),
        )
        .expect("drifted input");
        let drifted_players = [
            StrokePlayerResult::new(
                drifted_input,
                synthetic_stroke_reward_v1(
                    commit.config(),
                    drifted_input.strokes(),
                    drifted_input.completion(),
                )
                .expect("reward"),
                ServerBalances::from_persisted(100, 100),
            ),
            persisted.players()[1],
        ];
        let drifted =
            StrokeMatchResult::new(commit.match_id(), commit.result_key(), drifted_players);
        let pending = state.clone();
        assert_eq!(
            state.apply_commit(drifted),
            Err(StrokeMatchError::IdentityMismatch)
        );
        assert_eq!(state, pending);
        let abort = state
            .abort(MatchAbortReason::PersistenceFailure)
            .expect("abort");
        assert_eq!(state.pending_abort(), Some(abort));
        assert_eq!(state.apply_commit(persisted), Ok(persisted));
        assert_eq!(state.phase(), StrokeMatchPhase::Open);
    }

    #[test]
    fn stale_deadlines_are_exact_noops_and_current_deadlines_are_terminal() {
        let plan = plan(3);
        let mut state = StrokeMatchState::new();
        playing(&mut state, &plan, false);
        let before = state.clone();
        assert_eq!(
            state.deadline_expired(StrokeDeadline::Turn { generation: 0 }),
            Ok(StrokeDeadlineOutcome::Stale)
        );
        assert_eq!(state, before);
        let outcome = state
            .deadline_expired(StrokeDeadline::Turn { generation: 1 })
            .unwrap_or_else(|_| unreachable!());
        let StrokeDeadlineOutcome::Settlement(commit) = outcome else {
            unreachable!()
        };
        assert_eq!(
            commit.players()[0].completion(),
            DomainCompletion::TurnTimeout
        );
        assert_eq!(
            commit.players()[1].completion(),
            DomainCompletion::WinnerByForfeit
        );

        let mut game = StrokeMatchState::new();
        playing(&mut game, &plan, false);
        let outcome = game
            .deadline_expired(StrokeDeadline::Game { generation: 1 })
            .unwrap_or_else(|_| unreachable!());
        let StrokeDeadlineOutcome::Settlement(commit) = outcome else {
            unreachable!()
        };
        assert_eq!(commit.players()[0].place(), StrokePlace::First);
        assert_eq!(commit.players()[1].place(), StrokePlace::Second);
        assert!(
            commit
                .players()
                .iter()
                .all(|player| player.completion() == DomainCompletion::GameTimeout)
        );
    }

    #[test]
    fn loading_disconnect_and_timeout_retain_exact_abort() {
        for deadline in [false, true] {
            let plan = plan(3);
            let mut state = StrokeMatchState::new();
            loading(&mut state, &plan);
            let outcome = if deadline {
                state.deadline_expired(StrokeDeadline::Loading)
            } else {
                state.disconnect(connection(2))
            }
            .unwrap_or_else(|_| unreachable!());
            let StrokeDeadlineOutcome::Aborted(abort) = outcome else {
                unreachable!()
            };
            assert_eq!(state.pending_abort(), Some(abort));
            assert_eq!(state.abort(MatchAbortReason::Shutdown), Some(abort));
            assert!(state.acknowledge_abort(abort).is_ok());
            assert_eq!(state.phase(), StrokeMatchPhase::Open);
        }
    }

    type Rejection = Box<dyn Fn(&mut StrokeMatchState) -> Result<(), StrokeMatchError>>;

    #[test]
    fn every_public_rejection_class_preserves_clone_equality() {
        let plan = plan(3);
        let mut starting = StrokeMatchState::new();
        starting.prepare_start(plan.clone()).expect("starting");
        let mut loading_state = StrokeMatchState::new();
        loading(&mut loading_state, &plan);
        let mut persistence_pending = loading_state.clone();
        persistence_pending
            .loading_complete(connection(1), 100)
            .expect("first load");
        persistence_pending
            .loading_complete(connection(2), 100)
            .expect("second load");
        let mut awaiting_action = StrokeMatchState::new();
        playing(&mut awaiting_action, &plan, false);
        let mut awaiting_result = awaiting_action.clone();
        awaiting_result
            .accept_action(connection(1), action(1, 1.0))
            .expect("action");
        let mut results_pending = awaiting_action.clone();
        let commit = results_pending.give_up(connection(1)).expect("commit");
        let persisted = persisted(commit);
        let wrong_result = StrokeMatchResult::new(
            MatchId::new(Uuid::from_u128(999)),
            commit.result_key(),
            *persisted.players(),
        );
        let mut aborted = loading_state.clone();
        let abort = aborted.abort(MatchAbortReason::Disconnect).expect("abort");
        let wrong_abort = AbortStrokeMatch::new(
            abort.match_id(),
            MatchResultKey::new(Uuid::from_u128(998)),
            abort.reason(),
        );

        let mut cases: Vec<(&str, StrokeMatchState, StrokeMatchError, Rejection)> = vec![
            (
                "prepare start phase",
                starting.clone(),
                StrokeMatchError::InvalidPhase,
                Box::new({
                    let plan = plan.clone();
                    move |state| state.prepare_start(plan.clone()).map(drop)
                }),
            ),
            (
                "confirm begin identity",
                starting.clone(),
                StrokeMatchError::IdentityMismatch,
                Box::new(|state| {
                    state.confirm_begin(
                        MatchId::new(Uuid::from_u128(900)),
                        MatchResultKey::new(Uuid::from_u128(901)),
                    )
                }),
            ),
            (
                "cancel begin identity",
                starting,
                StrokeMatchError::IdentityMismatch,
                Box::new(|state| {
                    state.cancel_begin(
                        MatchId::new(Uuid::from_u128(900)),
                        MatchResultKey::new(Uuid::from_u128(901)),
                    )
                }),
            ),
            (
                "cancel begin phase",
                loading_state.clone(),
                StrokeMatchError::InvalidPhase,
                Box::new(move |state| {
                    state.cancel_begin(plan.begin().match_id(), plan.begin().result_key())
                }),
            ),
            (
                "loading progress",
                loading_state.clone(),
                StrokeMatchError::InvalidProgress,
                Box::new(|state| state.loading_complete(connection(1), 99).map(drop)),
            ),
            (
                "loading participant",
                loading_state.clone(),
                StrokeMatchError::NotParticipant,
                Box::new(|state| state.loading_complete(connection(3), 100).map(drop)),
            ),
            (
                "confirm in-game identity",
                persistence_pending,
                StrokeMatchError::IdentityMismatch,
                Box::new(|state| {
                    state.confirm_in_game(MarkStrokeInGame::new(
                        MatchId::new(Uuid::from_u128(900)),
                        MatchResultKey::new(Uuid::from_u128(901)),
                    ))
                }),
            ),
            (
                "action phase",
                loading_state.clone(),
                StrokeMatchError::InvalidPhase,
                Box::new(|state| state.accept_action(connection(1), action(1, 1.0)).map(drop)),
            ),
            (
                "action participant",
                awaiting_action.clone(),
                StrokeMatchError::NotParticipant,
                Box::new(|state| state.accept_action(connection(3), action(1, 1.0)).map(drop)),
            ),
            (
                "action turn",
                awaiting_action.clone(),
                StrokeMatchError::InvalidTurn,
                Box::new(|state| state.accept_action(connection(2), action(1, 1.0)).map(drop)),
            ),
            (
                "action sequence",
                awaiting_action.clone(),
                StrokeMatchError::InvalidSequence,
                Box::new(|state| state.accept_action(connection(1), action(2, 1.0)).map(drop)),
            ),
            (
                "result phase",
                awaiting_action.clone(),
                StrokeMatchError::InvalidPhase,
                Box::new(|state| {
                    state
                        .accept_result(connection(1), result(1, 0.0, false))
                        .map(drop)
                }),
            ),
            (
                "result participant",
                awaiting_result.clone(),
                StrokeMatchError::NotParticipant,
                Box::new(|state| {
                    state
                        .accept_result(connection(3), result(1, 0.0, false))
                        .map(drop)
                }),
            ),
            (
                "result turn",
                awaiting_result.clone(),
                StrokeMatchError::InvalidTurn,
                Box::new(|state| {
                    state
                        .accept_result(connection(2), result(1, 0.0, false))
                        .map(drop)
                }),
            ),
            (
                "result sequence",
                awaiting_result,
                StrokeMatchError::InvalidSequence,
                Box::new(|state| {
                    state
                        .accept_result(connection(1), result(2, 0.0, false))
                        .map(drop)
                }),
            ),
            (
                "give-up phase",
                loading_state.clone(),
                StrokeMatchError::InvalidPhase,
                Box::new(|state| state.give_up(connection(1)).map(drop)),
            ),
            (
                "give-up participant",
                awaiting_action.clone(),
                StrokeMatchError::NotParticipant,
                Box::new(|state| state.give_up(connection(3)).map(drop)),
            ),
            (
                "disconnect participant",
                awaiting_action,
                StrokeMatchError::NotParticipant,
                Box::new(|state| state.disconnect(connection(3)).map(drop)),
            ),
            (
                "settlement phase",
                loading_state.clone(),
                StrokeMatchError::InvalidPhase,
                Box::new(|state| state.prepare_settlement().map(drop)),
            ),
            (
                "commit identity",
                results_pending,
                StrokeMatchError::IdentityMismatch,
                Box::new(move |state| state.apply_commit(wrong_result).map(drop)),
            ),
            (
                "abort acknowledgement identity",
                aborted.clone(),
                StrokeMatchError::IdentityMismatch,
                Box::new(move |state| state.acknowledge_abort(wrong_abort)),
            ),
            (
                "abort acknowledgement phase",
                StrokeMatchState::new(),
                StrokeMatchError::InvalidPhase,
                Box::new(move |state| state.acknowledge_abort(abort)),
            ),
        ];
        for (name, mut state, expected, reject) in cases.drain(..) {
            let before = state.clone();
            assert_eq!(reject(&mut state), Err(expected), "{name}");
            assert_eq!(state, before, "{name}");
        }
    }

    proptest! {
        #[test]
        fn random_valid_command_streams_preserve_invariants_and_determinism(
            reverse_loading in any::<bool>(),
            operations in proptest::collection::vec(any::<u8>(), 1..100),
        ) {
            let plan = plan(8);
            let mut left = StrokeMatchState::new();
            let mut right = StrokeMatchState::new();
            playing(&mut left, &plan, reverse_loading);
            playing(&mut right, &plan, reverse_loading);
            for operation in operations {
                let apply = |state: &mut StrokeMatchState| -> Result<(), StrokeMatchError> {
                    match state.phase() {
                        StrokeMatchPhase::AwaitAction { active, sequence, .. } => {
                            if operation % 17 == 0 {
                                let caller = if operation & 1 == 0 {
                                    active
                                } else if active == connection(1) {
                                    connection(2)
                                } else {
                                    connection(1)
                                };
                                state.give_up(caller).map(drop)
                            } else {
                                state.accept_action(
                                    active,
                                    action(sequence, f32::from(operation % 100)),
                                ).map(drop)
                            }
                        }
                        StrokeMatchPhase::AwaitResult { active, sequence, .. } => state
                            .accept_result(
                                active,
                                result(sequence, f32::from(operation), operation % 11 == 0),
                            )
                            .map(drop),
                        StrokeMatchPhase::ResultsPending => state.prepare_settlement().map(drop),
                        _ => Ok(()),
                    }
                };
                let left_result = apply(&mut left);
                let right_result = apply(&mut right);
                prop_assert_eq!(left_result, right_result);
                prop_assert_eq!(&left, &right);
                prop_assert_eq!(left.roster(), Some(plan.roster()));
                match left.phase() {
                    StrokeMatchPhase::AwaitAction { turn, sequence, active } => {
                        prop_assert!(turn > 0);
                        prop_assert!(sequence > 0);
                        prop_assert!(plan.roster().contains(&active));
                        prop_assert!(left.turn_generation().is_some());
                        prop_assert_eq!(left.game_generation(), Some(1));
                    }
                    StrokeMatchPhase::AwaitResult { turn, sequence, active } => {
                        prop_assert!(turn > 0);
                        prop_assert!(sequence > 0);
                        prop_assert!(plan.roster().contains(&active));
                        prop_assert!(left.turn_generation().is_some());
                        prop_assert_eq!(left.game_generation(), Some(1));
                    }
                    StrokeMatchPhase::ResultsPending => {
                        let commit = left.prepare_settlement().expect("retained settlement");
                        prop_assert_ne!(commit.players()[0].place(), commit.players()[1].place());
                    }
                    _ => prop_assert!(false, "valid stream entered an impossible phase"),
                }
            }
        }
    }
}
