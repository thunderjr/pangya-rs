# M5 synthetic solo-practice evidence — 2026-08-05

## Claim boundary

The opt-in **local synthetic** M5 one-owner, one-member, one-hole solo checkpoint
is implemented. This evidence does not claim that `0x7f20..=0x7f24`,
`0x7fa0..=0x7fa7`, their layouts/order, synthetic Course record, deterministic
conditions, or `solo-v1` rewards match PangYa U.S. 852 or any retail service.
No proprietary client, IFF/PAK content, credential, personal data, or captured
retail packet bytes is present.

Scope stops at one local solo hole. There is no multiplayer, turn arbitration,
standings, item use, special-shot interpretation, equipment consumption, server
trajectory/collision physics, or M6 behavior. The tracked
[`docs/pangya.wiki`](../pangya.wiki/) corpus is retained unchanged and treated as
secondary behavior-only research, not packet/data/formula provenance.

## Requirement-to-implementation/test evidence

| Requirement | Implementation boundary | Test evidence |
|---|---|---|
| Generated local-only opcodes/layouts are explicit and strict | `pangya-protocol/src/m5_solo.rs`; five generated binary/YAML fixture pairs | `pangya-protocol/tests/m5_solo.rs`: exact fixture re-encoding, every truncation/trailing byte, all discriminants, constructor/wire bounds, arbitrary bodies |
| Start is exactly one authenticated owner in a one-member room | room actor derives caller; start request is empty; `solo_owner` checks owner and member count | `room::tests::solo_requires_authenticated_owner_and_exactly_one_member`; Game encrypted TCP setup |
| One actor solely owns match mutation | `SoloMatchState` lives in the room actor; bounded normal/priority paths | 49 game tests include actor ownership, queue saturation, timeout races, shutdown, and mutation-blocking coverage |
| Deterministic conditions are version-pinned | exact `rand_chacha = 0.3.1`, `ChaCha12Rng`, three `next_u32` modulo reductions | fixed zero-seed vector is rain, speed `13.3`, angle `129`; drift and bound tests; E2E recomputes wire and DB values from the seed |
| Action/result floats and sequences are bounded | strict finite ranges; action then same-sequence result; bit-exact duplicate coalescing; server stroke counter | protocol range/nonfinite/property tests; pure state rejection preservation; TCP conflicting/duplicate/skip and independent shot-rate tests |
| Client cannot claim identity or rewards | C -> S layouts contain neither account/connection identity nor score/Pang/EXP/balances/result key | protocol layout tests; relays assert server connection ID; TCP result asserts server formula and balances |
| Start is durable before match-start exposure | `begin_solo` locks active account and atomically inserts match/player/started audit; actor confirmation emits start events | begin replay/drift/stage rollback tests; E2E checks persisted immutable begin before loading |
| Loading completion is durable | actor accepts only 100; repository validates authority and idempotently marks `loading -> in_game` | protocol exact-progress tests; storage checked/idempotent transition test; TCP wrong-state closure and in-game row check |
| Result settlement is atomic and exactly once | row-lock match/profile; validate identity/config/strokes; compute server reward; update profile, ledgers, result, audit, terminal status in one transaction | sequential/concurrent replay, same-account concurrent matches, every-stage rollback, balance overflow, authority/config, foreign-key tests; E2E one reward and restart projection |
| Pang/EXP history is immutable | one currency and one progression row share the server result key; uniqueness/FKs; update/delete rejection triggers | composite-FK and immutable-ledger/audit PostgreSQL tests |
| Noncommitted termination never rewards | idempotent abort for disconnect, loading timeout, shutdown, persistence failure; committed result wins races | game unit failure/race tests; encrypted TCP disconnect/timeout/malformed/shutdown test; storage abort/no-reward tests |
| Startup recovery is bounded before listener bind | ordered row locks, `limit + 1` overflow detection, atomic quit/abort/audit; bind only after successful recovery | all/rollback/cap PostgreSQL tests; unclean in-game restart E2E; server pre-bind recovery failure test |
| Observability is bounded and redacted | fixed labels; seed/result key/shot values/rewards/identity absent from traces and metrics | happy-path E2E canaries and exact metric counts; protocol seed debug redaction |
| M4 room mutation cannot race active M5 | active or retained aborted solo state blocks room mutations until commit/abort acknowledgement | room active-solo mutation and commit-clear tests |

## Exact deterministic and reward formulae

For a fresh server-generated 32-byte seed, initialize `rand_chacha` 0.3.1
`ChaCha12Rng::from_seed(seed)` and consume exactly three `next_u32` outputs:

```text
weather = [clear, cloudy, rain][r0 % 3]
wind_speed_tenths = r1 % 151        # 0..=150, wire value / 10.0
wind_angle_degrees = r2 % 360       # 0..=359
```

Do not replace these reductions with distribution helpers. For `[0; 32]`, the
locked vector is rain, `wind_speed_tenths=133`, `wind_angle_degrees=129`.

`solo-v1` uses checked integer arithmetic over server-owned par and strokes:

```text
score = strokes - par
Pang  = 10 + 2 * max(par - strokes, 0)
EXP   = 5
```

For the E2E par-3, two-stroke fixture this is score `-1`, Pang `12`, EXP `5`.
No client reward field enters the repository request.

## Generated fixture hashes

The five YAML records say `generated-local-no-source`,
`local-synthetic-profile`, `MIT OR Apache-2.0`, and no proprietary/credential/
personal/client data. Their recorded binary SHA-256 values are:

- start request: `cc5edb495bb3a6a1cf36104d1a304e185a4d0a143532da7818a12525ff5e29ef`;
- shot action: `7daae5963c3ca58658e30689a5aa2d9f51629c41f9351ea35f89f1f42ce83cee`;
- shot result: `16c081b64f9ec7c6b7c0a8d838195d0d353f935c62378993e35c46b23da10503`;
- match started: `8a551b404f18b8e28a567d3366cf2fe8b2cc2d926846b3325b18ea7734a17c03`;
- hole result: `278c1fb8c70403db6e0f94276d931af3752a4fdd586865d6ea98e0a415fa566a`.

The generated synthetic catalog manifest records Character
`8e634d84dbf7ba1d9c8b8515d6ca1a4e0e87e270df97e28427d58dd53fd5b5c4`,
ClubSet `2bc63711f5c8e4abbda812fe5a413b49250830c6b1861fc7c2be39ac2ffb574e`,
Ball `7f270c607407c9fecedefa12ae5c69408a41badfd82c989d4cbc67ab4765045e`,
and Course `be82c128ac79c6e84117f79369b902de61d433595ba5d1da9b3670dfd8c04911`.
`Course.bin` is a generated 13-byte file with the synthetic header, local type
ID, and one par byte; it is not a retail IFF record.

## Database schema and migrations

- `0003_m5_solo_matches.sql` creates `matches`, the one-row-per-match
  `match_players`, exactly-one-key Pang `currency_ledger`, exactly-one-key EXP
  `progression_ledger`, and `match_audit_events`. Checks constrain solo mode,
  one hole, par/course/seed/catalog/weather/status/rewards; composite foreign
  keys tie ledger account and result key to the authoritative match/player.
  Immutable-history triggers reject ledger/audit update or delete.
- `0004_m5_match_wind.sql` adds required bounded persisted wind speed tenths
  (`0..=150`) and angle degrees (`0..=359`), backfills safely, then drops
  defaults so every new match supplies both values.
- `0005_m5_persistence_failure_abort.sql` extends the checked durable abort
  vocabulary with `persistence_failure` for runtime ambiguity.

Reservation, loading-to-in-game, abort, settlement, and startup recovery each use
short explicit PostgreSQL transactions. Settlement locks and validates before
updating profile balances and appending both ledgers; any stage failure rolls the
whole settlement back.

## Test inventory at checkpoint

Current compiled test inventories are:

- **49** `pangya-game` library actor/runtime tests, including 12 directly named
  solo/match-state ownership, transition, catalog, and event-capacity tests plus
  lifecycle failure/race/shutdown coverage;
- **10** `pangya-protocol --test m5_solo` fixture/boundary/property tests;
- **14** real-PostgreSQL `pangya-server --test game_e2e` tests, including four
  M5 tests for happy-path/restart, unclean recovery, sequence/rate bounds, and
  disconnect/timeout/malformed/shutdown no-reward behavior;
- **31** real-PostgreSQL `pangya-storage --test postgres` tests, including M5
  schema, exact-once concurrency, authority/drift, abort/recovery, rollback,
  overflow, foreign-key, and immutable-history coverage.

The targeted non-database M5 protocol and game suites pass, fixture hashes match
their YAML/manifest records, and the compiled inventory counts above are current.
The complete local validation matrix passed after the final review fixes:

- `cargo fmt --all --check`;
- strict workspace Clippy for all targets and features;
- all workspace/all-target/all-feature tests with PostgreSQL 17, including 14
  encrypted Game E2E and 31 storage integration tests;
- all workspace doc tests;
- SQLx online all-target metadata verification and locked offline all-target test
  compilation;
- root and fuzz-manifest `cargo deny check` (accepted duplicate-version warnings
  only; advisories, bans, licenses, and sources all passed);
- proprietary-asset guard and `git diff --check` with no staged files;
- `client-codec`, `server-decompression`, `packet-reader`, and `iff-parser` fuzz
  targets for 10,000 deterministic runs each.

This validates the local synthetic checkpoint only; it does not satisfy the
external retail-client exit below.

## External 12-step client/IFF gate

Real M5 remains open. Complete these steps with legally held material outside the
repository, then retain only privacy-reviewed, nonproprietary evidence:

1. Privately identify the exact U.S. 852 executable/package hash and custody.
2. Record legal authority for the client and mounted IFF/PAK data; commit neither.
3. Validate real Course/IFF headers, record sizes, course ID, hole count, and par.
4. Capture the retail channel/room-to-match transition with secrets and personal
   data excluded or redacted before review.
5. Establish exact retail C -> S and S -> C opcodes; never reuse synthetic names
   as evidence.
6. Establish exact field widths, signedness, float/coordinate encodings, string
   rules, unknown bytes, and limits with provenance per packet.
7. Establish exact start response/event order and confirm the client enters the
   expected one-player practice/loading UI.
8. Establish loading progress/completion order, deadlines, retries, and failure
   behavior accepted by the client.
9. Establish action and result synchronization order, sequence semantics, lie/
   holed fields, relays, and malformed/duplicate behavior for one legal test hole.
10. Establish finish, hole-result, balance/projection, disconnect, timeout, and
    reconnect/restart order, including any unsolicited packets.
11. Run one full client-visible hole and prove server-derived Pang/EXP persist
    exactly once across replay/reconnect without accepting client reward claims.
12. Privacy/license review the evidence and independent review the mapping before
    changing the real M5 exit; keep raw captures/client/IFF data out of git.

Synthetic fixtures, local TCP clients, behavior-only wiki research, and successful
PostgreSQL tests cannot satisfy any retail-compatibility claim by themselves.
