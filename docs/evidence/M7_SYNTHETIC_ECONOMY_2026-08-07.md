# M7 synthetic inventory, shop, and equipment evidence — 2026-08-07

## Claim boundary

This checkpoint is generated, local-only, and synthetic. The `0x7f40`/`0x7fc0` opcode
families, every layout, and every pricing, durability, stacking, and idempotency rule are
this project's own constructions. None has been accepted by, or captured from, a real
PangYa client. This file contains no M8 social/ranking or M9 parity implementation, and it
does not satisfy any retail gate.

## Requirement-to-implementation/test evidence

| Requirement | Implementation | Test |
|---|---|---|
| Disabled by default; enabling requires `game.enabled` and a consumable-bearing catalog | `configuration.rs` validation, `GameService::new` catalog check | `economy_defaults_disabled_and_validates_enablement_caps_and_timeout_relations`, `economy_composition_rejects_out_of_range_bounds_and_catalogs_without_consumables` |
| Every bound validated in config and again at composition | `configuration.rs`, `GameService::new` | both tests above |
| Catalog is the sole price authority | `handle_economy_command` resolves `shop_offer` before any repository call | `game_m7_encrypted_economy_is_catalog_priced_idempotent_and_restart_safe`, `game_m7_economy_reports_each_rejection_outcome_without_persisting` |
| Exactly-once by operation id, across restart | `pangya-storage` economy transactions, migration 0008 | `game_m7_encrypted_economy_is_catalog_priced_idempotent_and_restart_safe` |
| Replay with different parameters is drift, not a replay | storage-side operation comparison | `game_m7_economy_reports_each_rejection_outcome_without_persisting` |
| Optimistic equipment versioning | `EquipmentChange.expected_version` | `game_m7_economy_reports_each_rejection_outcome_without_persisting` |
| Disabled economy decodes, refuses, persists nothing, closes nothing | `handle_economy_command` early return | `game_m7_disabled_economy_refuses_each_command_without_closing` |
| Per-connection command budget | `LocalRateWindow` over `commands_per_window` | `game_m7_economy_commands_are_bounded_per_connection` |
| Repository deadline yields `Timeout` and commits nothing | `timeout(economy.command_timeout, …)` | `game_m7_economy_command_deadline_reports_timeout_without_persisting` |
| Economy opcodes require authentication and channel entry | opcode routing state check | `game_m7_economy_opcodes_require_authentication_and_channel_entry` |
| Fixed-label metrics with no identifiers | `GameEconomyCommand`/`GameEconomyOutcome`, `M2Metrics` | `pangya-observability` render test, plus metric assertions in the rejection E2E |

## Outcome matrix

All ten wire outcomes are exercised over encrypted TCP against real PostgreSQL:

| Outcome | Proven by |
|---|---|
| `Success` | purchase, consume, repair, equip, shop page |
| `Disabled` | all five commands against an uncomposed economy |
| `Invalid` | page past `total_pages`; quantity above the configured cap; `type_id` present in catalog but not sold |
| `NotOwned` | consume of an inventory row the account does not hold |
| `Incompatible` | consumable offered into the club-set equipment slot |
| `InsufficientPang` | 500-Pang club against a 25-Pang balance |
| `StackFull` | 100th unit of a `max_stack = 99` consumable |
| `VersionConflict` | equip with `expected_version = 7` against version 0 |
| `IdempotencyDrift` | operation id replayed with a different quantity |
| `Timeout` | stalling repository against a 100 ms deadline |

Storage, arithmetic-overflow, and corrupt-data repository errors are deliberately absent
from this table: they are not wire outcomes. They terminate the connection as
`EconomyPersistence`, so a client is never told a failed write succeeded.

## Ledger and balance assertions

The end-to-end flow asserts exact durable state rather than only packet shapes. After a
purchase, an idempotent replay, two consumes, a club purchase, a repair, and an equip:

- `economy_operations` = 6, `shop_currency_ledger` = 3, `item_ledger` = 5,
  `equipment_ledger` = 1
- `profiles.pang` = 4420, and the same value is re-read after a full service restart
- the idempotent replay returns a byte-identical `PurchaseCommitted` and moves no balance

Rejection paths assert the complementary property: after nine rejected commands the
balance reflects only the two successful purchases.

## Generated fixture hashes

| Fixture | SHA-256 |
|---|---|
| `m7-in-shop-page-synthetic` | `82e7288f4d16184bc0013b2f468d638ddc0574b8229a7916358f065cd3277afa` |
| `m7-in-purchase-synthetic` | `87826cc53a466e302589ce1359ec764db6930b923f7aed26865f00e109c22b34` |
| `m7-in-equip-synthetic` | `3c2df4ab8ca2fd46e96efba66b69731c7b59daa0a6cc33f2d518f47dea8d89cb` |
| `m7-in-consume-synthetic` | `60981183a25789576f6be3fc50a4789506843a1c9f0d1ad169823c415b587081` |
| `m7-in-repair-synthetic` | `741fe6e67929e00f83cdabd9a2cd4cf07eeadb557e9532f2c087849e2721f2dc` |
| `m7-out-shop-page-synthetic` | `9c8a4c090acf2203478cfbe203f2705b451115fb5e6fcef7e5e2292dbc6274fe` |
| `m7-out-command-result-synthetic` | `1b554ba479c1d2d86631a630a2166a406a2a8c47d1ebbdda65750370e2b69309` |
| `m7-out-purchase-committed-synthetic` | `5498e6c776ac153e34a479314c52e9db431582350ab9ba2c9236c0eb5bda4dfe` |
| `m7-out-inventory-changed-synthetic` | `dda9489665eabbd56b11c947c44a698d27bf6d60863b64442630e0a61d541768` |
| `m7-out-equipment-changed-synthetic` | `834387321493732267720969c73c1bfe759bb4df837193ea6bdea198ed0f1551` |
| `m7-out-repair-committed-synthetic` | `ec0094166f797a5e232eddde2d4131b5e263e21dcf7aae7d0b3921461c325234` |

The `synthetic-catalog-v2` fixture records per-file SHA-256 values in its own
`manifest.toml`. Its shop offers are `0x0800_0001` (character part), `0x1000_0001` (club
set, 500 Pang, unique, durable 100 at 3 Pang per point), `0x1800_0001` (ball), and
`0x1a00_0001` (consumable, stackable to 99). `0x1a00_0002` exists in the catalog and is
deliberately not sold, which is what makes the not-an-offer rejection testable.

## Database migration

Migration `0008_m7_synthetic_economy.sql` adds the economy operation ledger, currency
ledger, item ledger, and equipment ledger, plus the inventory and equipment state the
runtime mutates. All eight migrations apply cleanly from an empty database.

## Configuration, observability, and security

`[game.economy]` is documented in [`../CONFIGURATION.md`](../CONFIGURATION.md). Metrics are
two fixed-label counters, `pangya_game_economy_commands_total{command}` (5 series) and
`pangya_game_economy_outcomes_total{outcome}` (10 series). Neither carries an account,
item, inventory, or operation identifier, so cardinality is bounded at 15 series and no
player-identifying value reaches the metrics endpoint.

## Test inventory and validation state

| Suite | Count |
|---|---|
| `pangya-game` library | 74 |
| Game E2E against real PostgreSQL | 26 |
| `pangya-storage` PostgreSQL | 53 |
| `pangya-server` library | 27 |
| M7 protocol | 5 |
| **Workspace total** | **330 passed, 1 ignored** |

Validated locally on 2026-08-07 against PostgreSQL 17 in a container matching the CI
service definition:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo sqlx migrate run` and `cargo sqlx prepare --workspace --check -- --all-targets --all-features` (no drift)
- `cargo test --workspace --all-features --locked`
- `./scripts/check-proprietary-assets.sh`

## External retail gate

Unchanged and open. This checkpoint does not attempt it. The gate requires a legally held
U.S. client and legally supplied data to accept these exchanges, and the synthetic opcode
family must first be replaced by reference-derived retail layouts. The client acquired for
that work is characterized in
[`US_CLIENT_ACQUISITION_2026-08-07.md`](US_CLIENT_ACQUISITION_2026-08-07.md). No synthetic
opcode, price, durability rule, or ledger shape in this checkpoint is a retail claim.
