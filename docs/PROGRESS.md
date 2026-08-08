# PangYa-RS progress

> Last updated: **2026-08-08**
>
> Current stage: **The real U.S. 852 client reaches the lobby.** It completes the whole LoginService state machine — login, nickname, character setup, server list, selection, handover — then authenticates to GameService, receives the retail bootstrap, enters the channel, and renders its avatar with the lobby menu bar.
>
> Current stage: **a real client buys from the shop.** It logs in, reaches the lobby, opens the room directory, creates and enters a room, opens the shop and My Room, and completes a purchase: balance debited, item in inventory, clubs rendered on the character.
>
> Next gate: **a two-player retail match against the real client.** The server implements it now: a retail start in a room holding two players runs the stroke lifecycle, with turn arbitration, a relay of the client's own shot frames, and a settlement that pays both participants exactly once. It is proven over TCP against a real database by `game_retail_two_players_play_and_settle_one_versus_hole`. What remains for §19.6 steps 7-12 is the real client's acceptance of it. See blocker 23.

This is the project status ledger. Update it when a deliverable gains evidence or a new blocker appears; do not use estimated completion percentages.

## Status legend

| Mark | Meaning |
|---|---|
| ✅ | Exit criteria have evidence |
| 🟡 | In progress; exit evidence incomplete |
| ⬜ | Not started |
| ⛔ | Blocked by an explicit decision or missing artifact |
| 🔬 | Research/spike needed before commitment |

---

## Current snapshot

| Workstream | Status | Evidence / next proof |
|---|---:|---|
| Repository audit | ✅ | Initial repository contained only `docs/INITIAL_RESEARCH.md`; no Rust source or Cargo workspace existed |
| Upstream cloning | ✅ | 16 shallow clones under `opensource-references/`; manifest and ignore policy in [`../opensource-references/README.md`](../opensource-references/README.md) |
| Extended research | ✅ | [`INITIAL_RESEARCH.md`](INITIAL_RESEARCH.md) rewritten for Rust, U.S. 852, current revisions, protocol counts, licensing and proof sequence |
| Technical specification | ✅ | Normative scope, architecture, requirements and milestones in [`SPEC.md`](SPEC.md) |
| Visual implementation plan | ✅ | Standalone responsive plan in [`PLAN.html`](PLAN.html) |
| Cross-session memory | ✅ | Durable handoff in [`MEMORY.md`](MEMORY.md) |
| Final project license | ✅ | Dual MIT OR Apache-2.0; ADR-0001, root license files |
| Cargo workspace | ✅ | Ten required Rust 2024 crates, MSRV 1.93.0, Cargo.lock, lints and CI |
| Client startup contract | ✅ | Real U.S. 852 client answered 33/33 HTTP requests from `[client_web]` and mounted all 84 PAK archives; `updatelist` byte-identical to an independent encoder for the real directory. [`evidence/REAL_CLIENT_STARTUP_2026-08-07.md`](evidence/REAL_CLIENT_STARTUP_2026-08-07.md), ADR-0015 |
| Real client reaches LoginService | ✅ | Client authenticates over encrypted TCP: `connection accepted → account authenticated (account_id 1)`, opcode `0x0001` in and out. First real U.S. 852 protocol ever exchanged with this server |
| Real client first-character setup | ✅ | Character Creation displayed; the client's own pick (`0x0400000b`) is accepted and persisted; the documented `success` reply unblocks it |
| Real client server list and selection | ✅ | List renders the configured `PangYa-RS Local`; selection sends `0x0003`, receives the session key, and LoginService closes with `reason: "complete"` — the whole state machine |
| Real client GameService hello | ✅ | Retail nine-byte hello accepted; the client now sends its retail `0x0002` auth instead of disconnecting |
| Real client GameService auth | ⛔ | Blocker 15: the auth decodes but its `login_key` is empty, so the handover bearer never parses |
| Protocol/crypto | 🟡 | All local M1 vectors, fixtures, transport boundaries, audits, and bounded fuzz checks pass; real-client acceptance remains |
| LoginService | 🟡 | Local synthetic M2 runtime/config/CLI/health/TCP/PostgreSQL exit passes; real U.S. 852 order, token field/length, name limits, and server-list acceptance remain |
| GameService/bootstrap | 🟡 | Local synthetic Login-to-Game snapshot/catalog/channel flow passes; real U.S. 852 layouts and acceptance remain external |
| Lobby/rooms | 🟡 | Local synthetic M4 actor/registry/TCP exit is complete; real U.S. 852 opcodes, layouts, order, and create/enter acceptance remain external |
| Gameplay | 🟡 | Local synthetic M5 solo and M6 exactly-two-ready-player stroke/turn/standings/settlement checkpoints pass the complete local matrix, and the retail wire now drives the same two-player lifecycle end to end over TCP; real U.S. 852 one-/two-client gates remain open |
| Economy | 🟡 | Local synthetic M7 checkpoint complete; not retail-validated |
| Social/parity | ⬜ | M8+; no M8 implementation or checkpoint claim |

---

## M0 — planning and provenance

**Status: ✅ complete**

- [x] Audit existing project state and git status.
- [x] Read and replace the TypeScript-oriented initial research.
- [x] Clone the six originally discussed server implementations.
- [x] Add current server, protocol, file, packet-analysis, and client references.
- [x] Record exact clone revisions and observed licenses.
- [x] Ignore nested clone contents from the parent repository.
- [x] Count and inspect PacketDoc service/direction coverage.
- [x] Inspect PangCrypt algorithm and golden vectors.
- [x] Verify client-computed shot/result relay in modern source references.
- [x] Research permissive Rust LZO candidate (`lzokay`).
- [x] Verify current Tokio codec and SQLx patterns from documentation.
- [x] Write the full Rust product/technical specification.
- [x] Create a visual standalone HTML plan.
- [x] Create project progress and memory documents.

### M0 evidence

| Artifact | Purpose |
|---|---|
| [`INITIAL_RESEARCH.md`](INITIAL_RESEARCH.md) | Evidence base and recommendation |
| [`SPEC.md`](SPEC.md) | Normative product and technical plan |
| [`PLAN.html`](PLAN.html) | Visual milestone/architecture/risk plan |
| [`PROGRESS.md`](PROGRESS.md) | Ongoing status ledger |
| [`MEMORY.md`](MEMORY.md) | Next-session handoff |
| [`../opensource-references/README.md`](../opensource-references/README.md) | Reproducible upstream snapshot and license boundary |
| Validation pass | 88 local links resolved; `html-validate` passed; desktop and 500 px responsive renders inspected; 16 clone/ignore assertions passed; text hygiene passed |

---

## M1 — Cargo and protocol foundation

**Status: 🟡 all local M1 checks pass; real-client acceptance and login-order gates open**

### Decision prerequisites

- [x] ADR-0001: dual MIT OR Apache-2.0 project license.
- [x] ADR-0002: U.S. 852 first compatibility profile.
- [x] ADR-0003: modular-monolith deployment baseline.
- [x] ADR-0004: Tokio codec and bounded room-actor approach.
- [x] ADR-0005: accept `lzokay` for the bounded M1 foundation; keep real-client acceptance open.

### Implementation checklist

- [x] Initialize Rust 2024 Cargo workspace with MSRV 1.93.0 and all ten required crates.
- [x] Add workspace lint, format, test, deny, CI, asset-guard, and lockfile policies.
- [x] Implement `pangya-crypto` and `pangya-protocol`; other crates are compiling M1 skeletons.
- [x] Add `THIRD_PARTY_NOTICES.md` and `docs/PROVENANCE.md`.
- [x] Port PangCrypt oracle tables with ISC attribution and combined/table SHA-256 assertions.
- [x] Port all three unique PangCrypt client golden vectors with metadata.
- [x] Port/decode three representative server vectors, including repetitive data, with metadata.
- [x] Prove `lzokay` known-vector decompression and generated round trips; record conditional outcome.
- [x] Implement bounded client `Decoder` and append-only server `Encoder`.
- [x] Test cumulative bytewise input, every split/header boundary, coalescing/partial-next, all key/salt combinations, EOF truncation, exact transport edges, malformed LZO/opcodes, encoder append behavior, and strict caps.
- [x] Add property tests and three excluded cargo-fuzz targets; run every target with deterministic bounded time locally and in CI.
- [x] Add attributed U.S. 852 LoginService hello fixture and typed minimum login packet models.
- [x] Build a synthetic loopback Tokio TCP hello/login harness.
- [x] Prove generated LZO with an independent implementation; retain hashes and command evidence.
- [ ] Validate generated frames with a legally obtained real U.S. 852 client through login/channel entry.

### M1 exit evidence required

- Every adapted fixture has provenance and license metadata.
- Crypto vectors pass for all valid test keys/salts.
- Malformed input produces typed errors without panic.
- Frame and decompression allocation caps are demonstrated by tests.
- LZO decision is recorded with vector and real-client evidence plan.
- CI runs format, Clippy, tests, docs, and dependency/license audit.

---

## Future milestone board

| Milestone | Status | User-visible exit |
|---|---:|---|
| M2 — LoginService | 🟡 | Local synthetic exit reaches server selection and consumes one handover; external U.S. 852 validation remains |
| M3 — Game bootstrap | 🟡 | Local synthetic flow reaches one channel with catalog-validated snapshot; real U.S. 852 exit remains external |
| M4 — Lobby and rooms | 🟡 | Local synthetic create/enter and concurrent actor state pass; real-client create/enter exit remains open |
| M5 — Solo first playable | 🟡 | Local synthetic one-hole solo finishes and persists exactly one reward; real-client hole remains open |
| M6 — Multiplayer stroke | 🟡 | Local synthetic exactly-two one-hole flow and complete matrix pass; real two-client/Course acceptance remains open |
| M7 — Inventory/shop depth | 🟡 | Local synthetic catalog-priced, exactly-once purchases/equipment/consume/repair pass the full outcome matrix; real-client acceptance remains open |
| M8 — Social/ranking | ⬜ | Durable friends/messages/mail basics and rebuildable ranking projection |
| M9 — Broad parity | ⬜ | Each legacy feature group completes its own packet/state/persistence gate |

---

## M2 — LoginService domain and storage foundation

**Status: 🟡 local synthetic exit complete; external U.S. 852 gates remain open**

- [x] ADR-0006: PostgreSQL 17, SQLx 0.8.6, embedded forward-only migrations and offline metadata.
- [x] ADR-0007: strict canonical client MD5-hex secret, exact no-extension Argon2id-v19 PHC policy, digest-only handovers and privacy-minimized source prefixes.
- [x] Add checked private ID newtypes, normalized display/key value objects, M2 DTOs, repository contracts and typed errors to `pangya-domain`.
- [x] Add redacted credential/token services with OS randomness and deterministic unit-only RNG seams to `pangya-login`.
- [x] Add bounded PostgreSQL pool config, checked static SQL, migration, row translation and explicit aggregate/handover transactions to `pangya-storage`.
- [x] Prove migration-from-empty, trigger-injected rollback at every aggregate stage, normalized uniqueness races, write-free starter replay/drift rejection, handover consume and ban/consume races, persistent revocation, source-prefix minimization and DB checks on PostgreSQL.
- [x] Add CI PostgreSQL service, SQLx prepare check, and offline compilation/test mode.
- [x] Compose LoginService runtime state machine, configured listeners/endpoints, redacted tracing/metrics and bounded `spawn_blocking` credential work.
- [x] Add operator account creation command with stdin/named-env/file secret input and durable audit row.
- [x] Complete synthetic TCP login through nickname setup, configured server selection and single-use handover against isolated PostgreSQL.
- [x] Add layered validated/redacted config, exponential DB retry, read-only health/metrics, and bounded graceful shutdown.
- [ ] Validate exact field limits/order with a legally held U.S. 852 client.

Evidence: [`evidence/M2_STORAGE_FOUNDATION_2026-08-05.md`](evidence/M2_STORAGE_FOUNDATION_2026-08-05.md) and [`evidence/M2_LOGIN_VERTICAL_SLICE_2026-08-05.md`](evidence/M2_LOGIN_VERTICAL_SLICE_2026-08-05.md).

## M3 — local synthetic GameService handover/bootstrap

**Status: 🟡 local synthetic exit complete; legally supplied data/client gates remain open**

- [x] ADR-0010 cleanly separates generated synthetic layouts from U.S. 852 claims.
- [x] Immutable bounded manifest-driven Character/ClubSet/Ball catalog with opaque records, generated fixtures, property tests, and IFF fuzz target.
- [x] Coherent repeatable-read `PlayerSnapshot` repository projection with ownership/reference/range/status/setup invariants and PostgreSQL race/corruption tests.
- [x] Bounded GameService `AwaitHandover -> AwaitChannel -> InChannel`, authoritative single-use consume, identity-match check, catalog validation, duplicate-presence RAII, segmented inventory, deadlines/rates/drain, and redacted metrics/tracing.
- [x] Optional binary composition and readiness; `game.enabled=false` preserves M2.
- [x] Real-PostgreSQL local Login bearer -> Game consume -> snapshot/catalog -> three segments -> channel E2E.
- [ ] Validate real IFF header/record layouts and every provisional packet with legally held U.S. 852 evidence.

Evidence: [`evidence/M3_SYNTHETIC_GAME_BOOTSTRAP_2026-08-05.md`](evidence/M3_SYNTHETIC_GAME_BOOTSTRAP_2026-08-05.md).

## M4 — local synthetic lobby and rooms

**Status: 🟡 local synthetic exit complete; real U.S. 852 create/enter exit remains open**

- [x] ADR-0011 reserves the generated local-only `0x7f00`/`0x7f80` families and explicitly excludes U.S. 852 and M5 claims.
- [x] Add bounded room values/projections/errors and strict generated layouts for list, create, join, leave, settings, ready, chat, kick, state, command results, membership events, and chat events.
- [x] Add one bounded lobby registry plus one sole-owner actor per room, separate priority disconnect/shutdown control, deterministic room IDs/listing/owner transfer, and one-room-per-connection authority.
- [x] Prove capacity serialization, password-gated admission, owner-only mutation/kick, ready/chat broadcast, disconnect cleanup, actor failure isolation, queue/rate limits, and bounded shutdown.
- [x] Keep password input redacted/zeroized; retain only random-salted SHA-256 with constant-time verification and expose only a protected boolean.
- [x] Add state-aware GameService dispatch, fixed low-cardinality observations, and bounded unknown-opcode disconnect/ignore/metadata-digest capture. Known wrong-state opcodes always close.
- [x] Add four generated fixture/provenance pairs, 11 protocol M4 tests, 21 game actor/runtime tests, and 10 real-PostgreSQL Game E2E tests including 3 M4 tests.
- [x] Complete the local synthetic TCP room lifecycle through create, list, join, settings, ready, chat, kick, leave, owner transfer, capacity race, and disconnect removal.
- [x] Close independent-review findings with atomic command gates, a capacity-sized priority cleanup queue, per-member overflow cancellation, isolated actor-failure cleanup, and centralized one-shot room closure observation.
- [x] Pass the complete final format/strict-Clippy/workspace/PostgreSQL/doc/SQLx-online/offline/deny/asset/four-target-fuzz validation matrix.
- [ ] Validate exact room opcodes, layouts, limits, ordering, and successful create/enter behavior with a legally held U.S. 852 client.

M4 itself contains no match behavior; the separate M5 checkpoint below layers
solo practice on the completed room boundary.

Evidence: [`evidence/M4_SYNTHETIC_LOBBY_ROOM_2026-08-05.md`](evidence/M4_SYNTHETIC_LOBBY_ROOM_2026-08-05.md) and [`protocol/M4_SYNTHETIC_LOBBY_ROOM_FLOW.md`](protocol/M4_SYNTHETIC_LOBBY_ROOM_FLOW.md).

## M5 — local synthetic one-hole solo practice

**Status: 🟡 local synthetic exit complete; real U.S. 852 first-playable exit remains open**

- [x] ADR-0012 fixes the one-owner, one-member, one-hole scope, sole room-actor ownership, generated local opcodes, pinned ChaCha conditions, two persistence boundaries, recovery, and explicit M6 exclusions.
- [x] Add strict generated `0x7f20..=0x7f24` / `0x7fa0..=0x7fa7` layouts and exact start/loading/action/result/finish packet ordering with finite float and sequence bounds.
- [x] Add a manifest-validated generated Course record, server-generated seed, pinned `rand_chacha` 0.3.1 `ChaCha12Rng` weather/wind, and redacted observability.
- [x] Enforce one authenticated room owner, sole actor mutation, action/result sequencing, bit-exact duplicate coalescing, server-owned strokes, holed/stroke-cap completion, and active-match room-mutation blocking.
- [x] Add PostgreSQL migrations 0003-0005 for immutable match identity/history, checked lifecycle/abort state, exactly one Pang and EXP ledger entry, atomic server-computed `solo-v1` settlement, and bounded pre-bind startup recovery.
- [x] Prove disconnect, loading timeout, malformed input, shutdown, persistence ambiguity, restart recovery, replay/concurrency, overflow, and every-stage rollback paths award nothing or exactly once as appropriate.
- [x] Record the current inventories: 49 game tests, 10 M5 protocol tests, 14 real-PostgreSQL Game E2E tests, and 31 storage tests.
- [x] Pass the complete format/strict-Clippy/workspace/PostgreSQL/doc/SQLx-online/offline/deny/asset/four-target-fuzz local validation matrix.
- [ ] Validate exact real room-to-match and solo opcodes, layouts, order, limits, Course/IFF interpretation, one-hole client acceptance, and exactly-once visible result with a legally held U.S. 852 client and legally supplied data.

The M5 boundary itself contains no multiplayer, turn arbitration, standings,
items, special-shot interpretation, equipment consumption, or server physics;
those claims are not retroactively widened by the separate M6 checkpoint.

Evidence: [`adr/0012-synthetic-m5-solo-practice.md`](adr/0012-synthetic-m5-solo-practice.md), [`protocol/M5_SYNTHETIC_SOLO_FLOW.md`](protocol/M5_SYNTHETIC_SOLO_FLOW.md), and [`evidence/M5_SYNTHETIC_SOLO_2026-08-05.md`](evidence/M5_SYNTHETIC_SOLO_2026-08-05.md).

## M6 — local synthetic exactly-two one-hole stroke

**Status: 🟡 local synthetic checkpoint and full local matrix pass; real two-client/Course gate remains open**

- [x] ADR-0013 fixes exactly two distinct authenticated room members, both ready, owner-only start, stable captured roster, one hole, and sole room-actor authority.
- [x] Add strict generated `0x7f30..=0x7f34` / `0x7fb0..=0x7fb8` layouts, exact per-stream packet order, 14 generated fixture/provenance pairs, and closed state/discriminator validation.
- [x] Enforce independent per-player sequences, active action/result turns, bit-exact duplicate coalescing, any-participant give-up, load/turn/game deadlines, and game-deadline tie priority.
- [x] Distinguish loading timeout/disconnect no-reward abort from in-game disconnect/turn-timeout forfeit settlement; shutdown priority aborts every noncommitted aggregate.
- [x] Add truthful `WinnerByForfeit` with no score and fixed Pang 10/EXP 5 versus zero-reward loser; whole-game timeout fabricates no winner.
- [x] Add migrations 0006-0007 for ordered player authority, per-player result keys, places/completions, deferred forfeit pairing, atomic two-player settlement, normal four-ledger history, and holed-only Course records.
- [x] Retain hidden starting/loading-persistence/results/abort authority, one persistence coordinator, exact acknowledgement, stale-generation rejection, and committed-wins-abort race closure.
- [x] Complete bounded generic M5/M6 startup recovery once before Game bind and retain fixed-label/redacted M6 metrics, validated config caps, no packet-body logging, and digest-only unknown capture.
- [x] Verify current compiled inventories: 73 game tests, 11 M6 protocol tests, 19 real-PostgreSQL Game E2E tests, and 45 storage tests; targeted M6 protocol and game suites pass.
- [x] Pass the complete release matrix: format, strict Clippy, workspace/all-target/all-feature PostgreSQL tests, docs, SQLx online/offline, deny, asset guard, links/diff, and four bounded fuzz targets at 10,000 runs each.
- [ ] Validate exact retail ready/start/loading/turn/action/result/give-up/disconnect/standings/reward/record behavior with two legally held U.S. 852 clients and legally supplied Course/IFF data.

This checkpoint is generated synthetic/non-retail and contains no social/ranking or
parity implementation.

Evidence: [`adr/0013-synthetic-m6-two-player-stroke.md`](adr/0013-synthetic-m6-two-player-stroke.md), [`protocol/M6_SYNTHETIC_STROKE_FLOW.md`](protocol/M6_SYNTHETIC_STROKE_FLOW.md), and [`evidence/M6_SYNTHETIC_STROKE_2026-08-05.md`](evidence/M6_SYNTHETIC_STROKE_2026-08-05.md).

## M7 — synthetic inventory, shop, and equipment

**Status: 🟡 local synthetic exit complete; external retail gates remain open**
- [x] Add generated `0x7f40..=0x7f44` / `0x7fc0..=0x7fc5` layouts, 11 generated fixture/provenance pairs, and closed command/outcome discriminator validation.
- [x] Make the immutable catalog the sole price, stack, durability, and repair-rate authority; the wire never carries a price, and catalog items that are not shop offers are refused.
- [x] Add migration 0008 for the operation, currency, item, and equipment ledgers with exactly-once commits keyed by a client-chosen operation id that survives process restart.
- [x] Treat a replay with different parameters as `IdempotencyDrift` rather than a replay, and gate equipment changes on optimistic `expected_version`.
- [x] Keep storage/overflow/corrupt-data repository errors off the wire entirely; they terminate the connection so no client is ever told a failed write succeeded.
- [x] Bound the slice twice — in configuration and again at composition — and require `game.enabled` plus a consumable-bearing catalog before composing.
- [x] Prove all ten wire outcomes, per-connection rate limiting, and pre-auth/pre-channel rejection over encrypted TCP against real PostgreSQL.
- [x] Add fixed-label economy metrics bounded to 15 series carrying no account, item, inventory, or operation identifier.
- [x] Verify current compiled inventories: 74 game tests, 5 M7 protocol tests, 26 real-PostgreSQL Game E2E tests, 53 storage tests, and 27 server tests; workspace total 330 passed.
- [x] Pass the local release matrix: format, strict Clippy, SQLx migrate/prepare with no drift, workspace/all-target/all-feature PostgreSQL tests, and the asset guard.
- [ ] Validate catalog-priced purchase, equip, consume, and repair behavior against a legally held U.S. client with legally supplied data, after the synthetic family is replaced by retail layouts.

Evidence: [`adr/0014-synthetic-m7-economy.md`](adr/0014-synthetic-m7-economy.md), [`protocol/M7_SYNTHETIC_ECONOMY_FLOW.md`](protocol/M7_SYNTHETIC_ECONOMY_FLOW.md), and [`evidence/M7_SYNTHETIC_ECONOMY_2026-08-07.md`](evidence/M7_SYNTHETIC_ECONOMY_2026-08-07.md).

## Research proof ledger

| Claim | Evidence |
|---|---|
| U.S. 852 is documented as a preserved packet version | `pangbox--packetdoc/src/packets/common/version.ksy` |
| PangCrypt tables came from U.S. 852 | `pangbox--pangcrypt/common.go` |
| Client frame is 5-byte header and uncompressed | `pangbox--pangcrypt/client.go`, `README.md` |
| Server frame is 8-byte header with LZO | `pangbox--pangcrypt/server.go`, `README.md` |
| Transport key range is `0x00..=0x0f` | PangCrypt code and invalid-key tests |
| PacketDoc indexed definition counts are 7/6 login, 98/141 game, 8/2 message | Local count of `.ksy` files excluding each `index.ksy` |
| Shot trajectory/result is client-computed and relayed | alter-pangya shot handlers/`PracticeMatchDirector`; Pangbox `game/room/room.go` |
| Server still owns turn/score/reward state | Pangbox room actor and current C# game mode source |
| `lzokay` is a pure-Rust MIT LZO implementation with independent decode evidence | automated vectors plus `docs/evidence/LZO_INDEPENDENT_2026-08-05.md`; real-client adoption remains unproven |
| Tokio Util supports custom bounded incomplete-frame decoding | current `tokio-util::codec` documentation |
| SQLx supports checked queries, migrations, offline metadata, transactions and test macro | current SQLx documentation |

---

## Open blockers and decisions

1. **Real-client acceptance** — identify a legally held U.S. 852 build/hash privately and prove generated traffic through login/channel entry; do not commit the client.
2. **Exact LoginService order** — capture and record that client's login/channel packet order without committing proprietary captures or secrets.
3. **Legally supplied IFF layouts** — validate real header/count/binding/version/record sizes outside the repository; committed fixtures remain generated only.
4. **Exact Game bootstrap** — validate hello, auth, bootstrap segmentation, channel packet layouts/order, and acceptance with the selected client.
5. **Exact lobby/room flow** — validate real U.S. 852 lobby/room opcodes, fields, limits, ordering, password/failure behavior, and client create/enter acceptance; never identify the provisional `0x7f00` family as retail protocol.
6. **Real M5 solo exit** — complete the evidence file's external 12-step gate for real Course/IFF interpretation and start/loading/action/result/finish/reward acceptance; never identify `0x7f20`/`0x7fa0` or `solo-v1` as retail behavior.
7. **M6 local validation** — the complete format/Clippy/workspace/PostgreSQL/doc/SQLx/deny/asset/link/diff/fuzz matrix passed; preserve this evidence.
8. **Real M6 two-client/Course exit** — complete the M6 evidence file's external gate with two legally held clients and legally supplied data; never identify `0x7f30`/`0x7fb0`, `stroke-two-v1`, generated standings, or record rules as retail behavior.
9. **Real M7 economy exit** — validate catalog-priced purchase/equip/consume/repair against a legally held client; never identify `0x7f40`/`0x7fc0`, generated prices, durability rules, or ledger shapes as retail behavior.
10. **Synthetic-to-retail protocol pivot** — the `0x7f**` families are placeholders no real client will ever send. Every M3–M7 real-client gate is blocked behind replacing them with layouts derived from the vendored PacketDoc definitions, and behind correcting the three M3 bootstrap opcodes whose current meanings disagree with PacketDoc (`0x0070`, `0x0072`, `0x004d`).
11. **Client runtime host** — **resolved 2026-08-07.** The client runs on Windows 11 under QEMU/KVM with Rugburn, which identifies it as US 852 and patches GameGuard out. Two host prerequisites were found: the host must expose at least one audio device, and `IntegratedPak` must exist in the registry. Both are in [`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md).
12. **Undiagnosed storage flake** — `concurrent_stroke_matches_with_shared_accounts_are_deadlock_free` has failed once in CI with `MatchRepositoryError::Storage`, and passes on re-run. It did not reproduce in 67 isolated runs or in repeated full-suite runs under heavy contention, and a lock cycle is not reachable: both commits sort their account ids and take `FOR UPDATE` on the same shared profile row, so one simply waits. **Now instrumented rather than open**: `Storage` carries a classified `StorageFault`, so the next occurrence names its own cause in the panic message — `deadlock` and `serialization` would disprove the analysis above, `unexpected_row_count` or `write_verification` would move the fault to the repository's own invariants, and `insufficient_resources` or `pool_timed_out` would make it contention rather than a defect. No guess was committed; the recurrence now identifies itself.

13. **Client-side startup crash before any socket** — **resolved 2026-08-07.** Three separate requirements, each found by satisfying the previous one: the client is WinLicense-protected so it must be diagnosed from inside the process (a first-chance vectored exception handler inside Rugburn, since a debugger is terminated on attach and the packed image's symbols are meaningless); every resource it asks for is a **bare file name**, so the extracted `data/` tree must be flattened into the client directory (41,192 files, zero base-name collisions); and the advertised `patch_number` must not exceed the client's own patch level, or it withholds the login dialog entirely. With all three the client renders its login scene and logs in. Full narrative in [`evidence/REAL_CLIENT_STARTUP_2026-08-07.md`](evidence/REAL_CLIENT_STARTUP_2026-08-07.md).

14. **Setup state does not advance after character selection** — **resolved 2026-08-07.** Three separate causes, each found from the wire. The configured character allowlist was one entry while the client offers a wider roster and picked `0x0400000b`, so the selection was refused; the refusal is now logged with the identifier instead of closing silently. Auto-create already grants a starter, so replaying the grant with the player's own character is a drift error by design; a new `select_starter_character` repoints the provisional character while setup is incomplete, and the grant is then replayed so a success proves the aggregate agrees. And nothing was sent in reply at all: upstream documents the login packet being resent with `success` once the character is selected, and the client blocks on "Waiting for server's response." until it arrives. With all three the client proceeds to the server list.

15. **GameService auth carries an empty login key** — **resolved 2026-08-08.** Four separate causes, each uncovered by fixing the previous one.

    *The hello.* GameService sent its four-byte **synthetic** hello regardless of `game.retail_bootstrap`, while a real client expects the nine-byte retail hello `00 06 00 00 3f 00 01 01 <key>`. The length difference made the client read the next frame at the wrong offset and drop the connection.

    *The missing `0x0010`.* The empty login key had nothing to do with the bearer's value. Upstream sends the session key **twice** — `0x0010` right after authentication, and `0x0003` again after server selection — and it is the `0x0010` copy the client stores and later echoes. This server only ever sent `0x0003`, so the client had nothing to echo. The login E2E asserted only opcodes, never that the two packets carry the same non-empty bearer; it does now.

    *The missing channel-count byte.* Every game-server entry in `0x0002` ends with a channel-count byte and channel list. The vendored PacketDoc example is a **TH** capture whose entries stop at `char_icon`, so the encoder was built to 92 bytes per entry; U.S. 852 is 93. Without that byte the client read its channel count from whatever followed and reported the server as full. Advertising an actual channel here is wrong — upstream sends the count byte with **zero** channels and lets GameService supply the channel list afterwards, and a speculative channel entry reproduced the same "Server is full".

    *The setup status codes were off by one.* PacketDoc's TH captures use `0xd9` for "set nickname" and `0xda` for "select character"; U.S. 852 uses `0xd8` and `0xd9` (`pangbox/server` `login/msgserver.go`, and the client itself — sent `0xd9` it opens character creation, not the nickname dialog). Sending the TH codes meant the client was never asked for a nickname, so the account never got one, and GameService then refused player bootstrap because it requires one. A related gap in the same path: a successful `0x0006` was answered with another `0x000e` check response instead of the `0x0001` success, which left the client on an inert server list until the login deadline closed the connection.

    Two smaller things fell out of this. The retail channel-select `0x0004` is a **one-byte** sub-server ID, not the synthetic `u32`, and its `0x004e` reply is a single `0x01`. And the catch-all arm that mapped every unclassified `GameRuntimeError` to `Error` discarded the error entirely, which is why the bootstrap failure was indistinguishable from any other; it now logs the variant.

16. **Lobby opcodes are unanswered** — **resolved 2026-08-08.** Standing in the lobby a real client sends `0x016E` (login bonus status) and `0x009C` (recent-player history), and channel entry itself expects more than the connect response. All three are answered now: `0x004E` is followed by upstream's unclassified four-byte `0x01F6` notice, `0x016E` gets a `0x0248` reporting nothing claimable, and `0x009C` gets a `0x010E` of five zeroed recent-player slots — an empty history rather than invented opponents, which is what upstream sends too.

    Rather than keep finding these one client restart at a time, the remaining session-level chatter was enumerated from upstream's client opcode table and its handler bodies. Ten opcodes that upstream accepts and answers with nothing (online status, typing indicator, idle status, client exception reports, macro set, messenger list, and four unclassified ones) are now an explicit allowlist, `RETAIL_ACCEPTED_SESSION_OPCODES`. Room and match opcodes are deliberately excluded: those have real state handlers, and silently accepting one would hide a gap in them. The lobby is now stable under the shipped `unknown_opcode_policy = "disconnect"` rather than depending on a permissive policy.

17. **Equipment update `0x0020` is unanswered** — **resolved 2026-08-08.** Opening My Room, the client sends this repeatedly, and three unanswered ones exhausted the unknown-opcode strike budget and dropped the connection, which the client reported as `Network error occured. (Error 10054)`.

    It is a tagged union over eight equipment kinds. What this server sends back is the equipment it *actually holds*, not an acknowledgement of the requested change: character parts, caddies, consumables and decoration have no durable representation here, so echoing the request would report a change that was never stored and contradict itself on the next login. Reporting stored state is accurate, and a client that asks for something this server cannot keep simply sees it revert. Character parts and the two unclassified kinds are accepted without a reply, because none could be formed honestly.

    My Room now opens and stays open under the shipped `unknown_opcode_policy = "disconnect"`.

18. **The catalog yields no item definitions for real client data** — **resolved 2026-08-08.** The client-schema path parsed identity only and always set `definition: None`, so `shop_offers()` was empty and every purchase was refused for want of a price. Real client records do carry the item header `pangbox/server` (`pangya/iff/item.go`) documents — a four-byte active flag and id, a 40-byte name, a rank byte, a 40-byte icon, then price — and reading it back against the acquired client reproduces the client's own shop exactly: "Air Knight Utility Set" at 10000 and "Candy Club Set" at 7500. Loading now yields **2,664** priced offers across club sets, balls, consumables and character parts.

    Two things are stated rather than read, because they have not been located in these records: nothing is durable, and every part is compatible with any character. Characters and courses yield no definition at all, so the shop's character tab could not be served even so.

    An operator override, `data.price_override_pang`, reprices every item the client sells. It exists so the whole shop is reachable for local testing without grinding a balance, it warns loudly at startup, and it deliberately cannot make an unavailable item purchasable — it rewrites the amount on rows the client already sells and leaves the rest alone.

19. **The catalog was built from a superseded copy of the client's tables** — **resolved 2026-08-08.** With pricing working, a real client's purchase was still refused: it asked for `0x10000061`, which sits inside the club-set id range but existed in **no** table in the archive the catalog was built from.

    The client does not read its tables from a loose file. It resolves `pangya_gb.iff` through its **PAK chain**, where a later PAK supersedes a same-named entry in an earlier one, and this install carries `projectg700gb+.pak` through `projectg851gb.pak`. The copy in use was an early revision. `scripts/extract-client-iff.py` walks that chain, applies the overlay and writes the winning archive; the difference is not subtle:

    | Table | Superseded copy | Winning copy |
    |---|---:|---:|
    | `ClubSet.iff` | 60 | **83** |
    | `Ball.iff` | 85 | **87** |
    | `Item.iff` | 316 | **388** |
    | `Part.iff` | 6087 | **7325** |
    | `Character.iff` | 10 | **14** |

    Offers went from 2,664 to **3,109**, and `0x10000061` resolves to `ClubSet, Pang(20000)` — the name and price the client's own Buy dialog shows, "Papel Training Club Set". One parser change was needed: the current `Item.iff` spans family tag `0x1b` as well as `0x18` and `0x1a`, on seven rows that did not exist in the older revision.

    A catalog built from a superseded copy loads and validates cleanly and is still wrong, which is what made this expensive to find. The tell is a purchase refused with `stage: "not_in_catalog"` for an id inside a family's range.

20. **A real client completes a purchase** — verified 2026-08-08. `0x001D` in, then `0x00C8`, `0x0096` and `0x0068` out; the balance moved 5,000,000 to 4,999,999 under `price_override_pang = 1`, `0x10000061` landed in the inventory, and the client rendered the bought clubs on the character.

21. **Room ready and the roster's character** — verified 2026-08-08. Two gaps found by driving a real client into a room it created.

    The client sends `0x000D` before it will offer Start, and nothing answered it, so the room sat inert. It carries a single byte, zero meaning ready; the reply is the room census, because the client waits to see the roster change rather than for an acknowledgement.

    The census also reported every player's character as zero, which renders the roster as empty slots. The selected character now travels on the room member itself, taken from the snapshot at authentication, rather than being looked up per frame.

    Worth recording as a process note: the edit adding `0x000D` to the retail room opcode set silently did not apply, because the surrounding text had been reformatted since it was written. The opcode arrived, matched no branch, and was answered with nothing — indistinguishable from a handler bug until the set was read back. Verify a set membership change by reading the function, not by assuming the edit landed.

22. **Playing a hole needs a second client** — **the client-side half is resolved 2026-08-08.** A real client will not start a versus room holding fewer players than its capacity (the header reads `3 hole (1/2)`) and the smallest capacity its Make Room dialog offers is two, so a played hole needs two clients. It also refuses to run twice: the check is a named mutex, and `ProjectG.exe` is WinLicense-packed so it cannot be patched statically.

    That is solved at runtime from Rugburn, which already hooks `kernel32`. `patches/rugburn-allow-multiple-instances.patch` adds an `AllowMultipleInstances` option that appends a per-process suffix to every named mutex. Two instances were confirmed running, the second signed in as its own account. The harness gained per-instance window targeting for it (`Set-PangyaTarget`), because clicks are relative deltas from a corner pin and only one window can occupy the screen origin at a time.

    What remains on this blocker is harness reliability, not capability — see the closing note below.

23. **A two-player retail match is not implemented** — **resolved 2026-08-08.** This superseded the earlier claim in this file that the match was "implemented and covered end to end"; that was true only of the single-player case.

    `RETAIL_C2S_START_MATCH` (`0x000E`) builds a `BeginSoloMatch` for `identity.account_id` and routes it through the solo lifecycle. A two-player match lifecycle does exist — `StrokeStartPlan` takes `participants` and both members' connection ids, with turn and game timeouts — but it is reachable only from the synthetic `0x7f4x` opcodes. Nothing connects the retail opcode to it.

    So a real client, which can only ever start a two-player room, causes a solo match to begin for whoever pressed Start. `game_retail_start_with_two_players_begins_only_a_solo_match` pins this: with both players in the room and ready, the host gets a `match_players` row and **the guest gets none**, so nothing the second player does can be scored.

    The retail start now reads the room before it starts anything: two members run the stroke aggregate, one member still runs the solo one. Three things the versus flow needed did not exist. `StrokeMatchState::hole_out` is a completion rather than a shot, because the client plays the holing stroke through the ordinary action/result pair and only *then* announces the hole is over — counting the announcement as a stroke scored every hole one over. A bounded relay carries the client's own in-match frames (`0x0055` shot announce, `0x0064` ball sync) to the other participant unchanged, since the client owns trajectory and this server's authority is the stroke count and whose turn it is. And `0x0066` carries the final standings, so the results screen shows the durable settlement.

    `game_retail_two_players_play_and_settle_one_versus_hole` replaces the pin and drives two authenticated retail clients through a whole hole over TCP: both receive the hole intro, the turn alternates with `0x00cc`/`0x0063`, each shot reaches the other client as `0x0055`, and the second hole-out settles one durable match with **both** accounts as participants and one Pang and one EXP ledger row each.

    A process note that matters more than the fix. The pinning test proved less than it claimed: it read the room number from the wrong offset of the join reply, so its guest was refused the room with `RoomNotFound` and could not have been scored whatever the server did. `0x0049` is both the acceptance and the rejection, so asserting the opcode alone asserted nothing. The replacement asserts the status word, and the guest's census frame, before it believes anyone is in a room.

    The existing `game_retail_match_plays_and_settles_one_hole` remains valid for what it covers — one client, one hole — and is not coverage of a versus match.

24. **A room went stale while you sat in it, and a hole could not survive the client's own chatter** — **resolved 2026-08-08.** Two gaps found by reading what a real client sends around a versus hole rather than by driving one into it.

    A client in a room learns that anyone else arrived, left, or readied only from a census, and the census was sent on create and join alone. A host watching `3 hole (1/2)` therefore never saw it become 2/2, and the client will not offer Start until it does — so the two-player match was unreachable from the real client whatever the server did with it. Room snapshots now translate into a census, but only while the connection is in the room: one mid-hole would contradict the match. The join handler stopped sending its own, because joining mutates the room and the actor already broadcasts to everyone in it, the joiner included.

    Separately, thirteen in-match opcodes — aim rotation, the power meter, club changes, item use, relief drops, the client's own hole info, the active-player acknowledgement, pause, the aiming arrow, load progress, game end, last-player-leave — arrive during a hole and none of them changes a stroke, a turn, or a score. Under the shipped `unknown_opcode_policy = "disconnect"` each would have ended the session, so a hole could not have survived the first real shot. They are an explicit allowlist now, `RETAIL_ACCEPTED_MATCH_OPCODES`, listed with what upstream does with each: it relays them to the other participants and this server does not yet, so an opponent's aim does not animate. That is a stated gap rather than a silent one. `0x0034` is the exception that needs an answer — the client waits to be told it may take the first shot, and `0x0090` carries nothing but its arrival.

25. **A real client could never have started the match it is the only one able to start** — **resolved 2026-08-08.** The client's own button reads Start for the room master and Ready for everyone else, so a master never sends `0x000d`. The stroke aggregate requires every participant ready, so it refused every real start with `NotReady`: the one player who cannot say it is the only one who can begin the match. Pressing Start is the master saying it, and the retail start records that before it reserves the plan, which keeps the aggregate's invariant rather than weakening it. The versus test now has only the guest send a ready packet, because that is what two real clients do.

    Found the same way as blocker 24 — by reading what the client sends rather than by driving one — and it would have looked exactly like a dead Start button.

    A related mismatch went with it. The room record echoed the creator's course and hole count while the match plan was built from the configuration, so a room could promise three holes on one course and the match contradict it a moment later; and the course the client is told to load is a one-byte ordinal the configured id may not fit into at all. The record now reports what will actually run. The timers and the room type stay the creator's, because those this server does honour.

26. **Two real clients cannot be driven on one desktop, so the second seat is headless now** — **resolved 2026-08-08.** The game reads its mouse through DirectInput as accumulated relative deltas clamped to its own window, so parking the other instance elsewhere does not stop it seeing the cursor: both instances believe the pointer is over the same client-area pixel, and the one holding input focus is the one that acts. Focus could not be moved back to a client that had lost it. `SetForegroundWindow`, the foreground-lock-timeout trick, a synthetic title-bar click, the same click from the automation host, minimising the other window, and clicking the taskbar button all failed silently; `AttachThreadInput` hangs the automation host outright and `GetForegroundWindow` read from it names windows that are not foreground. All of it is recorded in the harness header so it is not rediscovered.

    `pangya-test-client` removes the requirement instead of solving it: a person drives one real client and a headless one takes the other seat over the same retail wire, so what the real client experiences is a genuine two-player match. It can host or join, because the seat the real client is not in is the one that has to be filled. `pangya-server account handover` mints its bearer.

    Two of them against the release binary played and settled a versus hole end to end — one durable match, both accounts scored, one Pang and one EXP ledger row each — which is the first time the retail versus path has run outside the test suite, against the shipped binary and the shipped config. It also immediately found two client-side ordering bugs of its own: a host that starts the moment the room fills races the other seat's ready packet, and a client that drains after readying swallows the match plan.

27. **A real client's room census was unparseable, and it could not recognise its own row** — **resolved 2026-08-08.** Two defects that a one-member room hid completely. The census player record is 341 bytes of identity followed by the 513-byte equipped-character block (`pangbox/server` `game/model/room.go` `RoomPlayerEntry`, whose last field is `CharacterData`); we wrote only the identity half, so the client found the second record 513 bytes early and drew a roster of garbage names next to a bogus occupancy count. PacketDoc's `0048.ksy` cannot settle this — its examples hold a single user, so "513 per record" and "513 once at the end" are indistinguishable in them, and it documents the block as "either 1 byte (0x00) or 513 bytes long".

    Separately the bootstrap reply carried a hardcoded zero where the client's own connection id belongs. The client matches that value against every census record to find itself; told zero it never did, so it drew the Master badge on the correct slot from the flags while believing it was not the master. Its button read Ready rather than Start, and pressing it sent `0x000d` — the room could not be started by the only player allowed to start it. Both are pinned by `game_retail_rooms_create_join_and_leave_over_tcp`.

    Also settled here: the census character field is the **catalog** id from the client's own `Character.iff` (PacketDoc `0048.ksy` `item_id_character`), not the inventory id. A room member now carries both, because the client resolves models by catalog id and has no inventory but its own.

28. **A real client crashes while loading a versus hole** — **open, and now localized to a
    specific instruction in the client.** Seven causes found and fixed so far, from the
    references and from the client's own memory rather than by guessing at the wire:

    - `0x0052` carries a fixed `[18]HoleInfo` array the client reads by width, not by the `hole_count` beside it (`pangbox/server` `game/packet/server.go` `ServerRoomGameData`, filled in `game/room/room.go`). Entries past `hole_count` are zeroed, which is what upstream sends.
    - `0x0076` is `ServerGameInit`, and its subtype picks between two bodies that share no shape: `0x00` is a player count followed by every player whole, `0x04` is a bare timestamp (`pangbox/packetdoc` `gameservice/server/0076.ksy`). We stamped the full subtype onto the timestamp body, so the client read a four-byte field's low byte as "one player" and parsed a multi-kilobyte record out of the nineteen bytes after it. The roster is now 24 KB of real players, and the record is the same `RetailPlayerData` the handover reply carries — upstream reuses one structure for both.
    - Client `0x0048` is hole-loading progress and is broadcast back as `0x00a3` to the whole match (`handleRoomLoadingProgress`). The client draws a bar per player and waits on all of them.

    What remains: the client now answers `0x000c`, `0x0048` and `0x001a`, receives its `0x00a3`
    echoes, runs its whole progress ramp — and dies at the end of it, before sending `0x0011`,
    with an access violation reading `[null + 0x150]` at `0x00883983`. The crash site moved once,
    when the roster was fixed, and has not moved since — through four further fixes, each of
    which was a real defect and none of which was this one:

    - the pre-match framing (`0x0230`, `0x0231`, `0x0077`, `0x016a`) and holding weather and wind
      until every player has loaded, both as all three reference servers do them;
    - a real `SYSTEMTIME` on the roster and the handover, where both had sent sixteen zero bytes;
    - the club set's catalog id in the player record, where a zero had been sent on the belief
      that the client ignored the block;
    - equipping a ball, which no account had: account creation grants a character and a club set
      and no comet, so every player entered the hole with none. The client's gear row now shows
      one. **This is a real gap in account creation and is not yet fixed in the server.**

    The crash is now localized rather than guessed at, by reading the running client's own memory
    (`OpenProcess`/`ReadProcessMemory` from the VM; the on-disk image is packed, so only a live
    process disassembles). The faulting function is `0x00883950`, a `__thiscall` whose first stack
    argument is null:

    ```
    00747a63  mov  eax, [0xe10fac]                  ; a global manager
    00747a68  mov  ecx, [eax + 0x103f4]
    00747a6e  movzx ecx, byte [ecx + 0x185]         ; an index, 0 in the lobby
    00747a75  imul ecx, ecx, 0x2f84                 ; stride of a per-player record
    00747a7b  mov  eax, [ecx + eax + 0x656e]        ; the record's first dword: a lookup key
    00747a83  call 0x6aeda0                         ; singleton getter -> 0xe40300
    00747a8a  call 0x6ad790                         ; list lookup by that key; 0 when not found
    00747a8f  mov  edi, eax
    00747a93  cmp  edi, ebx                         ; ebx = 0
    00747a95  je   0x747aa7                         ; not found -> the argument stays 0
    ...
    00747aae  call 0x883950                         ; faults on [arg + 0x150]
    ```

    Sampling that array every 100 ms through a real match start showed the key **zero**, which is
    exactly the not-found path. So the client is looking up a per-player record it never
    registered. Numbering the roster seats from zero instead of one was tried on that evidence —
    upstream numbers from one — and the crash did not move, so it was reverted rather than left
    in as a guess.

    A wider sample, two full records taken at the moment of the crash, pins the layout:

    ```
    0050:  14 00 20 00 07 00 00 00 ... "RsPlayerFive"    <- record 0 holds a player
    01a0 .. 2cf0: single small values every 0x2b0, reading 10 01 0a 05 10 0b 06 01 11 0c 07 02
                  12 0d 08 03 13                          <- a shuffled table of eighteen
    2fc0:  08 00 16 00 14 00 20 00 07 00 00 32 74 55 33 ...  <- record 1, same fields at +0x3c
    ```

    The 0x2f84 stride is confirmed by the two records, a nickname from the roster lands at
    record + 0x50, and the record's **first dword — the very field the lookup keys on — is zero**
    while the rest of the record parsed fine. So the client fills the record from our roster and
    leaves that one field unset, which means it is a field we send as zero or do not send at all.

    A second sample, with the roster carrying a real caddie catalog id instead of a zeroed
    caddie block, left the key **still zero** and the crash unchanged: the caddie is not the
    source, and is eliminated. That sample also placed the rest of the record:

    | record offset | content |
    |---|---|
    | `+0x00` | the lookup key — **zero**, and the crash |
    | `+0x1c` | `0x30` |
    | `+0x38` | the sixteen-byte `SYSTEMTIME` this server stamps on the roster |
    | `+0x48` | the player's nickname |

    So the client is not copying our wire order — the start time precedes the name in its own
    struct — it is filling named fields one by one, and every one we can see lands correctly.
    The key is a field the client has nothing to fill from. The **mascot** was eliminated the
    same way — a real mascot catalog id in the block left the key zero and the crash unchanged.
    Two of the four candidates are now ruled out by experiment rather than by argument, leaving
    the equipped-item slots and the title and guild fields in the trailing equipment slots.

    Three further things were checked and are **not** the cause:

    - Every catalog id this server sends is a real entry in the client's own tables — character
      `0x04000000` in `Character.iff`, club set `0x10000000` in `ClubSet.iff`, ball `0x14000000`
      in `Ball.iff`. Checked offline against the extracted tables, no client run needed.
    - The season-history block was widened from 21 courses per season to 22, which is what
      `Acrisio-Filho/SuperSS-Dev` writes (`MS_NUM_MAPS`, `Server Lib/Game Server/TYPE/pangya_game_st.h`)
      across its twelve blocks. The key stayed zero and the crash was unchanged, so the width was
      reverted rather than left changed on a shared record — it alters the handover for every
      client, and nothing observed distinguishes 21 from 22.
    - Both the nickname and the match start time land at their right offsets in the client's
      record, so the record is not shifted wholesale; only the key field is unset.

    The method is settled and cheap: sample the array, change one field, sample again. Each
    cycle needs the room number, which the server now logs.

    A debugger would settle it in one run instead of one field per run, and WinDbg turns out to
    be **already installed** on the VM — as an MSIX package, which is why an earlier filesystem
    search missed it. `cdb.exe` is at
    `C:\Program Files\WindowsApps\Microsoft.WinDbg_*\x86\cdb.exe`. An invasive attach to the
    running client does not work as-is: the break-in times out with the loader lock held and
    every `GetContextState` fails with `0x8007001F`, so no breakpoint can be set. The next thing
    to try is launching the client *under* the debugger rather than attaching to it
    (`cdb -g -G -o ProjectG.exe`), which avoids the break-in entirely.

    Note also that the anchor for "the key is at record offset 0" is not sound: the nickname and
    start time were measured from the array base `g + 0x656e`, not from a record base, so the
    key may sit mid-record. The candidate list built on that assumption should be re-derived
    once a breakpoint can read the addresses directly.

29. **A room's own settings are not remembered, so `0x004a` describes every room as Versus** —
    **fixed.** `RoomProfile` now travels with the room's settings and its summary, so the room
    record and the room status report the mode, course, hole count and timers the creator asked
    for. The mode is carried as the client's own byte rather than mapped onto the four modelled
    types: the client's single-player practice room is a mode this server does not model and
    still has to render. Pinned by `game_retail_two_players_play_and_settle_one_versus_hole`.
    Was, before the fix: The room actor stores a name, a password and
    a capacity; the mode, course, hole count and timers a client asked for are echoed once from
    the create request and then lost. `retail_room_from_snapshot` therefore rebuilds the record as
    a Versus room on course zero with default timers, and the new `0x004a` reply repeats that. A
    real client sitting in its own Single Player Practice Mode room is told it is in a versus room
    of one and leaves Start Game disabled. The settings belong on the room, beside its name.

30. **The room directory never lists rooms** — **open.** A client that opens Multiplay sees an empty list even when rooms exist, so a real client can only ever be the host: it has no way to join one. The room list is served on request but never pushed, and nothing was found that the client sends to refresh it.

31. **Account creation grants no ball** — **open.** `account create` grants a starter character
    and a club set; the equipped comet is left null, so every player enters a hole with no ball.
    Both test accounts were given one by hand to get past it. A starter comet belongs in the same
    grant as the club set.

32. **Nothing starts a single-player practice hole** — **open.** The client creates the room and
    then waits: it sends no `0x000e`, and its Start Game stays disabled in a room of one, both
    before and after the room learned its own settings. Two mode bytes in the room record were
    tried against its header — the first names the mode it renders and has no string for
    practice, the second changed nothing — and neither gates Start. Upstream references do not
    cover practice, so the next step is capture rather than inference.

33. **Quest status `0x0151` is unanswered and blocks the client modally** — **open.** The client's Quest button raises "Waiting for server's response" and stays there. Under `capture` it is recorded rather than fatal, but under the shipped `disconnect` it would end the session.

---

## Immediate next actions

1. **Find what starts a practice hole** (blocker 33), then use it to probe blocker 28. Step 7 is
   met — the client creates a room *and* starts practice, and its header reads
   `[Private] Single Player Practice Mode  1 hole (1/1)  No Time Limit`, with the private flag,
   hole count and "no time limit" all echoed from what it asked for. But it never sends `0x000e`
   in a room of one and its Start Game stays disabled, before and after the room learned its own
   settings. In the two-player room Start lit up only once the second seat readied, so the
   client appears not to offer Start for a room of one at all: something else starts practice,
   and it is not a client opcode we have seen. A practice hole is a one-player roster and would
   isolate the hole-load crash from everything the second seat contributes, which is why it is
   worth finding.
2. **Stop the real client crashing during hole load** (blocker 28). It is the whole of the §19.6 steps 8-12 gate now: the room, the roster, the master's Start and the match plan are all accepted, and the hole itself is not. Settle the remaining `0x0052`/`0x0076`/`0x005b` fields and the hole-load handshake against `pangbox/server` and PacketDoc **before** changing anything — iterating against the client costs a full sign-in per attempt and its crash dump names nothing.
3. **Old plan, now unnecessary: play a versus hole from two real clients.** The server side is implemented and covered end to end by `game_retail_two_players_play_and_settle_one_versus_hole` (blocker 23); §19.6 steps 7-12 now need the real client to accept it. Expect unanswered in-match opcodes — the productive loop is in [`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md).
2. **Implement equipment update `0x0020`** (blocker 17). It is the only thing left that drops a real client out of an otherwise working session.
2. Raise the shipped `security.login_timeout` guidance: 15 seconds closes the connection while the client's own first-time setup screens are open. Interactive setup needs a far larger allowance.
2. Re-enable live room broadcasts in retail mode by translating membership changes into census add/remove frames; the census is currently sent only on create and join, so a room does not update while you are sitting in it.
3. Extend the retail match beyond one player and one hole. The retail flow is wired onto the durable solo lifecycle, which is single-player and single-hole by construction; multi-hole plans, turn arbitration across a party, and the stroke/battle modes still need the generalized actor decided in ADR terms.
4. Measure the record field layouts inside the real client tables so real prices, stack limits, and durability can drive the economy. Course par is now settled as operator-declared: the client's `Course.iff` record carries none, and per-hole par lives in the course's own PAK data.
5. Port retail lobby/room layouts (`0x0008`/`0x0009`/`0x000a`/`0x000f`/`0x0081`/`0x0082`), then the match set.
6. Preserve the validated synthetic M2-M7 evidence and complete local matrix.

---

## Change log

### 2026-08-07 — the real client's startup contract, served and proven

- Executed the acquired U.S. 852 client against this server for the first time. It answered
  every one of its 33 startup HTTP requests and mounted its complete 84-archive PAK series,
  where previously it opened none.
- Proved `SPEC.md` §13.4's conditional: the client requires a string catalog, an
  XTEA-encrypted patch `updatelist`, and theme documents plus images **before it opens any
  socket**. Each failure mode was isolated by satisfying the previous prerequisite. §13.4 and
  the system-context diagram now record this; ADR-0015 records the design.
- Added `pangya-updater` with PangYa's `updatelist` XTEA variant, its nonstandard file CRC-32,
  and the document layout, all ISC-attributed and reproduced exactly rather than normalised.
  The generated list for the real client directory is byte-identical to an independent
  encoder's, pinned in CI by a golden fixture generated from that encoder.
- Added a `[client_web]` listener separate from `[http]`, so the client-reachable patch surface
  never carries health, readiness, or metrics — verified by 404s on `/metrics` and
  `/health/ready` from the client's own machine, and on a theme file the document does not name.
- Fixed two real defects the run exposed: a real-client catalog carries no course par, so solo
  practice could never resolve a course and startup died with a generic catalog error; and every
  listener or composition failure collapsed into "required runtime task exited" with no cause.
  Par is now operator-declared and cross-checked, with a typed error that names the problem.
- Recorded two host prerequisites no server can supply: an audio device must exist, and
  `IntegratedPak` must be present in the registry.
- Opened blocker 13 for the remaining client-side startup crash, with twelve hypotheses
  eliminated by direct observation rather than argument.
- Workspace total 402 tests passed, up from 330; fmt, Clippy with `-D warnings`, doc tests,
  SQLx offline check, `cargo deny`, and the asset guard all pass.

### 2026-08-07 — storage failures became self-describing

- Storage failures now carry a classified `StorageFault` instead of collapsing into an opaque `Storage` variant. The classification reads only the `SQLSTATE`, the driver's failure kind, or a server-side consistency check — never message text, statement text, bound parameters, or row values — so it is safe to return to a caller, log, and export.
- `pangya_storage_faults_total{fault="..."}` exports the closed fault set. Every series is present from process start, so the dimension's width is fixed at compile time rather than growing on first failure; a test pins uniqueness, density, and label shape against the enum itself.
- Every repository entry point reports faults: all twenty-two trait methods plus both inherent public methods. Observation is a pure side channel, proven by a test that runs the same operations with and without an observer and asserts identical outcomes.
- Six real `SQLSTATE`s are raised through PostgreSQL and the driver in an integration test, pinning the whole chain from server error to classified fault to observer. This is what makes blocker 12 self-identifying on its next occurrence rather than open.
- Separating `unexpected_row_count` and `write_verification` from database-reported faults means a repository-invariant violation can no longer hide inside the same counter as a genuine database error.

### 2026-08-07 — retail pivot begins: client acquired, catalog and bootstrap contract established

- Acquired and characterized the final U.S. client; settled the 851-vs-852 question from the wire, where the server must identify as `852.00` or the client raises a version mismatch.
- Measured the real catalog: `pangya_gb.iff` is a ZIP of 39 per-family tables, the documented `count`/`binding`/`version` header is confirmed exactly, and the documented record layout is not — `type_id` is at record offset four and a zero at offset zero marks an inactive row.
- Added `pangya-data` schema 3, which loads the real client catalog across all six declared families, covered in CI by a generated fixture containing no client bytes.
- Added reference-derived retail bootstrap packets: `0x0002` auth, `0x0044` in all four forms including the full handover reply, `0x0072` equipment, `0x004d` channel list, and the `0x0070`/`0x0071`/`0x0073` chunked containers.
- Recorded that three M3 bootstrap opcodes carry the wrong meaning today (`0x0070`, `0x0072`, `0x004d`), confirmed independently by PacketDoc and a working reference server.
- Wired the retail bootstrap into the runtime behind `game.retail_bootstrap`, proven over encrypted TCP: progress ticks, the full reply announcing `852.00`, roster, caddie container, equipment, inventory, and channel list, in that order.
- Accepted the retail `0x0002` auth packet inbound, completing the login-to-lobby path: a real client's own auth now drives the retail bootstrap end to end.
- Routed the retail match onto the durable solo lifecycle: start, hole load, shot commit, and hole finish, with server-authoritative stroke counting and exactly-once Pang/EXP settlement, proven over encrypted TCP. A complete hole is playable through the retail protocol.
- Added and routed the retail room packets: create, join, leave, list, and the 341-byte member census, proven over encrypted TCP including capacity accounting, the room-master flag, and a refused join. Room broadcasts are dropped in retail mode rather than emitted as synthetic packets, so the census does not yet update live.
- Added an operator guide for pointing a real client at a local instance. Everything past the lobby still speaks the synthetic protocol, and no layout is client-verified.

### 2026-08-07 — local synthetic M7 inventory, shop, and equipment checkpoint

- Added generated `0x7f40`/`0x7fc0` catalog-priced purchase, equip, consume, and repair with the catalog as sole price/stack/durability authority and no price on the wire.
- Added migration 0008 with operation/currency/item/equipment ledgers, exactly-once commits keyed by client-chosen operation id that survive restart, drift detection on mismatched replays, and optimistic equipment versioning.
- Kept storage/overflow/corrupt-data failures off the wire so no client is told a failed write succeeded; proved all ten outcomes, rate limiting, and state gating over encrypted TCP.
- Recorded 11 generated fixture hashes and inventories of 74 game, 5 M7 protocol, 26 Game E2E, 53 storage, and 27 server tests; workspace total 330 passed.
- Acquired and characterized the U.S. client for the retail pivot; added no retail claim and no M8 behavior.

### 2026-08-05 — local synthetic M6 exactly-two stroke checkpoint

- Added generated `0x7f30`/`0x7fb0` exactly-two-ready-player one-hole flow, owner start, sole actor turns, load/turn/game deadlines, and any-participant give-up.
- Added truthful winner-by-forfeit 10/5 versus zero loser, load abort versus in-game forfeit, shutdown priority abort, atomic normal four-ledger settlement, and holed-only Course records through migrations 0006-0007.
- Recorded 14 fixture hashes and current inventories of 73 game, 11 M6 protocol, 19 Game E2E, and 45 storage tests; the complete local validation matrix passed.
- Kept the real M6 exit open behind the external two-client/Course gate and added no M7 behavior.

### 2026-08-05 — local synthetic M5 solo-practice checkpoint

- Added generated `0x7f20`/`0x7fa0` one-hole flow, sole-owner match state, pinned deterministic ChaCha weather/wind, exact sequence/float bounds, and no M6 behavior.
- Added two-phase durable lifecycle/settlement boundaries, immutable exactly-once Pang/EXP ledgers, no-reward aborts, and bounded pre-bind startup recovery through migrations 0003-0005.
- Recorded five generated fixture hashes and current inventories of 49 game, 10 M5 protocol, 14 Game E2E, and 31 storage tests; the complete local validation matrix passed.
- Kept real U.S. 852 M5 open behind the external 12-step client/IFF gate; no synthetic opcode, formula, or Course record is a retail claim.

### 2026-08-05 — local synthetic M4 lobby and room checkpoint

- Added generated `0x7f00`/`0x7f80` room contracts, a bounded lobby registry and sole-owner room actors, authoritative membership/settings/ready/chat/kick/transfer/cleanup, and metadata-digest unknown capture.
- Recorded 21 game actor/runtime tests, 11 protocol M4 tests, and 10 real-PostgreSQL Game E2E tests including 3 M4 tests; the complete local validation matrix passed.
- Closed review findings for timeout/mutation races, saturated cleanup, cross-room overflow isolation, actor-failure cleanup, create ordering, and active-room accounting.
- Kept the real U.S. 852 M4 exit open for exact opcodes/layout/order and create/enter acceptance, and added no M5 start/loading/gameplay/reward behavior.

### 2026-08-05 — local synthetic M3 GameService bootstrap

- Added immutable manifest-driven synthetic catalog, PlayerSnapshot projection, bounded GameService, optional readiness composition, generated fixtures/fuzzing, and real-PostgreSQL Login-to-Game channel evidence.
- Kept all IFF record sizes and Game packet layouts explicitly synthetic; proprietary data and real-client acceptance remain external.

### 2026-08-05 — M2 terminal-outcome and boundary re-review closure

- Supervisor drain now inspects and propagates required-task cleanup/join/timeout failures without replacing an earlier primary error.
- Fixed connection termination outcomes distinguish completed, rejected, cancelled, peer EOF, timeout, limited, protocol, and error paths.
- The shared 256-item starter bound is enforced before collection allocation or SQL at all public PostgreSQL create/grant paths.

### 2026-08-05 — M2 final local re-review closure

- Zeroized every handover bearer generation/parse intermediate and CLI stdin/file byte buffer on all exits.
- Capped starter collections before collection work, added fixed protocol I/O metrics, and made nickname friendly-failure retries cumulative.
- Coupled credential timeout to shutdown grace, added supervisor cleanup allowance plus explicit runtime teardown timeout, and proved noncancellable over-grace worker behavior is bounded.

### 2026-08-05 — M2 second independent-review blocker closure

- Extended total login deadline/cancellation across DB and sends; bounded nickname retry exhaustion and credential-worker shutdown tracking.
- Removed the unused outbound-queue setting; zeroized protocol/crypto plaintext buffers and preserved stored-PHC policy failures as operational outcomes.
- Replaced stringly protocol metrics with fixed typed classes and separated known invalid-state from bounded true-unknown ranges.
- Added hard configuration/runtime upper bounds, checked retry schedules, probe timeouts with pending recovery, 128-byte race-safe secret-file reads, and hermetic config tests.

### 2026-08-05 — M2 independent-review blocker closure

- Moved all accept/connection admission ahead of task spawn and added hard-bound stress evidence.
- Completed global/source/username/connection weighted rate layers, live typed metrics/DB latency paths, credential overload/timeout/cancellation, RAII and NeedsStarter TCP evidence.
- Serialized setup against bans, made operator success audit atomic, added audited public binds, continuous DB readiness, unified supervision cleanup, exact config validation, zeroization, bounded trace capture, and all three CLI secret-source audit checks.
- Kept malformed transport policy intentionally at one observed strike because encrypted frames cannot be safely resynchronized.

### 2026-08-05 — M2 local synthetic LoginService exit

- Added bounded generic LoginService runtime/state machine, local-only optional auto-create, duplicate presence, rate/connection/time limits, provisional setup and single-use handover flow.
- Added layered config, server/account CLI, durable operator audit, exponential DB bootstrap, redacted tracing/metrics, Axum health, and supervised graceful shutdown.
- Added provisional nickname response fixture/docs and real-PostgreSQL synthetic TCP E2E; retained all real U.S. 852 compatibility gates explicitly open.

### 2026-08-05 — M2 domain and PostgreSQL foundation

- Added technology-neutral account/starter/handover domain contracts and redacted security value types.
- Added strict MD5-hex canonicalization, exact Argon2id policy, digest-only 256-bit handovers with masked source prefixes, checked PostgreSQL repositories, and real concurrency/constraint tests.
- Closed foundation review blockers with write-free starter replay/drift rejection, every-stage trigger rollback tests, persistent ban/reactivation and ban/consume race evidence, and complete checked static SQL metadata.
- Recorded the provisional ASCII normalized-name policy and kept M2 runtime/E2E/operator exit work open.

### 2026-08-05 — M1 implementation

- Added the ten-crate Rust 2024 workspace, MSRV 1.93.0, dual licenses, CI, deny policy, asset guard, and Cargo.lock.
- Added attributed safe PangCrypt transforms/tables/vectors, bounded lzokay boundary, checked packet APIs, state-aware codec/registry, typed login models, fuzz targets, and synthetic TCP harness.
- Local fmt, Clippy, workspace tests, doc tests, both cargo-deny graphs, asset guard, fuzz compilation, and three bounded fuzz runs pass.
- Recorded independent LZO interoperability; kept only real-client acceptance and exact login ordering as M1 external gates.

### 2026-08-05 — M0 planning

- Replaced initial Node.js/TypeScript plan with a Rust-focused research baseline.
- Cloned and recorded 16 upstream references (about 851 MiB locally as shallow checkouts).
- Added current SuperSS/community findings and corrected the “only one playable implementation” claim.
- Refined “physics are client-side” into a client-computed trajectory/server-authoritative state boundary.
- Selected a proposed Tokio/SQLx/PostgreSQL/room-actor architecture.
- Identified `lzokay` as the permissive Rust LZO candidate pending proof.
- Added full spec, visual plan, progress ledger, memory handoff, and clone manifest.

---

## How to update this document

- Move a checkbox/status only when its stated evidence exists.
- Add the command/test/capture path that proves completion.
- If work stops with uncertainty, record a blocker; do not mark complete.
- Keep daily narrative out of this file; use the change log only for meaningful project-state changes.
- Keep durable next-session facts synchronized in [`MEMORY.md`](MEMORY.md).
