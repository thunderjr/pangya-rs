# Real U.S. 852 inventory/equipment evidence — 2026-08-09

## Inventory row correction

PacketDoc `gameservice/server/0073.ksy:30-59` defines each inventory row as 196 bytes. The server
had emitted only `{inventory id, type id, quantity}` (12 bytes), so the client read every later row
184 bytes early. Only the already-equipped starter appeared in My Room; purchased rows did not.

`RetailInventoryItem` now writes the exact 196-byte row, including quantity, retail class, rental
fields, the observed `0x02` byte and bounded zero tail. The catalog maps ClubSet/Ball to class `1`,
Consumable to `5`, and CharacterPart to `0`.

After restart and relog the unmodified client displayed:

- Air Knight starter and purchased Papel Training Club Set in Item / Club Sets;
- the default Comet and both purchased Cobra Comet inventory rows in Item / Comet.

This is direct confirmation that the client strides the corrected container and retains purchases.

## Ball and club equipment request

The older Pangbox model names retail equipment type `3` as one Comet word. That is incomplete for
this client. SuperSS-Dev `GAME/channel.cpp:5233-5299` reads two request words together:

1. Ball catalog type id;
2. ClubSet inventory id.

Its `PACKET/packet_func_sv.cpp:4964-4970` sends the same pair in the `0x006b` reply. The protocol
model and server now do the same. Both requested rows are resolved from the authenticated player's
inventory and checked against the immutable catalog before one optimistic `EquipmentChange`
transaction commits them together. Repeated current-state frames are no-ops rather than needless
version/ledger increments.

The reusable client flow records the UI's actual commit behavior:

```powershell
Open-PangyaMyRoom
Select-PangyaMyRoomCategory -Top Item -Sub ClubSets
Set-PangyaMyRoomItem -Column 1 -Row 0 -ToggleEquipment -Commit
```

Selecting changes only the preview. Club sets also require the small equip toggle between the
columns, and the client sends `0x0020` only when My Room closes.

The real request decoded as Ball `0x140000c9` plus ClubSet inventory row `310`. PostgreSQL then held:

```text
version  club_item_id  club_type  ball_item_id  ball_type
6        310           10000061   311           140000c9
```

The corresponding economy operation records `result_club_item_id=310`,
`result_ball_item_id=311`, and `result_equipment_version=6`.

## Restart and room projection

Before the club update, a full restart/relog rendered Cobra Comet as the current 3D ball and marked
its owned row selected, proving durable Ball retention. After the combined update, the same durable
snapshot feeds `member_card`: its ClubSet inventory/catalog ids and Comet catalog id travel into the
room census and match roster rather than being invented per frame.

A two-seat real-client room rendered the selected club and Cobra icons under **Characters & Gear**,
and the subsequent Blue Lagoon hole rendered the same club and Cobra ball at the tee. This verifies
the room/member-card projection as well as My Room persistence.

Character parts and twelve character card slots remain explicitly zero/opaque because their Tier-D
mutable model does not yet exist; the server does not claim to persist or acknowledge them.
