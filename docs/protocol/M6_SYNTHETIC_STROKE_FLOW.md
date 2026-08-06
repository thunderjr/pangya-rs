# M6 local synthetic exactly-two stroke flow

This document specifies the generated PangYa-RS M6 contract implemented in
`pangya-protocol::m6_stroke`. It is **not PangYa U.S. 852 protocol**. None of
these opcodes, layouts, meanings, deadlines, rewards, or packet orders has been
validated with a retail client.

All integer and IEEE-754 `f32` fields are little-endian. A UUID is its 16
network-order bytes from `Uuid::as_bytes`. Each listed size includes the
plaintext `opcode:u16`; M1 framing/encryption wraps that plaintext. Decoders
reject truncation, trailing bytes, noncanonical booleans/optional values,
non-finite floats, unknown closed discriminators, nil required result UUIDs,
and values outside the bounds below before actor dispatch.

## State-aware opcode registry

| Direction | Opcode | Accepted state | Packet |
|---|---:|---|---|
| C -> S | `0x7f30` | `InRoom` | start exactly-two stroke |
| C -> S | `0x7f31` | `InMatchLoading` | loading complete |
| C -> S | `0x7f32` | `InMatch` | shot action |
| C -> S | `0x7f33` | `InMatch` | shot result |
| C -> S | `0x7f34` | `InMatch` | give up |
| S -> C | `0x7fb0` | event | match started |
| S -> C | `0x7fb1` | event | phase |
| S -> C | `0x7fb2` | event | turn started |
| S -> C | `0x7fb3` | event | action relay |
| S -> C | `0x7fb4` | event | result relay |
| S -> C | `0x7fb5` | event | committed standings |
| S -> C | `0x7fb6` | response | command result |
| S -> C | `0x7fb7` | event | aggregate aborted |
| S -> C | `0x7fb8` | event | receiving player's balances |

A known M6 opcode in any other state closes the connection under every unknown-
opcode policy. M6 is disabled unless `[game.stroke_two].enabled=true`. Start
requires exactly two distinct authenticated room members, both ready, with the
caller as owner. The captured roster is room join order, then connection ID.

## Exact plaintext layouts

### Client to server

| Opcode | Bytes | Exact layout |
|---:|---:|---|
| `0x7f30` | 2 | `opcode:u16` |
| `0x7f31` | 3 | `opcode:u16, progress:u8` where progress is exactly `100` |
| `0x7f32` | 23 | `opcode:u16, sequence:u32, club:u8, power:f32, angle:f32, spin:f32, curve:f32` |
| `0x7f33` | 20 | `opcode:u16, sequence:u32, x:f32, y:f32, z:f32, lie:u8, holed:u8` |
| `0x7f34` | 2 | `opcode:u16` |

Start and give-up are empty. Client packets contain no match/result key,
account/connection identity, course, par, weather, wind, turn number, place,
score, Pang, EXP, balance, or course-record claim. The authenticated connection,
captured room roster, actor, catalog, and repository supply that authority.

### Server to client

| Opcode | Bytes | Exact layout |
|---:|---:|---|
| `0x7fb0` | 94 | `opcode:u16, match_id:uuid[16], course_id:u32, hole:u8, par:u8, seed:u8[32], weather:u8, wind_speed:f32, wind_angle:f32, load_timeout_ms:u32, turn_timeout_ms:u32, game_timeout_ms:u32, participant_count:u8, participant_1_connection_id:u64, participant_2_connection_id:u64` |
| `0x7fb1` | 19 | `opcode:u16, match_id:uuid[16], phase:u8` |
| `0x7fb2` | 38 | `opcode:u16, match_id:uuid[16], turn_number:u32, active_connection_id:u64, required_sequence:u32, turn_timeout_ms:u32` |
| `0x7fb3` | 31 | `opcode:u16, connection_id:u64, sequence:u32, club:u8, power:f32, angle:f32, spin:f32, curve:f32` |
| `0x7fb4` | 28 | `opcode:u16, connection_id:u64, sequence:u32, x:f32, y:f32, z:f32, lie:u8, holed:u8` |
| `0x7fb5` | 113 | `opcode:u16, match_id:uuid[16], entry_count:u8, entry_1:standing[47], entry_2:standing[47]` |
| `0x7fb6` | 4 | `opcode:u16, command:u8, outcome:u8` |
| `0x7fb7` | 19 | `opcode:u16, match_id:uuid[16], reason:u8` |
| `0x7fb8` | 18 | `opcode:u16, pang_balance:u64, experience_balance:u64` |

`standing[47]` is exactly
`connection_id:u64, place:u8, completion:u8, strokes:u16, has_score:u8,
score:i16, pang:u64, experience:u64, player_result_id:uuid[16]`.
`entry_count` and match-start `participant_count` are exactly 2. Standings are
place 1 then place 2, with distinct nonzero connection IDs and distinct non-nil
player result IDs. When `has_score=0`, the score payload must be canonical zero;
score is present exactly for holed/stroke-cap entries.

`course_id`, all three timeouts, turn number, active/participant connection IDs,
and required sequence are nonzero; participant IDs are distinct. Hole is exactly
1; par is `1..=10`. Weather is clear `0`, cloudy `1`, or rain `2`. Wind speed is
finite `0..=15`; angle is finite `0 <= angle < 360`. The seed is sent for local
reproduction but is redacted from debug, metrics, and tracing.

Phases are loading `0`, playing `1`, results-pending `2`, and finished `3`.
Commands are start `0`, load `1`, action `2`, result `3`, and give-up `4`.
Outcomes are success `0`, invalid-sequence `1`, invalid-action `2`, invalid-phase
`3`, invalid-turn `4`, and timeout `5`. Queue/command/rate exhaustion maps to
timeout as applicable; malformed fields close rather than return invalid-action.

Abort reasons are loading-timeout `0`, loading-disconnect `1`, server-shutdown
`2`, persistence-failure `3`, and startup-recovery `4`. Completion values are
holed `0`, stroke-cap `1`, give-up `2`, disconnect `3`, turn-timeout `4`,
game-timeout `5`, and winner-by-forfeit `6`.

## Shot, standing, and reward bounds

- Each player has an independent nonzero sequence beginning at 1. Only the
  active player may submit the required action and matching result. Exact replay
  compares all integer fields and float bit patterns, returns success, and emits
  no duplicate relay or mutation; conflicting reuse is invalid-sequence.
- `club` is `0..=13`; `power` finite `0..=500`; `angle` finite
  `-360..=360`; `spin` and `curve` finite `-1..=1`.
- Coordinates are finite `-100000..=100000`. Lie is tee `0`, fairway `1`,
  rough `2`, bunker `3`, green `4`, or fringe `5`; holed is byte 0 or 1.
- An accepted result increments that player's strokes exactly once. Holed or the
  configured `1..=30` cap makes the player terminal. Turns skip terminal players.
- Either captured participant may give up during active play; it is not limited
  to the active player. Give-up, in-game disconnect, and turn timeout form one
  canonical place-1 `WinnerByForfeit` / place-2 direct-forfeit pair.
- `WinnerByForfeit` has no score and exactly Pang 10 / EXP 5. Every direct or
  game-timeout loser has no score and exactly zero reward. Holed/stroke-cap use
  `score=strokes-par`, `Pang=10+2*max(par-strokes,0)`, `EXP=5`.
- Normal places sort completion class holed, stroke-cap, winner-by-forfeit,
  forfeit; then lower strokes; then captured roster order. Whole-game timeout
  does not fabricate winner-by-forfeit.

## Exact successful packet order

Cross-connection delivery is independent. The sequences below are exact for
one receiving stream. A command sender receives its `0x7fb6` before room events
queued by that command are processed on that same stream; the other participant
receives only the broadcast events.

### Start and loading barrier

Owner stream:

1. C -> S `0x7f30 StartStrokeTwo`.
2. PostgreSQL atomically reserves the immutable match, two players, and audit.
3. S -> C `0x7fb6 CommandResult(start, success)`.
4. S -> C `0x7fb0 MatchStarted`.
5. S -> C `0x7fb1 Phase(loading)`.

The peer receives `MatchStarted`, then `Phase(loading)`. For loading:

1. First participant C -> S `0x7f31 LoadingComplete(100)`.
2. That stream receives only `0x7fb6 CommandResult(load, success)`; no play is
   exposed while the second participant remains outstanding.
3. Second participant C -> S `0x7f31 LoadingComplete(100)`.
4. PostgreSQL idempotently marks `loading -> in_game`.
5. The second stream receives `0x7fb6 CommandResult(load, success)`.
6. Both streams receive `0x7fb1 Phase(playing)`.
7. Both streams receive identical `0x7fb2 TurnStarted(turn=1, roster[0],
   required_sequence=1, configured timeout)`.

An exact loading replay gets only command success.

### Each action/result and turn advance

1. Active C -> S `0x7f32 ShotAction(sequence=required, ...)`.
2. Active stream S -> C `0x7fb6 CommandResult(action, success)`.
3. Both streams S -> C `0x7fb3 ActionRelay(authoritative connection, exact action)`.
4. Active C -> S `0x7f33 ShotResult(matching sequence, ...)`.
5. Active stream S -> C `0x7fb6 CommandResult(result, success)`.
6. Both streams S -> C `0x7fb4 ResultRelay(authoritative connection, exact result)`.
7. If the aggregate continues, both receive `0x7fb2 TurnStarted` for the next
   unfinished participant. No extra playing-phase packet is emitted per shot.

A rejected well-formed command receives only its command result. An exact
replay receives only success. The turn deadline continues across action and
matching result and resets only after an accepted result advances the turn.

### Automatic result settlement

After the second terminal result:

1. Result sender receives result command success.
2. Both receive the final result relay.
3. Both receive `0x7fb1 Phase(results-pending)`.
4. PostgreSQL atomically settles both players and the aggregate.
5. Both receive the same `0x7fb5 Standings`.
6. Each receives `0x7fb8 BalanceUpdate` containing only that receiver's balance.
7. Both receive `0x7fb1 Phase(finished)` and return to `InRoom`.

No standings, balance, or finished phase is sent before durable commit. There is
no separate finish command. Give-up follows: sender command success, both
results-pending, durable settlement, standings, own balance, finished. Turn and
game deadline settlement begins at results-pending without a command result.

## Disconnect, abort, recovery, and shutdown

- Loading timeout and loading disconnect abort both captured participants with
  no rewards/ledgers. Connected peers receive `0x7fb7` when delivery is possible.
- In-game disconnect settles that participant as `Disconnect` and the opponent
  as `WinnerByForfeit`; it is not an aggregate abort. Turn timeout does the same.
  Whole-game timeout marks each unfinished player `GameTimeout` and awards no
  unfinished reward. An exact game/turn deadline tie chooses game timeout.
- Persistence ambiguity attempts idempotent `persistence_failure` abort. A
  committed result wins the abort race and is projected instead.
- Shutdown replaces any noncommitted loading, disconnect, timeout, or pending
  settlement outcome with durable no-reward `shutdown` abort. Final packet
  delivery is best-effort; durable terminal state is authoritative.
- Before Game bind, one bounded generic recovery transaction locks at most
  `cap+1` ordered incomplete M5/M6 rows, rejects overflow without partial
  mutation, marks every participant quit, and aborts `loading`, `in_game`, and
  `results_pending` as `startup_recovery`. Failure prevents readiness.
- Hidden `Starting`, loading-persistence-pending, retained abort, and exclusive
  settlement-coordinator states block room mutation until exact durable
  acknowledgement. Priority cleanup and committed-wins-abort close duplicate
  ownership and terminal races.

## Explicit exclusions and external gate

This is generated synthetic M6 only. It contains no retail packet/data claim,
items, equipment durability, special-shot rules, server trajectory/collision
physics, inventory/shop, social/ranking, or M7 behavior.

Real M6 requires two legally held U.S. 852 clients and legally supplied Course/
IFF data to validate ready/start acceptance, exact opcodes/layout/order, loading,
turns, action/result relays, deadlines, give-up/disconnect, standings, balances,
rewards, records, restart behavior, and one visible completed hole on both
clients. Keep clients, proprietary data, credentials, personal data, and raw
sensitive captures out of git. Local fixtures and encrypted E2E peers cannot
satisfy that external retail gate.
