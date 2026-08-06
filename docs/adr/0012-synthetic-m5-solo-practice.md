# ADR-0012: synthetic M5 solo-practice checkpoint

- Status: Accepted for local synthetic M5
- Date: 2026-08-05

## Context

M4 provides generated local room behavior, but the repository has no legally
validated PangYa U.S. 852 match-start, loading, shot, result, finish, or reward
contract. A bounded first-playable checkpoint is still useful for proving actor
ownership, deterministic conditions, lifecycle recovery, and exactly-once
settlement without presenting invented behavior as retail compatibility.

The tracked [`docs/pangya.wiki`](../pangya.wiki/) research is secondary,
community-authored, behavior-only material. It is not a source for packet bytes,
reward constants, IFF records, or authoritative retail protocol layouts and is
not modified by this decision.

## Decision

1. M5 is an opt-in, local synthetic **one-owner, one-member, one-hole solo
   practice** mode. The authenticated room owner is the sole participant, and
   the existing room actor is the sole mutable owner of room and match state.
   Start is rejected unless the caller is that owner and the room contains
   exactly one member.
2. Reserve local-only C -> S `0x7f20..=0x7f24` and S -> C
   `0x7fa0..=0x7fa7`. These generated values and layouts are not observed,
   inferred, or accepted PangYa U.S. 852 protocol.
3. Resolve course ID, one-hole par, and catalog SHA-256 from the mounted,
   manifest-validated synthetic catalog before listener bind. The match uses
   hole 1 only. No proprietary IFF or client data is committed.
4. Generate a fresh 32-byte seed server-side and pin condition derivation to
   `rand_chacha` **0.3.1** `ChaCha12Rng::from_seed`. Consume three consecutive
   `next_u32` values: weather is `r0 % 3`; wind speed in tenths is `r1 % 151`;
   wind angle in whole degrees is `r2 % 360`. The modulo bias is intentional.
   Persist the seed and derived conditions, send them only in the generated
   start layout, and redact the seed from debug, metrics, and tracing.
5. Use two durable persistence boundaries:
   - **reservation/start:** atomically lock the active account and insert the
     immutable match identity, result key, sole player, course/par/catalog,
     seed/weather/wind, and `started` audit before exposing a successful start;
     loading completion then idempotently advances `loading -> in_game` before
     gameplay success is exposed;
   - **settlement/result:** after the actor reaches hole complete, atomically
     lock the match and profile, validate authoritative identity/config/strokes,
     compute `solo-v1` server-side, update balances, append both ledgers and the
     committed audit, record the result projection, and mark the match committed
     before result packets are emitted.
6. Use the match UUID plus a distinct result UUID as immutable authority and
   idempotency input. PostgreSQL uniqueness and foreign keys allow exactly one
   Pang ledger row and one EXP ledger row for that result key. Both ledgers and
   match audit history reject update/delete. An exact committed replay returns
   the persisted result; drift, wrong account, wrong key, wrong course/par, and
   balance overflow fail without a second reward.
7. The client supplies bounded shot action/result evidence only. The actor
   derives caller identity from the authenticated connection, enforces action
   then matching-result sequence, increments strokes once, and ends on `holed`
   or the configured stroke cap. The client cannot submit account identity,
   score, Pang, EXP, balances, result ID, course, par, weather, or wind.
8. Disconnect, loading timeout, shutdown, or persistence ambiguity aborts every
   noncommitted match without reward. Abort is idempotent and cannot reverse a
   committed result. On startup, before Game listener bind, a bounded recovery
   transaction aborts all found `loading`, `in_game`, or `results_pending`
   matches or fails startup when the configured cap would be exceeded.
9. Keep all work bounded: loading timeout is nonzero and at most 300 seconds;
   repository timeout is nonzero, at most 60 seconds, and no greater than
   shutdown grace; strokes are `1..=30`; startup recovery is `1..=10000` rows;
   per-connection shot packets are `1..=1000000` per existing fixed window.
   Existing bounded actor, event, connection, rate, and shutdown limits remain.
10. M5 contains no items, equipment consumption, special-shot interpretation,
    trajectory or collision physics, multiplayer, turn arbitration, standings,
    or M6 behavior.

## Lifecycle and recovery

The actor lifecycle is `Open -> Starting -> Loading -> AwaitAction <->
AwaitResult -> HoleComplete -> ResultsPendingCommit -> Open`. Any noncommitted
active phase can move to retained `Aborted` state until its exact durable abort
is acknowledged. Public wire phases intentionally collapse internal states to
`Loading`, `Playing`, `HoleComplete`, and `Finished`.

A loading deadline is owned by the room actor. Disconnect and shutdown use the
priority control path so cleanup is not dependent on normal command capacity.
Startup recovery is ordered by creation time and match ID, row-locks its bounded
set, marks the sole player quit, records `startup_recovery`, and awards nothing.
A recovery overflow or database error prevents listener bind/readiness.

## Alternatives considered

- **Wait for retail evidence before any gameplay:** safest for compatibility but
  leaves actor/persistence/recovery risks untested. Rejected in favor of an
  unmistakably synthetic namespace and gate.
- **Trust client score or rewards:** rejected; it permits identity and economy
  forgery and cannot establish exactly-once settlement.
- **Keep rewards or matches in memory:** rejected; disconnect/restart ambiguity
  would duplicate or lose settlement and provide no auditable recovery.
- **Use generic RNG distributions or an unpinned ChaCha version:** rejected;
  extraction details could change persisted deterministic outcomes.
- **Hold one database transaction for the whole hole:** rejected; a network-
  duration transaction would retain locks and connections without improving
  correctness. Reservation and settlement are separate short transactions.
- **Add multiplayer, turn arbitration, items, or server physics now:** rejected
  as M6+ scope and unsupported by this checkpoint.

## Consequences and external retail gate

The local checkpoint can prove strict generated packets, a sole-owner actor,
deterministic conditions, bounded action/result sequencing, no-reward aborts,
startup recovery, and PostgreSQL exactly-once Pang/EXP settlement. It does
**not** complete the real M5 exit.

A legally held U.S. 852 client and legally supplied IFF data must independently
establish retail opcodes, layouts, packet order, field meanings and limits,
course/hole interpretation, start/loading/action/result/finish acceptance, and
reward/projection behavior. Privacy-reviewed evidence must not commit the client,
proprietary IFF/PAK content, credentials, personal data, or raw sensitive packet
captures. Only a reviewed external gate may support a real U.S. 852 M5 claim;
synthetic tests and the `0x7f20` family never can.
