//! Pure, deterministic state for the local synthetic one-hole solo match.
//!
//! A room actor is the sole owner of this value. It contains no clock, I/O, repository,
//! synchronization, or client-provided reward/score state.

use std::time::Duration;

use pangya_domain::{
    AbortMatch, AccountId, BeginSoloMatch, CommitSoloHole, MatchAbortReason, MatchId,
    MatchResultKey, MatchSeed, SoloMatchResult, StrokeCount, Weather, WindConditions,
};
use pangya_protocol::{ShotAction, ShotResult};
use rand::{RngCore as _, SeedableRng as _};
use rand_chacha::ChaCha12Rng;
use thiserror::Error;

/// Loading may never reserve a room for longer than this hard cap.
pub const LOADING_TIMEOUT_HARD_CAP: Duration = Duration::from_secs(300);
/// The gameplay protocol and state machine cap a solo hole at thirty strokes.
pub const MAX_SOLO_STROKES: u8 = 30;

/// Stable deterministic weather and wind for a persisted match seed.
///
/// This algorithm is part of the persisted match contract: initialize the explicitly pinned
/// `rand_chacha` 0.3 `ChaCha12Rng` directly from the 32 seed bytes, extract three consecutive
/// little-endian RNG `u32` values through `next_u32`, then use fixed modulo reductions of 3,
/// 151, and 360 for weather, speed tenths, and angle degrees respectively. The small modulo bias
/// is intentional and must not be replaced by distribution helpers whose extraction may change.
///
/// # Errors
/// Returns [`SoloMatchError::DeterministicConditionsInvariant`] if the fixed reductions no longer
/// satisfy the checked wind domain bounds.
pub fn deterministic_conditions(
    seed: MatchSeed,
) -> Result<(Weather, WindConditions), SoloMatchError> {
    let mut rng = ChaCha12Rng::from_seed(*seed.as_bytes());
    let weather = match rng.next_u32() % 3 {
        0 => Weather::Clear,
        1 => Weather::Cloudy,
        _ => Weather::Rain,
    };
    let speed_tenths = (rng.next_u32() % 151) as u16;
    let angle_degrees = (rng.next_u32() % 360) as u16;
    // Both modulo ranges are subsets of the current domain constructor bounds. Propagate an
    // explicit invariant failure rather than silently substituting different persisted input if
    // those bounds ever change.
    let wind = WindConditions::new(speed_tenths, angle_degrees)
        .map_err(|_| SoloMatchError::DeterministicConditionsInvariant)?;
    Ok((weather, wind))
}

/// Immutable, checked reservation used before calling `MatchRepository::begin_solo`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SoloStartPlan {
    begin: BeginSoloMatch,
    loading_timeout: Duration,
    max_strokes: u8,
}

impl SoloStartPlan {
    /// Validates all local bounds and the deterministic conditions embedded in the begin request.
    ///
    /// # Errors
    /// Returns [`SoloMatchError::InvalidPlan`] for a zero/over-cap or sub-millisecond loading
    /// timeout, a stroke cap outside `1..=30`, or conditions that do not match the seed. Propagates
    /// [`SoloMatchError::DeterministicConditionsInvariant`] if seed derivation violates the checked
    /// wind domain bounds.
    pub fn new(
        begin: BeginSoloMatch,
        loading_timeout: Duration,
        max_strokes: u8,
    ) -> Result<Self, SoloMatchError> {
        let timeout_ms = loading_timeout.as_millis();
        let valid_timeout = !loading_timeout.is_zero()
            && loading_timeout <= LOADING_TIMEOUT_HARD_CAP
            && timeout_ms != 0
            && timeout_ms <= u128::from(u32::MAX);
        let expected = deterministic_conditions(begin.seed())?;
        if !valid_timeout
            || !(1..=MAX_SOLO_STROKES).contains(&max_strokes)
            || expected != (begin.weather(), begin.wind())
        {
            return Err(SoloMatchError::InvalidPlan);
        }
        Ok(Self {
            begin,
            loading_timeout,
            max_strokes,
        })
    }

    /// Immutable repository begin request.
    #[must_use]
    pub const fn begin(&self) -> &BeginSoloMatch {
        &self.begin
    }

    /// Checked actor loading timeout.
    #[must_use]
    pub const fn loading_timeout(&self) -> Duration {
        self.loading_timeout
    }

    /// Checked authoritative stroke cap.
    #[must_use]
    pub const fn max_strokes(&self) -> u8 {
        self.max_strokes
    }
}

/// Public phase projection without exposing mutable implementation details.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoloMatchPhase {
    /// No reserved, active, or unacknowledged aborted match.
    Open,
    /// A begin request is reserved while persistence is attempted.
    Starting,
    /// Begin was confirmed and loading is deadline-bound by the room actor.
    Loading,
    /// Waiting for the exact next action sequence.
    AwaitAction {
        /// Required action sequence.
        sequence: u32,
    },
    /// Waiting for the result matching the accepted action.
    AwaitResult {
        /// Required result sequence.
        sequence: u32,
    },
    /// The ball was holed or the authoritative stroke cap was reached.
    HoleComplete,
    /// A commit request has been prepared and awaits repository output.
    ResultsPendingCommit,
    /// An abort request remains available until explicitly acknowledged or room close.
    Aborted,
}

/// Whether a relay was newly accepted or was an exact idempotent replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayDisposition {
    /// State advanced and the server stroke/accounting rules were applied once.
    Accepted,
    /// The exact last accepted value was replayed; state and stroke count did not change.
    Duplicate,
}

/// Stable match transition rejection.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SoloMatchError {
    /// Fixed RNG reductions no longer satisfy the checked wind domain bounds.
    #[error("deterministic solo conditions violate domain invariants")]
    DeterministicConditionsInvariant,
    /// The start plan violates a fixed local bound or deterministic condition.
    #[error("solo match start plan is invalid")]
    InvalidPlan,
    /// The command is invalid in the current phase.
    #[error("solo match command is invalid in the current phase")]
    InvalidPhase,
    /// Match or result identity did not exactly match the reservation.
    #[error("solo match identity does not match")]
    IdentityMismatch,
    /// Account identity did not match the authenticated room owner.
    #[error("solo match account does not match")]
    AccountMismatch,
    /// A shot sequence was skipped, stale, or did not match its pending action.
    #[error("solo shot sequence is invalid")]
    InvalidSequence,
    /// A repeated sequence carried different content.
    #[error("solo shot replay conflicts with accepted content")]
    ConflictingReplay,
    /// Loading progress was not exactly one hundred.
    #[error("solo loading progress must be exactly 100")]
    InvalidProgress,
    /// Checked stroke construction failed despite local bounds.
    #[error("solo stroke count is invalid")]
    InvalidStrokes,
    /// The caller is not an authenticated room member.
    #[error("solo command caller is not a room member")]
    NotMember,
    /// The caller is not the authoritative room owner.
    #[error("solo command requires the room owner")]
    NotOwner,
    /// Solo start requires exactly one room member.
    #[error("solo match requires exactly one room member")]
    NotSolo,
    /// A bounded actor queue was full.
    #[error("solo match command queue is full")]
    QueueFull,
    /// The owning room actor closed.
    #[error("solo match room is closed")]
    Closed,
    /// A bounded actor command deadline elapsed.
    #[error("solo match command timed out")]
    Timeout,
}

#[derive(Clone, Debug, PartialEq)]
enum ActivePhase {
    Starting,
    Loading,
    AwaitAction,
    AwaitResult(ShotAction),
    HoleComplete,
    ResultsPendingCommit,
}

#[derive(Clone, Debug, PartialEq)]
struct ActiveMatch {
    plan: SoloStartPlan,
    phase: ActivePhase,
    expected_sequence: u32,
    strokes: u8,
    last_action: Option<ShotAction>,
    last_result: Option<ShotResult>,
}

/// Pure solo match aggregate owned exclusively by one room actor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoloMatchState {
    active: Option<ActiveMatch>,
    pending_abort: Option<AbortMatch>,
}

impl SoloMatchState {
    /// Constructs open match state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: None,
            pending_abort: None,
        }
    }

    /// Current stable phase projection.
    #[must_use]
    pub fn phase(&self) -> SoloMatchPhase {
        if self.pending_abort.is_some() {
            return SoloMatchPhase::Aborted;
        }
        self.active
            .as_ref()
            .map_or(SoloMatchPhase::Open, |active| match &active.phase {
                ActivePhase::Starting => SoloMatchPhase::Starting,
                ActivePhase::Loading => SoloMatchPhase::Loading,
                ActivePhase::AwaitAction => SoloMatchPhase::AwaitAction {
                    sequence: active.expected_sequence,
                },
                ActivePhase::AwaitResult(action) => SoloMatchPhase::AwaitResult {
                    sequence: action.sequence(),
                },
                ActivePhase::HoleComplete => SoloMatchPhase::HoleComplete,
                ActivePhase::ResultsPendingCommit => SoloMatchPhase::ResultsPendingCommit,
            })
    }

    /// Whether room mutations must be blocked until commit or abort acknowledgement.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.is_some() || self.pending_abort.is_some()
    }

    /// Immutable checked start plan while a match is reserved or active.
    #[must_use]
    pub fn start_plan(&self) -> Option<&SoloStartPlan> {
        self.active.as_ref().map(|active| &active.plan)
    }

    /// Immutable begin request while a match is reserved or active.
    #[must_use]
    pub fn begin(&self) -> Option<&BeginSoloMatch> {
        self.start_plan().map(SoloStartPlan::begin)
    }

    /// Loading duration when and only when the match is in Loading.
    #[must_use]
    pub fn loading_timeout(&self) -> Option<Duration> {
        self.active.as_ref().and_then(|active| {
            matches!(active.phase, ActivePhase::Loading).then_some(active.plan.loading_timeout())
        })
    }

    /// Reserves an immutable begin request without performing persistence.
    pub fn prepare_start(&mut self, plan: SoloStartPlan) -> Result<BeginSoloMatch, SoloMatchError> {
        if self.is_active() {
            return Err(SoloMatchError::InvalidPhase);
        }
        let begin = plan.begin().clone();
        self.active = Some(ActiveMatch {
            plan,
            phase: ActivePhase::Starting,
            expected_sequence: 1,
            strokes: 0,
            last_action: None,
            last_result: None,
        });
        Ok(begin)
    }

    /// Confirms the exact persisted reservation and enters deadline-bound loading.
    pub fn confirm_begin(
        &mut self,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), SoloMatchError> {
        let active = self.active.as_mut().ok_or(SoloMatchError::InvalidPhase)?;
        if active.phase != ActivePhase::Starting {
            return Err(SoloMatchError::InvalidPhase);
        }
        verify_ids(active, match_id, result_key)?;
        active.phase = ActivePhase::Loading;
        Ok(())
    }

    /// Cancels only the exact unpersisted start reservation and returns to Open.
    pub fn cancel_begin(
        &mut self,
        match_id: MatchId,
        result_key: MatchResultKey,
    ) -> Result<(), SoloMatchError> {
        let active = self.active.as_ref().ok_or(SoloMatchError::InvalidPhase)?;
        if active.phase != ActivePhase::Starting {
            return Err(SoloMatchError::InvalidPhase);
        }
        verify_ids(active, match_id, result_key)?;
        self.active = None;
        Ok(())
    }

    /// Accepts only canonical complete loading and begins sequence one.
    pub fn loading_complete(&mut self, progress: u8) -> Result<(), SoloMatchError> {
        if progress != 100 {
            return Err(SoloMatchError::InvalidProgress);
        }
        let active = self.active.as_mut().ok_or(SoloMatchError::InvalidPhase)?;
        if active.phase != ActivePhase::Loading {
            return Err(SoloMatchError::InvalidPhase);
        }
        active.phase = ActivePhase::AwaitAction;
        Ok(())
    }

    /// Accepts the expected validated action, or recognizes its exact idempotent replay.
    pub fn accept_action(
        &mut self,
        action: ShotAction,
    ) -> Result<RelayDisposition, SoloMatchError> {
        let active = self.active.as_mut().ok_or(SoloMatchError::InvalidPhase)?;
        let sequence = action.sequence();
        if active
            .last_action
            .is_some_and(|previous| previous.sequence() == sequence)
        {
            return if active
                .last_action
                .is_some_and(|previous| same_action(previous, action))
            {
                Ok(RelayDisposition::Duplicate)
            } else {
                Err(SoloMatchError::ConflictingReplay)
            };
        }
        if active.phase != ActivePhase::AwaitAction {
            return Err(SoloMatchError::InvalidPhase);
        }
        if sequence != active.expected_sequence {
            return Err(SoloMatchError::InvalidSequence);
        }
        active.last_action = Some(action);
        active.phase = ActivePhase::AwaitResult(action);
        Ok(RelayDisposition::Accepted)
    }

    /// Accepts one matching validated result and increments the server stroke exactly once.
    pub fn accept_result(
        &mut self,
        result: ShotResult,
    ) -> Result<RelayDisposition, SoloMatchError> {
        let active = self.active.as_mut().ok_or(SoloMatchError::InvalidPhase)?;
        let sequence = result.sequence();
        if active
            .last_result
            .is_some_and(|previous| previous.sequence() == sequence)
        {
            return if active
                .last_result
                .is_some_and(|previous| same_result(previous, result))
            {
                Ok(RelayDisposition::Duplicate)
            } else {
                Err(SoloMatchError::ConflictingReplay)
            };
        }
        let ActivePhase::AwaitResult(action) = active.phase else {
            return Err(SoloMatchError::InvalidPhase);
        };
        if sequence != action.sequence() {
            return Err(SoloMatchError::InvalidSequence);
        }
        let next_strokes = active
            .strokes
            .checked_add(1)
            .filter(|value| *value <= active.plan.max_strokes())
            .ok_or(SoloMatchError::InvalidStrokes)?;
        active.strokes = next_strokes;
        active.last_result = Some(result);
        if result.holed() || next_strokes == active.plan.max_strokes() {
            active.phase = ActivePhase::HoleComplete;
        } else {
            active.expected_sequence = active
                .expected_sequence
                .checked_add(1)
                .ok_or(SoloMatchError::InvalidSequence)?;
            active.phase = ActivePhase::AwaitAction;
        }
        Ok(RelayDisposition::Accepted)
    }

    /// Builds the server-owned commit request only after authoritative hole completion.
    pub fn prepare_finish(&mut self) -> Result<CommitSoloHole, SoloMatchError> {
        let active = self.active.as_mut().ok_or(SoloMatchError::InvalidPhase)?;
        if active.phase != ActivePhase::HoleComplete {
            return Err(SoloMatchError::InvalidPhase);
        }
        let strokes = StrokeCount::new(u16::from(active.strokes))
            .map_err(|_| SoloMatchError::InvalidStrokes)?;
        let begin = active.plan.begin();
        let commit = CommitSoloHole::new(
            begin.match_id(),
            begin.result_key(),
            begin.account_id(),
            begin.config(),
            strokes,
        );
        active.phase = ActivePhase::ResultsPendingCommit;
        Ok(commit)
    }

    /// Applies only the exact trusted repository result, then clears the match to Open.
    pub fn apply_commit(
        &mut self,
        result: SoloMatchResult,
    ) -> Result<SoloMatchResult, SoloMatchError> {
        if let Some(abort) = self.pending_abort {
            if result.match_id() != abort.match_id() || result.result_key() != abort.result_key() {
                return Err(SoloMatchError::IdentityMismatch);
            }
            if result.account_id() != abort.account_id() {
                return Err(SoloMatchError::AccountMismatch);
            }
            self.pending_abort = None;
            return Ok(result);
        }
        let active = self.active.as_ref().ok_or(SoloMatchError::InvalidPhase)?;
        if active.phase != ActivePhase::ResultsPendingCommit {
            return Err(SoloMatchError::InvalidPhase);
        }
        let begin = active.plan.begin();
        if result.match_id() != begin.match_id() || result.result_key() != begin.result_key() {
            return Err(SoloMatchError::IdentityMismatch);
        }
        if result.account_id() != begin.account_id() {
            return Err(SoloMatchError::AccountMismatch);
        }
        if result.strokes().get() != u16::from(active.strokes) {
            return Err(SoloMatchError::InvalidStrokes);
        }
        self.active = None;
        Ok(result)
    }

    /// Atomically moves any noncommitted reservation/match into the idempotent abort side state.
    pub fn abort(&mut self, reason: MatchAbortReason) -> Option<AbortMatch> {
        if let Some(abort) = self.pending_abort {
            return Some(abort);
        }
        let active = self.active.take()?;
        let begin = active.plan.begin();
        let abort = AbortMatch::new(
            begin.match_id(),
            begin.result_key(),
            begin.account_id(),
            reason,
        );
        self.pending_abort = Some(abort);
        Some(abort)
    }

    /// Returns the retained idempotent abort request without mutation.
    #[must_use]
    pub const fn pending_abort(&self) -> Option<AbortMatch> {
        self.pending_abort
    }

    /// Acknowledges only the exact retained abort request and clears to Open.
    pub fn acknowledge_abort(&mut self, acknowledged: AbortMatch) -> Result<(), SoloMatchError> {
        match self.pending_abort {
            Some(expected) if expected == acknowledged => {
                self.pending_abort = None;
                Ok(())
            }
            Some(_) => Err(SoloMatchError::IdentityMismatch),
            None => Err(SoloMatchError::InvalidPhase),
        }
    }

    /// Authoritative participant of an active or retained aborted match.
    #[must_use]
    pub fn account_id(&self) -> Option<AccountId> {
        self.active
            .as_ref()
            .map(|active| active.plan.begin().account_id())
            .or_else(|| self.pending_abort.map(AbortMatch::account_id))
    }
}

fn verify_ids(
    active: &ActiveMatch,
    match_id: MatchId,
    result_key: MatchResultKey,
) -> Result<(), SoloMatchError> {
    let begin = active.plan.begin();
    if begin.match_id() == match_id && begin.result_key() == result_key {
        Ok(())
    } else {
        Err(SoloMatchError::IdentityMismatch)
    }
}

fn same_action(left: ShotAction, right: ShotAction) -> bool {
    left.sequence() == right.sequence()
        && left.club() == right.club()
        && left.power().to_bits() == right.power().to_bits()
        && left.angle().to_bits() == right.angle().to_bits()
        && left.spin().to_bits() == right.spin().to_bits()
        && left.curve().to_bits() == right.curve().to_bits()
}

fn same_result(left: ShotResult, right: ShotResult) -> bool {
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
        CatalogFingerprint, CourseId, MatchResultKey, OneHoleConfig, ServerBalances, SoloReward,
    };
    use pangya_protocol::Lie;
    use uuid::Uuid;

    use super::*;

    fn account() -> AccountId {
        AccountId::new(7).unwrap_or_else(|_| unreachable!())
    }

    fn plan(max_strokes: u8) -> SoloStartPlan {
        let seed = MatchSeed::new([0; 32]);
        let (weather, wind) = deterministic_conditions(seed).unwrap_or_else(|_| unreachable!());
        let begin = BeginSoloMatch::new(
            MatchId::new(Uuid::from_u128(1)),
            MatchResultKey::new(Uuid::from_u128(2)),
            account(),
            OneHoleConfig::new(CourseId::new(1).unwrap_or_else(|_| unreachable!()), 4)
                .unwrap_or_else(|_| unreachable!()),
            CatalogFingerprint::new([3; 32]),
            seed,
            weather,
            wind,
        );
        SoloStartPlan::new(begin, Duration::from_secs(5), max_strokes)
            .unwrap_or_else(|_| unreachable!())
    }

    fn action(sequence: u32, power: f32) -> ShotAction {
        ShotAction::new(sequence, 1, power, 0.0, 0.0, 0.0).unwrap_or_else(|_| unreachable!())
    }

    fn result(sequence: u32, x: f32, holed: bool) -> ShotResult {
        ShotResult::new(sequence, x, 0.0, 0.0, Lie::Fairway, holed)
            .unwrap_or_else(|_| unreachable!())
    }

    fn begin_playing(state: &mut SoloMatchState, plan: &SoloStartPlan) {
        assert!(state.prepare_start(plan.clone()).is_ok());
        assert!(
            state
                .confirm_begin(plan.begin().match_id(), plan.begin().result_key())
                .is_ok()
        );
        assert!(state.loading_complete(100).is_ok());
    }

    #[test]
    fn fixed_seed_has_exact_deterministic_vector() {
        let (weather, wind) =
            deterministic_conditions(MatchSeed::new([0; 32])).unwrap_or_else(|_| unreachable!());
        assert_eq!(weather, Weather::Rain);
        assert_eq!(wind.speed_tenths(), 133);
        assert_eq!(wind.angle_degrees(), 129);
    }

    #[test]
    fn start_plan_rejects_timeout_stroke_and_condition_drift_bounds() {
        let valid = plan(3);
        assert_eq!(
            SoloStartPlan::new(valid.begin().clone(), Duration::ZERO, 3),
            Err(SoloMatchError::InvalidPlan)
        );
        assert_eq!(
            SoloStartPlan::new(
                valid.begin().clone(),
                LOADING_TIMEOUT_HARD_CAP + Duration::from_millis(1),
                3,
            ),
            Err(SoloMatchError::InvalidPlan)
        );
        assert_eq!(
            SoloStartPlan::new(valid.begin().clone(), Duration::from_secs(1), 0),
            Err(SoloMatchError::InvalidPlan)
        );
        assert_eq!(
            SoloStartPlan::new(
                valid.begin().clone(),
                Duration::from_secs(1),
                MAX_SOLO_STROKES + 1,
            ),
            Err(SoloMatchError::InvalidPlan)
        );
        let drifted_weather = match valid.begin().weather() {
            Weather::Clear => Weather::Cloudy,
            Weather::Cloudy | Weather::Rain => Weather::Clear,
        };
        let begin = valid.begin();
        let drifted = BeginSoloMatch::new(
            begin.match_id(),
            begin.result_key(),
            begin.account_id(),
            begin.config(),
            begin.catalog_fingerprint(),
            begin.seed(),
            drifted_weather,
            begin.wind(),
        );
        assert_eq!(
            SoloStartPlan::new(drifted, Duration::from_secs(1), 3),
            Err(SoloMatchError::InvalidPlan)
        );
    }

    #[test]
    fn every_rejecting_public_transition_preserves_exact_state() {
        type Rejection = Box<dyn FnOnce(&mut SoloMatchState) -> Result<(), SoloMatchError>>;

        fn at_loading(plan: &SoloStartPlan) -> SoloMatchState {
            let mut state = SoloMatchState::new();
            assert!(state.prepare_start(plan.clone()).is_ok());
            assert!(
                state
                    .confirm_begin(plan.begin().match_id(), plan.begin().result_key())
                    .is_ok()
            );
            state
        }

        fn at_await_action(plan: &SoloStartPlan) -> SoloMatchState {
            let mut state = at_loading(plan);
            assert!(state.loading_complete(100).is_ok());
            state
        }

        fn at_await_result(plan: &SoloStartPlan) -> SoloMatchState {
            let mut state = at_await_action(plan);
            assert!(state.accept_action(action(1, 10.0)).is_ok());
            state
        }

        fn at_hole_complete(plan: &SoloStartPlan) -> SoloMatchState {
            let mut state = at_await_result(plan);
            assert!(state.accept_result(result(1, 5.0, true)).is_ok());
            state
        }

        let plan = plan(3);
        let mut starting = SoloMatchState::new();
        assert!(starting.prepare_start(plan.clone()).is_ok());
        let loading = at_loading(&plan);
        let awaiting_action = at_await_action(&plan);
        let awaiting_result = at_await_result(&plan);
        let mut after_result = awaiting_result.clone();
        assert!(after_result.accept_result(result(1, 5.0, false)).is_ok());
        let hole_complete = at_hole_complete(&plan);
        let mut pending_commit = hole_complete.clone();
        assert!(pending_commit.prepare_finish().is_ok());
        let mut aborted = awaiting_action.clone();
        let retained_abort = aborted
            .abort(MatchAbortReason::Disconnect)
            .unwrap_or_else(|| unreachable!());

        let result_for = |match_id, result_key, account_id, strokes| {
            SoloMatchResult::new(
                match_id,
                result_key,
                account_id,
                StrokeCount::new(strokes).unwrap_or_else(|_| unreachable!()),
                SoloReward::from_persisted(-3, 16, 5),
                ServerBalances::from_persisted(16, 5),
            )
        };
        let wrong_match = MatchId::new(Uuid::from_u128(901));
        let wrong_key = MatchResultKey::new(Uuid::from_u128(902));
        let wrong_account = AccountId::new(8).unwrap_or_else(|_| unreachable!());
        let plan_match_id = plan.begin().match_id();
        let plan_result_key = plan.begin().result_key();
        let exact_result = result_for(plan_match_id, plan_result_key, account(), 1);
        let wrong_abort = AbortMatch::new(
            wrong_match,
            retained_abort.result_key(),
            retained_abort.account_id(),
            retained_abort.reason(),
        );

        let cases: Vec<(&str, SoloMatchState, SoloMatchError, Rejection)> = vec![
            (
                "prepare start phase",
                starting.clone(),
                SoloMatchError::InvalidPhase,
                Box::new(move |state| state.prepare_start(plan).map(drop)),
            ),
            (
                "confirm identity mismatch",
                starting.clone(),
                SoloMatchError::IdentityMismatch,
                Box::new(move |state| state.confirm_begin(wrong_match, plan_result_key)),
            ),
            (
                "confirm phase",
                loading.clone(),
                SoloMatchError::InvalidPhase,
                Box::new(move |state| state.confirm_begin(plan_match_id, plan_result_key)),
            ),
            (
                "cancel identity mismatch",
                starting,
                SoloMatchError::IdentityMismatch,
                Box::new(move |state| state.cancel_begin(plan_match_id, wrong_key)),
            ),
            (
                "cancel phase",
                loading.clone(),
                SoloMatchError::InvalidPhase,
                Box::new(move |state| state.cancel_begin(plan_match_id, plan_result_key)),
            ),
            (
                "loading progress",
                loading,
                SoloMatchError::InvalidProgress,
                Box::new(|state| state.loading_complete(99)),
            ),
            (
                "loading phase",
                awaiting_action.clone(),
                SoloMatchError::InvalidPhase,
                Box::new(|state| state.loading_complete(100)),
            ),
            (
                "action sequence mismatch",
                awaiting_action.clone(),
                SoloMatchError::InvalidSequence,
                Box::new(|state| state.accept_action(action(2, 1.0)).map(drop)),
            ),
            (
                "action phase",
                awaiting_result.clone(),
                SoloMatchError::InvalidPhase,
                Box::new(|state| state.accept_action(action(2, 1.0)).map(drop)),
            ),
            (
                "action conflicting replay",
                awaiting_result.clone(),
                SoloMatchError::ConflictingReplay,
                Box::new(|state| state.accept_action(action(1, 11.0)).map(drop)),
            ),
            (
                "result sequence mismatch",
                awaiting_result.clone(),
                SoloMatchError::InvalidSequence,
                Box::new(|state| state.accept_result(result(2, 1.0, false)).map(drop)),
            ),
            (
                "result conflicting replay",
                after_result,
                SoloMatchError::ConflictingReplay,
                Box::new(|state| state.accept_result(result(1, 6.0, false)).map(drop)),
            ),
            (
                "result phase",
                awaiting_action,
                SoloMatchError::InvalidPhase,
                Box::new(|state| state.accept_result(result(1, 1.0, false)).map(drop)),
            ),
            (
                "finish phase",
                awaiting_result,
                SoloMatchError::InvalidPhase,
                Box::new(|state| state.prepare_finish().map(drop)),
            ),
            (
                "apply match mismatch",
                pending_commit.clone(),
                SoloMatchError::IdentityMismatch,
                Box::new(move |state| {
                    state
                        .apply_commit(result_for(wrong_match, plan_result_key, account(), 1))
                        .map(drop)
                }),
            ),
            (
                "apply key mismatch",
                pending_commit.clone(),
                SoloMatchError::IdentityMismatch,
                Box::new(move |state| {
                    state
                        .apply_commit(result_for(plan_match_id, wrong_key, account(), 1))
                        .map(drop)
                }),
            ),
            (
                "apply account mismatch",
                pending_commit.clone(),
                SoloMatchError::AccountMismatch,
                Box::new(move |state| {
                    state
                        .apply_commit(result_for(plan_match_id, plan_result_key, wrong_account, 1))
                        .map(drop)
                }),
            ),
            (
                "apply stroke mismatch",
                pending_commit.clone(),
                SoloMatchError::InvalidStrokes,
                Box::new(move |state| {
                    state
                        .apply_commit(result_for(plan_match_id, plan_result_key, account(), 2))
                        .map(drop)
                }),
            ),
            (
                "apply phase",
                hole_complete,
                SoloMatchError::InvalidPhase,
                Box::new(move |state| state.apply_commit(exact_result).map(drop)),
            ),
            (
                "abort acknowledgement mismatch",
                aborted,
                SoloMatchError::IdentityMismatch,
                Box::new(move |state| state.acknowledge_abort(wrong_abort)),
            ),
            (
                "abort acknowledgement phase",
                SoloMatchState::new(),
                SoloMatchError::InvalidPhase,
                Box::new(move |state| state.acknowledge_abort(retained_abort)),
            ),
        ];

        for (name, mut state, expected, reject) in cases {
            let before = state.clone();
            assert_eq!(reject(&mut state), Err(expected), "{name}");
            assert_eq!(state, before, "{name}");
        }
    }

    #[test]
    fn cancel_begin_requires_exact_ids_and_returns_open() {
        let plan = plan(3);
        let mut state = SoloMatchState::new();
        assert!(state.prepare_start(plan.clone()).is_ok());
        let before = state.clone();
        assert_eq!(
            state.cancel_begin(
                MatchId::new(Uuid::from_u128(999)),
                plan.begin().result_key(),
            ),
            Err(SoloMatchError::IdentityMismatch)
        );
        assert_eq!(state, before);
        assert!(
            state
                .cancel_begin(plan.begin().match_id(), plan.begin().result_key())
                .is_ok()
        );
        assert_eq!(state.phase(), SoloMatchPhase::Open);
        assert!(state.pending_abort().is_none());
    }

    #[test]
    fn action_and_result_duplicates_conflicts_and_skips_are_exact() {
        let plan = plan(3);
        let mut state = SoloMatchState::new();
        begin_playing(&mut state, &plan);
        let first = action(1, 10.0);
        assert_eq!(state.accept_action(first), Ok(RelayDisposition::Accepted));
        let accepted = state.clone();
        assert_eq!(state.accept_action(first), Ok(RelayDisposition::Duplicate));
        assert_eq!(state, accepted);
        assert_eq!(
            state.accept_action(action(1, 11.0)),
            Err(SoloMatchError::ConflictingReplay)
        );
        assert_eq!(state, accepted);
        let first_result = result(1, 5.0, false);
        assert_eq!(
            state.accept_result(first_result),
            Ok(RelayDisposition::Accepted)
        );
        let next = state.clone();
        assert_eq!(
            state.accept_result(first_result),
            Ok(RelayDisposition::Duplicate)
        );
        assert_eq!(state, next);
        assert_eq!(
            state.accept_result(result(1, 6.0, false)),
            Err(SoloMatchError::ConflictingReplay)
        );
        assert_eq!(state, next);
        assert_eq!(
            state.accept_action(action(3, 1.0)),
            Err(SoloMatchError::InvalidSequence)
        );
        assert_eq!(state, next);
    }

    #[test]
    fn max_strokes_completes_and_commit_mismatch_does_not_mutate() {
        let plan = plan(1);
        let mut state = SoloMatchState::new();
        begin_playing(&mut state, &plan);
        assert!(state.accept_action(action(1, 10.0)).is_ok());
        assert!(state.accept_result(result(1, 5.0, false)).is_ok());
        assert_eq!(state.phase(), SoloMatchPhase::HoleComplete);
        let commit = state.prepare_finish().unwrap_or_else(|_| unreachable!());
        assert_eq!(commit.strokes().get(), 1);
        let before = state.clone();
        let wrong = SoloMatchResult::new(
            MatchId::new(Uuid::from_u128(99)),
            plan.begin().result_key(),
            account(),
            StrokeCount::new(1).unwrap_or_else(|_| unreachable!()),
            SoloReward::from_persisted(-3, 16, 5),
            ServerBalances::from_persisted(16, 5),
        );
        assert_eq!(
            state.apply_commit(wrong),
            Err(SoloMatchError::IdentityMismatch)
        );
        assert_eq!(state, before);
        let wrong_key = SoloMatchResult::new(
            plan.begin().match_id(),
            MatchResultKey::new(Uuid::from_u128(999)),
            account(),
            StrokeCount::new(1).unwrap_or_else(|_| unreachable!()),
            SoloReward::from_persisted(-3, 16, 5),
            ServerBalances::from_persisted(16, 5),
        );
        assert_eq!(
            state.apply_commit(wrong_key),
            Err(SoloMatchError::IdentityMismatch)
        );
        assert_eq!(state, before);
        let wrong_account = SoloMatchResult::new(
            plan.begin().match_id(),
            plan.begin().result_key(),
            AccountId::new(8).unwrap_or_else(|_| unreachable!()),
            StrokeCount::new(1).unwrap_or_else(|_| unreachable!()),
            SoloReward::from_persisted(-3, 16, 5),
            ServerBalances::from_persisted(16, 5),
        );
        assert_eq!(
            state.apply_commit(wrong_account),
            Err(SoloMatchError::AccountMismatch)
        );
        assert_eq!(state, before);
        let wrong_strokes = SoloMatchResult::new(
            plan.begin().match_id(),
            plan.begin().result_key(),
            account(),
            StrokeCount::new(2).unwrap_or_else(|_| unreachable!()),
            SoloReward::from_persisted(-2, 14, 5),
            ServerBalances::from_persisted(14, 5),
        );
        assert_eq!(
            state.apply_commit(wrong_strokes),
            Err(SoloMatchError::InvalidStrokes)
        );
        assert_eq!(state, before);
        let committed = SoloMatchResult::new(
            plan.begin().match_id(),
            plan.begin().result_key(),
            account(),
            StrokeCount::new(1).unwrap_or_else(|_| unreachable!()),
            SoloReward::from_persisted(-3, 16, 5),
            ServerBalances::from_persisted(16, 5),
        );
        assert_eq!(state.apply_commit(committed), Ok(committed));
        assert_eq!(state.phase(), SoloMatchPhase::Open);
    }

    #[test]
    fn every_noncommitted_phase_aborts_without_a_result() {
        for phase in 0..6 {
            let plan = plan(3);
            let mut state = SoloMatchState::new();
            assert!(state.prepare_start(plan.clone()).is_ok());
            if phase >= 1 {
                assert!(
                    state
                        .confirm_begin(plan.begin().match_id(), plan.begin().result_key())
                        .is_ok()
                );
            }
            if phase >= 2 {
                assert!(state.loading_complete(100).is_ok());
            }
            if phase >= 3 {
                assert!(state.accept_action(action(1, 10.0)).is_ok());
            }
            if phase >= 4 {
                assert!(state.accept_result(result(1, 0.0, true)).is_ok());
            }
            if phase >= 5 {
                assert!(state.prepare_finish().is_ok());
            }
            let abort = state
                .abort(MatchAbortReason::Disconnect)
                .unwrap_or_else(|| unreachable!());
            assert_eq!(abort.match_id(), plan.begin().match_id());
            assert_eq!(abort.result_key(), plan.begin().result_key());
            assert_eq!(abort.account_id(), plan.begin().account_id());
            assert_eq!(state.phase(), SoloMatchPhase::Aborted);
        }
    }

    #[test]
    fn abort_is_retained_and_idempotent_until_exact_acknowledgement() {
        let plan = plan(3);
        let mut state = SoloMatchState::new();
        assert!(state.prepare_start(plan).is_ok());
        let abort = state
            .abort(MatchAbortReason::Disconnect)
            .unwrap_or_else(|| unreachable!());
        assert_eq!(state.abort(MatchAbortReason::Shutdown), Some(abort));
        let before = state.clone();
        let wrong = AbortMatch::new(
            MatchId::new(Uuid::from_u128(9)),
            abort.result_key(),
            abort.account_id(),
            abort.reason(),
        );
        assert_eq!(
            state.acknowledge_abort(wrong),
            Err(SoloMatchError::IdentityMismatch)
        );
        assert_eq!(state, before);
        assert!(state.acknowledge_abort(abort).is_ok());
        assert_eq!(state.phase(), SoloMatchPhase::Open);
    }
}
