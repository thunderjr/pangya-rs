# ADR-0013: synthetic M6 exactly-two stroke checkpoint

- Status: Accepted for local synthetic M6
- Date: 2026-08-05

## Context

M5 proves a generated one-player hole, but it deliberately has no multiplayer,
ready barrier, active-turn arbitration, standings, or per-player settlement.
Those properties need a bounded checkpoint without presenting invented behavior
as PangYa U.S. 852 compatibility. No legally validated retail two-client packet
contract or Course/IFF interpretation is held in this repository.

## Decision

1. M6 is an opt-in local synthetic **exactly-two-player, one-hole stroke** mode.
   The room must contain exactly two distinct authenticated accounts, both room
   members must be ready, and only the room owner may start. Captured roster
   order is room join order, then connection ID. The existing room actor remains
   the sole mutable owner of room, loading, turn, and match state.
2. Reserve generated local-only C -> S `0x7f30..=0x7f34` and S -> C
   `0x7fb0..=0x7fb8`. Their exact layouts and order are specified separately;
   none is observed, inferred, or accepted PangYa U.S. 852 protocol.
3. Reuse the manifest-validated synthetic one-hole Course projection and M5's
   pinned server-generated conditions: a fresh 32-byte seed, `rand_chacha`
   0.3.1 `ChaCha12Rng::from_seed`, and three consecutive `next_u32` modulo
   3/151/360 reductions. Persist the seed and derived weather/wind before match
   exposure, send the seed only in the synthetic start packet, and redact it
   from debug, metrics, and tracing.
4. Loading is an exactly-two barrier. After both canonical `100` completions,
   persist `loading -> in_game` before exposing play. Roster position zero owns
   turn one. Turns alternate to the next unfinished player; each player has an
   independent sequence beginning at one. An accepted result increments that
   player's server-owned strokes once and ends that player on `holed` or the
   configured stroke cap.
5. The actor owns a single loading deadline, a generation-tagged deadline for
   each active turn, and a generation-tagged whole-game deadline. The turn
   deadline covers action and matching result; it resets only after an accepted
   result advances the turn. The whole-game deadline starts only after durable
   in-game confirmation and wins an exact tie with the turn deadline. Stale
   timer generations cannot mutate state.
6. Either participant, including the non-active participant, may give up during
   active play. Give-up, in-game disconnect, and active-turn timeout are direct
   forfeits: the direct loser receives no score, Pang, EXP, ledger, or course
   record, while the first-place `WinnerByForfeit` receives no fabricated golf
   score, fixed Pang 10 and EXP 5, and no course record. Whole-game timeout marks
   each unfinished player `GameTimeout`; it does not fabricate a winner.
7. Normal holed/stroke-cap rewards use server-only `stroke-two-v1`, reusing the
   checked M5 formula: `score=strokes-par`, `Pang=10+2*max(par-strokes,0)`, and
   `EXP=5`. Places sort completion class (holed, stroke-cap, winner-by-forfeit,
   forfeit), then lower strokes, then captured roster order. Exact places and
   two distinct per-player result keys are authoritative.
8. Reserve and settlement are short atomic PostgreSQL boundaries. Reservation
   locks both active accounts in sorted order and inserts one immutable match,
   exactly two ordered players, and start audit before match-start exposure.
   Settlement locks the aggregate and both profiles in sorted order, validates
   all captured authority, changes `in_game -> results_pending -> committed`,
   computes both results, updates both profiles, ledgers, player projections,
   eligible records, and commit audit in one transaction. A normal two-player
   reward therefore appends exactly four immutable ledgers (Pang and EXP for
   each player); a zero-reward loser appends none.
9. Only `Holed` is course-record eligible. Per account/course/`stroke_two`, each
   eligible completion increments `rounds_completed`; a strictly lower score,
   then fewer strokes at equal score, replaces the best authority and its
   `first_achieved_at`. Non-improvements retain the existing best and first-achieved
   time while updating the projection timestamp. Stroke-cap, all forfeits, and
   winner-by-forfeit never create or update a record.
10. Loading disconnect and loading timeout abort the entire aggregate without
    reward. In-game disconnect is a forfeit settlement. Persistence ambiguity
    attempts an idempotent no-reward abort. Shutdown has priority over pending
    disconnect, timeout, or settlement and durably aborts every noncommitted
    aggregate; an already committed aggregate wins an abort race.
11. Migration `0006_m6_stroke_records.sql` extends the M5 aggregate for
    `stroke_two`, ordered participants, distinct player result keys, places and
    completions, conditional ledger authority, record projection/authority, and
    immutable terminal history. Migration `0007_m6_winner_by_forfeit.sql` adds
    the truthful fixed-reward completion and a deferred exact-pair constraint.
12. Before Game listener bind, the existing bounded generic recovery transaction
    aborts all found `loading`, `in_game`, and `results_pending` M5/M6 aggregates,
    marks every captured player quit, and appends one `startup_recovery` abort
    audit. It fetches cap + 1 to reject overflow without partial mutation. With
    both modes enabled, startup performs recovery once with the larger enabled
    cap and timeout; failure prevents listener bind and readiness.
13. Internal `Starting`, loading-persistence-pending, results-pending, retained
    abort, and exclusive persistence-coordinator state remains hidden authority.
    Room mutations remain blocked until the exact commit or abort is durably
    acknowledged. Generation checks, atomic actor commands, a retained single
    persistence claim, shutdown replacement, and committed-wins-abort close the
    documented timeout/disconnect/shutdown races.
14. M6 remains disabled by default and bounded by validated loading, turn, game,
    commit, stroke, recovery, shot-window, actor-queue, connection, and shutdown
    limits. Observability uses only fixed `stroke_two` lifecycle/commit/shot/rate
    labels and an exact active gauge; it exposes no match/result key, seed,
    account, balance, reward, or shot coordinates. Existing no-body logging,
    digest-only unknown capture, redacted secrets, public-bind acknowledgement,
    readiness, and safe-Rust policies remain in force.
15. M6 adds no inventory/shop, item use, equipment durability, special-shot
    interpretation, server trajectory/collision physics, social/ranking, or M7
    behavior.

## Lifecycle, retained authority, and consequences

The actor lifecycle is `Open -> Starting -> Loading ->
LoadingPersistencePending -> AwaitAction <-> AwaitResult -> ResultsPending ->
Open`, with `Aborted` retained until the exact durable acknowledgement. Public
wire phases intentionally expose only loading, playing, results-pending, and
finished. Exactly one connected coordinator receives persistence work; priority
cleanup may claim only unclaimed work, preventing duplicate repository owners.

This checkpoint can prove generated two-client actor behavior, bounded turn and
game arbitration, truthful forfeit settlement, exact standings, four-ledger
normal settlement, course-record authority, and recovery. It does **not** close
the real M6 exit.

## External two-client and retail Course gate

A legally held U.S. 852 client pair and legally supplied Course/IFF data must
independently establish the real room-ready/start contract, exact opcodes,
layouts, order, roster/identity fields, loading barrier, active-turn and sequence
semantics, deadlines, action/result relays, give-up/disconnect behavior,
standings, balances, rewards, records, restart behavior, and successful visible
completion on both clients. Privacy-reviewed evidence must not commit clients,
proprietary IFF/PAK data, credentials, personal data, or raw sensitive captures.
Synthetic fixtures, local encrypted TCP peers, `0x7f30`/`0x7fb0`, and
`stroke-two-v1` can never satisfy or be labeled as that retail gate.
