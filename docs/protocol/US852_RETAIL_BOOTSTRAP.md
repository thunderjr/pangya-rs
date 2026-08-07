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
