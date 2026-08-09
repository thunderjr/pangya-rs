# Real U.S. 852 client shop-flag evidence — 2026-08-10

## Scope

This run establishes three things a real client confirmed on the same day:

1. `0x68` in an IFF record is `ShopFlag` alone. The currency is a **separate byte** at `0x69`
   (`MoneyFlag`). The "low nibble is currency" model this project used until now was wrong.
2. Bit `0x20` of `ShopFlag` is what makes the client's shop list a row. A priced row without it
   is purchasable by the protocol and invisible on screen.
3. The client does **not** mount an archive numbered past its own patch level, so an authored
   archive must rebuild one the client already mounts.

Generated client data stays under ignored `local-data/`; no proprietary payload is committed.

## 1. What was observed

After the 2026-08-09 widening to 9,235 offers the operator reported the shop showing only a
fraction of them, and named the missing sets: **SSAF**, **wings**, **rings**.

Those items were not missing from the server. The console reported every one of them as sellable
at 1 Pang with no drift:

```
0x08006024 SSAF Suit (N)           client=1 server=1 drift=false
0x0800082a Light-Cross Wing (N)    client=1 server=1 drift=false
0x08008824 Golden Knuckle Ring(N)  client=1 server=1 drift=false
```

So both halves of the "client shows and server allows" gate reported agreement while the client
drew nothing. The gate itself was measuring the wrong thing.

## 2. The header, from the references

`opensource-references/pangbox--server/pangya/iff/item.go` (`ItemV11_78`, `ItemV11_98`):

```
/* 0x5C */ Price          uint32
/* 0x60 */ DiscountPrice  uint32
/* 0x64 */ Condition      uint32
/* 0x68 */ ShopFlag       byte
/* 0x69 */ MoneyFlag      byte
/* 0x6A */ TimeFlag       byte
/* 0x6B */ TimeByte       byte
```

`ShopFlag` is one field and `MoneyFlag` is another. Reading a currency out of `ShopFlag`'s low
nibble — which `scripts/author-client-iff.py` and `docs/SPEC_SHOP_COVERAGE.md` SHOP-003 both did
— was decoding a field that is not there.

Confirmed independently from the data: in the pristine `Part.iff`, `ShopFlag = 0x21` occurs
against `MoneyFlag` 0 (1,149 rows), 1 (273) and 2 (33). A nibble that varies freely against the
real currency byte is not a currency.

## 3. Which bit lists a row, measured

Pristine U.S. 851 tables, from `local-data/us851-data/pak-iff/pangya_gb.iff`. The client marks a
row that is not really for sale with the price `10,000,000`
(`CLIENT_UNAVAILABLE_PRICE`). Cross-tabulating that marker against bit `0x20`:

| Table | `0x20` set | …priced 10,000,000 | `0x20` clear | …priced 10,000,000 |
|---|---:|---:|---:|---:|
| `Part.iff` | 2,036 | **0** | 5,289 | 4,365 |
| `Ball.iff` | 20 | **0** | 67 | 31 |
| `ClubSet.iff` | 23 | **0** | 60 | 56 |
| `Item.iff` | 60 | **0** | 328 | 260 |

Zero exceptions in four independent tables: the unavailable sentinel and the listed bit never
co-occur. `0x20` alone is also attested as a complete flag — 104 pristine `Skin.iff` rows and 9
`Part.iff` rows carry exactly it — so authoring a never-sold row to `0x20` writes a byte the
retail client already renders.

## 4. What the old model produced

Under the nibble model, authoring cleared what it believed was a currency and left the listed bit
untouched — which for most rows meant clear. Of 9,235 authored offers:

- **6,624** carried a `ShopFlag` with bit `0x20` clear
- SSAF authored `0x06` → `0x02`, wings `0x02` → `0x02`, rings `0x00` → `0x02`

The server sold all 9,235 because `pangya-data` treated *any* non-zero flag as sellable, so
nothing in the system could report the discrepancy. The console's `drift` column compared server
price against client price and found them equal — both were 1 Pang. Neither side was asking
whether the client would draw the row.

## 5. The fix

`scripts/author-client-iff.py`:

- `pang_shop_flag` returns `original | 0x20`, preserving every other bit. The currency branches
  are gone.
- `MoneyFlag` at `0x69` is written as `0`. **1,240** offers had a non-Pang retail currency and
  would otherwise have been listed at 1 Pang and charged in Points.
- Authoring **refuses** to report an offer whose authored flag lacks the listed bit.

`crates/pangya-data/src/lib.rs`: `ItemSale` now requires `CLIENT_SHOP_LISTED_BIT`, so the server's
sale test is the client's. This is the part that makes the failure impossible to repeat silently
— under the old rule the two halves could not disagree *visibly*, because they were not asking
the same question.

## 6. The published archive

Authored from the console's own document (see PATCH-009), `PANGYA_PATCH_MODE=replace`:

| Artifact | Value |
|---|---|
| base archive | `de7a00a64ada6d4effd4c555cab3f36a475e1415e27fdb9743da5ae954a3d82b` |
| authored `pangya_gb.iff` | `586b3fae0e8acb1047bf12c5cbf9a7058e335bad4284cc480fbdb191b5c4de04` |
| deployed `projectg851gb.pak` | `4712d19d4f4ec85b21946504c551011935987effffd10e0682d06a8570936d85`, 1,425,116 bytes |
| server `manifest.toml` | `8aed7bf59c0db60c85e0cb835b61cf606b39e630d33794366ca19a3ca99c7e79` |

9,235 offers across 12 tables: `Part.iff` 7,325 · `SetItem.iff` 595 · `Item.iff` 388 ·
`Skin.iff` 289 · `Card.iff` 180 · `HairStyle.iff` 110 · `Ball.iff` 87 · `ClubSet.iff` 83 ·
`CaddieItem.iff` 83 · `Mascot.iff` 34 · `Caddie.iff` 32 · `Furniture.iff` 29.

**6,636** rows gained the listed bit. **0** authored offers lack it. The server reported
`sold_count: 9235` under the new, stricter parse — every row qualifies on the client's own rule.

The startup cross-check (`client_web.publish_report`) agreed, and
`GET /launcher/v1/manifest` served 84 archives with `stale: false`.

## 7. Operator confirmation

The operator launched the U.S. 852 client on a Windows machine on the tailnet, pulled the archive
through the launcher, and confirmed the shop now shows the previously missing items. The three
sets named as missing before the fix are the three that were checked after it.

## 8. Archive ordering, measured the same week

Recorded here because it was disproved in the same investigation. On 2026-08-09 the shop was
published as a **new** one-entry `projectg852gb.pak` sitting past the retail series, leaving
`projectg851gb.pak` pristine at 690,312 bytes. The archive was built, hashed, served and listed
in the manifest correctly. The client ignored it and the shop showed retail prices.

So the earlier "the last archive in the series wins" rule holds only *within the set the client
mounts*, and that set ends at the client's own patch level. `PANGYA_PATCH_MODE=latest` is
retained but must not be selected without a fresh evidence run. See
`docs/SPEC_CLIENT_PATCH_DELIVERY.md` PATCH-001.

## 9. What this run does not establish

- **Whether all 9,235 render.** The listed bit is necessary; it is not sufficient. 3,554 rows
  were never client shop rows and may lack an icon, a `Desc.iff` entry, or sit above the
  hardcoded `level: 1`. See SHOP-004 and SHOP-005. SSAF, wings and rings all carry real retail
  metadata, which is why they were the right things to check first.
- **Whether the eight widened families are purchasable.** Caddie, CaddieItem, Mascot, Card,
  Furniture, Skin, HairStyle and SetItem now list, but no purchase from those tabs has been
  completed against a real client.
- **What the other `ShopFlag` bits mean.** `0x01`, `0x02`, `0x40` and `0x80` are preserved
  untouched precisely because they are unidentified.

## Reproduction

```bash
python3 -m unittest discover -s scripts/tests   # the listed bit, exhaustive over the byte
cargo test -p pangya-data                       # the server applying the same bit
scripts/publish-shop.sh --dry-run               # what a publish would author
```
