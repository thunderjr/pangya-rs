# PangYa Rust server rewrite — initial and extended research

> Research snapshot: **2026-08-05**
>
> Status: planning baseline; no Rust implementation exists in this repository yet
>
> Target assumed by this document: **PangYa U.S. Season 8, GB.852 / US 852.00**, local-first preservation server

## Executive conclusion

A clean-room Rust rewrite is feasible, but it should not begin as a line-for-line port of one legacy server. The safest and fastest route is to combine four kinds of evidence:

1. **Protocol facts and fixtures:** `pangbox/packetdoc` and `pangbox/pangcrypt`.
2. **Modern service and state-machine examples:** `hex-agon/alter-pangya` and `pangbox/server`.
3. **Breadth/feature inventories:** `Acrisio-Filho/SuperSS-Dev`, `K4T/Py_Source_US`, and the current C# community server.
4. **Client and file tooling:** Rugburn, PangFiles, PangLib, Pantrant, and the Wireshark dissector.

The network protocol is sufficiently understood to build a compatible transport and login flow. The difficult work is not ball-flight simulation: existing servers accept client-computed shot/result data, relay it, and maintain authoritative room, turn, score, inventory, currency, reward, and progression state. The project therefore needs a **deterministic game-state coordinator**, not a new golf physics engine for its first playable versions.

The recommended baseline is a Cargo workspace using Tokio, `tokio-util::codec`, `bytes`, SQLx/PostgreSQL, typed domain crates, and a room-per-actor concurrency model. The leading LZO candidate is the pure-Rust, MIT-licensed `lzokay` crate, but it must pass PangCrypt-vector and real-client compatibility gates before adoption.

The original research's Node.js/TypeScript recommendation is superseded. This document defines the evidence base for the Rust plan in [`SPEC.md`](SPEC.md).

---

## Research method and reproducibility

Sixteen upstream repositories were cloned as shallow snapshots under [`../opensource-references/`](../opensource-references/). The exact URLs, branches, revisions, dates, observed licenses, and intended uses are recorded in [`opensource-references/README.md`](../opensource-references/README.md).

Key measurements from the local snapshots:

- PacketDoc contains **286 Kaitai `.ksy` files**.
- Its packet indexes describe **7 login client**, **6 login server**, **98 game client**, **141 game server**, **8 message client**, and **2 message server** packet definitions.
- `alter-pangya` contains **39 game packet handlers** and **4 login packet handlers** at the cloned revision.
- `pangbox/server` contains **107 Go source files**.
- The current community C# server contains **570 C# source files** under `Server/`.
- SuperSS contains **968 C/C++ headers and sources**, including **394 Game Server database-command files**.
- No substantive Rust PangYa server repository was found in GitHub repository search at the snapshot date.

These counts indicate documentation and implementation breadth, not correctness or completeness. No legacy server binary was executed during this research pass, so playability claims below are clearly labeled as upstream claims or source-level observations.

---

## Target and compatibility assumptions

### Primary client

Use **PangYa U.S. GB.852 / US 852.00** as the first compatibility target because:

- `alter-pangya` explicitly targets GB.852.
- `Py_Source_US` explicitly targets U.S. S8 and lists GB.852 and GB.824.
- PacketDoc has a specific `us_852` version discriminator alongside `us_824` and older regions.
- Rugburn explicitly lists U.S. 852 as supported.
- PangCrypt's oracle tables were extracted from U.S. 852 `ProjectG.exe`.

References:

- [`alter-pangya/README.md`](../opensource-references/hex-agon--alter-pangya/README.md)
- [`Py_Source_US/README.md`](../opensource-references/K4T--Py_Source_US/README.md)
- [`packetdoc/common/version.ksy`](../opensource-references/pangbox--packetdoc/src/packets/common/version.ksy)
- [`rugburn/README.md`](../opensource-references/pangbox--rugburn/README.md)
- [`pangcrypt/common.go`](../opensource-references/pangbox--pangcrypt/common.go)

### Secondary targets

JP 983/984 and other regional clients are not first-release targets. They should be enabled later through a `ProtocolVersion`/`Region` compatibility layer rather than compile-time forks. SuperSS and the current community C# server are especially valuable for feature breadth, but their current documentation is JP-oriented and cannot be treated as byte-compatible with U.S. 852.

### Distribution assumption

The server repository must not distribute proprietary PangYa clients, IFF/PAK game data, or unfree Intel IJL binaries. Operators supply their own preserved client files. Rugburn remains an external tool and should be linked, not vendored.

---

## Upstream landscape: what each project contributes

### Core server implementations

| Project | Snapshot | Target / stack | Source-level value | Important limitation | License boundary |
|---|---:|---|---|---|---|
| [SuperSS-Dev](https://github.com/Acrisio-Filho/SuperSS-Dev) | `485ba29b4831`, 2026-07-17 | C/C++; current deployment docs emphasize JP | Broadest feature taxonomy: auth, login, game, message, rank, many game modes, inventory/events, multiple SQL schemas | Very large legacy design; U.S. compatibility is not the current documented default; not smoke-tested here | MIT at root; retain notice for adapted material |
| [Pangya-Server-Community](https://github.com/luismk/Pangya-Server-Community) | `a0e30985d271`, 2026-08-04 | C#/.NET, JP | Current five-server decomposition and named implementations for 14 gameplay/event mode files | “Full-featured” is an upstream claim, not independently verified in this pass | Root is MIT, but **`Server/LICENSE` is AGPL-3.0**; server source must be treated as AGPL |
| [Py_Source_US](https://github.com/K4T/Py_Source_US) | `dcfb75a84be8`, 2020-08-11 | C#, MSSQL, U.S. GB.852/824 | U.S. S8 packet behavior and a large feature list | README self-reports Login 100%, Game ~60%, Messenger ~20%; old snapshot and Windows/MSSQL assumptions | GPL-3.0; behavior reference unless Rust project intentionally adopts a compatible license |
| [alter-pangya](https://github.com/hex-agon/alter-pangya) | `5afc41b39260`, 2025-09-27 | Kotlin/JVM, PostgreSQL, Redis, GB.852 | Clean modules, typed handlers, modern persistence, explicit match events | README only promises practice mode and MyRoom equipment; repository has no license file | No license grant found; factual/behavioral reference only, no copying |
| [pangbox/server](https://github.com/pangbox/server) | `91d8a5a4f3be`, 2023-07-15 | Go, PostgreSQL/SQLite, U.S. client | Typed packets, room/lobby actor model, Minibox topology, migrations, SQL generation | README explicitly says it is not playable; MessageService is a stub | Primarily ISC, with file-specific BSD-3-Clause and Apache-2.0 notices |
| [hsreina/pangya-server](https://github.com/hsreina/pangya-server) | `1720cdceafcd`, 2021-10-16 | Delphi/Pascal, FreshUp U.S. | Historical login, character select, training, chatroom, room setup, `pang.dll` ABI | Requires external crypto DLL and extracted IFF data; limited feature surface | Apache-2.0 |
| [juanangel123/pangya-server](https://github.com/juanangel123/pangya-server) | `96010b0007e9`, 2020-06-15 | PHP 7.4, U.S. 851 | Minimal five-service decomposition and simple launch flow | No database; crypto/server connection are TODOs | MIT |

### Protocol, client, and file references

| Project | Best use in the Rust rewrite | Caveat |
|---|---|---|
| `pangbox/packetdoc` | Opcode registry, field layouts, version switches, packet naming | WIP; unknown fields remain, and indexes need validation against captures |
| `pangbox/pangcrypt` | Transport algorithm, 8 KiB oracle tables, key rules, golden vectors | Its Go LZO dependency is GPL-2.0; use a compatible Rust LZO implementation instead |
| `pangbox/pangfiles` | PAK/XTEA, PangYa CRC, lite XML, updatelist, region detection | Only implement formats when a server milestone actually needs them |
| `retreev/PangLib` | Cross-check IFF/DAT/PAK/PET/SBIN/UCC format behavior | AGPL-3.0; no source copying into a permissive crate |
| `pangbox/rugburn` | Client redirection and supported-client matrix | Mixed licensing; bundled Intel IJL is unfree but redistributable under separate terms |
| `pangbox/pantrant` | PCAP-to-cassette analysis workflow | Old Go/Node prerequisites and no guarantee of current packet coverage |
| `pangbox/wireshark-dissector` | Interactive capture inspection | Explicitly alpha quality |

### Updated ecosystem conclusion

The old statement “six implementations exist and only one is truly playable” is too absolute and now outdated. There are more relevant repositories, active 2026 SuperSS work, and a current community C# server. A defensible statement is:

> **SuperSS and its descendants provide the broadest implementation evidence; alter-pangya and Pangbox provide cleaner architectural evidence; none of the cloned projects should be assumed fully correct for U.S. 852 without real-client differential testing.**

---

## Protocol findings

### Session hello and key

Every TCP service starts with a service/region-specific plaintext hello. It embeds a connection key in the range `0x00..=0x0f`. In U.S. LoginService examples the key is at byte index 6; in U.S. GameService it is the last byte. The layout must therefore be selected by `(region, service)`, not hard-coded globally.

Primary reference: [`pangcrypt/README.md`](../opensource-references/pangbox--pangcrypt/README.md).

### Client-to-server frame

PangCrypt's implementation establishes this frame:

| Offset | Size | Meaning |
|---:|---:|---|
| 0 | 1 | Random salt |
| 1 | 2 | Little-endian length, excluding the first 4 bytes |
| 3 | 1 | Zero/padding |
| 4 | 1 | Key/salt oracle byte, transformed with two 4,096-byte tables |
| 5… | variable | Encrypted, **uncompressed** payload |

Decryption:

1. Reject keys `>= 0x10`.
2. Require at least 5 bytes.
3. Compute `index = (key << 8) + salt`.
4. Restore byte 4 from the private oracle table.
5. From index 8 forward, XOR each byte with the byte four positions behind it.
6. Remove the 5-byte transport header.

Primary code: [`pangcrypt/client.go`](../opensource-references/pangbox--pangcrypt/client.go).

### Server-to-client frame

| Offset | Size | Meaning |
|---:|---:|---|
| 0 | 1 | Random salt |
| 1 | 2 | Little-endian length, total frame size minus 3 |
| 3 | 1 | XOR of public/private oracle bytes |
| 4–7 | 4 | Encoded original/decompressed-size metadata; byte 7 also receives an oracle XOR |
| 8… | variable | LZO1X-compressed, then encrypted payload |

Encryption:

1. LZO1X-compress the plaintext packet.
2. Build the 8-byte header and encoded original-size metadata.
3. From the end down to index 10, XOR each byte with the byte four positions behind it.
4. XOR byte 7 with the private oracle byte.

Primary code: [`pangcrypt/server.go`](../opensource-references/pangbox--pangcrypt/server.go).

### Packet body

After decrypt/decompress, packet bodies begin with a **little-endian `u16` opcode** followed by opcode-specific data. PacketDoc models three services and two directions:

| Service | Client opcodes documented | Server opcodes documented |
|---|---:|---:|
| LoginService | 7 | 6 |
| GameService | 98 | 141 |
| MessageService | 8 | 2 |

Notable LoginService operations include login, nickname check/set, first-character selection, server selection, session key, game-server list, and message-server list. GameService coverage includes rooms, lobbies, equipment, shot actions, hole statistics, shop, lockers, mail, quests, achievements, login bonus, events, and character mastery.

Primary indexes:

- [`loginservice/client/index.ksy`](../opensource-references/pangbox--packetdoc/src/packets/loginservice/client/index.ksy)
- [`loginservice/server/index.ksy`](../opensource-references/pangbox--packetdoc/src/packets/loginservice/server/index.ksy)
- [`gameservice/client/index.ksy`](../opensource-references/pangbox--packetdoc/src/packets/gameservice/client/index.ksy)
- [`gameservice/server/index.ksy`](../opensource-references/pangbox--packetdoc/src/packets/gameservice/server/index.ksy)
- [`messageservice/client/index.ksy`](../opensource-references/pangbox--packetdoc/src/packets/messageservice/client/index.ksy)
- [`messageservice/server/index.ksy`](../opensource-references/pangbox--packetdoc/src/packets/messageservice/server/index.ksy)

### PacketDoc limitations discovered

PacketDoc is a documentation oracle, not an unquestionable generated API:

- It explicitly labels itself WIP.
- Many fields remain `unknown`.
- The GameService server index imports `010e` but maps opcode `0x010c` to a type named `gameservice_server_010e_unknown_opponent_related_response`, illustrating why every mapping needs a fixture.
- Its files use project-specific `#pragma.template` directives, so a direct Kaitai compiler invocation is not the complete generation pipeline.

Recommendation: keep PacketDoc as a versioned source corpus, but implement only milestone-required packets in hand-reviewed Rust types. Each type must have a captured or upstream golden fixture. An experimental code-generation path may follow later.

### Golden vectors

PangCrypt includes client and server plaintext/ciphertext vectors, invalid-key tests, and undersized-buffer tests. These are the first Rust protocol acceptance fixtures:

- [`client_test.go`](../opensource-references/pangbox--pangcrypt/client_test.go)
- [`server_test.go`](../opensource-references/pangbox--pangcrypt/server_test.go)

Any adapted fixture must retain applicable ISC attribution.

---

## Authentication and connection flow

The best-supported U.S. flow is:

1. Client connects to LoginService.
2. LoginService sends plaintext hello with a 4-bit session transport key.
3. Client sends username plus an MD5-derived password string in opcode `0x0001`.
4. Server validates credentials and handles nickname/first-character setup if required.
5. Server creates a short-lived handover/session token.
6. Server sends login success, MessageService list, and GameService list.
7. Client selects a GameService and receives/uses a game session key.
8. Client disconnects from LoginService and connects directly to GameService.
9. GameService validates the handover token, loads player state, then exposes channel/lobby/room flows.

The concrete Pangbox flow is visible in [`pangbox/server/login/conn.go`](../opensource-references/pangbox--server/login/conn.go). It is useful but includes TODOs, so packet ordering must be checked against U.S. 852 captures.

### Credential recommendation

The legacy client protocol cannot be made modern on the wire without modifying the client. The server should therefore treat the client-supplied MD5 hex value as a **legacy transport secret**, normalize it, and store only an **Argon2id hash of that transport secret**. Never store the plaintext password or bare MD5 value and never log the login packet.

---

## Gameplay authority: corrected finding

The earlier report's “physics are entirely client-side” conclusion is directionally right but too broad. The evidence supports this more precise model:

### Client-authoritative data

The client submits:

- shot power/click/accuracy/spin/curve/angle details;
- ball position and shot result synchronization;
- Pang/bonus-Pang values carried in shot sync packets;
- hole geometry in at least one Pangbox path.

`alter-pangya` reads complete shot parameters, then broadcasts acknowledgements/ghost updates rather than integrating a trajectory:

- [`MatchPlayerShotCommitPacketHandler.kt`](../opensource-references/hex-agon--alter-pangya/game-server/src/main/kotlin/work/fking/pangya/game/packet/handler/match/MatchPlayerShotCommitPacketHandler.kt)
- [`MatchPlayerShotSyncPacketHandler.kt`](../opensource-references/hex-agon--alter-pangya/game-server/src/main/kotlin/work/fking/pangya/game/packet/handler/match/MatchPlayerShotSyncPacketHandler.kt)
- [`PracticeMatchDirector.kt`](../opensource-references/hex-agon--alter-pangya/game-server/src/main/kotlin/work/fking/pangya/game/room/match/PracticeMatchDirector.kt)

Pangbox similarly broadcasts shot commit/rotate/power/item packets and consumes synchronized result coordinates:

- [`game/server/conn.go`](../opensource-references/pangbox--server/game/server/conn.go)
- [`game/room/room.go`](../opensource-references/pangbox--server/game/room/room.go)

### Server-authoritative data

The server must still own and validate:

- authenticated identity and session handover;
- lobby/channel/room membership;
- room owner, settings, readiness, load progress, active turn, and timeouts;
- course/hole sequence, pin/weather/wind seed where supported;
- stroke count, hole completion, score, standings, and match lifecycle;
- inventory use, currencies, rewards, experience, achievements, and persistence;
- sanity checks on positions, claimed rewards, packet ordering, and duplicate/replayed actions.

### Engineering consequence

A playable MVP does **not** need a trajectory simulator. It does need a strict state machine and anti-abuse bounds. For local solo use, validation can initially be permissive; all currency/reward writes must still be server-derived and transactional. Never persist client-claimed Pang/EXP directly.

---

## Service topology findings

The ecosystem uses up to six roles:

| Role | Responsibility | MVP disposition |
|---|---|---|
| LoginService | Credentials, account setup, server list, handover token | Required |
| GameService | Player data, lobbies, rooms, gameplay, inventory/shop | Required |
| MessageService | Friends, presence, direct messages | Stub or deferred |
| AuthService | Internal validation between independently deployed services | In-process module initially; separate only when topology demands it |
| RankingService | Player/character/guild leaderboards | Deferred |
| Update/Admin HTTP | Updatelist, health, metrics, operator actions | Minimal health/metrics required; updater/admin later |

### Recommended initial topology

Use a **modular monolith** with independently bound LoginService and GameService TCP listeners in one Rust process. Keep crate boundaries so they can become separate binaries later. This avoids distributed-session and service-discovery complexity during protocol bring-up.

PostgreSQL is the system of record. Redis is not required for the first vertical slice; adding it before multi-instance operation would duplicate state without solving a demonstrated problem.

Docker Compose should provide:

- `pangya-server` Rust binary;
- PostgreSQL;
- optional observability profile;
- no bundled proprietary client or game data.

SuperSS Docker proves the multi-service/container route is practical, while Pangbox Minibox proves all-in-one local operation is useful. See:

- [`SuperSS-Dev-Docker/README.md`](../opensource-references/Acrisio-Filho--SuperSS-Dev-Docker/README.md)
- [`pangbox/server/README.md`](../opensource-references/pangbox--server/README.md)

---

## Persistence findings

Legacy schemas are broad and frequently stored-procedure-heavy. A Rust rewrite should not mirror those schemas one table or procedure at a time.

Pangbox's compact initial schema demonstrates the minimum core relations: player, inventory, character, equipment references, and expiring session. It supports PostgreSQL and SQLite at the repository layer:

- [`migrations/0001_initial.sql`](../opensource-references/pangbox--server/migrations/0001_initial.sql)
- [`migrations/0002_exp.sql`](../opensource-references/pangbox--server/migrations/0002_exp.sql)
- [`database/dialect.go`](../opensource-references/pangbox--server/database/dialect.go)

SuperSS's 394 Game Server DB command files demonstrate the eventual feature breadth but also warn against a one-command-class-per-query architecture.

Recommended persistence principles:

- PostgreSQL first; SQLx checked queries and embedded migrations.
- Explicit repositories organized by aggregate (`AccountRepo`, `InventoryRepo`, `MatchRepo`), not by individual stored procedure.
- Atomic transactions for starter grants, purchases, consumable use, mail attachment claims, and match rewards.
- Nonnegative database constraints for currency/stack counts.
- Idempotency keys for reward and item-grant operations.
- Static client data (IFF-derived catalog) versioned separately from mutable player state.
- No database calls in the hot path for every aim/shot-relay packet.

Current SQLx documentation confirms Tokio support, checked query macros, embedded `migrate!`, offline `.sqlx` metadata, transactions, and `#[sqlx::test]`.

---

## File-format findings

### IFF

IFF is required early because item IDs, characters, equipment, courses, and shop validation depend on client data. PangLib shows the common file header shape:

- `u16` record count;
- `u16` binding ID;
- `u32` version;
- fixed-size records filling the remainder of the file.

Reference: [`PangLib.IFF/IffFile.cs`](../opensource-references/retreev--PangLib/PangLib.IFF/IffFile.cs).

The Rust parser must avoid native struct transmutation. It should parse little-endian fields explicitly, validate record count/length arithmetic, preserve unknown bytes, and reject truncation/overflow.

### PAK and update files

PangFiles contains PAK readers, XTEA, region-key detection, PangYa CRC, and updatelist/lite-XML code. These are useful after the server needs asset inspection or a local updater, but they are not blockers for the first login/practice vertical slice if extracted IFF files are supplied.

Reference modules: [`pangbox/pangfiles`](../opensource-references/pangbox--pangfiles/).

---

## Rust technology research and recommendation

| Concern | Recommended baseline | Why | Gate / caution |
|---|---|---|---|
| Async runtime | Tokio | Mature TCP, tasks, timers, synchronization, graceful shutdown ecosystem | Pin an MSRV and avoid blocking work on runtime threads |
| Framing | `tokio-util::codec::{Decoder, Encoder}` + `bytes` | Correct incomplete/multiple-frame buffering and bounded codecs | Custom codec is needed because PangYa length semantics differ by direction |
| LZO | `lzokay` 2.x | Pure Rust, MIT, compression + decompression, no external deps | Must decompress PangCrypt vectors and be accepted by real U.S. 852 client |
| Packet parsing | Hand-reviewed `bytes::Buf`/`BufMut` types initially | Transparent bounds checks and easier fixture review | Kaitai Rust codegen remains an experiment, not the MVP dependency |
| Errors | `thiserror` in libraries; `anyhow` in binaries only | Typed recovery in crates, contextual startup errors in entry points | No `unwrap`/`expect` in production paths |
| Database | SQLx + PostgreSQL | Async pool, checked queries, migrations, transactions, test support | Commit offline query metadata for reproducible CI |
| Config | `serde` + TOML/environment overrides | Typed, operator-friendly config | Secrets only via environment/file secret mounts |
| Logging | `tracing` + structured fields | Connection/packet/match correlation without string logs | Packet bodies and credentials redacted by default |
| Password storage | `argon2` | Modern at-rest hashing of legacy transport secret | Parameter set must be benchmarked and versioned |
| Testing | built-in tests, `proptest`, `cargo-fuzz`, SQLx tests | Binary parsers require golden, property, fuzz, and integration coverage | Real client remains a manual release gate |

Tokio Util's current codec guidance specifically requires returning `Ok(None)` for incomplete frames, preserving bytes after a complete frame, and rejecting over-limit lengths before reservation. That behavior maps directly to the PangYa decoder.

### Proposed workspace shape

```text
pangya-rs/
├── Cargo.toml
├── crates/
│   ├── pangya-crypto/        # oracle tables, XOR transform, LZO adapter
│   ├── pangya-protocol/      # framing, reader/writer, versions, packet types
│   ├── pangya-data/          # IFF first; PAK/updatelist later
│   ├── pangya-domain/        # IDs, accounts, inventory, room/match state
│   ├── pangya-storage/       # SQLx repositories and migrations
│   ├── pangya-login/         # LoginService application logic
│   ├── pangya-game/          # GameService, lobby and room actors
│   ├── pangya-message/       # MessageService (later milestone)
│   ├── pangya-observability/ # tracing/metrics helpers
│   └── pangya-server/        # binary and topology composition
├── fixtures/                 # attributed protocol vectors/captures, no secrets
├── config/
├── deploy/
└── docs/
```

---

## Licensing and provenance findings

“Use existing repos as a base” must mean **behavioral and architectural reference**, not indiscriminate source translation.

### Safe default

Adopt a clean-room process:

1. Record the upstream fact or behavior in research/spec/fixture documentation.
2. Implement it independently in Rust.
3. Add a differential or golden test.
4. Record direct adaptations in a provenance file and retain required notices.

### Repository-specific constraints

- SuperSS root: MIT.
- Pangbox protocol/tool projects: generally ISC; `pangbox/server` has documented file-level exceptions.
- hsreina server/sample: Apache-2.0.
- Py_Source_US: GPL-3.0.
- PangLib: AGPL-3.0.
- Pangya-Server-Community: root documentation says MIT, but `Server/` has its own AGPL-3.0 license.
- alter-pangya and SuperSS-Dev-Docker: no repository license found in the snapshots; no source copying.
- Rugburn: ISC code plus separately licensed Intel IJL material.
- Existing pure Rust `lzo1x` ports found in crates.io research are GPL; `lzokay` is the permissive candidate.

The final project license remains a project-owner decision. Until chosen, implementation should remain compatible with a permissive MIT/Apache-2.0 option by avoiding GPL/AGPL code copying. This is an engineering policy, not legal advice.

---

## Risks and unknowns

| Risk | Evidence | Mitigation / proof required |
|---|---|---|
| LZO output accepted by client | PangCrypt uses GPL Go LZO; Rust candidate differs | Decompress known server vectors, round-trip randomized data, then U.S. 852 login smoke test |
| PacketDoc incompleteness/typos | WIP status, unknown fields, index inconsistency | Golden fixture per packet; unknown-byte preservation; capture comparison |
| Client-authoritative abuse | Shot/result/Pang fields arrive from client | Derive rewards server-side; validate state/order/ranges; local-first bind defaults |
| Region drift | Feature-rich refs are often JP; target is U.S. 852 | Versioned packet registry and U.S. fixtures as release gates |
| License contamination | Mixed MIT/ISC/Apache/GPL/AGPL/no-license references | Provenance ledger, dependency audit, clean-room implementation rules |
| Proprietary assets | Servers need client IFF/data; Rugburn includes separate IJL terms | Never commit/distribute client assets; user-mounted data path |
| Scope explosion | Full SuperSS feature breadth is large | Vertical milestones; “packet implemented” requires an end-to-end user outcome |
| Stale upstream claims | Some README completion percentages are self-reported and old | Treat as hints; trust source inspection and real-client tests |
| Over-distributed design | Legacy stacks split 4–6 processes | Modular monolith first; split only after measured scaling/ops need |
| Unknown legal/trademark posture | PangYa assets and marks are third-party | Preservation/education positioning, trademark disclaimer, no official affiliation |

---

## Recommended first proof sequence

Do not begin with the full database or feature port. Prove these in order:

1. **Crypto fixture parity**
   - Port the two 4,096-byte oracle tables with provenance.
   - Pass all PangCrypt client-decrypt/encrypt vectors.
   - Decompress all PangCrypt server vectors with `lzokay`.
2. **Framing safety**
   - Handle fragmented headers, fragmented bodies, and multiple frames per read.
   - Reject invalid keys, lengths, oversized decompression, and malformed LZO without panic.
3. **Typed login packets**
   - Parse/encode only hello, login, login result, nickname/character setup, server list, and handover packets.
4. **Synthetic client integration**
   - Exercise LoginService through a Rust test client over loopback.
5. **Real U.S. 852 smoke test**
   - Rugburn redirects client to local listeners.
   - Client reaches account/nickname/character flow and displays server selection.
6. **Game handover**
   - Client connects to GameService and receives enough player data to enter a channel.
7. **Solo practice vertical slice**
   - Create room, load one hole, relay/synchronize shots, finish, derive and persist result exactly once.

Only after step 7 should shop, broad inventory, social, ranking, guild, and event systems begin.

---

## Research-backed answer to “what should be rewritten?”

### Must be original Rust implementation

- transport framing, encryption adapter, LZO boundary;
- typed packet reader/writer and version registry;
- login/game/message service application logic;
- room and match state machines;
- account/inventory/equipment/reward persistence;
- safe IFF parser and static data catalog;
- configuration, observability, admin controls, tests, and deployment.

### May be retained as external tooling

- Rugburn for client redirection/anti-cheat disabling;
- Wireshark with Pangbox dissector for capture inspection;
- Pantrant for legacy cassette analysis when useful;
- original clients and extracted data supplied by the operator.

### Deferred until the core loop is proven

- full PAK editing/updater;
- MessageService depth, guilds, ranking, mail attachments;
- advanced shops/gacha/events/quests/achievements;
- every SuperSS game mode;
- multi-region compatibility;
- multi-instance horizontal scaling;
- browser administration UI.

---

## Primary source map

| Question | Start here |
|---|---|
| Exact transport algorithm | [`pangbox--pangcrypt/client.go`](../opensource-references/pangbox--pangcrypt/client.go), [`server.go`](../opensource-references/pangbox--pangcrypt/server.go), [`common.go`](../opensource-references/pangbox--pangcrypt/common.go) |
| Crypto fixtures | [`pangbox--pangcrypt/client_test.go`](../opensource-references/pangbox--pangcrypt/client_test.go), [`server_test.go`](../opensource-references/pangbox--pangcrypt/server_test.go) |
| Packet/opcode names and layouts | [`pangbox--packetdoc/src/packets`](../opensource-references/pangbox--packetdoc/src/packets/) |
| U.S. 852 modern flow | [`hex-agon--alter-pangya`](../opensource-references/hex-agon--alter-pangya/) |
| Typed Go service/room example | [`pangbox--server`](../opensource-references/pangbox--server/) |
| Full feature taxonomy | [`SuperSS-Dev/Server Lib`](../opensource-references/Acrisio-Filho--SuperSS-Dev/Server%20Lib/), [`Pangya-Server-Community/Server`](../opensource-references/luismk--Pangya-Server-Community/Server/) |
| U.S. S8 legacy feature behavior | [`K4T--Py_Source_US`](../opensource-references/K4T--Py_Source_US/) |
| IFF model | [`PangLib.IFF/IffFile.cs`](../opensource-references/retreev--PangLib/PangLib.IFF/IffFile.cs) |
| PAK/XTEA/updatelist | [`pangbox--pangfiles`](../opensource-references/pangbox--pangfiles/) |
| Client support/redirection | [`pangbox--rugburn/README.md`](../opensource-references/pangbox--rugburn/README.md) |
| Reference revisions/licenses | [`opensource-references/README.md`](../opensource-references/README.md) |

---

## Decision carried into the specification

Proceed with a **U.S. 852, local-first, clean-room Rust modular monolith**. Use PostgreSQL for durable state, no Redis in the first vertical slice, room actors for deterministic mutation, hand-reviewed packet types with fixtures, and a strict compatibility gate before every new feature phase.

See [`SPEC.md`](SPEC.md) for normative requirements and [`PLAN.html`](PLAN.html) for the visual execution plan.
