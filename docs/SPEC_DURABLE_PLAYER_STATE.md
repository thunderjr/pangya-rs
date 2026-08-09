# PangYa-RS — durable player state specification

> Status: **draft, normative for the `E1`–`E4` milestones**
> Date: 2026-08-09
> Supersedes: [`SPEC.md`](SPEC.md) §14.2's `character_parts` table sketch.

## 1. Purpose and claim boundary

The U.S. 852 wire models a player as far more than this server persists. `RetailEquipment` is
116 bytes and `RetailCharacter` is 513 bytes, but the database stores exactly three equipped
selections. Every slot in between is emitted as a zero, and the equipment handler declines to
acknowledge changes to it rather than pretending to store them
(`crates/pangya-game/src/lib.rs:5290`).

That is honest, and it is also the single largest gap between "the client renders a player"
and "the server owns a player". This document specifies closing it.

**Claim boundary.** Nothing in this document is retail-proven. Every layout cited here comes
from the reference-derived work already recorded in
[`protocol/US852_RETAIL_BOOTSTRAP.md`](protocol/US852_RETAIL_BOOTSTRAP.md) and
[`protocol/US852_SUBSYSTEM_GAPS.md`](protocol/US852_SUBSYSTEM_GAPS.md). A legally held client
and legally supplied data must accept each exchange before any retail claim, exactly as in
ADR-0010 and ADR-0014. Zero is a truthful answer until a slot is durable; an invented value
is not.

## 2. Relationship to the existing documents

This specification owns **one axis and only one**: *which player state is durable, in which
table, and whether an operator may set it.* It does not restate packet layouts or re-order the
protocol backlog.

| This document | Defers to |
|---|---|
| Which slots must persist, and their schema | [`protocol/US852_RETAIL_BOOTSTRAP.md`](protocol/US852_RETAIL_BOOTSTRAP.md) for the byte layouts |
| Delivery order of `E1`–`E4` | [`protocol/US852_SUBSYSTEM_GAPS.md`](protocol/US852_SUBSYSTEM_GAPS.md) §7's ranking, which orders by unblocking value |
| Requirements traceability | [`SPEC.md`](SPEC.md) §22.2 (inventory/equipment) and §22.1 (profile) |
| What may be called retail | [`RETAIL_CONTRACT.md`](RETAIL_CONTRACT.md) |
| Definition of done | [`SPEC.md`](SPEC.md) §27, unchanged |

Status marks are [`PROGRESS.md`](PROGRESS.md)'s legend.

## 3. The gap, stated once

Wire surface versus durable surface, as of 2026-08-09.

| Slot family | Wire carrier | Slots | Durable today | Operator-settable today |
|---|---|---:|---|:--:|
| Character | `RetailCharacter.iff_id` / `.uid` | 1 | `equipment_sets.character_id` | ✅ |
| Club set | `RetailEquipment.club_set_uid` | 1 | `equipment_sets.club_item_id` | ✅ |
| Ball ("comet") | `RetailEquipment.comet_iff_id` | 1 | `equipment_sets.ball_item_id` | ✅ |
| Hair colour | `RetailCharacter.hair_color` | 1 | `characters.hair_color` — **stored, sent as zero** | ⬜ |
| Character mastery | `RetailCharacter.mastery` | 1 | `characters.mastery` — **stored, sent as zero** | ⬜ |
| Character stats | `RetailCharacter.stats` (power, control, accuracy, spin, curve) | 5 | — | ⬜ |
| Caddie | `RetailEquipment.caddie_uid`, `RetailCaddie` block, roster `0x0071` | 1 | — | ⬜ |
| Character parts | `RetailCharacter.part_iff_ids` + `.part_uids` | 24 | `character_part_slots` (migration 0011) | 🟡 wire decoded, table exists, storage not wired |
| Aux parts | character block, `CHARACTER_AUX_PARTS` | 5 | — | ⬜ |
| Cut-in | character block, and `RetailEquipmentUpdated::Decoration` slot 5 | 1 | — | ⬜ |
| Cards | character block, `CHARACTER_CARDS`, container `0x0138` | 12 | — | ⬜ |
| Consumable slots | `RetailEquipment.item_iff_ids` | 10 | — | ⬜ |
| Decoration | `RetailEquipmentUpdated::Decoration([u32; 6])` — background, frame, sticker, slot, cut-in, title | 6 | — | ⬜ |
| Skins / mascot / posters | `EQUIPMENT_TRAILING_SLOTS` | 15 u32 | — | ⬜ |

Sources: `crates/pangya-protocol/src/us852_bootstrap.rs:22,31,226,786-794,1004`;
`crates/pangya-protocol/src/us852_room.rs:490-507,568`;
`crates/pangya-storage/migrations/0001_m2_account_foundation.sql:57-112`.

Three facts follow from that table and drive everything below.

1. **Hair colour and mastery are the cheapest win in the document.** Both columns already
   exist and are already loaded into `PlayerSnapshot`. All three construction sites hardcode
   zero (`crates/pangya-game/src/lib.rs:5276,6063,6654`). This is a wire change with no
   migration.
2. **Character stats have no column at all,** despite `RetailCharacter.stats` being five bytes
   the client renders. Adding them is a migration, not just a wire change.
3. **`EQUIPMENT_TRAILING_SLOTS` is described twice in the same file, differently.** Its
   definition (`us852_bootstrap.rs:23-31`) documents `skin_id[6]`, `skin_typeid[6]`,
   `mascot_id` and `poster[2]` — fifteen `u32`, matching `UserEquip` in SuperSS-Dev. Its
   encoder (`us852_bootstrap.rs:246-247`) instead says "background, frame, sticker, slot,
   unknown, title, and the four skin variants plus one further unknown". **`DPS-050` must
   resolve this contradiction from a live client before any of these slots is made durable.**
   Until then the trailing block stays zeroed.

## 4. Requirements

Requirement IDs are stable. Each names the milestone that delivers it and the
[`protocol/US852_SUBSYSTEM_GAPS.md`](protocol/US852_SUBSYSTEM_GAPS.md) §7 rank it inherits its
priority from.

### 4.1 Profile projection (`DPS-001`–`DPS-009`) — `E2`, no gap rank

| ID | Requirement |
|---|---|
| `DPS-001` | `RetailCharacter.hair_color` must be projected from `characters.hair_color` at bootstrap, in the member card, and in the room census — the three sites that construct the block. |
| `DPS-002` | `RetailCharacter.mastery` must be projected from `characters.mastery` at the same three sites. |
| `DPS-003` | `characters` gains five stat columns (power, control, accuracy, spin, curve), each `SMALLINT NOT NULL DEFAULT 0` with a nonnegative CHECK and an upper bound taken from the catalog's character record, not invented. `RetailCharacter.stats` projects them. |
| `DPS-004` | Until `DPS-003` lands, `stats` stays `[0; 5]`. A stat value the server cannot derive is never guessed. |

**Exit:** a real client renders a non-default hair colour and a non-zero mastery for an
operator-set account, and the value survives a restart and relog.

### 4.2 Equipment slot storage (`DPS-010`–`DPS-019`) — `E1`, gap rank 6

The current model is three nullable columns on `equipment_sets`. Widening it to sixty-odd
columns, as the reference schemas do
(`opensource-references/pangbox--server/migrations/0001_initial.sql` carries
`part00..part23_item_id`, `slot0..slot9_type_id`, `background_id`, `frame_id`, … as separate
columns), makes every future slot a migration and every read a wide row.

| ID | Requirement |
|---|---|
| `DPS-010` | One table `player_equipment_slots` keyed `(account_id, slot_family, slot_index)`, holding `inventory_item_id BIGINT` and an `item_type_id BIGINT` snapshot. `slot_family` is a closed CHECK set extended by forward migration as each family lands. |
| `DPS-011` | Ownership is enforced in the database, not in Rust: the composite FK `(account_id, inventory_item_id) → inventory_items(account_id, id)`, reusing the existing `uq_inventory_owner_id` index that `equipment_sets` already relies on. |
| `DPS-012` | A trigger must refuse a slot whose `inventory_items.inventory_class` does not match the family, mirroring `enforce_m7_equipment_classes` from migration 0008. |
| `DPS-013` | Slot writes participate in the **existing** `equipment_sets.version` optimistic counter. One equip request commits its `equipment_sets` row and its `player_equipment_slots` rows in one transaction against one `expected_version`. There is no second version counter. |
| `DPS-014` | The `item_type_id` snapshot is denormalized deliberately, so a projection can be built without joining `inventory_items` on the bootstrap hot path. A trigger keeps it consistent with the referenced row. |
| `DPS-015` | `equipment_sets.character_id`, `.club_item_id` and `.ball_item_id` are **not** migrated into the new table. They are load-bearing for a real-client-proven path (`evidence/REAL_CLIENT_EQUIPMENT_2026-08-09.md`) and moving them would put proven behaviour at risk for tidiness. |

**Exit:** caddie and mascot are selectable in a real client, survive restart, and appear in the
room census; `Caddie.iff` and `Mascot.iff` load through the existing catalog path with manifest
hashes.

### 4.3 Catalog prerequisites (`DPS-020`–`DPS-029`)

The manifest declares six tables (`local-data/us851-data/pak-iff/iff/manifest.toml`):
`Character`, `ClubSet`, `Ball`, `Item`, `Part`, `Course`. The client ships 39
([`data/US_CLIENT_IFF_STRUCTURE.md`](data/US_CLIENT_IFF_STRUCTURE.md)).

| ID | Requirement | Needs | Milestone |
|---|---|---|---|
| `DPS-020` | `Caddie.iff` and `Mascot.iff` load as new `CatalogKind`s with their family tags (`0x1c`, `0x40`) | manifest v3 entries | `E1` |
| `DPS-021` | `AuxPart.iff` loads | manifest v3 entry | `E2` |
| `DPS-022` | `Card.iff` loads (tags `0x7c`, `0x7d`) | manifest v3 entry | `E4` |
| `DPS-023` | Every new table goes through `parse_client_iff_bytes` unchanged — same SHA-256 manifest lock, same bounded reader, same `cap-std` sandbox. No family gets a bespoke parser. |
| `DPS-024` | Adding a table changes `Catalog::fingerprint()`, which is recorded in `matches.catalog_sha256` and `economy_operations.catalog_sha256`. Confirm before shipping that neither is compared on replay (`replay_purchase`, `crates/pangya-storage/src/economy.rs:757`), so historical rows stay valid. |

### 4.4 Character parts and aux parts (`DPS-030`–`DPS-039`) — `E2`, gap rank 14

> **Progress, 2026-08-09.** Two of the three blocking pieces are done.
>
> - **The wire is decoded.** `RetailEquipmentRequested::CharacterParts` used to carry *nothing* —
>   the 513-byte character block was read and dropped whole, which is why nothing downstream
>   could persist an outfit. It now yields character id, hair colour, and both 24-slot arrays,
>   at the offsets `pangbox/server` `pangya/player.go:141-159` documents. Two tests pin it: one
>   asserting slot 23 is not clipped, one asserting a one-byte-short body is refused rather than
>   decoding a shifted outfit.
> - **The table exists.** Migration `0011_character_part_slots.sql`, keyed
>   `(account_id, character_id, slot_index)` — per character, because each character wears its
>   own outfit and the update arrives inside that character's block. Ownership rides the
>   composite key so a row cannot attach one account's character to another's.
> - **Not wired.** No storage read or write, and the three `RetailCharacter` construction sites
>   in `pangya-game` still emit `[0; 24]`. The handler logs the decoded slot count so the gap is
>   visible in a live session rather than silent.
>
> Remaining: a repository load/save pair, filling those three sites from it, and a real-client
> run proving an outfit survives relog.


| ID | Requirement |
|---|---|
| `DPS-030` | 24 part slots stored as `slot_family = 'character_part'`, `slot_index 0..=23`. |
| `DPS-031` | Parts are per-character, not per-account. The key must therefore carry `character_id`, not only `account_id` — this is the one place the `(account_id, slot_family, slot_index)` key of `DPS-010` is insufficient, and the table takes a nullable `character_id` column that is `NOT NULL` for this family. |
| `DPS-032` | `Part.iff` already loads and already carries `character_part_slot` (`0..=7`) in `ItemDefinition`. Reconcile that 8-slot catalog notion with the wire's 24 before storing anything; do not assume they are the same axis. |
| `DPS-033` | 5 aux-part slots stored as `slot_family = 'aux_part'`. |
| `DPS-034` | `RetailEquipmentSlot::CharacterParts` (tag 0) currently reverts to the stored projection. It may only start acknowledging once `DPS-030`–`DPS-032` are durable. |

### 4.5 Consumable and decoration slots (`DPS-040`–`DPS-059`) — `E3`

| ID | Requirement |
|---|---|
| `DPS-040` | 10 consumable slots as `slot_family = 'consumable_slot'`. These reference **catalog** ids on the wire (`item_iff_ids`) but must store the owning `inventory_item_id`, so a slot cannot outlive the stack it points at. |
| `DPS-041` | Emptying the referenced stack must clear the slot in the same transaction. A slot pointing at a consumed row is corrupt state, not a cosmetic defect. |
| `DPS-042` | No consumable is debited merely because the client named a catalog id — restating [`PROGRESS.md`](PROGRESS.md) *Immediate next actions* item 1, which is a precondition for `0x0017`. |
| `DPS-050` | **Resolve the `EQUIPMENT_TRAILING_SLOTS` contradiction in §3 fact 3 from a live client before storing any of the 15 trailing `u32`.** Record the finding in `docs/evidence/`. |
| `DPS-051` | 6 decoration slots (background, frame, sticker, slot, cut-in, title) as `slot_family = 'decoration'`, matching `RetailEquipmentUpdated::Decoration([u32; 6])`. |
| `DPS-052` | Mascot as `slot_family = 'mascot'`, one slot, delivered with `E1` because its roster and catalog work is shared with caddie. |
| `DPS-053` | `RetailEquipmentSlot::UnknownEight` and `UnknownNine` stay unclassified until observed. [`protocol/US852_SUBSYSTEM_GAPS.md`](protocol/US852_SUBSYSTEM_GAPS.md) §7 item 3 proposes renaming them to mascot and cut-in; note that `Decoration` slot 5 is already documented as cut-in, so the proposal conflicts with the current model and must be settled by observation, not by adopting either name. |

### 4.6 Cards (`DPS-060`–`DPS-069`) — `E4`, gap rank 14

| ID | Requirement |
|---|---|
| `DPS-060` | 12 card slots per character as `slot_family = 'card'`, `character_id NOT NULL` per `DPS-031`. |
| `DPS-061` | Container `0x0138` must be emitted; per [`protocol/US852_SUBSYSTEM_GAPS.md`](protocol/US852_SUBSYSTEM_GAPS.md), a partial card implementation is visible at login, so this ships whole or not at all. |
| `DPS-062` | Cards depend on the `0x0216` user-status primitive (gap rank 5). Do not implement cards before it exists. |

## 5. Operator surface

Every slot family made durable becomes settable through the admin API described in
ADR-0016, under `PUT /admin/v1/accounts/:id/equipment`. Three rules apply without exception.

| ID | Requirement |
|---|---|
| `DPS-070` | An operator write takes the same locks and satisfies the same triggers as the in-game path. It never bypasses `equipment_sets.version`, and it always bumps it. |
| `DPS-071` | Every operator mutation writes one `admin_audit_events` row in the same transaction. |
| `DPS-072` | The panel must not present a slot as settable before its `DPS-*` requirement is durable. An unimplemented slot is shown as unavailable, never as an empty editable field. |

## 6. Operator deferrals tracked here

These are not slots, but they share the milestone and belong in one place.

| ID | Item | Why it is here |
|---|---|---|
| `DPS-080` | **Invalidate or disconnect an online player.** There is no server-side handle to an authenticated connection: identity lives on the connection task's stack, and `active_accounts` is a `CapacityRegistry<AccountId>` holding no player data (`crates/pangya-login/src/limits.rs:204`). Needs an account-addressed `LobbyCommand` (`crates/pangya-game/src/lobby.rs`). Until it exists, every operator edit lands on the player's next relog and the panel must say so. |
| `DPS-081` | **Automate `scripts/sync-client-shop.sh` from the panel.** The DB shop overlay changes what the server charges and permits; the client renders names, prices and listing from its own IFF ([`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md)). Only re-authoring the PAK changes the display. |
| `DPS-082` | **Expose the online-player set.** `CapacityRegistry` has `len()` but no `snapshot()`. A small addition powers the dashboard without giving the panel any player data it should not have. |

## 7. Milestones

Ordered to match [`protocol/US852_SUBSYSTEM_GAPS.md`](protocol/US852_SUBSYSTEM_GAPS.md) §7 so
the two documents cannot drift.

| # | Milestone | Requirements | Gap rank | Status |
|---|---|---|---:|:--:|
| `E1` | Caddie and mascot | `DPS-010`–`DPS-015`, `DPS-020`, `DPS-052`, `DPS-070`–`DPS-072` | 6 | ⬜ |
| `E2` | Character parts, aux parts, hair colour, mastery, stats | `DPS-001`–`DPS-004`, `DPS-021`, `DPS-030`–`DPS-034` | 14 | 🟡 |
| `E3` | Consumable and decoration slots | `DPS-040`–`DPS-042`, `DPS-050`, `DPS-051`, `DPS-053` | — | ⬜ |
| `E4` | Cards | `DPS-022`, `DPS-060`–`DPS-062` | 14 | ⬜ |

`E1` is first because it is disproportionately cheap: caddie equip already works on the wire,
and the gap analysis rates caddie and mascot "Tier D by name and Tier B by cost". It also
forces `player_equipment_slots` into existence with the smallest possible blast radius, which
every later milestone reuses.

**Exit evidence, per milestone.** Same standard as M3–M7: a real U.S. 852 client sets the slot
through its own UI, the value survives a full server restart and relog, the room census and
match roster project the same value, and the operator API sets the same slot with an audit row.
Written up in `docs/evidence/`, and recorded in [`PROGRESS.md`](PROGRESS.md).

## 8. Deferred and unresolved

| Item | Why |
|---|---|
| Rings `0x015d` | Two 852 sources agree the opcode exists; none documents the body. Needs client observation before it can be scoped. |
| Achievements | Gap analysis trap 5.8 is unresolved. Until it is, the correct implementation is the well-formed empty pair already served for daily quests. |
| Guilds, personal shop, club workshop, rentals, MyRoom furniture and UCC | Deferred on evidence, not effort — [`SPEC.md`](SPEC.md) §22.6 defers guilds explicitly, and the rest have conflicting or single-source documentation. |
| Wide per-slot columns as used by the reference schemas | Rejected in favour of `DPS-010`. Recorded so the decision is not silently revisited. |

## 9. Definition of done

[`SPEC.md`](SPEC.md) §27 applies unchanged. Two additions specific to this document:

- a slot is not durable until a **restart and relog** shows the same value, because the
  bootstrap is pushed once and the client caches it;
- a slot is not done until the operator API can set it **and** refuses to set it for an
  account that does not own the underlying inventory row.
