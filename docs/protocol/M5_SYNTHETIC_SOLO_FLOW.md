# M5 local synthetic solo-practice flow

This document specifies the generated PangYa-RS M5 contract implemented in
`pangya-protocol::m5_solo`. It is **not PangYa U.S. 852 protocol**. None of these
opcodes, layouts, field meanings, or packet orders has been validated with a
retail client.

All integer and IEEE-754 `f32` fields are little-endian. A UUID is exactly its 16
network-order bytes (the order returned by `Uuid::as_bytes`), not a little-endian
integer. Every row includes the plaintext `opcode:u16`; M1 transport framing and
encryption wrap that plaintext. Decoders reject truncation, trailing bytes,
noncanonical booleans, non-finite floats, unknown closed discriminators, and
out-of-range values before actor dispatch.

## State-aware opcode registry

| Direction | Opcode | Accepted state | Packet |
|---|---:|---|---|
| C -> S | `0x7f20` | `InRoom` | start solo |
| C -> S | `0x7f21` | `InMatchLoading` | loading complete |
| C -> S | `0x7f22` | `InMatch` | shot action |
| C -> S | `0x7f23` | `InMatch` | shot result |
| C -> S | `0x7f24` | `InMatch` | finish hole |
| S -> C | `0x7fa0` | event | match started |
| S -> C | `0x7fa1` | event | match phase |
| S -> C | `0x7fa2` | event | authoritative action relay |
| S -> C | `0x7fa3` | event | authoritative result relay |
| S -> C | `0x7fa4` | event | committed hole result |
| S -> C | `0x7fa5` | event | authoritative balances |
| S -> C | `0x7fa6` | response | command result |
| S -> C | `0x7fa7` | event | match aborted |

A known M5 opcode in any other state closes the connection under every unknown-
opcode policy. M5 is disabled unless `[game.solo_practice].enabled=true`; start
also requires the authenticated room owner to be the room's only member.

## Exact plaintext layouts

### Client to server

| Opcode | Bytes | Exact layout |
|---:|---:|---|
| `0x7f20` | 2 | `opcode:u16` |
| `0x7f21` | 3 | `opcode:u16, progress:u8` where progress is exactly `100` |
| `0x7f22` | 23 | `opcode:u16, sequence:u32, club:u8, power:f32, angle:f32, spin:f32, curve:f32` |
| `0x7f23` | 20 | `opcode:u16, sequence:u32, x:f32, y:f32, z:f32, lie:u8, holed:u8` |
| `0x7f24` | 2 | `opcode:u16` |

`0x7f20` and `0x7f24` are deliberately empty. The client sends no match ID,
account/connection identity, course, hole, par, weather, wind, strokes, score,
Pang, EXP, balance, or result key. Authenticated connection and room membership
supply identity; the server supplies match configuration and every reward field.

### Server to client

| Opcode | Bytes | Exact layout |
|---:|---:|---|
| `0x7fa0` | 69 | `opcode:u16, match_id:uuid[16], course_id:u32, hole:u8, par:u8, seed:u8[32], weather:u8, wind_speed:f32, wind_angle:f32, load_timeout_ms:u32` |
| `0x7fa1` | 19 | `opcode:u16, match_id:uuid[16], phase:u8` |
| `0x7fa2` | 31 | `opcode:u16, connection_id:u64, sequence:u32, club:u8, power:f32, angle:f32, spin:f32, curve:f32` |
| `0x7fa3` | 28 | `opcode:u16, connection_id:u64, sequence:u32, x:f32, y:f32, z:f32, lie:u8, holed:u8` |
| `0x7fa4` | 55 | `opcode:u16, match_id:uuid[16], hole:u8, strokes:u16, score:i16, pang:u64, experience:u64, result_id:uuid[16]` |
| `0x7fa5` | 18 | `opcode:u16, pang_balance:u64, experience_balance:u64` |
| `0x7fa6` | 4 | `opcode:u16, command:u8, result:u8` |
| `0x7fa7` | 19 | `opcode:u16, match_id:uuid[16], reason:u8` |

`course_id` and `load_timeout_ms` are nonzero; `hole` is exactly 1; `par` is
`1..=10`. Weather is clear `0`, cloudy `1`, or rain `2`. Wind speed is finite
`0..=15`; wind angle is finite `0 <= angle < 360`. The server-generated seed is
sent in `0x7fa0` for local deterministic reproduction but redacted from debug,
metrics, and tracing.

Phase values are loading `0`, playing `1`, hole-complete `2`, and finished `3`.
Command values are start `0`, loading-complete `1`, action `2`, result `3`, and
finish `4`. Result values are success `0`, invalid-sequence `1`, invalid-action
`2`, invalid-phase `3`, and timeout `4`. The current runtime uses invalid-
sequence for conflicting replay and timeout for command/queue/rate exhaustion;
malformed action fields close rather than produce invalid-action.

Abort reason values are player-disconnected `0`, loading-timeout `1`, protocol-
violation `2`, and server-shutdown `3`. The closed codec reserves all four.
Current persistence-failure and startup-recovery/shutdown projections map to
server-shutdown when a live peer can receive an event; malformed live input is
closed and cleanup is persisted as a no-reward disconnect.

## Action and result bounds

- `sequence` is nonzero and must start at 1. A new action must equal the exact
  next sequence; its result must use the same sequence. After a non-holed result,
  the next action is sequence + 1.
- Exact replay compares every integer and every float bit pattern. It returns
  command success without incrementing strokes or emitting another relay/phase.
  Reusing a sequence with different content returns invalid-sequence.
- `club` is `0..=13`.
- `power` is finite `0..=500`; `angle` is finite `-360..=360`; `spin` and
  `curve` are finite `-1..=1`.
- Each of `x`, `y`, and `z` is finite `-100000..=100000`.
- Lie values are tee `0`, fairway `1`, rough `2`, bunker `3`, green `4`, and
  fringe `5`. `holed` is the canonical byte `0` or `1`.
- An accepted result increments server-owned strokes exactly once. HoleComplete
  is reached on `holed=1` or the configured stroke cap (`1..=30`). No trajectory,
  collision, lie plausibility, item, or special-shot physics is implemented.

## Exact successful packet order

The connection begins this flow already inside the sole-member M4 room.

### Start and loading

1. C -> S `0x7f20 StartSolo`.
2. The server durably reserves immutable match/player/start-audit state.
3. S -> C `0x7fa6 CommandResult(start, success)`.
4. S -> C `0x7fa0 MatchStarted`.
5. S -> C `0x7fa1 MatchPhase(loading)`.
6. C -> S `0x7f21 LoadingComplete(100)`.
7. The server durably and idempotently marks `loading -> in_game`.
8. S -> C `0x7fa6 CommandResult(loading-complete, success)`.
9. S -> C `0x7fa1 MatchPhase(playing)`.

### Each action/result pair

1. C -> S `0x7f22 ShotAction(sequence=N, ...)`.
2. S -> C `0x7fa6 CommandResult(action, success)`.
3. S -> C `0x7fa2 ShotActionRelay(server_connection_id, exact action)`.
4. S -> C `0x7fa1 MatchPhase(playing)`.
5. C -> S `0x7f23 ShotResult(sequence=N, ...)`.
6. S -> C `0x7fa6 CommandResult(result, success)`.
7. S -> C `0x7fa3 ShotResultRelay(server_connection_id, exact result)`.
8. S -> C `0x7fa1 MatchPhase(playing or hole-complete)`.

An exact duplicate gets only the corresponding success packet. A rejected but
well-formed action/result gets only its command-result packet. The action phase
stays wire `playing` while the actor internally waits for the matching result.

### Finish and settlement

After a holed result or stroke-cap completion:

1. C -> S `0x7f24 FinishHole`.
2. S -> C `0x7fa1 MatchPhase(hole-complete)` for the precommit transition. This
   repeats the hole-complete phase already emitted by the final result.
3. PostgreSQL atomically computes and commits score, Pang, EXP, both immutable
   ledger rows, post-balances, player result, audit, and terminal match state.
4. S -> C `0x7fa4 HoleResult`.
5. S -> C `0x7fa5 BalanceUpdate`.
6. S -> C `0x7fa6 CommandResult(finish, success)`.
7. S -> C `0x7fa1 MatchPhase(finished)`; the connection returns to `InRoom`.

No result, balance, finish success, or finished phase is sent before durable
settlement. `HoleResult.result_id` is the server-generated immutable PostgreSQL
idempotency key, not a client claim.

## Error, disconnect, timeout, recovery, and shutdown behavior

- Invalid UTF/wire structure, noncanonical values, unknown discriminators,
  non-finite/out-of-range floats, trailing bytes, or known wrong-state opcodes
  are protocol errors and close the connection.
- Well-formed invalid sequence/replay/phase commands return one `0x7fa6` and do
  not mutate accepted state. The fixed shot-packet window is separate from the
  general packet budget; exhaustion returns timeout without accepting the shot.
- Disconnect in any noncommitted phase uses priority actor/lobby cleanup,
  persists `aborted/disconnect`, marks the player quit, and appends no reward
  ledger. No disconnect notification is promised to the disconnected peer.
- Loading deadline expiry persists `aborted/loading_timeout`, sends
  `0x7fa7 MatchAborted(loading-timeout)` when possible, and returns to `InRoom`.
- Shutdown drains within configured grace and persists `aborted/shutdown` for
  active noncommitted matches. Delivery of a final packet is best-effort; durable
  no-reward terminal state is authoritative.
- Persistence ambiguity attempts an idempotent no-reward
  `persistence_failure` abort. A previously committed result wins over abort.
- Before Game listener bind, startup recovery fetches and row-locks at most
  `configured cap + 1` rows to detect overflow without partial mutation. It
  aborts at most the cap of `loading`, `in_game`, or `results_pending` matches
  as `startup_recovery`. Overflow or storage failure prevents startup/readiness.

## Explicit exclusions and external gate

This is M5 solo only. It has no multiplayer, turn arbitration, standings, item
use, equipment durability, special-shot rules, server physics, or M6 behavior.
It includes no proprietary data and makes no real-client claim.

Real U.S. 852 opcodes, layouts, packet order, limits, course/IFF interpretation,
and start/loading/action/result/finish acceptance remain an external gate using
a legally held client and legally supplied data. The generated `0x7f20` and
`0x7fa0` families must never be labeled retail protocol.
