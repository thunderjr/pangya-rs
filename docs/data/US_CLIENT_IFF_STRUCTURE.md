# U.S. client IFF structure — measured 2026-08-07

## Claim boundary

This file records structural facts measured from the acquired U.S. client's own data
files. It contains no client bytes, no record contents beyond the few field values needed
to state a structural finding, and no extracted asset. The client and everything unpacked
from it stay under gitignored `local-data/`.

These findings supersede the "unattested" caveats in
[`M3_SYNTHETIC_CATALOG.md`](M3_SYNTHETIC_CATALOG.md) for the specific facts stated here.
They are measurements of a client's data, not evidence that this server's packet layouts
are correct.

## How the catalog is packaged

The client ships a `gb`-suffixed PAK series — a base `projectg700gb+.pak` plus incremental
patches `701`–`730` and `801`–`851`, treated as one incremental archive where later
members override earlier ones. Extracted with the vendored `pangbox--pangfiles`
`pak-extract -region us`.

Inside, the catalog is `data/pangya_gb.iff`. **That file is a ZIP container**, not a single
table: it holds 39 per-family `.iff` tables totalling 8,320,580 bytes. Standard `unzip`
skips its entries as "volume label" because of how their attributes are set; reading them
through a permissive ZIP reader works.

The 39 tables are `Character`, `Part`, `Club`, `ClubSet`, `Ball`, `Item`, `Caddie`,
`CaddieItem`, `SetItem`, `Course`, `Match`, `Enchant`, `Desc`, `Skin`, `HairStyle`,
`Mascot`, `CounterItem`, `AuxPart`, `QuestStuff`, `QuestItem`, `Card`, `Furniture`,
`CadieMagicBox`, `CadieMagicBoxRandom`, `FurnitureAbility`, `TikiRecipe`,
`TikiPointTable`, `TikiSpecialTable`, `CutinInfomation`, `TimeLimitItem`,
`SpecialPrizeItem`, `ShopLimitItem`, `PointShop`, `NonVisibleItemTable`,
`SubscriptionItemTable`, `TwinsItemTable`, `ScratchRewardSetting`, `LevelUpPrizeItem`, and
`ErrorCodeInfo`.

This means catalog loading needs a container step the synthetic model does not have: the
manifest names tables inside one ZIP, not sibling files on disk.

## The header format is confirmed

`M3_SYNTHETIC_CATALOG.md` models the header as LE `count:u16`, `binding:u16`, `version:u32`
with total length `8 + count * record_size`.

**This holds exactly across all 39 real tables.** Every table with `count > 0` yields an
integer `record_size` from `(len - 8) / count`. The two exceptions, `FurnitureAbility` and
`ScratchRewardSetting`, are 8 bytes long with `count = 0` — header only, which is the same
rule with an empty body rather than a counterexample.

`version` is uniformly **13** across every table.

`binding` is **not** a family discriminator. It is 0 for most core tables, 65 for many
smaller and later-added tables, and 27424 for `CadieMagicBox`. The synthetic model assigns
each family its own sequential binding; real data does not work that way, so a loader must
not derive family identity from it.

Record sizes vary widely by family and are not multiples of a common stride — `Character`
380, `Part` 524, `Club` 204, `ClubSet` 192, `Ball` 772, `Item` 208, `Caddie` 208, `Course`
320, `Desc` 516.

## The record layout assumption is wrong

`M3_SYNTHETIC_CATALOG.md` states: "The first four bytes of every record are a LE `u32`
`type_id`, globally unique across all declared families."

**Measurement contradicts this.** The real record begins with a `u32` activity word at
offset 0, and the `type_id` is at offset **4**. Across all populated tables the offset-0
word takes values 1 (7802 records), 0 (1279), and a long tail of 2, 4, 5, and 101.

**Zero means inactive.** This was confirmed by testing the rule across seven tables:
skipping every record whose offset-0 word is zero leaves *all* remaining records correctly
family-tagged, with no exceptions. Inactive rows carry sentinel type IDs that respect no
family tag — `Item`'s dummy row is `0x17ffffff`, one family below the `0x18` it would
otherwise need — so a loader that does not skip them will reject a valid table. The
distinction between the non-zero values (1, 2, 4, 5, 101) is still unexplained; all of them
mark active records.

Per-table inactive counts: `ClubSet` 3 of 60, `Ball` 26 of 85, `Item` 106 of 316, `Part`
724 of 6087, `Course` 1 of 21, `Caddie` 3 of 30, `Character` 0 of 10.

A fixed-size name string follows at offset 8.

Any loader ported to real data must read `type_id` at record offset 4. Reading offset 0
yields a near-constant 1 for almost every record in the catalog.

## Family tags are the high byte of `type_id`

The top byte of the offset-4 word identifies the family, confirming the general scheme the
synthetic fixture assumes:

| Table | Family tag(s) |
|---|---|
| `Character` | `0x04` |
| `Part` | `0x08` |
| `Club` | `0x0c` |
| `ClubSet` | `0x10` |
| `Ball` | `0x14` |
| `Item` | `0x18` and `0x1a` |
| `Caddie` | `0x1c` |
| `Course` | `0x28` |
| `Skin` | `0x38`, `0x39` |
| `Mascot` | `0x40` |
| `Card` | `0x7c`, `0x7d` |

Checked against the `synthetic-catalog-v2` fixture:

- character `0x04`, character part `0x08`, and club set `0x10` **match** the real tags.
- consumable `0x1a` **matches** a real `Item` tag; `Item` spans both `0x18` and `0x1a`.
- ball `0x18` is **wrong**. The real `Ball` family is `0x14`; `0x18` belongs to `Item`.

A single table may span more than one family tag, so a loader cannot assume one tag per
table.

## Implemented

`CLIENT_MANIFEST_VERSION = 3` in `pangya-data` implements all of the above: `type_id` at
record offset 4, family identity from the high byte with a table allowed to span several
tags, inactive rows skipped on the offset-0 rule, and record width taken from header
arithmetic rather than a per-family constant. `binding` is still compared against the
manifest for change detection but is deliberately not used to derive family identity.

The ZIP container is handled as an operator extraction step rather than in-process, which
keeps a new decompressor out of the parsing path. See
[`../RUNNING_THE_CLIENT.md`](../RUNNING_THE_CLIENT.md).

Schema 3 is covered in CI by `tests/fixtures/synthetic-client-v3`, a generated fixture in
the real record format containing no client bytes. It has been exercised against the real
tables, which load with correct type IDs across all six declared families.

## Still unestablished

Why the active word takes 2, 4, 5, or 101 rather than 1; the field layout inside each
record beyond `type_id` and the name string — which is what real prices, stack limits, and
durability need; the semantics of `binding`; and the relationship between `pangya.iff` and
`pangya_gb.iff`. **Course par is not in `Course.iff`**, which is only an id-to-name table,
so one-hole configuration for real courses still has no source.

Nothing here says anything about packet layouts; catalog structure and wire protocol are
independent questions, and the wire side remains gated on the retail layout port.
