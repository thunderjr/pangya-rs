# M4 synthetic lobby and room evidence — 2026-08-05

## Claim boundary

The local synthetic M4 lobby/room checkpoint is implemented. This evidence does
**not** claim that the provisional `0x7f00` packet family, layouts, ordering,
room semantics, or create/enter flow is PangYa U.S. 852 compatible. The complete
local validation matrix passed after independent-review blocker closure.

M4 stops at process-local lobby and room state. It contains no M5 room start,
loading, hole/shot gameplay, scoring, persistence, finish, or rewards.

## Requirement-to-test evidence

| Requirement | Implementation boundary | Test evidence |
|---|---|---|
| Generated opcodes/layouts are exact and source-free | `pangya-protocol::m4_room`; four generated fixture pairs | `pangya-protocol/tests/m4_room.rs`: exact create/list/state/chat re-encoding and fixed discriminants |
| Parsing is strict and bounded before allocation | UTF-8/domain checks, canonical booleans, trailing-byte rejection, list/member caps | M4 protocol length, count, boolean/discriminant, trailing-byte, inconsistent-snapshot, and arbitrary-body property tests |
| Known packets are state-aware | list/create/join only in `InChannel`; remaining commands only in `InRoom` | M4 registry test plus TCP wrong-state closure under every unknown policy |
| Client cannot claim caller/sender authority | authenticated `RoomIdentity`; requests omit sender/account/nickname | protocol no-sender test; TCP chat event and kick/settings authorization assertions |
| One actor owns room mutation | separate bounded normal and priority control queues | room pure-state invariants, actor lifecycle, saturation, disconnect, and shutdown tests |
| Lobby serializes discovery/admission | sole registry owns rooms and connection-to-room map; atomic command gates prevent timeout-after-mutation | registry cap/unique ID/one-room tests, concurrent-join capacity, and queued-cancel/begun-commit tests |
| Capacity and failures do not corrupt state | occupancy `<= 30`; rejected commands do not mutate | pure rejection/property tests; real-PostgreSQL TCP concurrent final-slot race |
| Passwords remain private | zeroized input; random-salted SHA-256 digest; constant-time verify; public boolean only | actor wrong/correct password tests; TCP absent/wrong/correct password flow; fixture/metrics secret searches |
| Ownership and cleanup are authoritative | owner-only settings/kick; deterministic transfer; a separately sized priority lobby-control queue reaches room cleanup even when normal work is saturated | pure and registry transfer tests; saturated-normal-queue disconnect; TCP non-owner failures, kick, owner leave, disconnect transfer, room removal |
| Ready, chat, and public state are bounded | validated ready/chat commands and bounded per-connection event queues | actor ready/chat test; TCP state/chat delivery, chat-rate, command-rate, and outbound-queue tests |
| Unknown capture retains no raw body | bounded oldest-evicted metadata/digest ring | game unit capture/policy tests; TCP disconnect/ignore/capture continuity, digest metadata, and metric redaction |
| Observability is low-cardinality and redacted | fixed room/queue/chat/unknown labels; registry lifecycle messages carry exact active counts | TCP metric presence, exact active-room gauge cleanup, lifecycle-order metrics, and room/password/chat/bearer absence checks |
| Shutdown cannot wait without bound | command/control timeouts, connection drain, lobby/room shutdown | actor/registry shutdown tests and GameService TCP shutdown-grace coverage |

## Test inventory at checkpoint

- `cargo test -p pangya-game --lib --locked -- --list` reports **21** game
  actor/runtime unit and property tests.
- `cargo test -p pangya-protocol --test m4_room --locked -- --list` reports
  **11** generated M4 protocol fixture/boundary/property tests.
- `cargo test -p pangya-server --test game_e2e --locked -- --list` reports **10**
  real-PostgreSQL GameService E2E tests, including **3** named M4 tests:
  - room lifecycle, authority, password, capacity race, transfer, and cleanup;
  - unknown policies versus known wrong-state closure;
  - command/chat/outbound queue bounds.

After blocker closure, the complete local matrix passed: formatting, strict Clippy
for every workspace target/feature, workspace and PostgreSQL-backed E2E tests,
doc tests, SQLx online metadata check and locked offline compilation, root and fuzz
`cargo deny`, all four fuzz targets for 10,000 runs each, proprietary-asset guard,
`git diff --check`, and no staged files. Dependency-version duplication remains
warning-only under the accepted deny policy.

## Generated fixture evidence

All four fixture binaries and YAML provenance records say
`generated-local-no-source`, `local-synthetic-profile`, and
`MIT OR Apache-2.0`. They contain no captured client bytes or personal data.
The binary SHA-256 values are:

- create request: `9f4d10286837e128cf0b886ebd38d7af1cf71670dea6dbee431d4bfb2d654bc3`;
- room list: `fb8eaa590f7335449871fdc2a24970ad766e47395d522dff6430096eb57b9aab`;
- room state: `4dc155a159a8862cdef6611568be259eae6071bcb1c42b8770047c3990329997`;
- room chat: `8407de6143c2f0291d0408594629d899b5eba0a9cf8fa5b4afa59f82ec3a79e6`.

## Runtime and security validation claims

- Room list order is deterministic by ascending process-local nonzero `u32` ID.
- A connection belongs to at most one room; room actors serialize concurrent
  admission and mutation.
- State has unique nonzero connection IDs, occupancy no greater than capacity,
  and exactly one owner while nonempty. Owner transfer selects longest-present,
  then lowest connection ID for a join-order tie.
- Password material is not stored in summaries, snapshots, fixtures, metrics, or
  debug output. Only salted digest state lives in the actor.
- Full outbound queues never grow: normal actor and lobby submission reject;
  per-connection cancellation isolates event overflow to the affected member;
  disconnect and shutdown use a separately bounded, capacity-sized priority lobby
  control path and then the room priority path.
- A queued command may time out only if its atomic gate cancels before execution;
  once execution begins, the caller awaits the committed outcome instead of
  reporting a timeout that could desynchronize connection and actor state.
- Created and Closed lifecycle transitions are published in sole-registry order
  with the exact post-transition active count; Closed is emitted only after a room
  is removed and only once. The service observes only received event types and
  stores every received exact count, draining retained counts after broadcast lag.
  Affected member tokens cancel while unrelated rooms remain live.
- Unknown capture records only state, opcode, body length, and SHA-256 body digest
  in a fixed-capacity process-local ring. Known wrong-state packets always close.
- Rooms are not durable and are removed when empty or when their actor closes.

## External real-client gates

The real M4 exit remains open until all of the following are established with a
legally held PangYa U.S. 852 client and privacy-reviewed evidence:

1. Exact client/server lobby and room opcodes; the `0x7f00` family is provisional.
2. Exact field widths, encodings, limits, defaults, result codes, and unknown
   fields for room list, create, enter/join, leave, settings, ready, chat, kick,
   state, and membership notifications.
3. Exact channel-to-lobby, create, and enter packet ordering, including repeated
   or unsolicited state/list packets.
4. Client acceptance of an authoritative create-room flow and successful entry
   into that room; local synthetic TCP clients do not satisfy this gate.
5. Real password and capacity behavior, owner semantics, disconnect cleanup, and
   the client-visible failure mapping.

Only after those gates are reviewed may the progress ledger mark the real M4
exit complete or use real U.S. 852 room-compatibility language. M5 remains a
separate, unopened gameplay/start/loading/reward gate.
