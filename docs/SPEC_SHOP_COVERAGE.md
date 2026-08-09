# Shop coverage

What this server sells, what it deliberately does not, and what still refuses. Companion to
[`SPEC.md`](SPEC.md) §22 (economy requirements) and
[`SPEC_DURABLE_PLAYER_STATE.md`](SPEC_DURABLE_PLAYER_STATE.md) (what persists once bought).

Status legend matches [`PROGRESS.md`](PROGRESS.md): ✅ done · 🟡 partial · ⬜ not started ·
⛔ blocked · 🔬 needs a real-client evidence run.

## Claim boundary

Everything here describes the server's catalog and the authored client tables. Only one property
has been in front of a real U.S. 852 client: that an authored archive changes what the shop
displays and what the server charges (2026-08-09, confirmed by the operator at 1 Pang across the
four original sellable tables). The widened families below are **🔬 unverified against a client**.

## The two gates

A purchasable item must clear both, and they live in different places:

| Gate | Where | What decides it |
|---|---|---|
| **client shows** | the IFF row inside the client's PAK series | bit `0x20` of the shop flag at `0x68`, price at `0x5c` |
| **server allows** | the parsed catalog (`data.iff_directory`) plus the DB shop overlay | `ItemDefinition::sale`, which applies the same bit |

### The record header, corrected

`0x68` is `ShopFlag` and nothing else. The currency is a **separate byte** at `0x69`
(`MoneyFlag`), followed by `0x6a` `TimeFlag` and `0x6b` `TimeByte` — the layout in
`opensource-references/pangbox--server/pangya/iff/item.go`.

This project previously read `0x68` as a packed byte: low nibble currency, upper nibble display
state. That model produced a working server and a shop that looked almost empty. Authoring
cleared what it thought was a currency nibble and left bit `0x20` unset on **6,624 of 9,235**
offers, so the server sold them, the console reported them, and the client drew none of them —
SSAF, wings, rings and most of `Part.iff`. Nothing reported the gap, because the server's own
sale test accepted any non-zero flag.

Bit `0x20` means *listed in the shop*, measured rather than assumed: across the pristine U.S.
tables a row carrying it is **never** priced at the 10,000,000 unavailable sentinel, and a row
without it almost always is (Part.iff 2,036 set / 0 sentinels versus 5,289 clear / 4,365).
Authoring now sets that bit and preserves every other, writes `MoneyFlag = 0` so a Pang price is
charged in Pang, and **refuses** to report an offer whose authored flag lacks the bit.
`pangya-data` applies the same test, so the two halves can no longer disagree about what is on
sale.

`scripts/author-client-iff.py` writes both halves from one authored ZIP, so within a run they
cannot disagree. Deployment skew is caught at startup by `client_web.publish_report`
(`crates/pangya-server/src/publish_report.rs`), which refuses to boot and names the stale side.

## Coverage today

The client ships **57** IFF tables. The catalog parses **14**.

| Family | Table | Rows | Status |
|---|---|---:|---|
| character | `Character.iff` | 14 | 🟡 parsed, **not sellable** — see SHOP-001 |
| club set | `ClubSet.iff` | 83 | ✅ |
| ball | `Ball.iff` | 87 | ✅ |
| consumable | `Item.iff` | 388 | ✅ |
| character part | `Part.iff` | 7,325 | ✅ |
| course | `Course.iff` | 21 | ✅ parsed, never sellable (not an item) |
| caddie | `Caddie.iff` | 32 | 🔬 |
| caddie item | `CaddieItem.iff` | 83 | 🔬 |
| mascot | `Mascot.iff` | 34 | 🔬 |
| card | `Card.iff` | 180 | 🔬 |
| furniture | `Furniture.iff` | 29 | 🔬 |
| skin | `Skin.iff` | 289 | 🔬 |
| hair style | `HairStyle.iff` | 110 | 🔬 |
| set item | `SetItem.iff` | 595 | 🔬 |

**9,235** rows are currently authored as sellable. The eight lower families were added because
three of the client's six shop tabs — Caddie, Mascot, Decoration — had no server catalog behind
them at all, so every purchase from them was refused with `not_in_catalog` while the client
displayed the items perfectly happily.

## Requirements

### SHOP-001 — characters must be purchasable ⛔

`inventory_class_text` returns `EconomyError::Invalid` for `ItemKind::Character`
(`crates/pangya-storage/src/economy.rs`), and `ck_inventory_class` excludes it. This is not an
oversight: an owned character is a row in `characters` with its own `hair_color`, `mastery` and
`starter_key`, not a row in `inventory_items`. Selling one needs a destination the economy commit
path does not have.

Exit criteria: a purchase of a `0x04……` type id commits a `characters` row inside the same
transaction as its `economy_operations` and ledger rows, with an acquisition key distinguishable
from a starter grant; the client's Char tab completes a purchase; the bought character is
selectable.

Blocked on a decision this spec does not make: whether the character purchase reuses
`purchase_economy` with a branch, or becomes a sibling command with its own ledger authority row.

### SHOP-002 — `AddonPart.iff` is excluded ⛔ (by decision)

Its 61 rows carry type-id tags `0x04` and `0x08` — the same space `Character` and
`CharacterPart` occupy. Admitting it would make `Catalog::find_record` ambiguous for the sake of
**3** shop rows. Revisit only if the client is shown to disambiguate by table rather than by tag.

### SHOP-003 — the "unknown currency nibbles" did not exist ✅ resolved

Previously recorded as: 298 rows carry a low nibble of `0x3` or `0x6` whose currency meaning is
unidentified. There was nothing to identify — that nibble is not a currency. `0x06` is a shop
flag with bits 1 and 2 set and the listed bit clear; the currency is `MoneyFlag` at `0x69`, which
is independent of it (Part.iff pairs flag `0x21` with MoneyFlag 0, 1 and 2 alike).

Authoring now preserves those bits and sets `MoneyFlag = 0`. The 1,240 rows whose retail currency
was not Pang are converted explicitly rather than left to be listed at a Pang price the client
would try to charge in Points.

### SHOP-004 — inventing metadata for never-sold rows 🟡

**3,554** rows in the four original tables were never client shop rows (`shop flag == 0`). The
`invent_shop_metadata` opt-in gives them `0x20` — the listed bit alone, attested verbatim on 104
pristine `Skin.iff` rows and 9 `Part.iff` rows. What is *not* known is whether such a row looks
right: quest and reward rows may have no icon, no `Desc.iff` entry, or a `minLevel` that hides
them.

Exit criteria: a real client run reporting how many enabled rows actually render, and what the
unrenderable ones have in common.

### SHOP-005 — `minLevel` is unmodelled 🟡

Every record carries a minimum level at `0x30`. The retail bootstrap hardcodes `level: 1`
(`crates/pangya-game/src/lib.rs`), so any row above level 1 may be hidden client-side regardless
of its shop flag. Measured spread: ~500 of 7,883 rows in the original four tables sit above
level 1, concentrated in `Part.iff`.

Exit criteria: either a real player level on the wire, or a documented decision to zero
`minLevel` in authored tables.

### SHOP-006 — the shop is operator-controlled on both halves ✅

`shop_offer_overrides` changes what the server charges and permits, live, with no restart. It
still never changes what the client displays by itself — but the console can now publish, which
re-authors the client's own tables from that same overlay and ships the archive. Rows where the
two disagree are flagged `drift` until then, and the publish panel says whether players' clients
are behind the server. Mechanism, worker and evidence: PATCH-009 in
[`SPEC_CLIENT_PATCH_DELIVERY.md`](SPEC_CLIENT_PATCH_DELIVERY.md).

Two things a reader should not over-read. The panel reaches the client's *shop tables*, not the
client's behaviour: an item enabled here still needs a server-side purchase path, which is why
SHOP-001 stays blocked. And the publish base is the last published set, so clearing an override
keeps the published price rather than restoring the retail one.

### SHOP-007 — item icons are 90% covered 🟡

`build-icons.sh` resolves the icon stem at record offset `0x31` against the client's extracted
texture tree: **7,127 of 7,918** records in the original six tables, 4,378 distinct files. The
gap is rows with an empty icon field and rows naming a texture absent from the extraction. The
console draws a family glyph instead. The eight widened families have not been measured.

## Verification

1. `python3 -m unittest discover -s scripts/tests` — the authoring rules, including the
   `invent_shop_metadata` opt-in and the refusals it does not relax.
2. `cargo test -p pangya-data` — family tags, manifest validation, fingerprint stability.
3. A real-client run per family: open the tab, confirm rows render, buy one, confirm the charge
   matches the catalog and an `economy_operations` row exists. **Not yet done for the eight
   widened families.**
