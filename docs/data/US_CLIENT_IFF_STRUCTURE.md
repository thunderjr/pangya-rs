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

**Measurement contradicts this.** The real record begins with a small-valued `u32` at
offset 0, and the `type_id` is at offset **4**. Across all populated tables the offset-0
word takes values 1 (7802 records), 0 (1279), and a long tail of 2, 4, 5, and 101. Its
meaning is not established here; it is clearly not an identifier, since it is neither
unique nor family-tagged. Records whose `type_id` is a dummy sentinel tend to carry 0.

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

## Consequences for `pangya-data`

1. Add a ZIP container step; the manifest must address tables inside `pangya_gb.iff`.
2. Read `type_id` at record offset 4, not 0.
3. Do not derive family identity from `binding`; derive it from the `type_id` high byte,
   and allow a table to carry several tags.
4. Correct the ball family tag in the synthetic fixture to `0x14` so generated fixtures
   stop teaching a wrong tag, and keep the fixture's own hashes updated when doing so.
5. Record sizes are per-family constants to be read from the header arithmetic, never
   hardcoded.

## Still unestablished

The meaning of the offset-0 word, the full field layout inside each record beyond
`type_id` and the name string, the semantics of `binding`, and the relationship between
`pangya.iff` and `pangya_gb.iff` are all unmeasured. Nothing here says anything about
packet layouts; catalog structure and wire protocol are independent questions, and the
wire side remains gated on the retail layout port.
