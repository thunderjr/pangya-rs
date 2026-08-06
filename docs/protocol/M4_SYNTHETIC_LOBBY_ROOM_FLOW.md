# M4 local synthetic lobby and room flow

This is the exact generated PangYa-RS M4 contract. It is **not** the PangYa
U.S. 852 room protocol, and none of these room opcodes, layouts, or ordering has
been accepted by a retail client.

All integer fields are little-endian. Each layout below starts with the plaintext
`opcode:u16`; transport framing/encryption remains the M1 contract. `pstring(N)`
means `length:u16` followed by exactly that many bytes, with `length <= N`.
Decoders reject truncation, trailing bytes, noncanonical booleans (anything other
than `0` or `1`), invalid UTF-8, zero IDs, and domain-invalid values.

## State and opcode registry

After the M3 `0x004e channel_id:u32` response, the connection is `InChannel`.
Only list, create, and join are registered there. A successful create or join
moves it to `InRoom`; only leave, settings, ready, chat, kick, and state are
registered there. A successful leave or an incoming kick returns it to
`InChannel`. EOF, cancellation, timeout, limits, malformed input, or a known
opcode in the wrong state closes the connection.

| Direction | Opcode | State | Packet |
|---|---:|---|---|
| C -> S | `0x7f00` | `InChannel` | room list request |
| C -> S | `0x7f01` | `InChannel` | create room |
| C -> S | `0x7f02` | `InChannel` | join room |
| C -> S | `0x7f03` | `InRoom` | leave room |
| C -> S | `0x7f04` | `InRoom` | replace settings |
| C -> S | `0x7f05` | `InRoom` | set ready |
| C -> S | `0x7f06` | `InRoom` | room chat |
| C -> S | `0x7f07` | `InRoom` | owner kick |
| C -> S | `0x7f08` | `InRoom` | fetch room state |
| S -> C | `0x7f80` | response | room list |
| S -> C | `0x7f81` | response/event | room state |
| S -> C | `0x7f82` | response | command result |
| S -> C | `0x7f83` | event | membership event |
| S -> C | `0x7f84` | event | chat event |

Known room opcodes in the wrong state are never treated as unknown and are never
ignored or captured.

## Client-to-server layouts

| Opcode | Exact plaintext layout |
|---:|---|
| `0x7f00` | `opcode:u16` |
| `0x7f01` | `opcode:u16, name:pstring(32), has_password:u8, [password:pstring(16)], max_members:u8` |
| `0x7f02` | `opcode:u16, room_id:u32, has_password:u8, [password:pstring(16)]` |
| `0x7f03` | `opcode:u16` |
| `0x7f04` | `opcode:u16, max_members:u8` |
| `0x7f05` | `opcode:u16, ready:u8` |
| `0x7f06` | `opcode:u16, text:pstring(128)` |
| `0x7f07` | `opcode:u16, target_connection_id:u64` |
| `0x7f08` | `opcode:u16` |

The authenticated connection supplies caller account, connection ID, nickname,
and sender identity. No request contains a client-claimed sender. Room names are
trimmed and must be 1..=32 UTF-8 bytes without controls. Chat is preserved and
must be 1..=128 UTF-8 bytes without controls. Password input is 1..=16 UTF-8
bytes when present. Capacity is `2..=30`.

## Shared server projections

A room summary is:

```text
room_id:u32
name:pstring(32)
owner_nickname:pstring(64)
members:u8
max_members:u8
password_protected:u8
```

A member is:

```text
connection_id:u64
account_id:i64
nickname:pstring(64)
is_owner:u8
is_ready:u8
```

Nickname strings must also be canonical, 3..=16 ASCII bytes using only
alphanumerics, `_`, or `-`; the 64-byte wire limit is only a defensive decoding
cap. Account IDs are positive `i64` values. A room snapshot is one room summary
followed by `member_count:u16` and that many members. The count must
equal the summary occupancy and cannot exceed 30.

## Server-to-client layouts

| Opcode | Exact plaintext layout |
|---:|---|
| `0x7f80` | `opcode:u16, room_count:u16, room_summary[room_count]` |
| `0x7f81` | `opcode:u16, room_summary, member_count:u16, member[member_count]` |
| `0x7f82` | `opcode:u16, command:u8, result:u8` |
| `0x7f83` | `opcode:u16, room_id:u32, kind:u8, member` |
| `0x7f84` | `opcode:u16, room_id:u32, sender:member, text:pstring(128)` |

Room lists contain at most 4096 summaries and are ordered by ascending room ID.
Passwords, password salts, and password digests never appear in public output.

Command discriminators are fixed: list `0`, create `1`, join `2`, leave `3`,
settings `4`, ready `5`, chat `6`, kick `7`, state `8`. Result discriminators are
success `0`, queue-full `1`, closed `2`, already-member `3`, full `4`, invalid-
password `5`, not-member `6`, not-owner `7`, cannot-kick-self `8`, member-not-
found `9`, capacity-below-occupancy `10`, maximum-rooms `11`, room-not-found `12`,
ID-exhausted `13`, timeout `14`.

Membership kinds are joined `0`, left `1`, kicked `2`, owner-changed `3`. The
current runtime emits `kicked`; joins, leaves, ready/settings changes, and owner
transfer are conveyed by authoritative state snapshots. The other discriminants
are reserved in the generated codec and are not claims about U.S. 852.

## Successful response and event order

The generated runtime uses this ordering:

1. **List:** server sends `0x7f80`. A registry error sends `0x7f82` instead.
2. **Create:** server sends create success `0x7f82`, then creator state `0x7f81`.
3. **Join:** server sends join success `0x7f82`, then a direct `0x7f81`; the actor
   also broadcasts the post-join `0x7f81` to every member, so the joiner receives
   a second identical state and existing members receive one.
4. **Settings/ready:** on success, server sends `0x7f82`, a direct `0x7f81`, then
   the actor-delivered `0x7f81` broadcast. Other members receive the broadcast.
5. **Chat:** sender receives chat success `0x7f82`; every member, including the
   sender, receives `0x7f84` with the server-derived sender projection.
6. **Kick:** owner receives kick success `0x7f82`, a direct `0x7f81`, and the
   actor state broadcast. Remaining members receive the state broadcast. The
   target receives `0x7f83` kind `2`, naming the authoritative kicking owner,
   and returns to `InChannel`.
7. **State:** server sends one `0x7f81` and no success result.
8. **Leave:** server sends leave success `0x7f82`, returns the leaver to
   `InChannel`, and broadcasts the resulting state to remaining members. If the
   room becomes empty, it is removed instead.
9. **Disconnect:** cleanup uses the priority control queue. Remaining members get
   the resulting state; the last member removes the room. Shutdown is bounded.

Failures that are valid command outcomes send only `0x7f82` and retain the
current state. Malformed packets, known wrong-state packets, and exhausted
connection-level rate/queue policy close instead of sending a room result.

## Authority, ordering, and bounds

One lobby task owns the ascending room registry and one-room-per-connection map.
One actor task solely owns each room's mutable members, settings, password digest,
ready flags, and join order. Owner-only settings and kick are enforced there.
Owner departure transfers ownership to the longest-present member; an equal join
order is broken by lowest connection ID. Concurrent joins serialize and cannot
exceed capacity.

Room passwords are zeroized after use. The actor stores a fresh 32-byte random
salt and `SHA-256(salt || password)` only, checked with constant-time equality.
Rooms and IDs are process-local and non-durable.

Production configuration defaults/hard caps are:

| Bound | Default | Hard cap |
|---|---:|---:|
| rooms | 1024 | 4096 |
| lobby command queue | 256 | 8192 |
| lobby actor-event queue | 256 | 8192 |
| normal commands per room | 64 | 4096 |
| priority control commands per room | 16 | 64 |
| outbound room events per connection | 64 | 4096 |
| room commands per connection/window | 30 | 10000 |
| chat messages per connection/window | 10 | 1000 |
| unknown strikes | 3 | 32 |
| metadata-digest captures | 256 | 4096 |
| command timeout | 3 seconds | 300 seconds and no greater than shutdown grace |

Room commands also consume the existing global/source/connection packet and byte
budgets. A full normal/lobby queue yields a fixed result where possible. A full
per-connection actor-event queue signals the runtime and closes that connection
as limited. Disconnect/shutdown use bounded priority queues and deadlines.

## Truly unknown post-channel opcodes

`protocol.unknown_opcode_policy` is `disconnect`, `ignore`, or `capture`.
Disconnect closes on the first unknown opcode. Ignore continues until the fixed
strike limit, then closes. Capture also continues until that limit while adding
only `(GameState, opcode, payload_len, SHA-256(payload))` to a fixed-capacity
process-local oldest-evicted ring. Here `payload` is the plaintext body after the
opcode. Raw bodies, room text, passwords, bearers, and account secrets are not
retained by capture or emitted as metric labels.

## Explicit M5 exclusion and external gates

There is no room start, match loading, hole/shot flow, gameplay relay, score,
finish, persistence, reward, Pang, or EXP behavior in M4. No M5 behavior is
implemented or implied.

External gates remain the exact U.S. 852 room opcodes, layouts, ordering, list
semantics, limits, password behavior, and successful create/enter acceptance with
a legally held client. The `0x7f00` family must never be labeled U.S. 852.
