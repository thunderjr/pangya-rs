# Real U.S. 852 Course Practice evidence — 2026-08-09

## Scope

Manual compatibility evidence for the one-player Practice first-playable path, including a full physical hole, durable result, and clean return to the Practice room. The legally held client and screenshots remain outside Git.

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

## Full physical-hole run

The first playable tee exposed four separate shot-phase defects. They were resolved from the references and the decrypted frame shapes, not by treating every frame as a stroke:

- Client `0x0012` carries a two-byte subtype (and nine extra putt bytes for subtype 1) that does **not** appear in server `0x0055`. The real normal-shot payload was 64 bytes; stripping its subtype and preserving the remaining 62 opaque bytes made the client's ball animation begin. Relaying all 64 bytes shifted every shot field and crashed it.
- Client `0x001b` is the authoritative stroke boundary. Practice replies with reduced server `0x006e` — connection ID, hole, X/Z, shot-state bits, and duration — rather than versus `0x0064`. SuperSS-Dev defines the packed 38-byte input at `TYPE/game_type.hpp:222-350` and the exact output at `GAME/tourney_base.cpp:2118-2137`; PacketDoc independently pins the 19-byte `006e.ksy` body.
- Client `0x001c` receives five-byte server `0x00cc`: connection ID plus one-byte zero collectable count (`PacketDoc server/00cc.ksy`; SuperSS-Dev `TourneyBase::sendEndShot`). The former eight-byte body model was wrong.
- This client flow uses the versus-shaped `0x0053` hole intro, so it also needs `0x0063` to hand its perpetual one-player turn back. Without it the landed ball remained visible, then the client timed out exactly sixty seconds later. With it every next shot began from the new lie.

The visual run then completed eight physical strokes. Each cycle was client `0x0012` → server `0x0055`, client `0x001b` → server `0x006e`, client `0x001c` → server `0x00cc`/`0x0063`. The client advanced through tee, fairway, bunker, and approach lies rather than resetting to the tee. It finally sent `0x0031`; the server marked the already-counted last shot holed (no ninth stroke), committed, sent `0x0065` plus authoritative one-player `0x0066`, and the client sent game-end `0x0006` and returned cleanly to its Practice room.

Durable final run:

- match: `89d9db42-795a-4959-b760-724b56e1b807`
- result key: `c3adcbf6-c2f7-4281-8966-fb5c0ab7de1b`
- completion: `holed`, strokes: 8
- reward: +10 Pang, +5 EXP
- balances after commit: 8,642 Pang, 15 EXP
- exactly one currency-ledger row and one progression-ledger row for the match

An immediately preceding physical run committed match `da4e1de7-0d9d-43d4-9508-95f51d8a1516` with the same eight-stroke/+10/+5 outcome. The final run additionally proved the post-result `0x0066` path returns to the room instead of trying to load an unsupported next hole.

## Automated evidence

`game_retail_match_plays_and_settles_one_hole` creates semantic Practice type 19, exercises exact 17-byte `TC_ALL` `0x000c`, the normal-shot subtype stripping, `0x0055`/`0x006e`/five-byte `0x00cc`/`0x0063` sequence, already-counted hole-out transition, durable one-stroke settlement, `0x0065`/`0x0066`, and updated Pang projection. Protocol tests pin type 19, the type-7 equipment order, the 38-byte Practice sync request, and the 19-byte response.
