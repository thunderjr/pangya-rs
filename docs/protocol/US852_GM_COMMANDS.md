# U.S. 852 GM command boundary

The checked references agree on client opcodes `0x008f`, `0x0041`, `0x0057`, `0x0060`,
`0x0061`, and `0x003e`. The multiplexer subcommands and minimum layouts used here are
from `Acrisio-Filho/SuperSS-Dev` `game_server.cpp:2763-3070`, `channel.cpp:4788-4988`,
and `hsreina/pangya-server` `GameServer.pas:1037-1120` (with the K4T superset listed
in issue #27).

Implemented exact layouts:

| Wire | Body | Handling |
|---|---|---|
| `0x008f/10` | `u32 OID, u8 force` | kick the live target from the authoritative lobby |
| `0x008f/11` | `u32 OID` | cancel the live target connection after authoritative OID resolution |
| `0x008f/13` | `u16 room` | destroy authoritative room |
| `0x008f/14` | `u8 speed, u8 direction` | broadcast validated wind frame |
| `0x008f/15` | `u8 weather` | broadcast validated weather frame |
| `0x008f/18` | `u32 OID, u32 type, u32 quantity` | resolve OID to the durable account mapping, then apply a catalog-validated grant (including offline targets) |
| `0x0057` | length-prefixed notice | broadcast as the reference notice chat frame |
| `0x0060` | `u16 room` | same room destroy path |

Capability is read from the persisted `accounts.game_master` flag at authentication. The
`0x0041` identity packet is parsed only to preserve its exact body boundary; its client-claimed
capability is never used. Non-GM requests are logged and terminate the protocol session without
mutation. OIDs are resolved through the process-authoritative account/session registry; a GM wire
integer is never cast into a local connection ID. The account half of that mapping is retained
within the bounded process registry so a catalog-valid grant can complete after the target logs
out, while disconnect still cancels the target's live connection token.

The local references do **not** establish an accepted body layout for `0x003e` enter/observe,
`0x0061` disconnect, or subcommands `3`, `4`, `5`, `8`, `9`, `16`, `19`, `25`, `26`, `28`, and
`31`. They are therefore explicitly refused, with no body interpretation or state mutation.
This is an evidence blocker, not a protocol guess: a client capture or a checked reference layout
is required before accepting any of those union members.
