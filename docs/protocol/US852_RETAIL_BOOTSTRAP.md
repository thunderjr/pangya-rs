# U.S. 852 retail GameService bootstrap — reference-derived specification

## Claim boundary

Every layout below is **derived from vendored open-source references**, not captured from a
client and not yet verified against one. It is the specification this project will implement
to replace the synthetic families; until the acquired client accepts an exchange, no layout
here may be described as verified.

Sources, both vendored under `opensource-references/`:

- `pangbox--packetdoc` — Kaitai definitions keyed by hex opcode, including a `us_852`
  version discriminator.
- `hex-agon--alter-pangya` — a GB.852-targeting server that reaches playable practice mode.
  Behavioral reference only; it carries no license grant, so nothing is copied from it. What
  is recorded here are protocol facts (field order, sizes, opcode values), not its code.

## The version question is settled

`evidence/US_CLIENT_ACQUISITION_2026-08-07.md` could only infer that the archive's "851"
and the project's `Us852` name the same client, because the client binary is packed.

The bootstrap reply resolves it. The server must send its version as the **PString
`"852.00"`** in the `0x0044` handover reply, and a mismatch has a dedicated client-visible
error (`SERVER_VERSION_MISSMATCH`). A client whose content patches run to 851 therefore
expects a server identifying as 852.00.

`CompatibilityProfile::US_852` is correct and stays. Archive "851" is the content patch
level; "852.00" is the protocol/build version the wire carries.

## Client authentication — `0x0002`

Sent by the client immediately after handover from LoginService.

| Field | Type |
|---|---|
| `username` | PString |
| `user_id` | `u32` |
| padding | 4 bytes |
| unknown | 2 bytes |
| `login_key` | PString |
| `client_version` | PString |
| unknown | `u32` |
| unknown | `u32` |
| `session_key` | PString |

The project's current `GameAuth` (`account_id: i64` plus a token) does not match this and
must be replaced.

## Bootstrap response sequence

The client expects this order. Each `write` is a separate packet.

1. `0x0044` progress-bar updates, one per completed load step
2. `0x0044` handover reply (full body below)
3. `0x0070` character roster — chunked container
4. `0x0071` caddie roster — chunked container
5. `0x0072` equipment
6. `0x0073` inventory — chunked container
7. treasure-hunter packet
8. mascot roster
9. cookie balance
10. Pang balance
11. card inventory
12. `0x004d` server channel list

PacketDoc lists a longer sequence for retail (including `0x011f`, `0x00e1`, `0x0131`,
`0x021d`, `0x021e`, `0x0096`, `0x0158`, `0x0210`, and several undocumented ids). The
sequence above is the reduced set proven sufficient by a working server, and is what this
project targets first.

### Opcode corrections this forces

The current `pangya-protocol/src/game.rs` disagrees with retail on three opcodes. All three
are confirmed wrong by both references independently:

| Opcode | Current meaning | Retail meaning |
|---|---|---|
| `0x0070` | player/profile bootstrap | **character roster** |
| `0x0072` | character list | **equipment** |
| `0x004d` | equipment selection | **server channel list** |

`0x0073` (inventory) is already correct.

## `0x0044` — handover reply

The first field after the opcode is a subtype byte:

| Subtype | Form |
|---|---|
| `0x00` | full handover reply (body below) |
| `0xd2` | progress bar; followed by one byte, `0..=15` |
| `0xd3` | login OK; written as a `u16` subtype followed by one zero byte |
| error | error code written as a `u16` in place of the subtype |

Error codes are client-visible and meaningful: 1/9/10/12/13 make the client return to
LoginService and reconnect, 3 is "cannot connect to login server", 5 permanent block, 7
block, 11 **server version mismatch**, 14/15 non-whitelisted, 16/17 geo-blocked, 19–31
account transferred.

Full reply body, in order:

1. PString server version — **must be `"852.00"`**
2. PString server name
3. player basic info (below)
4. player statistics
5. trophies
6. equipment (below)
7. season historical statistics — **12 seasons × 21 courses** of course-statistics structs
8. active character
9. active caddie
10. active club set: `u32 uid`, `u32 iff_id`, 5 × `u16` zero, 5 × `u16` zero — the client
    appears to ignore this block
11. active mascot: `u32 uid`, `u32 iff_id`, `u8 level`, `u32 experience`, fixed 16-byte
    string, 33 zero bytes
12. server time as a packed local date-time
13. `u16` zero, then three `u16` zeros (papel shop)
14. `u32` zero
15. `u64` disabled-feature flags — `1 << 18` is known-good
16. `u32` zero (login count), `u32` zero (server flags)
17. 277 zero bytes (guild info)

A player with no equipped club set cannot be encoded, so bootstrap must guarantee one.

### Player basic info

`u16` room id (`0xffff` when not in a room), fixed 22-byte username, fixed 22-byte nickname,
fixed 17-byte guild name, fixed 24-byte guild image, `u32` connection id, 12 zero bytes,
`u32` zero, `u32` zero, `u16` zero, 6 zero bytes, 16 zero bytes, fixed 128-byte global id,
`u32` user id.

### Equipment

`u32` caddie uid, `u32` character uid, `u32` club-set uid, `u32` comet iff id, then **10**
`u32` equipped item iff ids, then eleven further `u32` slots — background, frame, sticker,
slot, unknown, title, skin background, skin frame, skin sticker, skin slot, unknown.

## Chunked container packets — `0x0070`, `0x0071`, `0x0073`

| Field | Type |
|---|---|
| opcode | `u16` |
| total entries across all chunks | `u16` |
| entries in this chunk | `u16` |
| entries | repeated |

Chunk size is 50 entries. The project's existing inventory segmentation constant is 50,
which already matches.

## `0x004d` — server channel list

`u16` opcode, `u8` channel count, then per channel: fixed 64-byte name, `u16` capacity,
`u16` current player count, `u16` channel id, `u16` packed restriction flags, 5 zero bytes.

## Implementation order

1. Replace `GameAuth` with the retail `0x0002` layout.
2. Implement `0x0044` in all four forms; the error codes give real diagnostics during
   client bring-up, so they are worth having first.
3. Correct `0x0070`/`0x0072`/`0x004d`, and add `0x0071`.
4. Add the remaining bootstrap packets (treasure hunter, mascot roster, balances, cards).
5. Only then move to lobby/room and match layouts.

Fixed-size string and PString helpers already exist in `pangya-protocol`; the fixed-size
variants must zero-pad to the exact widths above, since several are longer than any
plausible value.

## Open questions

Player statistics, trophies, course statistics, active character, active caddie, and card
inventory sub-structures are referenced above but not yet transcribed field by field. The
`12 × 21` course-statistics block dominates the reply's size and must be measured before
the reply can be emitted. These are the next things to pin down.


---

# Retail lobby and room contract

Same provenance and same caveat as the bootstrap above: reference-derived, not
client-verified. Implemented in `pangya-protocol::us852_room`.

## Client opcodes

| Opcode | Meaning |
|---|---|
| `0x0004` | Select channel |
| `0x0008` | Room create |
| `0x0009` | Room join |
| `0x000a` | Room settings update |
| `0x000b` | Equipment update in lobby |
| `0x000c` | Equipment update in room |
| `0x000e` | Start match |
| `0x000f` | Room leave |
| `0x0081` | Join lobby |
| `0x0082` | Leave lobby |

## Server opcodes

| Opcode | Meaning |
|---|---|
| `0x0047` | Room list |
| `0x0048` | Room player census |
| `0x0049` | Join result |
| `0x004a` | Room settings |
| `0x004c` | Leave acknowledgement |

Note `0x0048` is the *room* census. PacketDoc's `0x0046` "User Census" is a different,
lobby-level packet; do not conflate them.

## Room record — 210 bytes

Fixed 64-byte name, public flag, in-lobby flag, in-game-joinable flag, capacity,
occupancy, 17 zero bytes, the constant 30, hole count, room type, `u16` room number, hole
progression, course, `u32` shot timer, `u32` game timer, `u32` trophy catalog id, `u16`
zero, 66 zero bytes of guild info, two `u32` hundreds, `u32` owner id, room type again,
`u32` artifact catalog id, `u32` natural-wind flag, and four `u32` event fields.

The two state flags are mutually exclusive and both zero means in-game-and-closed.
PacketDoc subdivides the fixed middle differently but reaches the same total; where they
disagree the reference server is followed, because it is the one a client demonstrably
accepts.

## Asymmetric join result

`0x0049` success writes a `u16` status followed by the room record; rejection writes a
single status byte. The widths genuinely differ — this is not an oversight to "fix".
Rejection codes: 8 already started, 18 cannot create.

`0x004c` carries the room the client now occupies, with `0xffff` meaning the lobby.

## What remains to make rooms work

The packets exist and are unit-tested. They are **not routed in the runtime**: room
opcodes still dispatch to `handle_room_command`, which speaks the synthetic `0x7f00`
family and drives the lobby/room actor. Wiring requires translating retail commands onto
the existing actor commands and emitting the retail replies, gated the same way the
bootstrap is. The actor model itself is protocol-agnostic and should not need changes —
this is a wire-layer translation, deliberately left as a separate step rather than rushed
alongside the packet definitions.


---

# Retail match contract

Same provenance and caveat. Implemented in `pangya-protocol::us852_match`.

## Client opcodes

| Opcode | Meaning |
|---|---|
| `0x000e` | Start match |
| `0x0011` | Hole load finished |
| `0x001a` | Hole start |
| `0x0012` | Shot commit |
| `0x0013` | Rotate aim |
| `0x0014` | Shot start |
| `0x001b` | Shot sync |
| `0x001c` | Shot end / turn end |
| `0x0031` | Player hole finish |
| `0x0034` | Finish player preview |
| `0x0130` | Player quit |

## Server opcodes

| Opcode | Meaning | Body |
|---|---|---|
| `0x004a` | Room status | `u16` `0xffff`, room type, course, hole count, progression, `u32` natural wind, capacity, `u16` `30`, `u32` shot timer, `u32` game timer, `u32` flags, owner byte, room name |
| `0x0230` | Pre-match framing, first half | empty |
| `0x0231` | Pre-match framing, second half | empty |
| `0x0077` | Pang rate for the match | `u32` percentage, `100` for the plain rate |
| `0x0076` | Match start | subtype `0x00` then `u8` count then one whole player record per seat, or subtype `0x04` then `u32` 1 then the packed server time |
| `0x016a` | Mascot effect seed | `u32` |
| `0x0052` | Match plan | course, UI type, hole mode, hole count, `u32` trophy, `u32` shot timer, `u32` game timer, the hole plan, `u32` seed, then **18** collectible counts |
| `0x009e` | Hole weather | `u16` weather ordinal, one zero byte |
| `0x005b` | Hole wind | `u8` strength, `u8` silent flag, `u16` bearing, `u8` 1 to set rather than accumulate |
| `0x0053` | Play a player's hole intro | `u32` connection id |
| `0x0063` | Turn start | `u32` connection id |
| `0x00cc` | Turn end | `u32` connection id, `u32` zero |
| `0x0056` | Aim rotation relay | `u32` connection id, `f32` bearing |
| `0x0055` | Shot commit relay | `u32` connection id, then the client's shot payload verbatim |
| `0x0065` | Hole finished | empty |
| `0x0090` | Finish-preview acknowledgement | empty |

One hole record is `u32` random id, `u8` pin, `u8` course, `u8` number.

Four details that are easy to get wrong and were pinned by tests:

- `0x0052` must always write **eighteen** collectible count bytes regardless of how many
  holes the match actually has. The client reads all eighteen.
- The order of the start is fixed: `0x0230`, `0x0231`, `0x0077`, `0x0076`, `0x0052`,
  `0x016a`. Every reference server that sends a given frame sends it in that position.
- **Weather and wind are not part of the start.** They are withheld until every player has
  sent `0x0011`, and then sent immediately before `0x0053`. No reference server emits
  either during the load, and the retail client dies at the end of its load ramp when told
  early. See `pangbox/server` `game/room/room.go` `startHole`,
  `Acrisio-Filho/SuperSS-Dev` `versus_base.cpp` `checkAllLoadHole`, and
  `hsreina/pangya-server` `Game.pas` `HandlePlayerLoadOk`.
- A wind strength of `0` is never sent; upstream picks `1..8`, so a still hole is reported
  as the weakest breeze.
- Shots are relayed, not recomputed. The client owns trajectory; the server owns turn
  order, scoring, and persistence. The relay body is deliberately opaque.

## Current retail match scope

The retail room and match packets are routed. Versus rooms require exactly two ready players;
the room-driven plan retains 3/6/9/18-hole cards and the selected front/back/random/shuffle
ordering. The stroke aggregate advances each hole and settles the whole card once, while each
hole emits its required `0x0053` opening-player introduction. Synthetic M5/M6 remain separate
one-hole compatibility checkpoints; that limitation does not describe the retail path.
