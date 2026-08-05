# PangYa-RS progress

> Last updated: **2026-08-05**
>
> Current stage: **M0 — planning and provenance complete; implementation not started**
>
> Next gate: **M1 — Cargo and protocol foundation**

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
| Final project license | ⛔ | ADR-0001 needs owner decision before adapting upstream source |
| Cargo workspace | ⬜ | Starts only after implementation authorization |
| Protocol/crypto | ⬜ | First implementation target: PangCrypt vectors and bounded codec |
| LoginService | ⬜ | M2 |
| GameService/bootstrap | ⬜ | M3 |
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

**Status: ⬜ not started**

### Decision prerequisites

- [ ] ADR-0001: choose final project license.
- [ ] ADR-0002: confirm U.S. 852 as first supported compatibility profile.
- [ ] ADR-0003: accept modular-monolith deployment baseline.
- [ ] ADR-0004: accept Tokio codec and room-actor approach.
- [ ] ADR-0005: decide `lzokay` after the compatibility spike.

### Implementation checklist

- [ ] Initialize Rust 2024 Cargo workspace and declared MSRV.
- [ ] Add workspace lint, format, test, deny, and CI policies.
- [ ] Create `pangya-crypto` and `pangya-protocol` crates.
- [ ] Add `THIRD_PARTY_NOTICES.md` and `docs/PROVENANCE.md`.
- [ ] Port PangCrypt oracle tables with ISC attribution and table hash.
- [ ] Port all PangCrypt client golden vectors.
- [ ] Port/decode PangCrypt server vectors.
- [ ] Prove or reject `lzokay` compatibility.
- [ ] Implement bounded client `Decoder` and server `Encoder`.
- [ ] Test one-byte fragmentation, every header split, coalesced frames, truncation, oversized frames and invalid keys.
- [ ] Add property and fuzz harnesses.
- [ ] Add U.S. 852 LoginService hello fixture.
- [ ] Build a synthetic TCP hello/login harness.

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
| M2 — LoginService | ⬜ | Synthetic client reaches server selection; handover can be consumed once |
| M3 — Game bootstrap | ⬜ | Real U.S. 852 reaches one channel with starter character/equipment |
| M4 — Lobby and rooms | ⬜ | Real client creates and enters a room; concurrent synthetic state tests pass |
| M5 — Solo first playable | ⬜ | One real-client hole finishes and persists exactly one reward |
| M6 — Multiplayer stroke | ⬜ | Two real clients finish with consistent standings and durable records |
| M7 — Inventory/shop depth | ⬜ | Catalog-derived transactional purchases and equipment validation |
| M8 — Social/ranking | ⬜ | Durable friends/messages/mail basics and rebuildable ranking projection |
| M9 — Broad parity | ⬜ | Each legacy feature group completes its own packet/state/persistence gate |

---

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
| `lzokay` is a current pure-Rust MIT LZO candidate | crates.io/docs and `encounter/lzokay-rs`; adoption not yet proven |
| Tokio Util supports custom bounded incomplete-frame decoding | current `tokio-util::codec` documentation |
| SQLx supports checked queries, migrations, offline metadata, transactions and test macro | current SQLx documentation |

---

## Open blockers and decisions

1. **Final project license** — recommended options are MIT, Apache-2.0, or dual MIT/Apache-2.0 while preserving a clean-room boundary. Requires owner choice.
2. **Exact U.S. 852 client build/hash** — needed before real-client fixtures and smoke tests; do not commit the client.
3. **LZO implementation** — `lzokay` is preferred on license/implementation grounds but remains a spike outcome.
4. **Account provisioning** — spec recommends operator CLI plus local-only auto-create; confirm before M2.
5. **Advertised ports/order** — confirm against the chosen client capture during M1/M2 rather than relying on legacy defaults.

---

## Immediate next actions

When the owner says to start implementation:

1. Resolve ADR-0001 (license).
2. Create ADR skeletons 0001–0005.
3. Initialize the Cargo workspace only—no gameplay modules yet.
4. Add provenance/third-party notice policy.
5. Implement PangCrypt fixture parity.
6. Run the LZO compatibility spike.
7. Implement bounded Tokio framing.
8. Update this file after each proof, not at the end of the milestone.

---

## Change log

### 2026-08-05

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
