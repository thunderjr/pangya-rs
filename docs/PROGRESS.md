# PangYa-RS progress

> Last updated: **2026-08-08**
>
> Current stage: **The real U.S. 852 client reaches the lobby.** It completes the whole LoginService state machine — login, nickname, character setup, server list, selection, handover — then authenticates to GameService, receives the retail bootstrap, enters the channel, and renders its avatar with the lobby menu bar.
>
> Next gate: **the lobby actions.** The client sits in the lobby indefinitely under the shipped `unknown_opcode_policy = "disconnect"`; what has not been driven yet is Start Game, room creation, and playing a hole from the real client.

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
| Gameplay | 🟡 | Local synthetic M5 solo and M6 exactly-two-ready-player stroke/turn/standings/settlement checkpoints pass the complete local matrix; real U.S. 852 one-/two-client gates remain open |
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

---

## Immediate next actions

1. **Drive a room and a hole from the real client.** Lobby entry is done; §19.6 steps 5-12 need Start Game, room creation, and a played hole against the retail room and match packets.
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
