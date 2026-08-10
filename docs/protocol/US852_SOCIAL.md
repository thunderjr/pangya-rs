# U.S. 852 lobby/social contracts

Owner: issue #12 implementation. Wire layouts are derived from the vendored
`opensource-references/pangbox--packetdoc` corpus, with K4T's `TPLAYER_ACTION`
sub-byte values taking priority for lounge actions.

| Direction | Opcode | Behavior |
|---|---:|---|
| C→S | `0x0003` | Decode four-byte padding, nickname and message; broadcast `0x0040` to the same lobby/room. |
| C→S | `0x002a` | Decode target/message; deliver `0x0084` only when target accepts whispers; return documented offline/refusal statuses otherwise. |
| C→S | `0x0018` | Decode `1/-1`; relay `0x005d` to same scope. |
| C→S | `0x0055` | Decode one-byte SuperSS accept flag and apply it to future whispers. |
| C→S | `0x0063` | Preserve action subtype/payload and relay `0x00c4`; no guessed subtype payload widths. |
| C→S | `0x0069` | Decode nine fixed 64-byte macros and persist them in `profiles`; LoginService serves `0x0006`. |
| C→S | `0x002f` | Return `0x0089`, then the packetdoc/K4T ordered fan-out: real target name (`0x0157`), character (`0x015e`), equipment (`0x0156`), and durable experience/pang statistics (`0x0158`) when online. |
| C→S | `0x00eb` | Decode target connection and answer packetdoc `0x0196` with five `1.0` fields. |

The remaining issue-listed social opcodes (`0x0007`, `0x0032`, `0x0038`, `0x003a`,
`0x003c`, `0x004f`, `0x0054`, `0x0066`, `0x0067`, `0x0088`, `0x008b`, `0x00c1`,
`0x00fe`) are explicitly accepted as safe no-reply outcomes until their reference
layouts or side effects are established. No unknown-opcode policy change is made.
