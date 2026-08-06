# M6 synthetic exactly-two stroke evidence — 2026-08-05

## Claim boundary

The disabled-by-default **local synthetic** M6 exactly-two-ready-player,
one-hole stroke checkpoint is implemented. This evidence does not claim that
`0x7f30..=0x7f34`, `0x7fb0..=0x7fb8`, their layouts/order, the generated Course
record, conditions, turn policy, standings, `stroke-two-v1`, or record rules
match PangYa U.S. 852 or any retail service. No proprietary client, IFF/PAK
content, credential, personal data, or captured retail packet bytes is present.

M6 stops at one hole and exactly two players. It adds no inventory/shop, item
use, equipment durability, special-shot interpretation, trajectory/collision
physics, social/ranking, or M7 behavior. The tracked `docs/pangya.wiki` corpus
remains secondary behavior-only research and is not packet, layout, constant,
formula, or Course provenance.

## Requirement-to-implementation/test evidence

| Requirement | Implementation boundary | Test evidence |
|---|---|---|
| Exactly two ready members; owner starts | `room::prepare_stroke_start` checks owner, exactly two, both ready, distinct accounts, and captured join/connection roster | game actor tests; encrypted two-client setup rejects wrong cardinality/readiness/authority and starts exact roster |
| Sole actor and bounded deadlines | `StrokeMatchState` lives only in room actor; loading, generation-tagged turn and game deadlines; game wins exact tie | pure state alternation/stale timer/race tests; M6 deadline E2E covers load abort, turn forfeit, game tie |
| Either participant may give up | actor `give_up` derives caller from authenticated captured roster and is not active-turn restricted | pure non-active give-up test; encrypted non-active give-up E2E |
| Strict generated packet contract | `pangya-protocol/src/m6_stroke.rs`; 14 binary/YAML fixture pairs; exact registry states | 11 M6 protocol fixture/truncation/discriminator/bounds/property/forfeit-pair tests |
| Server-owned turns and strokes | independent per-player sequence; bit-exact duplicate coalescing; active-only action/result; accepted result increments once | pure state tests; encrypted out-of-turn, duplicate, relay, turn, and no-midgame-write E2E |
| Truthful forfeit | give-up/disconnect/turn-timeout loser has no score/reward; opponent is `WinnerByForfeit` with no score and fixed 10 Pang/5 EXP | protocol canonical pair tests; domain shape tests; storage deferred-pair and winner tests; deadline/disconnect/give-up E2E |
| Loading abort vs in-game forfeit | loading timeout/disconnect abort aggregate with no reward; in-game disconnect commits forfeit standings | actor lifecycle and encrypted deadline/disconnect E2E |
| Shutdown always aborts noncommitted work | priority actor control replaces pending disconnect/settlement; one retained cleanup claim | shutdown cancellation/close-order and blocked-settlement replacement E2E; no ledger/partial player projection assertions |
| Atomic exactly-once settlement | one transaction validates authority, locks aggregate/profiles, updates two players/profiles/records/audit and commits terminal state | sequential/concurrent replay, deadlock-free shared-account, every-stage rollback, drift/FK/immutability tests; two-client E2E |
| Four normal ledgers | each rewarded normal player gets one Pang and one EXP row keyed by its distinct player result key | happy-path E2E asserts exactly two currency plus two progression ledgers and exact balances |
| Course records are truthful | only `Holed`; lower score then strokes; count eligible rounds; immutable FK-backed best authority | storage deterministic best/count test; E2E checks two authoritative holed records and restart persistence |
| Startup recovery is pre-bind and bounded | generic ordered cap+1 transaction aborts incomplete M5/M6 once, marks all players quit, audits, fails closed | storage mixed/all-player/cap/rollback tests; server pre-bind recovery coverage |
| Retained authority closes races | hidden starting/load-persistence/results/abort states, one coordinator claim, generation checks, committed-wins-abort | queue saturation, persistence retention, shutdown replacement, close-order, and commit/abort race tests |
| Metrics/config/security are bounded | fixed `stroke_two` metrics; validated durations/caps/queue; no packet bodies; seed/debug redaction; digest-only unknown capture | observability fixed-label test, configuration relation/cap tests, protocol seed debug test, E2E forbidden-canary assertions |

## Exact deterministic, standing, reward, and record policy

The generated conditions are unchanged from M5. With a fresh 32-byte seed,
initialize `rand_chacha` 0.3.1 `ChaCha12Rng::from_seed(seed)` and consume exactly
three consecutive `next_u32` outputs:

```text
weather = [clear, cloudy, rain][r0 % 3]
wind_speed_tenths = r1 % 151
wind_angle_degrees = r2 % 360
```

The actor starts roster position zero at global turn one. Each player sequence
starts at one. Accepted action and result pairs are active-player-only and
bit-exact duplicate safe. Turns advance to the next unfinished player; the
configured turn deadline covers both action and result and resets after result.
The whole-game deadline starts only after the durable in-game transition and
wins an exact timer tie.

Normal `stroke-two-v1` reuses checked `solo-v1` per player:

```text
score = strokes - par
Pang  = 10 + 2 * max(par - strokes, 0)
EXP   = 5
```

A direct give-up, in-game disconnect, or turn timeout produces exactly one
first-place `WinnerByForfeit` with `score=None`, Pang 10, EXP 5 and exactly one
second-place loser with `score=None`, Pang 0, EXP 0. The loser receives no ledger
row. Game-timeout unfinished players receive no score/reward and do not create a
winner. Normal place comparison is completion class (holed, stroke-cap,
winner-by-forfeit, forfeit), then lower strokes, then captured roster order.

Only `Holed` updates `(account, course, stroke_two)`. Every eligible round
increments `rounds_completed`. Best replaces only for a lower score or fewer
strokes at equal score; replacement captures the authoritative match/player key
and new first-achieved time. A non-improvement preserves the prior best and
first-achieved time while advancing the record update timestamp. Stroke-cap,
give-up, disconnect, turn/game timeout, and winner-by-forfeit are ineligible.

## Generated fixture hashes

All 14 YAML records identify `pangya-rs local synthetic M6 model`,
`generated-local-no-source`, `local-synthetic-profile`, `MIT OR Apache-2.0`, and
no proprietary, credential, bearer, personal, or client data. Recorded binary
SHA-256 values are:

| Fixture | SHA-256 |
|---|---|
| C -> S start | `eaafcab540a2e6b02bab7574775692148c69c706eacaad32ddfec67178fbff29` |
| C -> S loading complete | `1a253cda01ec2c954196e713e7bab0a9dbd3ba2545cdd576dc81d3e04d115627` |
| C -> S action | `249ed0d3b76b8b3436d6c2707163f2109bf48887c9f5c4dab362841daf919c82` |
| C -> S result | `86c9b57decb7856f0561b36cff870879fd89b4f61f4531e81c639e3763607b88` |
| C -> S give-up | `44c4239a8716ea0e993d46bff69fef6b107c1ed9c70894ba29a4384b29136d52` |
| S -> C match started | `e47403f904da21cd6d7d698a3329d4e1137692610b2a64550d39bf04cedfd305` |
| S -> C phase | `a802300575ff75b26c6bf38f64c879b8085c554c459dae65df867777f4740cc5` |
| S -> C turn started | `97985d231d7b66ef68272519bb4b2c993f79f4f4ab26e72481c181375cc59cd7` |
| S -> C action relay | `8a5077ea313336c5c7d558af21015f929b08ae8cccfc7ecc56242ba9e828235d` |
| S -> C result relay | `ea1542d7bef6329f51451f170a5e5e802506c9dfe0173b3d76dfe747ba67ae9f` |
| S -> C standings | `77b698d85ca7362aab75d2c1bf41232ede16855b4e55bc69b1411f5974653359` |
| S -> C command result | `08b2d8aba99c22488e7fb61bae890ad07635bf3eefcd2174428db62f6c331984` |
| S -> C match aborted | `67a3dee93798a2d0914b59eaa28f16f0675f85869401200c3130a23c4d1314f3` |
| S -> C own balance | `51c38770429a984bbf6eeecc0f2eaff146f95ab8dd437f863bb5d4fa7ce513c1` |

M6 reuses the generated 13-byte Course file documented at M5: synthetic header,
local type ID, and one par byte, with no retail IFF source bytes.

## Database migrations and recovery

- `0006_m6_stroke_records.sql` admits only the paired `stroke_two` /
  `stroke-two-v1` authority alongside existing solo rows. It backfills M5 player
  order/key/place/completion, adds exactly-two order and distinct player-key
  constraints, conditional settlement/ledger triggers, FK-backed
  `course_records`, and terminal identity/history immutability.
- `0007_m6_winner_by_forfeit.sql` adds `winner_by_forfeit`, fixes its canonical
  no-score 10/5 shape, and installs a deferred aggregate constraint requiring it
  to pair exactly with one place-two give-up/disconnect/turn-timeout loser.

Reservation locks both active accounts in sorted order and atomically inserts
match, both players, and start audit. Loading confirmation atomically marks the
aggregate in-game. Settlement locks match and both profiles, validates immutable
configuration/roster/player keys, stages results-pending, computes rewards,
updates balances/player rows, appends nonzero ledgers and eligible records, adds
audit, and commits the terminal row in one transaction. Any injected stage
failure rolls everything back. Exact committed replay returns persisted results;
drift or a prior abort fails without another reward.

Generic startup recovery runs once before Game bind, ordered by creation time and
match ID. It locks at most configured cap + 1 incomplete rows to detect overflow,
marks every captured player quit, changes `loading`, `in_game`, or
`results_pending` to `aborted/startup_recovery`, and appends one audit per match.
Overflow or storage failure rolls back and prevents readiness. When M5 and M6 are
both enabled, the larger enabled recovery cap and timeout govern the single pass.

## Configuration, observability, and security

`[game.stroke_two]` is disabled by default and requires enabled GameService plus
a manifest Course matching `course_id`. Loading is nonzero and at most 300s;
turn/game are nonzero, wire-`u32`-millisecond representable, at most one hour,
and turn cannot exceed game. Commit is nonzero, at most 60s, and no greater than
shutdown grace. Strokes are `1..=30`, recovery `1..=10000`, shot packets
`1..=1000000`; the outbound room queue holds at least the three-event terminal
burst. Existing bounded actor/control/connection/rate/shutdown limits remain.

Metrics use fixed `mode="stroke_two"` buckets: exact active gauge, fixed
lifecycle events, fixed commit outcomes, fixed shot outcomes, and a fixed
stroke-packet rate class. Traces/metrics do not carry match/result keys, seeds,
account IDs, balances, rewards, or shot values/coordinates. Packet-body logging
remains rejected; unknown capture stores only bounded state/opcode/length and
SHA-256 digest. Seeds are debug-redacted. Public binds still require explicit
acknowledgement and durable audit; shutdown clears readiness before cancellation.
The client cannot submit identity, place, score, rewards, balances, record keys,
or settlement IDs.

## Test inventory and validation state

Current compiled inventories were re-listed at this checkpoint:

- **73** `pangya-game` library actor/runtime tests;
- **11** `pangya-protocol --test m6_stroke` tests;
- **19** real-PostgreSQL `pangya-server --test game_e2e` tests;
- **45** real-PostgreSQL `pangya-storage --test postgres` tests.

The complete local validation matrix passed after independent review closure:
formatting; strict workspace Clippy for all targets/features; all workspace tests
with PostgreSQL 17; doc tests; SQLx online all-target metadata verification and
locked offline all-target test compilation; root and fuzz deny graphs (accepted
duplicate-version warnings only); proprietary-asset and diff/link/staged-file
checks; and all four fuzz targets for 10,000 deterministic runs each. The 14
fixture hashes above match their YAML records. This validates only the local
synthetic checkpoint; no M6 retail claim depends on that matrix.

## External two-client/Course retail gate

Real M6 remains open. Complete these steps with legally held material outside
this repository and retain only privacy-reviewed nonproprietary evidence:

1. Privately identify and record custody/authority for the exact U.S. 852 client
   build on **two** test clients and for the mounted Course/IFF/PAK data.
2. Validate the real Course header, record size, course/hole identifiers, par,
   and any binding/version rules; commit none of the proprietary source data.
3. Establish exact retail ready/start opcodes, layouts, limits, failure behavior,
   and server packet ordering; never reuse synthetic names as evidence.
4. Prove both clients enter the same room-to-match/loading transition with the
   correct authoritative roster and no identity ambiguity.
5. Establish loading progress/barrier order, retries, deadlines, disconnects,
   and abort projections accepted by both clients.
6. Establish active-turn, turn-number/sequence, timeout, duplicate, and
   out-of-turn semantics, including any unsolicited retail packets.
7. Establish action/result field widths, signedness, floats/coordinates, lie and
   holed meanings, relays, malformed handling, and order on both clients.
8. Establish voluntary give-up, active/non-active eligibility, in-game
   disconnect, reconnect, turn timeout, and whole-game timeout behavior.
9. Establish standings/tie/forfeit completion representations, reward formulae,
   balance projections, and exact client-visible terminal order.
10. Establish course-record eligibility, tie-break/update rules, and persistent
    projection with legally supplied data rather than the generated Course byte.
11. Complete one visible two-client hole and prove server-derived settlement is
    atomic/exactly once across replay, disconnect, and restart without accepting
    client reward/result claims.
12. Privacy/license review and independently review the mapping before changing
    the real M6 gate; keep raw captures, clients, IFF/PAK data, credentials, and
    personal data out of git.

Synthetic fixtures, local encrypted TCP clients, community behavior research,
and PostgreSQL tests cannot satisfy this external compatibility gate.
