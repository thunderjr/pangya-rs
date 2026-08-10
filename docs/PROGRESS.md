# PangYa-RS progress

> Last updated: **2026-08-09**
>
> Current stage: **the first-playable U.S. 852 path is real-client proven.** The client completes LoginService and GameService bootstrap, enters a channel, lists/creates/joins rooms, starts Course Practice, and renders Blue Lagoon. A two-seat retail match includes a physically rendered stroke, durable forfeit settlement, exactly-once Pang/EXP ledgers, replay resistance, and visibly restart-retained balance.
>
> Current stage: **the mounted-PAK shop and minimum durable equipment are real-client proven.** One authoring run generates the mounted client PAK and the server IFF manifest; exact Pang prices render, ClubSet/Ball purchases commit exactly once, and owned/catalog-checked ClubSet/Ball equipment survives restart. Daily Quest reports an honest empty Tier-D state instead of invented progress.
>
> Remaining breadth is explicit: normal two-player holed-out standings UI, retail consumable/repair presentation, MessageService/social/ranking, and Tier-D modes are not claimed. See [`evidence/REAL_CLIENT_MATCH_2026-08-09.md`](evidence/REAL_CLIENT_MATCH_2026-08-09.md), [`evidence/REAL_CLIENT_SHOP_2026-08-09.md`](evidence/REAL_CLIENT_SHOP_2026-08-09.md), and [`evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md`](evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md).

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
| Real client GameService auth | ✅ | Blocker 15 resolved; the same non-empty single-use bearer is sent at both required LoginService stages and consumed by GameService |
| Protocol/crypto | ✅ | Local vectors/fixtures/bounds/fuzz checks pass and the real client accepts the encrypted/compressed service traffic |
| LoginService | ✅ | Real U.S. 852 login, setup, server list, selection, handover, and reconnect are accepted |
| GameService/bootstrap | ✅ | Real U.S. 852 hello/auth/bootstrap/channel entry are accepted with legally supplied catalog data |
| Lobby/rooms | ✅ | Real create/list/join/leave/ready/census plus a headless-hosted directory join are accepted under disconnect-on-unknown policy |
| Gameplay | ✅ | First-playable gate: rendered U.S. 852 Blue Lagoon stroke, durable two-seat settlement, restart-retained +10 Pang/+5 EXP, and immutable finish-frame replay coverage. Broader modes remain separate Tier-C/D breadth |
| Economy | 🟡 | Retail mounted-PAK purchase and owned/catalog-checked ClubSet/Ball equipment persist across restart; synthetic consume/repair pass, but no retail consume/repair claim is made |
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

**Status: ✅ complete, including real-client acceptance**

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
- [x] Validate generated frames with a legally obtained real U.S. 852 client through login/channel entry.

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
| M2 — LoginService | ✅ | Real U.S. 852 login/setup/server-selection/handover accepted |
| M3 — Game bootstrap | ✅ | Real U.S. 852 auth/bootstrap/channel entry accepted against the supplied catalog |
| M4 — Lobby and rooms | ✅ | Real create/list/join/leave/ready/census accepted |
| M5 — Solo first playable | ✅ | Real Course Practice plays eight physical strokes, settles one durable hole, emits authoritative results, and returns cleanly to the room |
| M6 — Multiplayer stroke | ✅ | Real rendered stroke plus two-seat durable settlement/restart/replay evidence pass |
| M7 — Inventory/shop depth | 🟡 | Real-client mounted-PAK purchase plus ClubSet/Ball equip and restart retention pass; consume/repair remain open |
| M8 — Social/ranking | ⬜ | Durable friends/messages/mail basics and rebuildable ranking projection |
| M9 — Broad parity | ⬜ | Each legacy feature group completes its own packet/state/persistence gate |

### Durable player state

Specified by [`SPEC_DURABLE_PLAYER_STATE.md`](SPEC_DURABLE_PLAYER_STATE.md), ordered to match
[`protocol/US852_SUBSYSTEM_GAPS.md`](protocol/US852_SUBSYSTEM_GAPS.md) §7. The 116-byte
equipment block and the 513-byte character block carry fourteen slot families; three are
durable today, so the rest are emitted as truthful zeros rather than invented values.

| Milestone | Status | User-visible exit |
|---|---:|---|
| E1 — Caddie and mascot | ⬜ | A real client selects a caddie and a mascot; both survive restart and appear in the room census |
| E2 — Character parts, aux parts, hair colour, mastery, stats | ⬜ | A real client renders operator-set hair colour, mastery and fitted parts after relog |
| E3 — Consumable and decoration slots | ⬜ | The ten consumable slots and six decoration slots persist, and a slot cannot outlive the stack it points at |
| E4 — Cards | ⬜ | Container `0x0138` and twelve per-character card slots render whole, not partially |

### Operator admin surface

Named in *Immediate next actions* item 2. `crates/pangya-admin` plus a separate SPA at
`../pangya-admin`; boundary recorded in ADR-0016.

| Phase | Status | Scope |
|---|---:|---|
| 0 — foundations | ✅ | Migration 0009, admin crate, sessions, `account role` CLI, [`INDEX.md`](INDEX.md), [`SPEC_DURABLE_PLAYER_STATE.md`](SPEC_DURABLE_PLAYER_STATE.md), ADR-0016. Evidence: [`evidence/ADMIN_PANEL_2026-08-09.md`](evidence/ADMIN_PANEL_2026-08-09.md) |
| 1–2 — accounts | ✅ | List, detail, ledger, and audited status/role/password/balance mutations |
| 3–4 — items | ✅ | Catalog item names parsed from the client's own tables; inventory and equipment control under the same optimistic version the in-game path uses |
| 5 — shop | ✅ | Migration 0010, DB-backed overlay resolved live at purchase time — no restart, and it can offer an item the client's tables mark unavailable |
| 6 — operations | ✅ | Status readouts, audit log, course-record leaderboard |

Server-side only: no real U.S. 852 client has been driven through any of it. That an
operator-granted item appears in My Room, and that an overlay price is what the client is
actually charged, are the open gate — the same checks
[`evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md`](evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md)
and [`evidence/REAL_CLIENT_SHOP_2026-08-09.md`](evidence/REAL_CLIENT_SHOP_2026-08-09.md)
already define.

---

## M2 — LoginService domain and storage foundation

**Status: ✅ complete, including U.S. 852 login/setup/server-selection acceptance**

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
- [x] Validate field order and the exercised limits with a legally held U.S. 852 client.

Evidence: [`evidence/M2_STORAGE_FOUNDATION_2026-08-05.md`](evidence/M2_STORAGE_FOUNDATION_2026-08-05.md) and [`evidence/M2_LOGIN_VERTICAL_SLICE_2026-08-05.md`](evidence/M2_LOGIN_VERTICAL_SLICE_2026-08-05.md).

## M3 — GameService handover/bootstrap

**Status: ✅ complete for the U.S. 852 first-playable bootstrap**

- [x] ADR-0010 cleanly separates generated synthetic layouts from U.S. 852 claims.
- [x] Immutable bounded manifest-driven Character/ClubSet/Ball catalog with opaque records, generated fixtures, property tests, and IFF fuzz target.
- [x] Coherent repeatable-read `PlayerSnapshot` repository projection with ownership/reference/range/status/setup invariants and PostgreSQL race/corruption tests.
- [x] Bounded GameService `AwaitHandover -> AwaitChannel -> InChannel`, authoritative single-use consume, identity-match check, catalog validation, duplicate-presence RAII, segmented inventory, deadlines/rates/drain, and redacted metrics/tracing.
- [x] Optional binary composition and readiness; `game.enabled=false` preserves M2.
- [x] Real-PostgreSQL local Login bearer -> Game consume -> snapshot/catalog -> three segments -> channel E2E.
- [x] Validate the required real IFF headers/record identities and retail bootstrap packets with legally held U.S. 852 evidence. Opaque family-specific metadata remains opaque rather than guessed.

Evidence: [`evidence/M3_SYNTHETIC_GAME_BOOTSTRAP_2026-08-05.md`](evidence/M3_SYNTHETIC_GAME_BOOTSTRAP_2026-08-05.md).

## M4 — lobby and rooms

**Status: ✅ complete for real U.S. 852 create/list/join/leave/ready behavior**

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
- [x] Validate the exercised room opcodes/layout/order and successful create/list/join/enter behavior with a legally held U.S. 852 client.

M4 itself contains no match behavior; the separate M5 checkpoint below layers
solo practice on the completed room boundary.

Evidence: [`evidence/M4_SYNTHETIC_LOBBY_ROOM_2026-08-05.md`](evidence/M4_SYNTHETIC_LOBBY_ROOM_2026-08-05.md) and [`protocol/M4_SYNTHETIC_LOBBY_ROOM_FLOW.md`](protocol/M4_SYNTHETIC_LOBBY_ROOM_FLOW.md).

## M5 — one-hole solo practice

**Status: ✅ complete, including the full real U.S. 852 physical-hole exit**

- [x] ADR-0012 fixes the one-owner, one-member, one-hole scope, sole room-actor ownership, generated local opcodes, pinned ChaCha conditions, two persistence boundaries, recovery, and explicit M6 exclusions.
- [x] Add strict generated `0x7f20..=0x7f24` / `0x7fa0..=0x7fa7` layouts and exact start/loading/action/result/finish packet ordering with finite float and sequence bounds.
- [x] Add a manifest-validated generated Course record, server-generated seed, pinned `rand_chacha` 0.3.1 `ChaCha12Rng` weather/wind, and redacted observability.
- [x] Enforce one authenticated room owner, sole actor mutation, action/result sequencing, bit-exact duplicate coalescing, server-owned strokes, holed/stroke-cap completion, and active-match room-mutation blocking.
- [x] Add PostgreSQL migrations 0003-0005 for immutable match identity/history, checked lifecycle/abort state, exactly one Pang and EXP ledger entry, atomic server-computed `solo-v1` settlement, and bounded pre-bind startup recovery.
- [x] Prove disconnect, loading timeout, malformed input, shutdown, persistence ambiguity, restart recovery, replay/concurrency, overflow, and every-stage rollback paths award nothing or exactly once as appropriate.
- [x] Record the current inventories: 49 game tests, 10 M5 protocol tests, 14 real-PostgreSQL Game E2E tests, and 31 storage tests.
- [x] Pass the complete format/strict-Clippy/workspace/PostgreSQL/doc/SQLx-online/offline/deny/asset/four-target-fuzz local validation matrix.
- [x] Validate the exercised real room-to-match and solo opcodes/layout/order, Course/IFF interpretation, a complete eight-stroke physical hole, exactly-once result, updated balance projection, and clean room return with a legally held U.S. 852 client and legally supplied data.

The M5 boundary itself contains no multiplayer, turn arbitration, standings,
items, special-shot interpretation, equipment consumption, or server physics;
those claims are not retroactively widened by the separate M6 checkpoint.

Evidence: [`adr/0012-synthetic-m5-solo-practice.md`](adr/0012-synthetic-m5-solo-practice.md), [`protocol/M5_SYNTHETIC_SOLO_FLOW.md`](protocol/M5_SYNTHETIC_SOLO_FLOW.md), and [`evidence/M5_SYNTHETIC_SOLO_2026-08-05.md`](evidence/M5_SYNTHETIC_SOLO_2026-08-05.md).

## M6 — exactly-two one-hole stroke

**Status: ✅ complete for the first-playable U.S. 852 versus path**

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
- [x] Validate retail ready/start/loading/first-turn/stroke/disconnect-forfeit/reward/record behavior with one visible U.S. 852 client plus a second retail-wire seat and legally supplied Course/IFF data. The normal holed-out standings overlay remains optional breadth; durable `0x0066` settlement is regression-tested.

This checkpoint is generated synthetic/non-retail and contains no social/ranking or
parity implementation.

Evidence: [`adr/0013-synthetic-m6-two-player-stroke.md`](adr/0013-synthetic-m6-two-player-stroke.md), [`protocol/M6_SYNTHETIC_STROKE_FLOW.md`](protocol/M6_SYNTHETIC_STROKE_FLOW.md), and [`evidence/M6_SYNTHETIC_STROKE_2026-08-05.md`](evidence/M6_SYNTHETIC_STROKE_2026-08-05.md).

## M7 — synthetic inventory, shop, and equipment

**Status: 🟡 local synthetic exit complete; retail purchase/equipment accepted, consume/repair gates open**
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
- [x] Validate a catalog-priced purchase against the legally held U.S. client with legally supplied data: the client and server are generated from one authored archive, the mounted PAK displays four exact Pang offers, a 77-Pang Ball purchase commits, and balance/inventory survive restart.
- [x] Validate owned/catalog-checked ClubSet and Ball equipment against the retail client: the exact 196-byte inventory row exposes purchased equipment, type `3` persists Ball type plus ClubSet row atomically, and My Room/room gear survive restart.
- [ ] Validate retail consume and repair behavior against that client; do not infer those gates from purchase/equipment acceptance.

Evidence: [`adr/0014-synthetic-m7-economy.md`](adr/0014-synthetic-m7-economy.md), [`protocol/M7_SYNTHETIC_ECONOMY_FLOW.md`](protocol/M7_SYNTHETIC_ECONOMY_FLOW.md), [`evidence/M7_SYNTHETIC_ECONOMY_2026-08-07.md`](evidence/M7_SYNTHETIC_ECONOMY_2026-08-07.md), [`evidence/REAL_CLIENT_SHOP_2026-08-09.md`](evidence/REAL_CLIENT_SHOP_2026-08-09.md), and [`evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md`](evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md).

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
9. **Real M7 economy exit** — mounted-PAK purchase and owned/catalog-checked ClubSet/Ball equipment with restart retention are verified; consume/repair remain. Never identify `0x7f40`/`0x7fc0`, generated prices, durability rules, or ledger shapes as retail behavior.
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

17. **Equipment update `0x0020` is unanswered** — **resolved 2026-08-08 and completed for minimum durable equipment 2026-08-09.** Opening My Room, the client sends this repeatedly, and three unanswered ones exhausted the unknown-opcode strike budget and dropped the connection, which the client reported as `Network error occured. (Error 10054)`.

    It is a tagged union over eight equipment kinds. Character parts, caddies, consumables and decoration still have no durable representation here, so they report stored zero/current state rather than a change that was never committed. Character and type `3` now use the same optimistic owned/catalog-validated equipment transaction as M7. SuperSS-Dev revealed that type `3` is not only a Comet word as the older Pangbox model says: it carries Ball catalog id **and ClubSet inventory id together** (`GAME/channel.cpp:5233-5299`), and `0x006b` returns the same pair (`PACKET/packet_func_sv.cpp:4964-4970`).

    A second wire defect sat in front of that acceptance: `0x0073` inventory rows are 196 bytes (`pangbox--packetdoc` `gameservice/server/0073.ksy:30-59`), while the server sent 12. The corrected row makes purchased Papel and Cobra equipment visible in My Room. The real client selected Cobra plus Papel, closing My Room committed one operation, PostgreSQL held ClubSet `310`/`0x10000061` and Ball `311`/`0x140000c9`, and restart/room gear retained both. Current-state frames are no-ops. See [`evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md`](evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md).

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

20. **A real client completes a purchase** — verified 2026-08-08 and strengthened 2026-08-09. The first acceptance used `price_override_pang = 1`. The later run authored the client's actual mounted `projectg850gb.pak` and generated the server manifest from the same ZIP: Cowboy Hat 4,321 Pang, Papel Training Club Set 1,234 Pang, Cobra Comet (50) 77 Pang, and Spin Mastery 5 Pang all rendered exactly. A Cobra purchase decoded retail quantity 50, committed one durable Ball row, debited exactly 77, and retained its balance/inventory across a GameService restart and full relog. Retail frame replay keys are connection-scoped and bounded, while new intentional purchases receive distinct operation IDs. See [`evidence/REAL_CLIENT_SHOP_2026-08-09.md`](evidence/REAL_CLIENT_SHOP_2026-08-09.md).

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

    `game_retail_two_players_play_and_settle_full_card` replaces the pin and drives two authenticated retail clients through all eighteen holes over TCP: each hole gets its authoritative intro and turn sequence, each shot reaches the other client as `0x0055`, and the final hole settles one durable match with **both** accounts as participants and one Pang and one EXP ledger row each.

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

28. **A real client crashes while loading a versus hole** — **fixed 2026-08-09.** The final
    cause was another sixteen-byte width error in the full `0x0076` player record. The roster
    wrote a 46-byte mascot block, omitting the `SYSTEMTIME` that all three references include:
    `pangbox/server` `pangya/player.go:181-197`, PacketDoc `0076.ksy:79-85`, and SuperSS-Dev
    `pangya_game_st.h:1170-1185`. The first entry remained plausible; its start time, card count,
    and every later entry began sixteen bytes early, so the client never constructed player 1's
    model. The full mascot is 62 bytes. Player data is now `0x2f82` bytes and the leading seat
    brings it to the client's measured `0x2f84` boundary before start time.

    A real U.S. 852 client then completed the loading ramp and rendered Blue Lagoon hole 1 at the
    playable tee with both player rows. It answered first-shot-ready and received `0x0090`.
    The host did not finish a stroke in that first run; its turn deadline produced a durable
    two-player forfeit settlement and returned the client to the room. A later run completed a
    real rendered stroke, settled the disconnected second seat, and visibly retained the exact
    +10 Pang balance after server restart. The retail-wire lifecycle test replays both finish
    frames and proves one immutable settlement. Evidence:
    [`evidence/REAL_CLIENT_MATCH_2026-08-09.md`](evidence/REAL_CLIENT_MATCH_2026-08-09.md).

    The diagnosis history below is retained because it records the eliminated causes and the
    live-memory method that found the field.

    Seven causes found and fixed before the final width correction, from the references and from
    the client's own memory rather than by guessing at the wire:

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

    **That reading was wrong, and the offsets above are measured from the wrong origin.** The
    instruction folds the array base and the field offset into one displacement, so `0x656e` is
    `array_base + key_offset` and neither is recoverable from the disassembly alone. Everything
    tabulated above is relative to the key, not to the record.

    The origin was pinned by measurement rather than argument. `PacketWriter::mark_zero_words_from`
    — a diagnostic enabled only by the `PANGYA_MARK_ROSTER` environment variable — overwrites every
    four-byte-aligned all-zero word of a roster entry with `0xc0000000 | its own offset`, so any
    word the client copies names where it came from. One match start with it on gives:

    | address | contains |
    |---|---|
    | `g + 0x6570` | the marker for entry offset `0x2f30`, then `0x2f34`, `0x2f38`, `0x2f3c` contiguously |
    | `g + 0x6580` | *no* marker — entry `0x2f40` is not all-zero, and reads `00 00 00 30` |
    | `g + 0x6584` | markers `0x2f44` through `0x2f70`, contiguous |
    | `g + 0x65b4` | the sixteen-byte `SYSTEMTIME`, so the roster's start time is at entry `0x2f74` |

    Three things follow, and they overturn the paragraph above:

    - The client **does** copy our wire order, verbatim and contiguously. The record is the
      roster entry.
    - The entry is `0x2f74 + 16 + 1 = 0x2f85` bytes and the client's record stride is `0x2f84`,
      the same thing without the trailing card-count byte. Our entry's **total width is right**.
    - The key is read at `g + 0x656e`, which is entry offset **`0x2f2e`**. That is the number to
      work from; `+0x00` in the older table is this offset, and the nickname the table put at
      `+0x48` is really at entry `0x2f2e + 0x5e`.

    `0x2f2e` lands twenty bytes past where this server starts the club-set block, inside
    `ClubSetInfo`'s enchant slots. Moving the block twenty bytes later, so the club set's
    inventory id landed exactly on `0x2f2e`, was tried and **the crash did not move** — so the
    key is not simply a club-set id displaced by a missing twenty bytes, and that change was
    reverted rather than left in.

    Two size discrepancies against `SuperSS-Dev` are known and may account for the twenty:
    its `UserEquip` is 116 bytes where this server's equipment block is 100 (its `skin_id[6]`,
    `skin_typeid[6]`, `mascot_id` and `poster[2]` total 60 against eleven zeroed words here),
    and its `MS_NUM_MAPS` is 22 where `HISTORY_COURSES` is 21. Neither can simply be corrected
    in isolation: the entry's total width already matches the client's stride exactly, so any
    twenty bytes added ahead of the club set have to come out of somewhere behind it.

    **The lookup was a catalog lookup, and the missing key was the club set's catalog id.**
    Writing `ClubSetInfo::_typeid` on entry offset `0x2f2e` — reached by carrying the sixteen
    bytes this server's equipment block is short of the reference `UserEquip` in front of the
    club set, so the entry's total width still matches the record stride — resolves it. The
    null dereference at `0x00883983` is gone, and the client goes much further: it answers
    `0x000c`, `0x0048` and `0x001a`, runs its whole progress ramp, and **sends `0x0011`**.

    It still does not reach the hole. It shows the course loading screen, the one carrying the
    course name, and the process exits there. The new fault is `0x00b65bb5`, reading
    `[null + 0x6c]`, reached from `0x0074160d`. That path is a loop over players:

    ```
    00741570  push ebx / push 0xceb500 / call 0x97cc70   ; format "PLAYER_%d" for this index
    007415f0  mov  ecx, [esi + 0x6c]                     ; per-player array, stride 0x358
    007415f6  add  ecx, edi
    00741601  call 0x8a1420                              ; == mov eax,[ecx+4]; ret
    00741606  mov  ecx, eax
    00741608  call 0xb65b60                              ; faults: this == 0
    ```

    The string constants name the subsystem, and they are decisive: `0xceb500` is `"PLAYER_%d"`,
    `0xcd8860` is `"Bip01 Head"`, `0xcf2a48` is `"Breathe"`, and `0xd266b0`/`0xd266bc` are both
    `"Recursive"`. This is character-model setup — a Biped skeleton bone, an idle animation and a
    recursive visibility flag, per scene node `PLAYER_n`. So `[record + 4]` is **that player's 3D
    character model**, and it is null: the client did not build a model for player index 0.

    **Two more causes, both from the references, and the crash has moved twice more.**

    - The server was handing a turn — `0x0063` — as part of starting the hole. No reference
      does: `0x0053` carries the connection whose turn opens the hole and that is the whole
      announcement. `pangbox/server` `game/room/room.go` emits `0x0063` only from `nextTurn`,
      reached only from `endTurn`; `Acrisio-Filho/SuperSS-Dev` `GAME/versus_base.cpp` only from
      `sendPlayerTurn`, called only from `changeTurn`; `hsreina/pangya-server` `Game.pas`
      `HandlePlayerLoadOk` sends none. The client's `0x0063` handler walks the same per-player
      array, and on the loading screen it holds no scene objects yet.
    - The three `0x0115` voice and effect rate tables that follow `0x0053` were not sent at all.
    - The roster's per-entry leading `u16` is a **one-based seat**, not the room number. The
      room number is the same value for every player, so every entry filed into one slot.

    Where it now stands: the client reaches the **course loading screen**, the one carrying the
    course name, and exits there. The fault is `0x00b65c25`, again `[null + 0x6c]`, and the
    registers say which player: `EBX = 1`, `EDI = 0x358`. **Player index 0 now has a model and
    player index 1 does not** — the first roster entry produces a player and the second does
    not, which is the signature of a per-entry width error rather than a per-field one.

    Two further measured facts, from the marker instrument:

    - The stored entry begins at entry offset zero and reads `slot=1`, `user='rsp8'`,
      `nick='HostEight'` at the offsets this server writes them. **The entry layout is right**
      and the copy is verbatim.
    - The trailing card-count byte **is** required, and removing it was a mistake since
      corrected. `Acrisio-Filho/SuperSS-Dev` (`GAME/versus_base.cpp`
      `VersusBase::sendInitialData`) writes `addUint8(count)` after the start time and then
      that many card records, so a roster without it makes the client read the *next* entry's
      first byte as a card count and eat the entry. Reading the array directly shows index 1
      empty either way, so the byte was never the difference.
    - **That array is not the roster.** Reversing the order the two seats are written in moves
      which player lands at index 0, and splitting the roster into one frame per player still
      leaves everything at index 0 with the last frame winning. It holds one entry, not a
      player list, so "the second entry is not stored" was measured against the wrong
      structure. The array the crash walks is a different one — `[esi+0x6c]`, stride `0x358` —
      and what fills *it* is the open question.

    What has not moved: the fault is still `0x00b65c25`, `[null + 0x6c]`, with `EBX = 1` — the
    second player still has no character model even though its entry now reaches memory. The
    two entry bases the scan reports are `0x2b63` apart rather than the `0x2f84` the client's
    own stride instruction uses, which is not yet explained and is the next thread to pull.

    Client opcode `0x0033` is now decoded and logged: it is the client's own exception report
    (`pangbox/server` `game/packet/client.go` `ClientException`), and it is the one channel
    through which this closed-source client explains itself. It fired once, under the marker
    diagnostic, and is the fastest route to the remaining cause.

    The next measurement is the one that settles it. With `PANGYA_MARK_ROSTER` set, dump the
    window around the *second* record and decode the markers there: each names its own offset
    within its entry, so the difference between what they read and what they should read is
    exactly how far the second entry is misplaced. Sampling the first record this way has
    already been done and is what located the club set.

    The **mascot** was eliminated by experiment — a real mascot catalog id in the block left the
    key zero and the crash unchanged — as was the **caddie**.

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
    (`cdb -g -G -o ProjectG.exe`), which avoids the break-in entirely. **That was tried and the
    client refuses it too**: launched under `cdb` it raises a stream of first-chance
    `c0000096` (privileged instruction) exceptions — the packer's anti-debug — and exits before
    reaching its login dialog. So both debugger routes are closed by the protection on the
    binary, and reading the live process with `ReadProcessMemory`, which the packer does not
    block, remains the only instrument. Defeating the anti-debug is a much larger undertaking
    than this blocker warrants; the field-by-field method still works and needs no debugger.

    Note also that the anchor for "the key is at record offset 0" is not sound: the nickname and
    start time were measured from the array base `g + 0x656e`, not from a record base, so the
    key may sit mid-record. The candidate list built on that assumption should be re-derived
    once a breakpoint can read the addresses directly.

29. **A room's own settings are not remembered, so `0x004a` describes every room as Versus** —
    **fixed.** `RoomProfile` now travels with the room's settings and its summary, so the room
    record and the room status report the mode, course, hole count and timers the creator asked
    for. The mode is carried as the client's own byte rather than mapped onto the four modelled
    types: the client's single-player practice room is a mode this server does not model and
    still has to render. Pinned by `game_retail_two_players_play_and_settle_full_card`.
    Was, before the fix: The room actor stores a name, a password and
    a capacity; the mode, course, hole count and timers a client asked for are echoed once from
    the create request and then lost. `retail_room_from_snapshot` therefore rebuilds the record as
    a Versus room on course zero with default timers, and the new `0x004a` reply repeats that. A
    real client sitting in its own Single Player Practice Mode room is told it is in a versus room
    of one and leaves Start Game disabled. The settings belong on the room, beside its name.

30. **The room directory never lists rooms** — **resolved 2026-08-09.** Opening Multiplay sends client `0x0081`; the server now pushes the current `0x0047` initial list before acknowledging entry with `0x00f5`, matching the transition ordering the retail client accepts. A headless retail-wire account hosted room 1 while a fresh real U.S. 852 client opened Multiplay under `unknown_opcode_policy = "disconnect"`: the row rendered as `001`, `VS`, `Stroke`, `PangYa-RS`, `1/2`, Blue Lagoon. Double-clicking that row sent `0x0009`, joined successfully, and rendered both named players with the real account's **Ready** control. `Join-PangyaFirstListedRoom` preserves the learned UI path. Evidence: [`evidence/REAL_CLIENT_ROOM_DIRECTORY_2026-08-09.md`](evidence/REAL_CLIENT_ROOM_DIRECTORY_2026-08-09.md).

31. **Account creation grants no ball** — **fixed.** `account create` grants a starter character
    and a club set; the equipped comet is left null, so every player enters a hole with no ball.
    Both test accounts were given one by hand to get past it. A starter comet belongs in the same
    grant as the club set.

32. **Nothing starts a single-player practice hole** — **resolved 2026-08-09.** The lobby's own **Practice** button opens *Single Player Practice Mode* and Course Practice opens a separate Strategy dialog. The defect was the room record's two mode fields, not the start opcode. The GB.852-targeting `alter-pangya` defines `PRACTICE(19, uiType = 4)` and serializes UI family 4 in the early field plus semantic type 19 later (`RoomType.kt:8-28`, `Room.kt:145-174`). The server had echoed 19 early and zero later; the header showed `(null)`, skipped the Strategy dialog on later attempts, and left the in-room Start grey.

    Practice records now carry UI mode 4 plus semantic mode 19 while the authoritative room profile retains 19. The real client displays the Strategy dialog with an active Start, sends ordinary create `0x0008` followed by start `0x000e`, loads Blue Lagoon, and renders a playable one-player tee. Its immediate loading equipment syncs are explicit too: room `0x000c` is documented by SuperSS-Dev `channel.cpp:9588-9619`, and channel `0x000b` by `packet_func_sv.h:52-55`/`packet_func_sv.cpp:379-395`; neither falls through the shipped disconnect policy. The full retail TCP solo test now creates type-19 Practice and exercises the exact 17-byte `TC_ALL` layout as an alternate start barrier. `Start-PangyaCoursePractice` captures the three-click real UI path and waits for the hole header.

    The full physical exit now passes too. Eight visible strokes advanced through distinct lies; each exact `0x0012`/`0x001b`/`0x001c` phase received the correct `0x0055`/Practice `0x006e`/five-byte `0x00cc` plus `0x0063` response. Client `0x0031` marked the already-counted last stroke holed, match `89d9db42-795a-4959-b760-724b56e1b807` committed +10 Pang/+5 EXP with one ledger row each, authoritative `0x0066` ended the one-hole session, and client `0x0006` returned cleanly to its Practice room. Evidence: [`evidence/REAL_CLIENT_PRACTICE_2026-08-09.md`](evidence/REAL_CLIENT_PRACTICE_2026-08-09.md).

33. **Quest status `0x0151` is unanswered and blocks the client modally** — **resolved server-side 2026-08-09.** PacketDoc identifies a payload-free `0x0151` request with companion replies `0x0216` and `0x0225` (`gameservice/client/0151.ksy:4-14`). Its response schemas make an empty result unambiguous: time plus zero deltas for `0x0216`, then success, zero dates, zero quest ids, and zero slots for `0x0225` (`server/0216.ksy:16-25`, `server/0225.ksy:14-38`). SuperSS-Dev sends those two frames in that order (`UTIL/mgr_daily_quest.cpp:57-121`; `PACKET/packet_func_sv.cpp:5914-5939`).

    Daily quests remain Tier D, so the server now returns that honest empty state rather than inventing mutable quest records. The retail TCP bootstrap test enters a channel under `unknown_opcode_policy = "disconnect"`, sends exact client `0x0151`, and asserts the eight-byte `0x0216` followed by the twenty-zero-byte `0x0225`. The local real-client config has also been restored from `capture` to `disconnect`; fresh login, My Room/equipment close, shop entry, and room creation remained connected with no unknown-opcode observation.

---

## Immediate next actions

1. Add a real U.S. 852 consumable-slot aggregate before claiming `0x0017` consumption; never debit an item merely because the client named a catalog id. Map any retail repair UI only after its exact client opcode and durability fields are evidenced.
2. Implement Tier C MessageService friends/presence/messages, mail, ranking, and the operator admin surface before claiming Tier C.
3. Treat each Tier-D mode/system as an independent M9 feature with packet, state, migration, and real-client evidence; keep daily quests and unsupported cards as truthful empty/opaque state until then.
4. Preserve the passing full CI matrix, the proprietary-asset guard, and the manual mounted-PAK/server-catalog synchronization contract.

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
