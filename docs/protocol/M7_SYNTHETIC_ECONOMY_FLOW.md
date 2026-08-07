# M7 local synthetic inventory, shop, and equipment flow

> **Not a retail claim.** Every opcode and layout below is generated for this project.
> The `0x7f40`/`0x7fc0` families are provisional local values chosen to avoid colliding
> with observed U.S. 852 opcodes. No byte in this document has been accepted by, or
> captured from, a real PangYa client. Nothing here may be described as retail protocol.

The economy is disabled by default. It is composed only when `[game.economy].enabled` is
true, which additionally requires `game.enabled` and a catalog carrying at least one shop
offer of which at least one is a consumable.

## State-aware opcode registry

Economy opcodes are accepted only from an authenticated connection that has entered a
channel. A frame arriving before authentication, or before channel entry, is a protocol
violation and closes the connection; it is never answered with a command result.

| Direction | Opcode | Packet |
|---|---|---|
| C2S | `0x7f40` | `ShopPageRequest` |
| C2S | `0x7f41` | `PurchaseRequestPacket` |
| C2S | `0x7f42` | `EquipRequest` |
| C2S | `0x7f43` | `ConsumeOneRequest` |
| C2S | `0x7f44` | `RepairRequest` |
| S2C | `0x7fc0` | `ShopPage` |
| S2C | `0x7fc1` | `EconomyCommandResult` |
| S2C | `0x7fc2` | `PurchaseCommitted` |
| S2C | `0x7fc3` | `InventoryChanged` |
| S2C | `0x7fc4` | `EquipmentChanged` |
| S2C | `0x7fc5` | `RepairCommitted` |

When the section is disabled, all five client opcodes are still decoded for shape and are
answered with `EconomyCommandResult{command, Disabled}`. A disabled economy never closes a
connection and never writes to storage.

## Exact plaintext layouts

All integers are little-endian. `uuid` is 16 bytes. `durability` is encoded as a
one-byte present/absent tag followed by a `u32`; absent is exactly `(0, 0)`.

### Client to server

| Packet | Layout |
|---|---|
| `ShopPageRequest` | `u16 page` |
| `PurchaseRequestPacket` | `uuid operation_id`, `u32 type_id`, `u32 quantity` |
| `EquipRequest` | `uuid operation_id`, `u32 expected_version`, `u64 character_id`, `u64 club_id`, `u64 ball_id` |
| `ConsumeOneRequest` | `uuid operation_id`, `u64 inventory_id` |
| `RepairRequest` | `uuid operation_id`, `u64 inventory_id` |

`club_id` and `ball_id` use `0` to mean "clear this slot". `quantity` is rejected by the
decoder above `MAX_PURCHASE_QUANTITY` (99), so the wire type cannot carry an
over-protocol quantity; `[game.economy].max_purchase_quantity` may cap it lower still, and
that policy check runs before any repository work.

### Server to client

| Packet | Layout |
|---|---|
| `ShopPage` | `u16 page`, `u16 total_pages`, `u8 count`, then `count` offers |
| offer entry | `u32 type_id`, `u8 kind`, `u64 pang_price`, `u32 max_stack`, `u32 max_durability`, `u32 repair_rate` |
| `EconomyCommandResult` | `u8 command`, `u8 outcome` |
| `PurchaseCommitted` | `uuid operation_id`, `u64 inventory_id`, `u32 type_id`, `u32 quantity_after`, `durability`, `u64 pang_balance` |
| `InventoryChanged` | `uuid operation_id`, `u64 inventory_id`, `u32 type_id`, `u32 quantity_after`, `durability` |
| `EquipmentChanged` | `uuid operation_id`, `u64 character_id`, `u64 club_id`, `u64 ball_id`, `u32 version` |
| `RepairCommitted` | `uuid operation_id`, `u64 inventory_id`, `u32 durability`, `u64 pang_balance` |

`count` is bounded by `MAX_SHOP_PAGE_ENTRIES` (50) on both encode and decode.

## Fixed command and outcome values

| `command` | Value |
|---|---|
| `ShopPage` | 0 |
| `Purchase` | 1 |
| `Equip` | 2 |
| `Consume` | 3 |
| `Repair` | 4 |

| `outcome` | Value | Meaning |
|---|---|---|
| `Success` | 0 | Committed, or replayed an identical prior commit |
| `Disabled` | 1 | Economy not composed; request decoded and refused |
| `Invalid` | 2 | Failed a bound, catalog, or identifier check |
| `NotOwned` | 3 | Referenced inventory or character not held |
| `Incompatible` | 4 | Referenced item cannot satisfy the command |
| `InsufficientPang` | 5 | Balance below the catalog price |
| `StackFull` | 6 | Stack limit would be exceeded |
| `VersionConflict` | 7 | Equipment version did not match |
| `IdempotencyDrift` | 8 | Operation id replayed with different parameters |
| `Timeout` | 9 | Repository command exceeded its deadline |

Storage, arithmetic-overflow, and corrupt-data repository errors are never mapped to an
outcome. They terminate the connection as `EconomyPersistence` so a client can never be
told a failed write succeeded.

## Exact successful packet order

Every mutating command answers with `EconomyCommandResult{command, Success}` **first**,
then exactly one payload packet:

| Command | Success sequence |
|---|---|
| Shop page | `ShopPage` only — the page is itself the success reply, so no result packet precedes it |
| Purchase | `EconomyCommandResult{Purchase, Success}` then `PurchaseCommitted` |
| Equip | `EconomyCommandResult{Equip, Success}` then `EquipmentChanged` |
| Consume | `EconomyCommandResult{Consume, Success}` then `InventoryChanged` |
| Repair | `EconomyCommandResult{Repair, Success}` then `RepairCommitted` |

A rejection emits the result packet alone. No payload packet ever follows a non-`Success`
outcome.

## Pricing, durability, and idempotency bounds

- Prices come from the catalog, never from the wire. A `type_id` absent from the catalog's
  shop offers is `Invalid`, including items that exist in the catalog but are not sold.
- Repair cost is `(max_durability - current) × repair_pang_per_point`, both from the
  catalog. Repair restores to the catalog maximum.
- Stacking is catalog-driven: `Unique` items are one row each, `Stackable` items are
  capped at `max_stack`.
- Every mutating command carries a client-chosen `operation_id`. Replaying the same id
  with identical parameters returns the original commit unchanged and moves no balance;
  replaying it with different parameters is `IdempotencyDrift` and commits nothing.
- Equipment changes are optimistically versioned. `expected_version` must match the
  current version or the change is `VersionConflict`.

## Rate limiting and deadlines

`commands_per_window` bounds economy commands per connection within the shared rate
window; exhausting it is a rate termination that closes the connection rather than an
outcome. `command_timeout` bounds each repository call; exceeding it yields `Timeout`,
leaves the connection usable, and commits nothing.

## Explicit exclusions and external gate

This flow implements no gifting, no premium currency, no trading, no mail, no auction, no
card or gacha system, and no ranking interaction. It has not been validated against a real
client. The external gate is unchanged from M2–M6: a legally held U.S. client and legally
supplied data must accept these exchanges before any retail claim is made, and this
synthetic family will be replaced by reference-derived retail layouts before that gate can
be attempted. See [`../evidence/US_CLIENT_ACQUISITION_2026-08-07.md`](../evidence/US_CLIENT_ACQUISITION_2026-08-07.md).
