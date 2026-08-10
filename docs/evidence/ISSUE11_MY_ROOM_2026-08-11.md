# Issue #11 My Room protocol evidence

This implementation is reference-derived and does not claim real-client acceptance.
The checked-out `opensource-references/pangbox--packetdoc` corpus establishes:

- client `0x00b5`: two little-endian `u32` values (`00b5.ksy`);
- client `0x00b7`: local user `u32` plus one byte (`00b7.ksy`);
- server `0x012d`: option `u32 = 1`, `u16` count, then each furniture entry as four opaque
  bytes, a Furniture.iff `u32`, and nineteen opaque bytes (`012d.ksy`);
- client `0x00b9`: option `u8`, inventory slot `u32`, trailing option `u8` (`00b9.ksy`);
- server `0x012e`: custom-asset metadata with a public HTTP download URL (`012e.ksy`).

`Acrisio-Filho/SuperSS-Dev` corroborates visitor lookup, `PlayerRoomInfoEx`, persisted My Room
items, and the mascot-message success response (`0x00e2`, status `4`, mascot id, PString,
Pang). In `Server Lib/Game Server/GAME/channel.cpp`, that response is sent by the authenticated
owner's mascot-message update handler after persistence; the corpus does not establish an
unsolicited `0x00e2` response when a visitor opens a room. Its packed UCC upload-key request is
`u8 option, u32 owner, u8 sequence, i32 item id` and its refusal response is server `0x0153`:
status `1`, status `1`, error `0x05100100`.

The project does not have a configured authenticated UCC upload service or proprietary asset
storage. Therefore `0x00b9` and `0x00c9` are validated and answered with that explicit `0x0153`
refusal; no URL, bearer, asset, or silent drop is fabricated. Furniture has no documented client
placement opcode in the checked-out references, so the server serves only durable rows and does
not invent a placement wire command.
