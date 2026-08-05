# PangYa-RS progress

> Last updated: **2026-08-05**
>
> Current stage: **M3 — local synthetic GameService bootstrap complete; external client/data gates remain**
>
> Next gate: **legally held U.S. 852 IFF/packet validation; no M4 until reviewed**

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
| Protocol/crypto | 🟡 | All local M1 vectors, fixtures, transport boundaries, audits, and bounded fuzz checks pass; real-client acceptance remains |
| LoginService | 🟡 | Local synthetic M2 runtime/config/CLI/health/TCP/PostgreSQL exit passes; real U.S. 852 order, token field/length, name limits, and server-list acceptance remain |
| GameService/bootstrap | 🟡 | Local synthetic Login-to-Game snapshot/catalog/channel flow passes; real U.S. 852 layouts and acceptance remain external |
| Rooms/gameplay | ⬜ | M4–M6 |
| Economy/social/parity | ⬜ | M7+ |

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
| M4 — Lobby and rooms | ⬜ | Real client creates and enters a room; concurrent synthetic state tests pass |
| M5 — Solo first playable | ⬜ | One real-client hole finishes and persists exactly one reward |
| M6 — Multiplayer stroke | ⬜ | Two real clients finish with consistent standings and durable records |
| M7 — Inventory/shop depth | ⬜ | Catalog-derived transactional purchases and equipment validation |
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

---

## Immediate next actions

1. Obtain a legally held U.S. 852 client build/hash privately and validate exact login ordering, nickname limits, server-list acceptance, and handover field/length without committing proprietary artifacts.
2. Preserve the synthetic M2/M3 exits and perform independent review; do not begin M4 until local findings and external compatibility gates are recorded.

---

## Change log

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
