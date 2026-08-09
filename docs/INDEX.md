# PangYa-RS documentation index

> Last updated: **2026-08-09** — 60 documents across seven directories.

**This file is navigation plus a status rollup. It is never a source of truth.**
Every row links out to the document that owns the claim. When this index and a linked
document disagree, the linked document wins and this one is stale.

| Concern | Owner document |
|---|---|
| What the project must do | [`SPEC.md`](SPEC.md) |
| Which player state is durable, and who may set it | [`SPEC_DURABLE_PLAYER_STATE.md`](SPEC_DURABLE_PLAYER_STATE.md) |
| What is done, and what blocks the rest | [`PROGRESS.md`](PROGRESS.md) |
| Which packets are retail versus generated | [`RETAIL_CONTRACT.md`](RETAIL_CONTRACT.md) |
| Why a structural decision was made | [`adr/`](adr/) |
| That a claim actually happened | [`evidence/`](evidence/) |

Status marks are [`PROGRESS.md`](PROGRESS.md)'s legend, unchanged:

| Mark | Meaning |
|---|---|
| ✅ | Exit criteria have evidence |
| 🟡 | In progress; exit evidence incomplete |
| ⬜ | Not started |
| ⛔ | Blocked by an explicit decision or missing artifact |
| 🔬 | Research/spike needed before commitment |

---

## 1. Start here

Read in this order. Roughly two hours to a working mental model.

| # | Document | Why |
|---:|---|---|
| 1 | [`../README.md`](../README.md) | What the project is, how to build and run it |
| 2 | [`SPEC.md`](SPEC.md) §1–3 | Purpose, product definition, and the Tier A–D scope ladder every other document refers to |
| 3 | [`PROGRESS.md`](PROGRESS.md) *Current snapshot* | Where the work actually stands today |
| 4 | [`RETAIL_CONTRACT.md`](RETAIL_CONTRACT.md) | The single most important distinction in the repo: which packet families are retail-derived and which are generated placeholders |
| 5 | [`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md) | How to get a real U.S. 852 client talking to a local server |
| 6 | [`RELATED.md`](RELATED.md) | The installer repository that automates §1, §5 and §6 of the above, and which side owns what |

Two short operational memories are worth reading before debugging anything:
[`memory/check-references-on-error.md`](memory/check-references-on-error.md) and
[`memory/reading-the-live-client.md`](memory/reading-the-live-client.md).

---

## 2. Normative specifications

| Document | Authoritative for | Updated |
|---|---|---|
| [`SPEC.md`](SPEC.md) | Product scope, architecture, requirements by domain (§22), milestones M0–M9 (§23), definition of done (§27) | 2026-08-07 |
| [`SPEC_DURABLE_PLAYER_STATE.md`](SPEC_DURABLE_PLAYER_STATE.md) | Which player state is persisted, in which table, and whether an operator can set it. Supersedes `SPEC.md` §14.2's `character_parts` sketch | 2026-08-09 |
| [`RETAIL_CONTRACT.md`](RETAIL_CONTRACT.md) | Retail versus synthetic packet surface, and the removal plan for the `0x7f**` placeholders | 2026-08-08 |
| [`CONFIGURATION.md`](CONFIGURATION.md) | Every configuration key and its bounds | 2026-08-07 |

`SPEC.md` and `SPEC_DURABLE_PLAYER_STATE.md` are complementary, not overlapping: the first
states *what the server must do*, the second states *what survives a restart and who may
change it*. `RETAIL_CONTRACT.md` constrains both by stating which wire layouts may be claimed.

---

## 3. Milestones

Statuses transcribed from [`PROGRESS.md`](PROGRESS.md) *Future milestone board*. That table is
the ledger; this is a shortcut with links.

| # | Milestone | Status | Spec | ADR | Evidence |
|---|---|:--:|---|---|---|
| M0 | Planning and provenance | ✅ | [§23](SPEC.md) | — | [`PROGRESS.md` §M0](PROGRESS.md) |
| M1 | Cargo and protocol foundation | ✅ | [§23](SPEC.md) | [0001](adr/0001-project-license.md) [0002](adr/0002-us-852-profile.md) [0003](adr/0003-modular-monolith.md) [0004](adr/0004-tokio-codec-room-actors.md) [0005](adr/0005-lzokay.md) | [LZO](evidence/LZO_INDEPENDENT_2026-08-05.md) |
| M2 | LoginService vertical slice | ✅ | [§23](SPEC.md) | [0006](adr/0006-postgresql-sqlx-migrations.md) [0007](adr/0007-legacy-secret-argon2id.md) | [login](evidence/M2_LOGIN_VERTICAL_SLICE_2026-08-05.md) · [storage](evidence/M2_STORAGE_FOUNDATION_2026-08-05.md) |
| M3 | GameService handover and bootstrap | ✅ | [§23](SPEC.md) | [0010](adr/0010-synthetic-m3-catalog-game-bootstrap.md) | [synthetic](evidence/M3_SYNTHETIC_GAME_BOOTSTRAP_2026-08-05.md) · [real startup](evidence/REAL_CLIENT_STARTUP_2026-08-07.md) |
| M4 | Lobby and rooms | ✅ | [§23](SPEC.md) | [0011](adr/0011-synthetic-m4-lobby-room.md) | [synthetic](evidence/M4_SYNTHETIC_LOBBY_ROOM_2026-08-05.md) · [real directory](evidence/REAL_CLIENT_ROOM_DIRECTORY_2026-08-09.md) |
| M5 | Solo practice first playable | ✅ | [§23](SPEC.md) | [0012](adr/0012-synthetic-m5-solo-practice.md) | [synthetic](evidence/M5_SYNTHETIC_SOLO_2026-08-05.md) · [real practice](evidence/REAL_CLIENT_PRACTICE_2026-08-09.md) |
| M6 | Multiplayer stroke and records | ✅ | [§23](SPEC.md) | [0013](adr/0013-synthetic-m6-two-player-stroke.md) | [synthetic](evidence/M6_SYNTHETIC_STROKE_2026-08-05.md) · [real match](evidence/REAL_CLIENT_MATCH_2026-08-09.md) |
| M7 | Inventory and shop depth | 🟡 | [§23](SPEC.md) | [0014](adr/0014-synthetic-m7-economy.md) | [synthetic](evidence/M7_SYNTHETIC_ECONOMY_2026-08-07.md) · [real shop](evidence/REAL_CLIENT_SHOP_2026-08-09.md) · [real equipment](evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md) |
| M8 | Social and ranking | ⬜ | [§23](SPEC.md) | — | — |
| M9 | Broad parity program | ⬜ | [§23](SPEC.md) | — | — |

**M7 is 🟡 for a specific reason:** retail mounted-PAK purchase and ClubSet/Ball equipment
persist across restart, but retail consume and repair are not claimed. See
[`PROGRESS.md`](PROGRESS.md) blocker 9.

### Durable-state milestones (`SPEC_DURABLE_PLAYER_STATE.md`)

Ordered to match [`protocol/US852_SUBSYSTEM_GAPS.md`](protocol/US852_SUBSYSTEM_GAPS.md) §7 so
the two documents cannot drift.

| # | Milestone | Status | Covers |
|---|---|:--:|---|
| E1 | Caddie and mascot | ⬜ | `DPS-010…019` — rosters `0x0071`/`0x00e1`, `Caddie.iff`/`Mascot.iff` |
| E2 | Character parts, aux parts, hair colour, mastery | ⬜ | `DPS-020…039` — 24 + 5 slots, and two columns already stored but sent as zero |
| E3 | Consumable and decoration slots | ⬜ | `DPS-040…059` — 10 + 7 slots |
| E4 | Cards | ⬜ | `DPS-060…069` — 12 slots, container `0x0138`, `Card.iff` |

### Operator admin surface

Named in [`PROGRESS.md`](PROGRESS.md) *Immediate next actions* item 2 as Tier C work.
Implemented as `crates/pangya-admin` plus a separate SPA at `../pangya-admin`.

| Phase | Scope | Status |
|---|---|:--:|
| 0 | Migration 0009, admin crate, sessions, `account role` CLI, this index, ADR-0016 | ✅ |
| 1 | Account read endpoints and UI | ✅ |
| 2 | Account mutations | ✅ |
| 3 | Catalog item names, catalog browser | ✅ |
| 4 | Inventory, character, and equipment control | ✅ |
| 5 | Migration 0010, live shop overlay | ✅ |
| 6 | Status dashboard, audit log, leaderboard | ✅ |

---

## 4. Feature backlog, ordered

The rank and the *why* columns are
[`protocol/US852_SUBSYSTEM_GAPS.md`](protocol/US852_SUBSYSTEM_GAPS.md) §7's, which orders by
unblocking value, then evidence quality, then cost. The status column is derived from
[`PROGRESS.md`](PROGRESS.md) — read the gap analysis for the reasoning, not this table.

| Rank | Work | Status | Note |
|---:|---|:--:|---|
| 1 | Retail chat `0x0003` → `0x0040` | ⬜ | Only the synthetic `0x7f**` chat path exists today; retail chat under `unknown_opcode_policy = "disconnect"` kills a live session |
| 2 | Match completion set + live room census | ✅ | Blocker 23; two-seat retail match evidenced |
| 3 | Room settings `0x000a`, equipment-in-room `0x000c`/`0x000b` | ✅ | Blockers 29 and 32 |
| 4 | Statistics submit `0x0006`/`0x0031` → `0x0045`, `0x002f` burst | 🟡 | `0x0031` and `0x0006` are handled on the practice path and `0x0045` exists as a bootstrap packet; the submit→reply path and `0x002f` are not claimed |
| 5 | `0x0216` user status update as a first-class primitive | 🟡 | Emitted as the honest empty pair for daily quests (blocker 33); not yet a reusable grant primitive |
| 6 | Caddie and mascot rosters `0x0071`/`0x00e1` | ⬜ | `E1`; "Tier D by name and Tier B by cost" |
| 7 | Locker `0x00cd`–`0x00d5` | ⬜ | |
| 8 | Mail `0x0143`–`0x0147`, `0x0210` | ⬜ | M8 |
| 9 | MessageService, `0x008b` → `0x00fc` | ⬜ | M8. `crates/pangya-message` is a 12-line stub |
| 10 | Login bonus `0x016e`/`0x016f` | ⬜ | |
| 11 | Rare shop, Papel, scratchy, memorial, lootbox | ⬜ | Needs rank 5 first |
| 12 | Quests `0x0151`–`0x0154` | 🟡 | Honest empty state served (blocker 33); no mutable quest records |
| 13 | Achievements `0x0157`, `0x021d`, `0x021e` | ⛔ | Gap analysis trap 5.8 unresolved |
| 14 | Cards and character mastery | ⬜ | `E4` and part of `E2` |
| 15 | Events and Grand Prix | ⬜ | See [`protocol/US852_TOURNAMENT_MODE.md`](protocol/US852_TOURNAMENT_MODE.md) |
| 16 | Personal shop, club workshop, rentals, guilds, MyRoom furniture/UCC | ⛔ | Deferred on evidence, not effort |
| — | Rings `0x015d` | 🔬 | Opcode exists in two sources; no source documents the body |

---

## 5. Architecture decision records

| ADR | Subject | Status | Updated |
|---|---|:--:|---|
| [0001](adr/0001-project-license.md) | Project license — dual MIT OR Apache-2.0 | ✅ Accepted | 2026-08-05 |
| [0002](adr/0002-us-852-profile.md) | U.S. 852 first compatibility profile | ✅ Accepted | 2026-08-05 |
| [0003](adr/0003-modular-monolith.md) | Modular-monolith baseline | ✅ Accepted | 2026-08-05 |
| [0004](adr/0004-tokio-codec-room-actors.md) | Tokio codec and bounded room actors | ✅ Accepted | 2026-08-05 |
| [0005](adr/0005-lzokay.md) | `lzokay` 2.x adoption | ✅ Accepted | 2026-08-05 |
| [0006](adr/0006-postgresql-sqlx-migrations.md) | PostgreSQL and SQLx forward migrations | ✅ Accepted | 2026-08-05 |
| [0007](adr/0007-legacy-secret-argon2id.md) | Legacy MD5 transport secret, Argon2id at rest | ✅ Accepted | 2026-08-05 |
| 0008 | Packet fixture and provenance policy | ⛔ **Queued in [`SPEC.md`](SPEC.md) §24; no file exists** | — |
| 0009 | Client-authoritative shot data and the validation/reward boundary | ⛔ **Queued in [`SPEC.md`](SPEC.md) §24; no file exists** | — |
| [0010](adr/0010-synthetic-m3-catalog-game-bootstrap.md) | Synthetic M3 catalog and GameService bootstrap | ✅ Accepted | 2026-08-05 |
| [0011](adr/0011-synthetic-m4-lobby-room.md) | Synthetic M4 lobby and room checkpoint | ✅ Accepted | 2026-08-05 |
| [0012](adr/0012-synthetic-m5-solo-practice.md) | Synthetic M5 solo-practice checkpoint | ✅ Accepted | 2026-08-06 |
| [0013](adr/0013-synthetic-m6-two-player-stroke.md) | Synthetic M6 exactly-two stroke checkpoint | ✅ Accepted | 2026-08-06 |
| [0014](adr/0014-synthetic-m7-economy.md) | Synthetic M7 inventory, shop, and equipment | ✅ Accepted | 2026-08-07 |
| [0015](adr/0015-client-patch-web-service.md) | Serve the client's patch web contract from `pangya-updater` | ✅ Accepted | 2026-08-07 |
| [0016](adr/0016-admin-api-on-the-http-listener.md) | Operator admin API on the `[http]` listener | ✅ Accepted | 2026-08-09 |

**Two numbering defects, recorded rather than papered over:**

1. **ADR-0008 and ADR-0009 do not exist.** [`SPEC.md`](SPEC.md) §24 queues both. Packet
   fixture provenance is in practice governed by [`PROVENANCE.md`](PROVENANCE.md) and
   [`RETAIL_CONTRACT.md`](RETAIL_CONTRACT.md); the client-authoritative shot boundary is
   governed by `SPEC.md` §12.4. Either write the ADRs or amend §24 to point at the documents
   that actually hold those decisions.
2. **ADR-0010's subject drifted.** `SPEC.md` §24 item 10 reserves 0010 for "proprietary data
   mounting and no-assets repository policy"; the file at `adr/0010-*.md` is "synthetic M3
   catalog and GameService bootstrap". The no-assets policy is currently enforced by
   `scripts/check-proprietary-assets.sh` and `.gitignore` with no ADR behind it.

---

## 6. Protocol

**Retail — reference-derived, may be claimed as U.S. 852 behaviour.**

| Document | Covers | Updated |
|---|---|---|
| [`protocol/US852_RETAIL_BOOTSTRAP.md`](protocol/US852_RETAIL_BOOTSTRAP.md) | Hello, auth, the 116-byte equipment block, the 513-byte character block, containers `0x0070`–`0x0073`, lobby/room opcode table | 2026-08-08 |
| [`protocol/US852_SUBSYSTEM_GAPS.md`](protocol/US852_SUBSYSTEM_GAPS.md) | Every unhandled client opcode by subsystem, the traps, and the ranked order reproduced in §4 above | 2026-08-08 |
| [`protocol/US852_TOURNAMENT_MODE.md`](protocol/US852_TOURNAMENT_MODE.md) | Tournament/Grand Prix mode specification | 2026-08-08 |

**Synthetic — the `0x7f**` families. Placeholders no real client will ever send. Never
identify these as retail protocol** (ADR-0010 through ADR-0014; removal plan in
[`RETAIL_CONTRACT.md`](RETAIL_CONTRACT.md) §4).

| Document | Family | Updated |
|---|---|---|
| [`protocol/M2_SYNTHETIC_LOGIN_FLOW.md`](protocol/M2_SYNTHETIC_LOGIN_FLOW.md) | Login | 2026-08-05 |
| [`protocol/M3_SYNTHETIC_GAME_FLOW.md`](protocol/M3_SYNTHETIC_GAME_FLOW.md) | Bootstrap | 2026-08-05 |
| [`protocol/M4_SYNTHETIC_LOBBY_ROOM_FLOW.md`](protocol/M4_SYNTHETIC_LOBBY_ROOM_FLOW.md) | `0x7f00` lobby/room | 2026-08-05 |
| [`protocol/M5_SYNTHETIC_SOLO_FLOW.md`](protocol/M5_SYNTHETIC_SOLO_FLOW.md) | `0x7f20`/`0x7fa0` solo | 2026-08-06 |
| [`protocol/M6_SYNTHETIC_STROKE_FLOW.md`](protocol/M6_SYNTHETIC_STROKE_FLOW.md) | `0x7f30`/`0x7fb0` stroke | 2026-08-06 |
| [`protocol/M7_SYNTHETIC_ECONOMY_FLOW.md`](protocol/M7_SYNTHETIC_ECONOMY_FLOW.md) | `0x7f40`/`0x7fc0` economy | 2026-08-07 |

**Transport.** [`protocol/LZO_COMPATIBILITY.md`](protocol/LZO_COMPATIBILITY.md) — `lzokay` 2.x
compatibility report (2026-08-05).

---

## 7. Data and catalog

| Document | Covers | Updated |
|---|---|---|
| [`data/US_CLIENT_IFF_STRUCTURE.md`](data/US_CLIENT_IFF_STRUCTURE.md) | ZIP-in-PAK packaging, all 39 client tables, header format, family-tag table, per-family record sizes | 2026-08-07 |
| [`data/M3_SYNTHETIC_CATALOG.md`](data/M3_SYNTHETIC_CATALOG.md) | Synthetic manifest and mount format; v2 shop-metadata record layout | 2026-08-07 |

**The server parses 6 of the client's 39 declared tables.** Loaded today: `Character`,
`ClubSet`, `Ball`, `Item`, `Part`, `Course`. Not loaded: `Caddie`, `CaddieItem`, `Mascot`,
`Card`, `Skin`, `AuxPart`, `HairStyle`, `SetItem`, `Club`, `Enchant`, `PointShop`,
`ShopLimitItem`, `QuestStuff`, `QuestItem`, `CounterItem`, `Furniture`, `CutinInfomation`, and
the rest. Each `E`-milestone in [`SPEC_DURABLE_PLAYER_STATE.md`](SPEC_DURABLE_PLAYER_STATE.md)
names the tables it needs.

---

## 8. Evidence ledger

Newest first. Every claim of real-client acceptance in this repository traces to one of these.

| Date | Document | Kind | Establishes |
|---|---|---|---|
| 2026-08-09 | [`evidence/ADMIN_PANEL_2026-08-09.md`](evidence/ADMIN_PANEL_2026-08-09.md) | operator surface | Admin sessions, per-request authorisation, append-only audit, account/inventory/equipment control, live shop overlay, catalog names, same-origin panel |
| 2026-08-09 | [`evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md`](evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md) | real client | 196-byte inventory rows render; ClubSet + Ball equip commits atomically and survives restart |
| 2026-08-09 | [`evidence/REAL_CLIENT_SHOP_2026-08-09.md`](evidence/REAL_CLIENT_SHOP_2026-08-09.md) | real client | Authored IFF in the mounted PAK; server-authoritative price charged; replay resistance |
| 2026-08-09 | [`evidence/REAL_CLIENT_MATCH_2026-08-09.md`](evidence/REAL_CLIENT_MATCH_2026-08-09.md) | real client | Two-seat versus hole with durable forfeit settlement |
| 2026-08-09 | [`evidence/REAL_CLIENT_PRACTICE_2026-08-09.md`](evidence/REAL_CLIENT_PRACTICE_2026-08-09.md) | real client | Eight physical strokes, one committed hole, +10 Pang/+5 EXP |
| 2026-08-09 | [`evidence/REAL_CLIENT_ROOM_DIRECTORY_2026-08-09.md`](evidence/REAL_CLIENT_ROOM_DIRECTORY_2026-08-09.md) | real client | Multiplay lists rooms |
| 2026-08-07 | [`evidence/REAL_CLIENT_STARTUP_2026-08-07.md`](evidence/REAL_CLIENT_STARTUP_2026-08-07.md) | real client | 33/33 startup HTTP requests answered; 84 PAK archives mounted |
| 2026-08-07 | [`evidence/US_CLIENT_ACQUISITION_2026-08-07.md`](evidence/US_CLIENT_ACQUISITION_2026-08-07.md) | provenance | Which client build was acquired and what could be established about it |
| 2026-08-07 | [`evidence/M7_SYNTHETIC_ECONOMY_2026-08-07.md`](evidence/M7_SYNTHETIC_ECONOMY_2026-08-07.md) | synthetic | Idempotent purchase/equip/consume/repair over encrypted TCP |
| 2026-08-05 | [`evidence/M6_SYNTHETIC_STROKE_2026-08-05.md`](evidence/M6_SYNTHETIC_STROKE_2026-08-05.md) | synthetic | Exactly-two stroke settlement |
| 2026-08-05 | [`evidence/M5_SYNTHETIC_SOLO_2026-08-05.md`](evidence/M5_SYNTHETIC_SOLO_2026-08-05.md) | synthetic | One-hole solo practice |
| 2026-08-05 | [`evidence/M4_SYNTHETIC_LOBBY_ROOM_2026-08-05.md`](evidence/M4_SYNTHETIC_LOBBY_ROOM_2026-08-05.md) | synthetic | Room actor state and property tests |
| 2026-08-05 | [`evidence/M3_SYNTHETIC_GAME_BOOTSTRAP_2026-08-05.md`](evidence/M3_SYNTHETIC_GAME_BOOTSTRAP_2026-08-05.md) | synthetic | Catalog load and channel entry |
| 2026-08-05 | [`evidence/M2_LOGIN_VERTICAL_SLICE_2026-08-05.md`](evidence/M2_LOGIN_VERTICAL_SLICE_2026-08-05.md) | synthetic | Login through handover |
| 2026-08-05 | [`evidence/M2_STORAGE_FOUNDATION_2026-08-05.md`](evidence/M2_STORAGE_FOUNDATION_2026-08-05.md) | synthetic | Transactional aggregate creation and handover races |
| 2026-08-05 | [`evidence/LZO_INDEPENDENT_2026-08-05.md`](evidence/LZO_INDEPENDENT_2026-08-05.md) | independent | `lzokay` output matches an independent LZO1X implementation |

---

## 9. Operations

| Document | Covers | Updated |
|---|---|---|
| [`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md) | Windows VM, Rugburn, registry prerequisites, extracting the client's item tables, authoring a custom shop, funding an account | 2026-08-09 |
| [`CONFIGURATION.md`](CONFIGURATION.md) | Every key in `config/*.toml` and its bounds | 2026-08-07 |
| [`PROVENANCE.md`](PROVENANCE.md) | Which upstream artifact each adapted algorithm came from, and under which license | 2026-08-07 |
| [`RELATED.md`](RELATED.md) | The installer repository, and the concern boundary between the two | 2026-08-09 |

**Scripts** (`../scripts/`): `grant-balance.sh` (wraps the audited `account grant`),
`second-seat.sh`, `sync-client-shop.sh` + `author-client-iff.py` + `extract-client-iff.py`
(client shop authoring), `check-proprietary-assets.sh` (CI guard), `tailnet-forward.py`,
`windows/pangya-client.ps1`.

**Patches** (`../patches/`): the Rugburn `AllowMultipleInstances` patch moved to
[`thunderjr/pangya-client`](https://github.com/thunderjr/pangya-client), which builds and ships
the DLL. `patches/README.md` here is a pointer; the directory remains the right home for any
future diff against a gitignored `opensource-references/` clone.

---

## 10. Reference and background

| Document | Covers |
|---|---|
| [`INITIAL_RESEARCH.md`](INITIAL_RESEARCH.md) | The evidence base and technology recommendation the project was founded on |
| [`PLAN.html`](PLAN.html) | Standalone visual milestone/architecture/risk plan |
| [`MEMORY.md`](MEMORY.md) | Cross-session handoff narrative |
| [`memory/check-references-on-error.md`](memory/check-references-on-error.md) | Read `opensource-references/` first on any client error; never guess-and-check against the client |
| [`memory/reading-the-live-client.md`](memory/reading-the-live-client.md) | The `.exe` is packed — disassemble the running process, not the file |
| [`../opensource-references/README.md`](../opensource-references/README.md) | 16 upstream clones, their revisions, licenses, and the boundary rule: reference only, never vendored |
| [`thunderjr/pangya-client`](https://github.com/thunderjr/pangya-client) | The Windows installer that points a client at this server. Owns Rugburn, `rugburn.json`, and the client's registry profile; its [`docs/PATCHING.md`](https://github.com/thunderjr/pangya-client/blob/main/docs/PATCHING.md) explains how the runtime patches work |

**pangya.wiki synthesis** — background research from the community wiki, frozen at 2026-08-05.

| Document | Covers |
|---|---|
| [`pangya.wiki/README.md`](pangya.wiki/README.md) | Index and method for the six documents below |
| [`pangya.wiki/COURSES.md`](pangya.wiki/COURSES.md) | Courses and walkthrough data |
| [`pangya.wiki/GAMEPLAY_AND_MODES.md`](pangya.wiki/GAMEPLAY_AND_MODES.md) | Gameplay and modes |
| [`pangya.wiki/CHARACTERS_HISTORY_AND_AUDIO.md`](pangya.wiki/CHARACTERS_HISTORY_AND_AUDIO.md) | Characters, service history, seasons, audio |
| [`pangya.wiki/CLIENT_TECHNOLOGY.md`](pangya.wiki/CLIENT_TECHNOLOGY.md) | Client technology and implementation clues |
| [`pangya.wiki/PATCH_HISTORY.md`](pangya.wiki/PATCH_HISTORY.md) | Patch history synthesis |
| [`pangya.wiki/SOURCE_COVERAGE.md`](pangya.wiki/SOURCE_COVERAGE.md) | Which upstream sources cover which subsystem |

---

## 11. Open blockers

Full narrative in [`PROGRESS.md`](PROGRESS.md) *Open blockers and decisions*. Counts only here.

| Class | Count | Notes |
|---|---:|---|
| Standing external gates (entries 1–10) | 10 | Legally supplied client and data. Tracked continuously; several are satisfied for M2–M6 and remain open for later milestones |
| Resolved, verified, or fixed with a date (entries 11, 13–33) | 22 | Kept in place as the record of what was actually wrong |
| Open and instrumented (entry 12) | 1 | `concurrent_stroke_matches_with_shared_accounts_are_deadlock_free` failed once in CI and never reproduced; `StorageFault` now classifies the next occurrence rather than a guess being committed |
| **Total numbered entries** | **33** | |

The two that constrain new work most:

- **Entry 10, the synthetic-to-retail protocol pivot.** The `0x7f**` families are placeholders.
  Any new subsystem should be built against reference-derived retail layouts, not by extending
  the synthetic families.
- **Entry 9, retail consume and repair.** M7 stays 🟡 until these are evidenced.

---

## 12. Document health

| Document | Updated | Flag |
|---|---|---|
| [`PROGRESS.md`](PROGRESS.md) | 2026-08-09 | current |
| [`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md) | 2026-08-09 | ⚠ §6's `IntegratedPak = "0"` is disputed — see [`RELATED.md`](RELATED.md) and the open question in the installer's `docs/PATCHING.md`; unresolved until tested against a client |
| [`RELATED.md`](RELATED.md) | 2026-08-09 | current |
| [`INDEX.md`](INDEX.md) | 2026-08-09 | current |
| [`SPEC_DURABLE_PLAYER_STATE.md`](SPEC_DURABLE_PLAYER_STATE.md) | 2026-08-09 | current |
| [`RETAIL_CONTRACT.md`](RETAIL_CONTRACT.md) | 2026-08-08 | current |
| `protocol/US852_*.md` | 2026-08-08 | current |
| [`SPEC.md`](SPEC.md) | 2026-08-07 | ⚠ §14.2's `character_parts` sketch is superseded by [`SPEC_DURABLE_PLAYER_STATE.md`](SPEC_DURABLE_PLAYER_STATE.md); §24's ADR queue disagrees with `adr/` (see §5) |
| [`MEMORY.md`](MEMORY.md) | 2026-08-07 | ⚠ predates the 2026-08-09 real-client practice, match, shop, and equipment runs |
| [`CONFIGURATION.md`](CONFIGURATION.md) | 2026-08-09 | current |
| [`PROVENANCE.md`](PROVENANCE.md) | 2026-08-07 | current |
| `data/*.md` | 2026-08-07 | current |
| `protocol/M*_SYNTHETIC_*.md` | 2026-08-05 … 08-07 | frozen by design — these describe placeholder families scheduled for removal |
| [`INITIAL_RESEARCH.md`](INITIAL_RESEARCH.md), [`PLAN.html`](PLAN.html), `pangya.wiki/*` | 2026-08-05 | historical; not expected to change |

---

## Maintaining this file

1. Adding a document under `docs/` must add its row here **in the same change**. CI asserts
   that every `docs/**/*.md` path appears in this file.
2. Never copy a claim here. Link to the document that owns it.
3. When a status mark here disagrees with [`PROGRESS.md`](PROGRESS.md), `PROGRESS.md` is right
   and this file is the bug.
