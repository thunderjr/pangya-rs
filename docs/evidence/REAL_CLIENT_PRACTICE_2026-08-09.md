# Real U.S. 852 Course Practice evidence — 2026-08-09

## Scope

Manual compatibility evidence for the optional one-player Practice path. The legally held client and screenshots remain outside Git.

- Client: U.S. 852 (`2016110200`)
- Account: 257 (`rsp8`)
- Server policy: `game.unknown_opcode_policy = "disconnect"`
- Course: Blue Lagoon, hole 1

## Reference-first diagnosis

The client initially created a room whose header contained `(null)` and whose in-room Start control stayed grey. The room record has two mode fields, and they are not duplicates for Practice:

- The GB.852-targeting `alter-pangya` reference defines Practice as semantic type 19 with UI type 4 (`RoomType.kt:8-28`) and serializes the UI family early plus the semantic type later (`Room.kt:145-174`).
- SuperSS-Dev exempts Practice from ready and one-player start rejection (`opensource-references/Acrisio-Filho--SuperSS-Dev/Server Lib/Game Server/GAME/room.cpp:3535-3564,5934-5951`).
- SuperSS-Dev registers ordinary start request `0x000e` (`PACKET/packet_func_sv.h:55`) and parses the type-7 room equipment submission `0x000c` as character, caddie, ClubSet, and Ball (`GAME/channel.cpp:9588-9619`).
- It independently identifies `0x000b` as the channel equipment update (`PACKET/packet_func_sv.h:52-53`, `PACKET/packet_func_sv.cpp:379-395`).

The server had emitted semantic 19 in the UI-family field and zero in the later semantic field. It now emits UI 4 plus semantic 19 while retaining semantic 19 in authoritative room state.

## Real-client run

`Start-PangyaCoursePractice` captures the observed path:

1. Click **Practice** in the lobby.
2. Click **Course Practice** in the Single Player Practice Mode dialog.
3. The corrected room projection allows the **Strategy** dialog to render with an active **Start Game** control.
4. Click Start and wait for the course header.

The real wire sequence was:

- client `0x0008` room create;
- server `0x0049` accepted and `0x0048` census;
- client `0x000e` start;
- server `0x0230`, `0x0231`, `0x0077`, `0x0076`, `0x0045`, `0x0052`, `0x016a`, and census `0x0048`;
- client loading equipment syncs `0x000c` and `0x000b`;
- repeated client `0x0048` progress and final `0x0011` load complete;
- server `0x009e` weather, `0x005b` wind, and `0x0090` first-shot permission.

The client completed loading and rendered the playable Blue Lagoon tee with one player, a live shot meter, 242y lie, Pang 0, and no network or client exception. This run used the shipped disconnect policy, so every observed opcode above was explicitly handled or allowlisted.

## Automated evidence

`game_retail_match_plays_and_settles_one_hole` now creates semantic Practice type 19 and exercises the exact 17-byte `TC_ALL` `0x000c` model before completing the durable one-hole solo lifecycle. Protocol tests pin type 19 and the type-7 equipment order.
