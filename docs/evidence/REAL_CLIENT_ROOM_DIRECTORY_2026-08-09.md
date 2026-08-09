# Real U.S. 852 room-directory evidence — 2026-08-09

## Run

The server ran with `game.unknown_opcode_policy = "disconnect"`. Account 149 hosted room 1 through the headless retail-wire client:

```text
scripts/second-seat.sh --username rsp5 --host --strokes 0
```

A fresh real U.S. 852 client logged in as account 257 and opened Multiplay. The server sent the initial `0x0047` room list before the `0x00f5` multiplayer-entry acknowledgement.

The real directory displayed one row with:

- room `001`;
- `VS` / `Stroke`;
- name `PangYa-RS`;
- occupancy `1/2`;
- Blue Lagoon.

Double-clicking the row sent retail join `0x0009`. The client entered the room and displayed both `RsPlayerFive` as master and `HostEight` as the second seat, with account 257's **Ready** control active. No direct room number or out-of-band join was used.

`Open-PangyaMultiplay` and `Join-PangyaFirstListedRoom` in `scripts/windows/pangya-client.ps1` preserve the exact UI pattern.

No proprietary screenshot, packet body, client binary, or client asset is committed.
