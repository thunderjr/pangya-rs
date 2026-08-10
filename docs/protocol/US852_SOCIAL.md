# U.S. 852 lobby/social contracts

Owner: issue #12 implementation. Wire layouts are derived from the vendored
`opensource-references/pangbox--packetdoc` corpus, with K4T's `TPLAYER_ACTION`
sub-byte values taking priority for lounge actions.

| Direction | Opcode | Behavior |
|---|---:|---|
| C→S | `0x0003` | Decode four-byte padding, nickname and message; broadcast `0x0040` to the same lobby/room. |
| C→S | `0x002a` | Decode target/message; deliver `0x0084` when accepted, or `0x0040` status `4` (blocked) / `5` (offline) with the target name. `0x0084` is `[status, nickname pstring, message pstring]`, with status `0` for the recipient and `1` for the sender acknowledgement. |
| C→S | `0x0018` | Decode `1/-1`; relay `0x005d` to same scope. |
| C→S | `0x0055` | Decode one-byte SuperSS accept flag and apply it to future whispers. |
| C→S | `0x0007` | Decode discriminator `1` and username; answer `0x00a1` with the one-byte body `0x02`. |
| C→S | `0x0063` | Preserve action subtype/payload and relay `0x00c4`; no guessed subtype payload widths. |
| C→S | `0x0069` | Decode nine fixed 64-byte macros and persist them in `profiles`; LoginService serves `0x0006`. |
| C→S | `0x002f` | For a valid target, emit the exact 13-packet SuperSS/K4T order: `0x0157`, `0x015e`, `0x0156`, `0x0158`, `0x015d`, `0x015c` (natural), `0x015c` (Grand Prix), `0x015b`, `0x015a`, `0x0159`, `0x015c` (original), `0x0257`, then `0x0089`; preserve season/total request types and use PacketDoc's fixed/count-zero bodies. |
| C→S | `0x00eb` | Decode target connection and answer packetdoc `0x0196` with five `1.0` fields. |
| C→S | `0x008b` | Empty request body; answer `0x00fc` with the exact zero-server count byte `0x00`. |

The remaining issue-listed social opcodes (`0x0032`, `0x0038`, `0x003a`, `0x0054`,
`0x0066`, `0x0067`, `0x0088`, `0x00c1`, `0x00fe`) are explicitly accepted as client-safe no-reply outcomes until their reference layouts or side effects are established. `0x003c` is the durable offline-note bridge: subtype `0x0111` resolves the recipient by account id, persists the bounded message and charges ten Pang atomically; retries are idempotent and the sender receives `0x0095` with the resulting balance. Pending rows are leased after authentication and acknowledged only after the `0x0084` outbound write succeeds; expired leases retry after crash/disconnect. Opcode `0x004f` is also accepted, but only with its exact empty body documented by `pangbox/server/game/packet/client.go`; non-empty payloads are rejected. No unknown-opcode policy change is made.
