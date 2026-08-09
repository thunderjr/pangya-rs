# Real U.S. 852 client custom-shop evidence — 2026-08-09

## Scope

This run proves that a custom-authored IFF archive is loaded from the client's actual mounted PAK,
that the server catalog comes from the same archive, and that the retail purchase path applies the
server-authoritative Pang price atomically and survives restart. Generated client data remains
under ignored `local-data/`; no proprietary payload or screenshot is committed.

## One-source build and manual synchronization

The pristine `projectg850gb.pak` was copied from the supplied backup and verified before use:

- bytes: `1,625,279`
- SHA-256: `d18fffcb151406383ed8ebecf74f8dae5821b27bec0862ad6922cec8ce4cc6b3`

`scripts/sync-client-shop.sh` invoked `scripts/author-client-iff.py` once to write the authored ZIP,
server tables/manifest, and replacement PAK. The final output report recorded:

- authored `pangya_gb.iff` SHA-256: `9997e9e5eee1781b43a29a83891ad10db8af22eeb9b0a9882d3a745bdc841658`
- authored `projectg850gb.pak` SHA-256: `0514b20f9bffb8034b8ac2729aca851ba67bd2ac835c0f1389172f6eab4c1cc8`
- server `manifest.toml` SHA-256: `3df391c03d76aa84098c05223ae665eb783ef46263d3e5bb1b1f0ad162287364`

The Windows install reported the same uppercase PAK hash after download. The client launched without
the retail corruption dialog, and `/health/ready` returned `{"status":"ok"}` after the server was
restarted against `local-data/custom-shop/iff-gb`.

## Visible authored catalog

The unmodified client rendered all four authored offers with the exact Pang denomination and
price:

| Client category | Type ID | Retail name | Price |
|---|---:|---|---:|
| Clothes / Accessories | `0x08000800` | Cowboy Hat | 4,321 Pang |
| Item / Club Sets | `0x10000061` | Papel Training Club Set | 1,234 Pang |
| Item / Comet | `0x140000c9` | Cobra Comet (50) | 77 Pang |
| Item / Active Items | `0x18000000` | Spin Mastery | 5 Pang |

The Ball row was originally a Points row (`ShopFlag 0x21`). Authoring changed only its low money
nibble to the non-tradeable Pang form (`0x20`), while the other rows retained their existing Pang
forms (`0x22`, `0x60`, and `0x62`). Unknown currency nibbles are refused rather than guessed.

## Retail purchase and authority boundary

The reusable Windows flow `Invoke-PangyaShopFirstItemPurchase` performed both required modal
confirmations. For Cobra Comet the client displayed:

- current balance: 8,766 Pang
- cost: 77 Pang
- end balance: 8,689 Pang
- point cost: 0

The retail packet decoded as type `0x140000c9`, retail package quantity `50`, claimed Pang `77`,
and claimed Points `0`. The server ignored both claimed prices, resolved the row from its catalog,
normalized the retail 50-ball package to one durable Ball inventory row, and charged exactly 77.
Post-commit storage was:

```text
pang  inventory_id  type_id    quantity  inventory_class  result_pang_cost  result_pang_balance
8689  311           140000c9  1         ball             77                8689
```

The matching currency ledger contained one `purchase` delta of `-77` for the same operation ID.
A second intentional UI purchase used a new retail frame identity and correctly created a second
Ball row and charged another 77; this is a new purchase, not a replay.

## Persistence and replay resistance

After rebuilding/restarting GameService and fully relogging the retail client, the lobby header
still displayed `8,612` Pang, matching PostgreSQL. Both Ball inventory rows and both `-77` ledger
entries remained present.

Retail opcode `0x001d` has no application operation UUID. GameService therefore assigns a
connection-scoped sequence to each distinct bounded `(client salt, plaintext payload digest)` wire
identity. An exact repeated frame reuses the sequence and derives the same economy operation ID;
a later intentional client purchase with a new wire identity derives a new ID. Tests prove:

- exact wire keys reuse their sequence;
- a new salt or payload receives a new sequence;
- the replay map is bounded and evicts oldest entries;
- operation IDs are stable for replay and distinct across purchase sequences and basket lines;
- the storage economy path returns the committed result for an identical operation instead of
  inserting another inventory/currency/item ledger mutation.

The live purchase also established the client/server authority split: retail-claimed prices were
logged for diagnosis but never used to calculate the debit.
