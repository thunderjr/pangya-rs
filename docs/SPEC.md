# PangYa-RS — product and technical specification

> Version: **0.1.0 (planning baseline)**
>
> Date: **2026-08-05**
>
> Status: **Proposed planning baseline; implementation not started**
>
> Compatibility target: **PangYa U.S. Season 8 — GB.852 / US 852.00**
>
> Related: [`INITIAL_RESEARCH.md`](INITIAL_RESEARCH.md) · [`PLAN.html`](PLAN.html) · [`PROGRESS.md`](PROGRESS.md) · [`MEMORY.md`](MEMORY.md)

---

## 1. Purpose

PangYa-RS is a clean-room Rust implementation of the server-side services needed to run a preserved PangYa U.S. 852 client in a private, local-first environment. It replaces legacy C++, C#, Kotlin, Go, Delphi, and PHP server implementations with a safe, testable, observable Rust codebase while preserving wire compatibility with the unmodified game client.

This specification covers:

- product scope and compatibility promises;
- transport, packet, service, game-state, and persistence architecture;
- security and licensing boundaries;
- Rust workspace organization and dependency policy;
- deployment, observability, and operations;
- phased feature delivery and acceptance criteria;
- long-term breadth expected by “rewrite everything.”

It does not authorize distribution of proprietary clients or assets.

### 1.1 Normative language

**MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** are normative requirements. Uncertainty is written explicitly as a validation gate rather than silently treated as fact.

---

## 2. Product definition

### 2.1 Vision

A developer can clone PangYa-RS, supply a legally obtained U.S. 852 client and extracted game data, start the local stack with one command, redirect the client with Rugburn, create an account, enter the game, play and finish a practice/stroke session, and retain progress across restarts.

### 2.2 Primary user

A preservation-minded developer/operator who:

- owns or has access to a U.S. 852 client;
- can run Docker or native Rust/PostgreSQL;
- wants an auditable local/private server;
- values protocol correctness and maintainability over rapid feature hacks.

### 2.3 Product principles

1. **Compatibility before breadth.** A small set of fixture-proven packets is better than hundreds of guessed handlers.
2. **Local-first and safe by default.** Listeners bind to loopback unless explicitly configured otherwise.
3. **Server authority where it matters.** Identity, state transitions, currency, inventory, rewards, and persistence are authoritative even though shot trajectory is client-computed.
4. **Make invalid states difficult.** Typed IDs, explicit state machines, bounded actors, checked arithmetic, and transactions are mandatory.
5. **No proprietary payloads.** Client binaries, game assets, and unfree IJL binaries are operator-supplied.
6. **Evidence and provenance.** Every implemented opcode has a fixture/provenance note; every direct source adaptation retains its license notice.
7. **Modular monolith first.** Preserve service boundaries in crates without introducing distributed-systems complexity before it is needed.

---

## 3. Scope

### 3.1 Release tiers

#### Tier A — protocol proof

- PangYa client/server frame decode and encode.
- Per-service hello packet support for U.S. 852.
- PangCrypt golden vectors.
- Safe LZO compression/decompression boundary.
- Typed minimum LoginService packet set.
- Synthetic TCP test client.

#### Tier B — first playable release

- LoginService and GameService listeners.
- Account creation through operator CLI or development auto-create mode.
- Login, nickname, first character, server selection, and handover.
- Player profile, starter character/equipment, and durable sessions.
- One channel/lobby.
- Room create/join/leave/ready.
- Practice and basic stroke room lifecycle.
- Shot action/result relay, turn/hole/match progression.
- Server-derived score, Pang, and EXP persistence.
- Minimal inventory/equipment load and save.
- Health, readiness, metrics, structured logs, Docker Compose.

#### Tier C — useful private server

- Multiplayer stroke/tournament behavior.
- Shop purchase and equipment management validated against IFF.
- Course records and player statistics.
- MessageService basics: presence, friends, direct messages.
- Mail without complex attachments, then transactional attachments.
- Rankings.
- Operator CLI/admin API.

#### Tier D — broad legacy feature parity

- Additional modes: versus, match, tourney, approach, chip-in practice, Pang Battle, Guild Battle, Grand Prix, Grand Zodiac, Special Shuffle Course.
- Guilds, personal shop, locker, cards, caddies, mascots, MyRoom furniture/UCC.
- Quests, achievements, daily/login rewards, memorial/papel/scratch systems, rentals, events, drop/treasure systems.
- Ranking and internal auth services as independent deployables if required.
- Multi-region/version adapters after U.S. 852 remains stable.

### 3.2 Explicit non-goals for the first playable release

- Reimplementing client ball-flight physics.
- GameGuard emulation or connection to an official server.
- Public internet hosting hardening beyond documented safe defaults.
- Bundling or downloading proprietary clients/assets.
- Pixel-identical web portal or launcher.
- Full anti-cheat detection.
- Horizontal GameService scaling.
- Redis, NATS, or Kubernetes.
- Every PacketDoc opcode.

---

## 4. Compatibility contract

### 4.1 Supported client

The initial supported client is **U.S. 852.00 / GB.852**. A release MUST document:

- client build identifier;
- expected Rugburn version/configuration;
- required extracted IFF data version/hash;
- implemented service endpoints and ports;
- known broken screens/features.

### 4.2 Region/version abstraction

The protocol layer MUST model:

```rust
pub enum Region {
    Us,
    // Added only with fixtures and integration coverage.
}

pub enum ClientVersion {
    Us852,
}

pub enum ServiceKind {
    Login,
    Game,
    Message,
    Auth,
    Ranking,
}
```

Packet layouts, hello layouts, encodings, and feature flags MUST be selected using a compatibility profile. Handlers MUST NOT scatter checks such as `if version == 852` across business logic.

### 4.3 Backward compatibility

Before the first stable release, protocol API changes are allowed. After `1.0`:

- database migrations MUST be forward-only in released artifacts;
- config deprecations MUST have at least one release of warnings;
- saved player data MUST survive upgrades;
- supported client behavior MUST be regression-tested by fixtures and a release smoke checklist.

---

## 5. System context

```text
┌───────────────────────────── Operator machine ─────────────────────────────┐
│                                                                            │
│  PangYa US 852 client                                                      │
│        │ Winsock / WinHTTP                                                 │
│        ▼                                                                   │
│  Rugburn (external) ─────── redirects to loopback ─────────────────────┐    │
│                                                                       │    │
│  ┌──────────────────────── PangYa-RS process ───────────────────────┐  │    │
│  │ Login TCP :10103 ◄──────────────────────────────────────────────┘  │    │
│  │ Game TCP  :20201                                                  │    │
│  │ Message TCP :30303 (later)                                        │    │
│  │ Health/metrics HTTP :8080                                         │    │
│  │ Client patch/theme HTTP :8090 (required before client startup)    │    │
│  │                                                                   │    │
│  │ protocol → application services → domain/room actors → storage    │    │
│  └───────────────────────────┬───────────────────────────────────────┘    │
│                              │ SQLx                                      │
│                              ▼                                           │
│                       PostgreSQL :5432                                   │
│                                                                          │
│  Operator-supplied extracted IFF directory ──read-only──► data catalog   │
└──────────────────────────────────────────────────────────────────────────┘
```

### 5.1 Runtime topology

The first playable release MUST ship one binary, `pangya-server`, that can bind multiple listeners. Each listener composes a separate application service and protocol profile.

Separate binaries MAY be introduced later without moving domain rules into transport crates.

### 5.2 Process responsibilities

- Accept TCP connections and enforce resource limits.
- Send service-specific hello and negotiate the per-connection transport key.
- Decode, validate, dispatch, and encode packets.
- Run login/game/message application workflows.
- Serialize mutation of each room/match.
- Persist durable player state atomically.
- Export health, metrics, and structured events.
- Shut down gracefully without accepting new sessions or duplicating rewards.

---

## 6. Rust workspace architecture

### 6.1 Required workspace

```text
crates/
├── pangya-crypto/
├── pangya-protocol/
├── pangya-data/
├── pangya-domain/
├── pangya-storage/
├── pangya-login/
├── pangya-game/
├── pangya-message/
├── pangya-observability/
└── pangya-server/
```

Optional later crates:

```text
├── pangya-ranking/
├── pangya-admin/
├── pangya-updater/
├── pangya-test-client/
└── pangya-tools/
```

### 6.2 Dependency direction

```text
pangya-server
  ├── pangya-login ─┐
  ├── pangya-game ──┼──► pangya-domain ──► (std only where practical)
  ├── pangya-message┘          │
  │                            └── interfaces used by pangya-storage
  ├── pangya-storage ─────────────► SQLx/PostgreSQL
  ├── pangya-protocol ────────────► pangya-crypto, bytes
  ├── pangya-data
  └── pangya-observability
```

Rules:

- Domain types MUST NOT depend on Tokio, SQLx, or wire packet types.
- Protocol crates MUST NOT query the database.
- Application crates MAY depend on repository traits and concrete storage only at composition boundaries.
- Storage MUST translate rows into domain types and MUST NOT expose SQLx rows as public API.
- No cyclic crate dependencies.

### 6.3 Rust edition and MSRV

- Use Rust edition **2024**.
- Set `rust-version` in the workspace manifest.
- Initial MSRV SHOULD be the latest stable compiler available when implementation starts; changes require a changelog entry.
- CI MUST test the declared MSRV and current stable.

### 6.4 Workspace lint policy

At minimum:

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"

[workspace.lints.clippy]
all = { level = "deny", priority = 10 }
redundant_clone = "deny"
large_enum_variant = "warn"
needless_collect = "warn"
```

Required checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo test --doc --workspace --locked
cargo deny check
```

A lint suppression MUST use `#[expect(...)]` locally with a reason.

### 6.5 Error policy

- Library crates MUST expose typed errors with `thiserror`.
- Binary startup/composition MAY use `anyhow` for context.
- Production code MUST NOT use `unwrap()` or `expect()`.
- Parser errors MUST include direction, service, opcode when known, field offset, and error class without including secrets.
- Connection-level malformed input MUST terminate only that connection, not the process.

### 6.6 Unsafe code

Workspace-owned code MUST remain `unsafe`-free for initial releases. If a future performance need requires unsafe code:

1. isolate it in one audited crate/module;
2. add property/fuzz/Miri tests;
3. document invariants in a `# Safety` section;
4. record an ADR and independent review.

---

## 7. Dependency baseline

| Concern | Dependency | Requirement |
|---|---|---|
| Runtime/network | `tokio` | TCP, tasks, timers, signals; selected features only |
| Framing | `tokio-util` | Custom `Decoder`/`Encoder` and cancellation token |
| Buffers | `bytes` | `Bytes`/`BytesMut`, explicit LE operations |
| Futures | `futures-util` | Stream/Sink extensions where needed |
| LZO | `lzokay` 2.x candidate | Adopt only after protocol spike gates pass |
| Errors | `thiserror`, binary-only `anyhow` | Typed libraries, contextual entry point |
| Database | `sqlx` | PostgreSQL, Tokio runtime, macros, migrations |
| Serialization/config | `serde`, `toml` | Typed config and admin payloads |
| Secrets | `secrecy`, `zeroize` where applicable | Prevent accidental formatting and reduce secret lifetime |
| Password hash | `argon2` | Hash normalized legacy transport secrets |
| Randomness | `rand` with OS-backed RNG | Salts and nonces; tokens use CSPRNG |
| IDs | `uuid` or typed integer newtypes | UUID for external idempotency/audit; DB bigints for legacy IDs |
| Logging | `tracing`, `tracing-subscriber` | Structured spans/events |
| Metrics | `metrics` ecosystem or OpenTelemetry adapter | Stable names and low-cardinality labels |
| Testing | `proptest`, `insta` selectively, `testcontainers` or SQLx test support | Parser properties, small snapshots, Postgres integration |

Dependencies MUST pass `cargo deny` license/advisory/source checks. Git dependencies require an ADR and pinned revision.

---

## 8. Transport and crypto specification

### 8.1 Connection lifecycle

```text
Accepted
  └─► HelloSent(key)
        └─► AwaitingFirstPacket
              └─► ServiceAuthenticated
                    └─► Active
                          └─► Draining
                                └─► Closed
```

A connection MUST own exactly one immutable `CompatibilityProfile` and one transport key for its lifetime.

### 8.2 Hello

- Hello is plaintext and service/version-specific.
- The key MUST be selected uniformly from `0x00..=0x0f` using an OS-backed RNG.
- A deterministic key MAY be injected only in tests.
- Hello packet templates MUST be fixture-tested for each `(service, version)`.

### 8.3 Client frame decode

The codec MUST:

1. wait for at least 4 bytes before reading the length;
2. calculate total frame length using PangYa's client length semantics;
3. reject total lengths below the 5-byte header;
4. reject lengths above `max_client_frame_bytes` before reserving;
5. return `Ok(None)` for incomplete frames without discarding bytes;
6. preserve subsequent frames in the buffer;
7. decrypt into an owned or uniquely mutable bounded buffer;
8. require at least a 2-byte opcode after decryption;
9. emit a typed `InboundFrame { opcode, payload, metadata }`.

### 8.4 Server frame encode

The codec MUST:

1. serialize opcode + payload;
2. reject plaintext beyond `max_server_plaintext_bytes`;
3. LZO1X-compress;
4. reject compressed output that cannot fit protocol length semantics;
5. calculate original-size metadata with checked arithmetic;
6. apply the PangYa encryption transform;
7. append to the provided output buffer without overwriting queued frames.

### 8.5 Limits

Initial configurable defaults:

| Limit | Default | Notes |
|---|---:|---|
| Client encrypted frame | 65,539 bytes maximum implied by `u16` length semantics; operational cap 65,535 | Validate exact edge behavior in spike |
| Server encrypted frame | Protocol-derived `u16` bound | Fail encode rather than truncate |
| Decompressed server/plain packet | 8 MiB | Conservative until inventory fixtures establish lower bound |
| Decompression expansion ratio | 128× | Both absolute and ratio limits apply |
| Buffered bytes per connection | 256 KiB before backpressure/close | Large outbound inventory flow requires measured adjustment |
| Outbound queue | 256 messages per connection | Bounded; policy differs by packet criticality |
| Login timeout | 15 seconds | Configurable |
| Idle timeout | 120 seconds | Keepalive-aware, configurable |

No buffer reservation may be derived from untrusted input without a prior cap.

### 8.6 Oracle tables

- Store the two `[u8; 4096]` tables in `pangya-crypto`.
- Record origin and ISC attribution from PangCrypt.
- Expose no mutable access.
- Add a compile-time or test hash assertion so accidental edits are detected.

### 8.7 LZO acceptance gates

`lzokay` MUST NOT become the production default until all pass:

- known PangCrypt server ciphertext decompresses to the expected plaintext;
- generated output round-trips through `lzokay` for boundary/property cases;
- independent LZO implementation can decompress generated output in a test tool;
- U.S. 852 accepts server packets through login and channel entry;
- malformed streams never panic or allocate beyond configured limits.

Exact compressed byte equality is not required if streams are valid and accepted by the client.

### 8.8 Crypto fixture requirements

At minimum, port with attribution:

- all three PangCrypt client vectors;
- representative PangCrypt server vectors including repetitive data;
- invalid key `0x10`;
- undersized headers;
- truncated compressed body;
- randomized round-trip properties for valid key/salt/data ranges.

---

## 9. Packet model and registry

### 9.1 Packet traits

Conceptual API:

```rust
pub trait DecodePacket: Sized {
    const OPCODE: u16;
    fn decode(reader: &mut PacketReader<'_>, profile: &CompatibilityProfile)
        -> Result<Self, PacketDecodeError>;
}

pub trait EncodePacket {
    const OPCODE: u16;
    fn encode(
        &self,
        writer: &mut PacketWriter,
        profile: &CompatibilityProfile,
    ) -> Result<(), PacketEncodeError>;
}
```

Static dispatch SHOULD be preferred. Dynamic handler registration MAY be used at the application boundary if it materially simplifies dispatch and is measured not to affect the hot path.

### 9.2 Reader/writer requirements

`PacketReader` MUST provide checked methods for:

- LE signed/unsigned integers;
- LE `f32`/`f64` preserving bit pattern;
- fixed-size byte arrays;
- length-prefixed PangYa strings;
- fixed-width NUL-terminated strings;
- bounded vectors/counts;
- unknown-tail capture;
- current offset and remaining bytes.

`PacketWriter` MUST:

- reject string/count overflow;
- write explicit encodings and terminators;
- use checked size conversions;
- never silently truncate unless the specific protocol field documents truncation and has a test.

### 9.3 String policy

- U.S. 852 packet definitions default to ASCII-compatible fields.
- Wire strings MUST be stored as raw bytes until decoded by a field-specific rule.
- Invalid text MUST produce a typed error or lossless escaped diagnostic; never use unchecked UTF-8 conversion.
- Nickname normalization and allowed characters are domain rules separate from wire decoding.

### 9.4 Unknown fields

Unknown bytes MUST be represented explicitly:

```rust
pub struct UnknownBytes<const N: usize>(pub [u8; N]);
```

Do not label an unknown field with a guessed semantic name until confirmed by at least two independent observations or upstream evidence plus a fixture.

### 9.5 Registry

The registry key is:

```text
(service, direction, client_version, connection_state, opcode)
```

The registry MUST prevent an opcode valid in one state from being accepted blindly in another.

### 9.6 Unknown opcode policy

- During login/handover: log a redacted diagnostic and close the connection.
- In lobby/room: count and log at debug/warn level; configurable `disconnect`, `ignore`, or `capture` behavior, default `disconnect` until a packet is classified.
- In production, raw capture is disabled unless an operator explicitly enables a bounded, redacted capture sink.

### 9.7 Minimum packet set

#### LoginService inbound

- login (`0x0001`);
- select server (`0x0003`);
- set nickname (`0x0006`);
- check nickname (`0x0007`);
- select character (`0x0008`);
- reconnect (`0x000b`) only after initial flow is stable.

#### LoginService outbound

- login/result (`0x0001`);
- game server list (`0x0002`);
- session key (`0x0003`);
- chat macros (`0x0006`) as empty/default if required;
- message server list (`0x0009`) as empty/stub until service exists;
- login key (`0x0010`) if required by capture order.

#### GameService first playable set

Implement only packets needed for:

- handover/authentication;
- player roster/profile/inventory/equipment;
- channel/lobby entry;
- room create/join/list/settings/ready/leave;
- loading progress and hole setup;
- shot commit/rotate/power/club/item/sync/turn-end;
- hole finish and match results;
- Pang/EXP/equipment updates.

The exact opcode list MUST be frozen after the Tier A capture/spike and recorded in `docs/protocol/US_852_MVP_PACKETS.md`.

---

## 10. Application services

### 10.1 LoginService

#### Responsibilities

- Authenticate account credentials.
- Apply login rate limits and bans.
- Drive nickname and first-character setup.
- Generate a short-lived GameService handover token.
- Return available service endpoints.
- Enforce duplicate-login policy.

#### Functional requirements

- **FR-LOGIN-001:** LoginService MUST send the U.S. 852 hello immediately after accept.
- **FR-LOGIN-002:** It MUST accept only login/setup packets valid for the current login state.
- **FR-LOGIN-003:** It MUST normalize the client MD5 hex transport secret consistently before Argon2 verification.
- **FR-LOGIN-004:** Development auto-create mode MUST default off outside the `local` profile.
- **FR-LOGIN-005:** Account creation MUST atomically create credentials, profile, starter inventory, and setup status or roll back.
- **FR-LOGIN-006:** Nickname uniqueness MUST be enforced by a database unique constraint and friendly protocol error.
- **FR-LOGIN-007:** Handover tokens MUST be random, short-lived, single-use, and stored hashed or HMAC-protected.
- **FR-LOGIN-008:** The service list MUST advertise configured addresses, never infer an unusable container-internal address.
- **FR-LOGIN-009:** Credential packets MUST never appear in logs, traces, metrics, or captures by default.

#### Login state machine

```text
AwaitLogin
  ├─ bad credentials ─► Rejected ─► Closed
  └─ valid
      ├─ needs nickname ─► AwaitNicknameCheck/Set
      ├─ needs character ─► AwaitCharacterSelect
      └─ ready ─► IssueHandover ─► AwaitServerSelect ─► Complete ─► Closed
```

Retries are bounded. A packet from an invalid state closes the connection with a diagnostic counter.

### 10.2 GameService

#### Responsibilities

- Validate handover and load player snapshot.
- Maintain connection/session presence.
- Manage channels, lobbies, and rooms.
- Dispatch gameplay actions to room actors.
- Validate inventory/equipment use.
- Commit match results exactly once.

#### Functional requirements

- **FR-GAME-001:** A handover token MUST be consumed atomically; replay fails.
- **FR-GAME-002:** Player data MUST be loaded into a coherent snapshot before channel entry.
- **FR-GAME-003:** One connection MUST belong to at most one channel and one room.
- **FR-GAME-004:** Disconnect cleanup MUST remove presence/room membership even after handler errors.
- **FR-GAME-005:** High-frequency shot/aim relay MUST not perform database I/O.
- **FR-GAME-006:** Currency, inventory, and reward changes MUST be server-calculated and transactional.
- **FR-GAME-007:** Duplicate client finish packets MUST not duplicate rewards.
- **FR-GAME-008:** Unsupported features MUST return a client-safe response or remain inaccessible; they MUST NOT hang the client knowingly.
- **FR-GAME-009:** A room actor panic/error MUST not terminate unrelated rooms or listeners.

### 10.3 MessageService

Tier B MAY advertise no MessageService endpoints if the client tolerates it; otherwise it MUST provide a safe handshake stub.

Tier C requirements:

- credential declaration/handover validation;
- online/offline presence;
- friend list and requests;
- direct text messages;
- bounded offline message storage;
- block/mute enforcement;
- no global unbounded broadcast.

### 10.4 AuthService

The internal handover validation contract starts as a Rust interface implemented in-process. A separate AuthService is deferred.

```rust
pub trait HandoverStore: Send + Sync {
    async fn issue(&self, request: IssueHandover) -> Result<HandoverToken, HandoverError>;
    async fn consume(&self, token: SecretString, target: ServiceKind)
        -> Result<AuthenticatedSession, HandoverError>;
}
```

If separated later, the network API MUST be mutually authenticated and inaccessible from the public listener interface.

### 10.5 RankingService

Deferred until course results are stable. Rankings are derived projections, not primary match truth. Rebuilding a ranking projection from durable results SHOULD be possible.

### 10.6 Admin/health HTTP

Tier B endpoints:

- `GET /health/live` — process event loop alive;
- `GET /health/ready` — config/data catalog/database/listeners ready;
- `GET /metrics` — optional configured exposition;
- no mutation endpoints.

Tier C admin mutation requires authentication, authorization, CSRF protection for browser use, audit logs, and a separate bind address.

---

## 11. Concurrency and state ownership

### 11.1 Connection tasks

Each TCP connection runs one supervised task with:

- framed read stream;
- bounded outbound sender;
- cancellation token;
- service-specific session state;
- connection ID and structured tracing span.

Read and write halves MAY be separate tasks, but ownership and shutdown semantics must be explicit.

### 11.2 Room actors

Each room owns all mutable room and match state in one task. Commands arrive through a bounded MPSC channel.

```rust
pub enum RoomCommand {
    Join(JoinRoom),
    Leave(PlayerConnectionId),
    SetReady(SetReady),
    UpdateSettings(UpdateRoomSettings),
    StartGame(StartGame),
    LoadingProgress(LoadingProgress),
    Shot(ShotEvent),
    FinishHole(FinishHole),
    Disconnect(PlayerConnectionId),
    Shutdown,
}
```

Rules:

- Only the actor mutates room state.
- Commands include actor-validated identity; never trust packet-supplied player IDs.
- Every state transition is deterministic for a given command stream and RNG seed.
- Outbound events are produced after state mutation and validation.
- Queue saturation has a defined policy: reject noncritical updates first, disconnect abusive producers, never allocate an unbounded queue.
- Room shutdown cancels timers and releases all connection references.

### 11.3 Registries

Global registries store only discovery handles and immutable summaries:

- connected player → connection handle;
- room ID → room actor handle;
- channel → room summary index.

Hot mutable match state MUST NOT be shared through `Arc<Mutex<Room>>`.

### 11.4 Blocking work

IFF loading, large decompression experiments, and expensive password hashing MUST use startup time or `spawn_blocking`/bounded worker pools as appropriate. No unbounded blocking task creation.

---

## 12. Game domain and state machines

### 12.1 Typed IDs

All identifiers MUST use newtypes, not interchangeable integers:

```rust
pub struct AccountId(i64);
pub struct CharacterId(i64);
pub struct InventoryItemId(i64);
pub struct ItemTypeId(u32);
pub struct ConnectionId(u32);
pub struct RoomId(u32);
pub struct MatchId(uuid::Uuid);
```

Database conversion checks range and sign.

### 12.2 Room lifecycle

```text
Open
  ├─ join/leave/settings/ready
  └─ owner starts
       ▼
Loading
  ├─ progress/disconnect/timeout
  └─ all required players ready
       ▼
InGame
  ├─ HoleSetup
  ├─ AwaitTurn
  ├─ ShotInProgress
  ├─ ShotSynchronized
  ├─ HoleComplete
  └─ next hole or Results
       ▼
ResultsPendingCommit
  ├─ commit exactly once
  └─ broadcast balances/standings
       ▼
Open or Closed
```

Illegal transitions return a domain error and a protocol-safe outcome.

### 12.3 Match authority

The room/match actor owns:

- participant roster captured at start;
- course and ordered hole list;
- deterministic seed, pin/weather/wind configuration;
- active player/turn order;
- per-hole stroke count and completion state;
- score/standings;
- item-use authorization snapshot;
- match result idempotency key.

### 12.4 Client-computed shot data

The server MAY relay client shot parameters/results after validating:

- sender is a participant;
- sender is the active player when mode requires it;
- packet is valid in the current phase;
- finite floats only (`!NaN`, `!±Inf`);
- coordinates/power/spin/curve are within configurable sanity bounds;
- item use exists in the captured inventory and is allowed;
- duplicate sequence/action is idempotent or rejected.

The server MUST NOT credit client-claimed Pang, bonus Pang, EXP, or items directly. Tier B reward formulas may be simplified but must be server-side and documented.

### 12.5 Disconnect/reconnect

Tier B:

- disconnect removes the player safely;
- solo disconnect aborts without rewards unless completion was already committed;
- multiplayer mode applies a documented quit/forfeit policy;
- no in-match reconnect requirement.

Tier C MAY add reconnect using a match reservation token and bounded grace period.

### 12.6 Randomness

- Security tokens use OS CSPRNG.
- Match generation uses a seed captured in match state for reproducibility.
- Reward RNG uses server-side RNG and persists enough audit data to investigate outcomes.
- Tests can inject deterministic RNG implementations.

---

## 13. Data catalog and IFF

### 13.1 Startup data flow

1. Resolve configured client data directory.
2. Read a manifest of required IFF files.
3. Validate file presence, hash, header, record arithmetic, and expected version/profile.
4. Parse into immutable typed catalog.
5. Cross-check configured starter items and course IDs.
6. Publish catalog through `Arc<DataCatalog>` only after full success.
7. Report readiness false on failure.

### 13.2 Parser safety

The parser MUST:

- use explicit little-endian reads;
- use checked multiplication/addition for `count × record_size`;
- reject zero-record divide cases and trailing/truncated data unless documented;
- preserve unknown fields as bytes;
- never transmute disk bytes into native structs;
- impose file and record count caps;
- fuzz every parser entry point.

### 13.3 Static vs mutable data

- Static catalog: item definitions, characters, courses, shop metadata.
- A real U.S. client `Course` record is a presentation row and carries **no par**; per-hole par lives in the course's own PAK data. Where a mode needs par, it MUST be operator-declared and cross-checked against the catalog for course existence. A catalog MUST NOT be made to invent one.
- Mutable state: owned inventory rows, equipment selections, balances, progression.
- Inventory rows reference `ItemTypeId`; startup readiness verifies critical referenced IDs exist.
- Catalog version/hash used for a match is recorded with results.

### 13.4 PAK/updater

PAK/XTEA/updatelist support is Tier D unless a real client startup path proves it is required earlier. It belongs in `pangya-data`/`pangya-updater`, not the game domain.

**This condition fired on 2026-08-07.** Running the U.S. 852 client proved `updatelist` support is required before Tier B, not at Tier D: the client makes three HTTP requests before it opens any socket, and failing any one of them ends the run.

| Request | Failure when absent |
|---|---|
| `GET /Translation/Read.aspx` | client aborts with "string load failed." |
| `GET .../S4_Patch/updatelist` | client aborts asking for a re-install or the update program |
| `GET .../S4_Patch/extracontents/extracontents.xml`, then the theme document and every image it names | client exits silently |

Only afterwards does the client mount its PAK series. Consequences, recorded in ADR-0015:

- `pangya-updater` is now a required crate, holding PangYa's `updatelist` XTEA variant, its nonstandard file CRC-32, and the document layout. Both algorithms MUST be reproduced exactly rather than normalised to their textbook forms.
- The service MUST run on a listener separate from `[http]`. The patch surface has to be reachable by the machine running the client; health, readiness, and metrics MUST NOT be.
- The theme base URL MUST be an absolute URL derived from a configured advertised address, for the same reason as FR-LOGIN-008: the client passes it to the OS HTTP client verbatim.
- Translation catalogs and theme images are client content and MUST remain operator-supplied.

Independent of any server, the client also requires the registry value `HKLM\SOFTWARE\WOW6432Node\Ntreev USA\Pangya\IntegratedPak`, and a host with at least one audio device. Both are documented in [`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md).

---

## 14. Persistence model

### 14.1 PostgreSQL baseline

PostgreSQL is required for Tier B production/local Compose. SQLite MAY be used for isolated protocol/domain tests but is not a compatibility promise.

SQLx requirements:

- checked `query!`/`query_as!` where practical;
- migrations embedded with `migrate!`;
- committed offline `.sqlx` metadata;
- `cargo sqlx prepare --check` in CI;
- explicit transactions for aggregate changes;
- connection pool limits in config.

### 14.2 Core schema

#### Identity

`accounts`

- `id BIGSERIAL PRIMARY KEY`
- `username_normalized TEXT UNIQUE NOT NULL`
- `username_display TEXT NOT NULL`
- `status` (`active`, `banned`, `disabled`)
- timestamps

`credentials`

- `account_id PRIMARY KEY REFERENCES accounts`
- `scheme TEXT NOT NULL` (`argon2id-client-md5-v1`)
- `password_hash TEXT NOT NULL`
- `updated_at`

`profiles`

- `account_id PRIMARY KEY`
- `nickname TEXT UNIQUE`
- rank/exp/Pang/points with nonnegative checks
- setup state
- selected character/equipment references where cyclic constraints permit

`handover_sessions`

- generated ID
- account ID
- token digest unique
- target service
- source address metadata (privacy-minimized)
- expiry, consumed timestamp

#### Inventory and equipment

`characters`

- ID, account ID, item type, hair/mastery/progression

`inventory_items`

- ID, account ID, `item_type_id BIGINT CHECK 0..4294967295`
- quantity/durability/expiry
- uniqueness rules appropriate to stackable/non-stackable type

`equipment_sets`

- account ID, selected character/club/ball/caddie/mascot/decorations
- version column for optimistic conflict detection if needed

`character_parts`

- character ID, slot, inventory item ID, item type snapshot
- unique `(character_id, slot)`

#### Match and progression

`matches`

- UUID, mode, course, catalog hash, seed, started/finished/status
- unique result commit key

`match_players`

- match/account IDs, score, strokes, place, quit status, Pang/EXP reward

`course_records`

- account/course/mode aggregates

`currency_ledger`

- immutable entry ID, account, currency, signed delta, reason, idempotency key, balance-after, timestamp

`item_ledger`

- immutable item grant/use/move audit with idempotency key

Tier C/D adds friends, mail, guilds, achievements, quests, rankings, UCC/MyRoom, rentals, and event tables through separate migrations.

### 14.3 Monetary invariants

- Currency columns have nonnegative constraints.
- Every currency mutation writes a ledger entry in the same transaction.
- Purchase/reward APIs require an idempotency key.
- Server uses checked integer arithmetic; overflow is an error.
- Floating point is prohibited for currency.
- Balance changes are never accepted from packet values.

### 14.4 Starter grant

Starter account setup is one idempotent transaction:

1. lock/read setup state;
2. insert required character/items if absent;
3. equip only owned valid items;
4. mark setup complete;
5. commit.

Re-running after a crash MUST NOT duplicate items.

### 14.5 Match result commit

`commit_match_result` MUST:

- verify match exists and is not already committed;
- lock relevant match/player rows;
- calculate or verify server-side rewards;
- update balances/EXP/records;
- write ledgers;
- set committed timestamp;
- return the already committed result on idempotent replay;
- commit before success is broadcast.

---

## 15. Security and abuse controls

### 15.1 Safe defaults

- TCP and admin listeners bind `127.0.0.1` by default.
- Public bind requires explicit config and warning.
- Database is not exposed outside Compose network by default.
- Development account auto-create is off in non-local profiles.
- Raw packet logging is off.

### 15.2 Credentials

- Normalize username separately from display form.
- Normalize client MD5 hex to one canonical case/format.
- Hash the normalized legacy transport secret using Argon2id with versioned parameters.
- Use constant-time verification through the hashing library.
- Redact credentials from all diagnostics.
- Rate-limit attempts by normalized username and source address with bounded memory.

### 15.3 Session tokens

- At least 128 bits of entropy.
- Protocol-compatible encoding/length confirmed by fixture.
- Expiry default 60 seconds for login→game handover.
- Single use.
- Store digest, not bearer token.
- Constant-time digest comparison.
- Revoke outstanding handovers on ban/disable.

### 15.4 Network/resource controls

Per-source and global configurable limits:

- concurrent connections;
- accepts per interval;
- login attempts per interval;
- packets and bytes per second;
- unknown/malformed packet strikes;
- actor queue occupancy;
- decompression size/ratio;
- connection and room timers.

Rate-limit labels MUST avoid unbounded metrics cardinality.

### 15.5 Input validation

- Reject NaN and infinity.
- Validate enum discriminants.
- Validate all count fields before loops/allocation.
- Validate IDs against authenticated account and current state.
- Never construct file paths directly from packet strings.
- Ensure custom/UCC data paths are rooted and traversal-safe before Tier D support.

### 15.6 Secrets and config

- Secrets come from environment variables or mounted secret files.
- Config debug output redacts secrets.
- Example config contains no usable production password.
- Docker Compose may use obvious local-only credentials only with a warning and loopback binding.

### 15.7 Supply chain

- Commit `Cargo.lock`.
- Run `cargo deny check` in CI.
- Generate an SBOM for releases.
- Sign/checksum release artifacts when release automation is introduced.
- No dependencies with incompatible licenses.

### 15.8 Legal/trademark

- Include a trademark/non-affiliation notice.
- Do not distribute game binaries/assets.
- Maintain `THIRD_PARTY_NOTICES.md` and `docs/PROVENANCE.md` before source adaptation.
- Treat unlicensed and GPL/AGPL references as behavior-only unless project licensing deliberately changes.

---

## 16. Configuration

### 16.1 Precedence

```text
defaults < TOML file < environment variables < CLI flags
```

Secrets SHOULD be environment/file-only, not CLI flags that appear in process listings.

### 16.2 Example shape

```toml
[server]
profile = "local"
shutdown_grace = "10s"

[login]
bind = "127.0.0.1:10103"
advertise = "127.0.0.1:10103"
client_profile = "us_852"
auto_create_accounts = true

[game]
bind = "127.0.0.1:20201"
advertise = "127.0.0.1:20201"
name = "PangYa-RS Local"
capacity = 200

[http]
bind = "127.0.0.1:8080"
metrics = true

[database]
url_env = "DATABASE_URL"
max_connections = 10
acquire_timeout = "5s"

[data]
iff_directory = "./local-data/pangya_gb.iff"
manifest = "./config/us_852_data.toml"

[protocol]
max_client_frame_bytes = 65535
max_plaintext_bytes = 8388608
max_expansion_ratio = 128
unknown_opcode_policy = "disconnect"

[logging]
filter = "info,pangya_protocol=debug"
format = "pretty"
packet_bodies = false
```

### 16.3 Validation

Startup MUST fail with actionable, aggregated configuration errors for:

- invalid/duplicate bind addresses;
- advertised addresses missing or not representable in protocol fields;
- unsafe public binds in a local-only profile unless explicitly acknowledged;
- missing data files;
- invalid durations/limits;
- absent secret environment variables;
- database migration failure.

---

## 17. Observability

### 17.1 Tracing

Every connection span includes low-cardinality fields:

- `connection_id`;
- `service`;
- `client_profile`;
- remote network prefix or privacy-safe address according to config;
- authenticated `account_id` only after authentication;
- `room_id`/`match_id` when applicable.

Packet events include opcode and direction but not body by default.

### 17.2 Metrics

Minimum metrics:

- accepted/active/closed connections by service/reason;
- frames and bytes by direction/service;
- decode/encode/decompress errors by class;
- unknown opcodes by service/opcode (bounded known range);
- login attempts/outcomes;
- active rooms and matches;
- room command queue utilization/drops;
- DB pool utilization and query latency classes;
- match start/finish/abort and reward-commit outcomes;
- storage faults by classified cause.

Never label metrics with username, nickname, token, arbitrary IP, room name, or message text.

#### Storage faults

`pangya_storage_faults_total{fault="..."}` counts every storage failure a repository
produces, classified by cause. The label set is the closed `StorageFault` enum, so the
dimension's width is fixed at compile time and every series is exported from process
start rather than appearing on first failure.

A fault is derived only from the `SQLSTATE` class, the driver's own failure kind, or a
server-side consistency check. Server message text, statement text, bound parameters, and
row values are never read, so a fault is safe to export, log, and return to a caller.
This is what makes the classification safe to attach to the error itself: `Storage`
variants carry their fault, so a failure explains its own cause at every layer that
handles it, including test assertions.

Three faults are server-side rather than database-reported and are worth distinguishing
when reading a dashboard: `unexpected_row_count` and `write_verification` mean a statement
succeeded but violated an invariant the repository enforces, and `unsupported` means a
composed repository has no implementation for the operation at all. `plpgsql_raise`
(`SQLSTATE` class `P0`) is what the test suite's fault injection produces.

Bootstrap remains outside this dimension: pool construction and migration run before the
exporter exists, and their failures are already fatal and typed.

### 17.3 Health semantics

- Liveness fails only when the process cannot make progress.
- Readiness requires DB connectivity/migrations, data catalog, and required listeners.
- During graceful shutdown readiness becomes false before listeners close.

### 17.4 Audit events

Structured durable audit events for:

- operator account actions;
- bans/unbans;
- currency/item mutations;
- match result commits;
- failed token replays;
- config mode that enables public binding or raw packet capture.

---

## 18. Performance and reliability requirements

Initial test targets, not public hosting guarantees:

- 500 concurrently connected idle test clients on a four-core developer machine.
- 100 active synthetic clients across rooms without unbounded memory growth.
- p95 in-memory packet dispatch under 10 ms excluding network and DB.
- no database query in aim/shot relay handlers.
- bounded memory per connection and room.
- graceful shutdown within 10 seconds under normal local load.
- no duplicate committed match reward after forced disconnect/retry tests.

Performance work MUST use release builds and measurement. Benchmarks:

- frame encode/decode at representative packet sizes;
- LZO compress/decompress;
- packet reader/writer;
- room actor command throughput;
- IFF startup parse;
- login Argon2 parameters.

Do not add unsafe code, pooling, or custom allocators without profile evidence and an ADR.

---

## 19. Testing strategy

### 19.1 Test layers

| Layer | Required coverage |
|---|---|
| Unit | Checked reader/writer, crypto transforms, state transitions, reward formulas, validation errors |
| Golden | PangCrypt vectors, PacketDoc/upstream packet bytes, hello/service lists, IFF records |
| Property | Crypto round trips, fragmented/coalesced frames, parser never overreads, checked length arithmetic |
| Fuzz | Client decoder, server decompressor, packet parser by opcode, IFF parser |
| Integration | PostgreSQL migrations/repositories, login/game handover, room actor + fake connections |
| End-to-end synthetic | TCP LoginService→GameService→room→result flow |
| Differential/manual | Rugburn + U.S. 852 client, capture comparison and visual outcome checklist |

### 19.2 Golden fixture metadata

Each fixture directory includes:

```text
fixture.bin
fixture.yaml
```

Metadata fields:

- source project/capture;
- upstream revision/URL;
- license/provenance;
- client version and service/direction;
- encryption key/salt if relevant;
- expected opcode/type;
- redaction status;
- expected parse/encode behavior.

Credentials, real tokens, chat, email, and personal data MUST be synthetic or irreversibly redacted.

### 19.3 Fragmentation tests

For every core frame:

- feed one byte at a time;
- split at every header boundary;
- concatenate two or more frames;
- include complete frame plus partial next frame;
- disconnect at every offset;
- vary salt/key across valid range.

### 19.4 State-machine tests

Use table-driven tests to assert:

- valid transition and emitted events;
- invalid transition error;
- no state change after rejected command;
- disconnect from every state;
- duplicate finish/reward idempotency;
- deterministic result for fixed RNG seed and command stream.

### 19.5 Database tests

- Run migrations from empty database.
- Test rollback on every transactional failure point.
- Test unique nickname race.
- Test concurrent purchase/reward balance constraints.
- Test handover consume race: exactly one succeeds.
- Test migration upgrade from every released schema snapshot after first release.

### 19.6 Real-client release checklist

A first playable release is blocked until a human confirms on U.S. 852:

1. Client launches through supported Rugburn configuration.
2. Login screen accepts a test account.
3. Nickname and first-character setup work from a clean DB.
4. Server list displays and selection succeeds.
5. GameService/channel entry succeeds without crash/hang.
6. Player sees starter character/equipment.
7. Room creation and practice start succeed.
8. One hole can be played and completed.
9. Result/balance screen displays coherent values.
10. Restart retains account, inventory, equipment, Pang, and EXP.
11. Replaying finish packet does not duplicate rewards.
12. Logs contain no credential/token packet content.

Capture hashes and server/client build identifiers are attached to the release evidence, not necessarily committed if legally/privacy sensitive.

---

## 20. Developer workflow and CI

### 20.1 Pull request gates

- format, clippy, unit/integration/doc tests;
- SQLx offline metadata check;
- `cargo deny`;
- changed packet requires fixture/provenance;
- changed schema requires migration and rollback/upgrade test policy;
- changed public config requires example/docs update;
- no proprietary assets or nested reference repo content staged.

### 20.2 Documentation required with code

- public API rustdoc;
- `# Errors` on public fallible APIs;
- design rationale in ADR rather than long comments;
- protocol docs for packet/state changes;
- `PROGRESS.md` milestone update;
- `MEMORY.md` only for durable cross-session facts, not a daily log.

### 20.3 Reference repository policy

`opensource-references/*--*/` remains ignored local clones. CI and builds MUST NOT depend on those directories. Any required fixture or algorithm fact must be copied into the main tree with provenance and license compliance.

---

## 21. Deployment and operations

### 21.1 Docker Compose

Tier B must include:

- multi-stage Rust image;
- unprivileged runtime user;
- read-only root filesystem where practical;
- PostgreSQL with named volume and healthcheck;
- server waits on readiness through retry/backoff, not fixed sleep;
- operator-mounted read-only IFF path;
- loopback host port mappings by default;
- graceful SIGTERM handling.

### 21.2 Native development

Document:

```bash
cp .env.example .env
# Set DATABASE_URL and client data path
docker compose up -d postgres
cargo sqlx migrate run
cargo run -p pangya-server -- --config config/local.toml
```

Exact commands are added only when files exist and have been verified.

### 21.3 Backups

Before Tier C:

- document `pg_dump`/restore;
- record schema and app version in backups;
- test restoration into a clean environment;
- exclude client assets unless operator separately backs them up.

### 21.4 Graceful shutdown

1. Mark readiness false.
2. Stop accepting new connections.
3. Reject new room starts.
4. Cancel login handshakes.
5. Allow bounded in-flight transactions/result commits to finish.
6. Tell room actors to stop and await them.
7. Flush logs/metrics.
8. Exit before configured grace timeout; forced exit reports unfinished operations.

---

## 22. Feature requirements by domain

### 22.1 Accounts/profile

- create, authenticate, ban/disable;
- nickname set/check;
- first-character setup;
- rank/EXP progression;
- duplicate-login policy;
- operator password reset expressed as new legacy transport secret hash.

### 22.2 Inventory/equipment

- typed item catalog validation;
- stackable vs unique item semantics;
- starter grants;
- character parts, club, ball, caddie, mascot, consumable slots;
- equipment must be owned and compatible;
- expiry/durability deferred unless required by starter flow;
- item/currency ledger.

### 22.3 Lobby/room

- channel list and capacity;
- room list, create, join, leave, owner transfer;
- password support after unprotected rooms;
- ready state and settings authorization;
- chat with length/rate limits;
- kick restricted to room owner/operator;
- room summaries updated from actor events.

### 22.4 Gameplay

Tier B mode order:

1. solo practice;
2. solo stroke/tourney-shaped flow if client requires that packet family;
3. multiplayer stroke;
4. versus/match;
5. other modes.

Common behaviors:

- deterministic hole order/seed;
- weather/wind;
- load barrier with timeout;
- turn order and active player;
- shot event relay;
- hole completion/give-up;
- score and standings;
- result commit exactly once;
- disconnect/abort.

### 22.5 Economy/shop

- catalog-derived prices, never packet-provided prices;
- atomic affordability check and deduction;
- item grant in same transaction;
- purchase idempotency;
- balance and inventory response after commit;
- gacha/event randomness deferred and auditable.

### 22.6 Social/ranking

Tier C:

- friend request/accept/remove/block;
- presence and direct message;
- mail with safe text limits;
- rankings from durable result projections;
- guilds later due broad schema/protocol surface.

---

## 23. Milestones and exit criteria

### M0 — planning and provenance

**Deliverables**

- research, spec, visual plan, progress, memory;
- local reference clone manifest and ignore policy;
- license strategy decision queued;
- initial ADR list.

**Exit:** documentation is internally linked and validated; no implementation claimed.

### M1 — Cargo and protocol foundation

**Deliverables**

- workspace/crates/lints/CI;
- `pangya-crypto` and `pangya-protocol`;
- PangCrypt vectors and attribution;
- `lzokay` compatibility report;
- custom framed codec and fuzz target.

**Exit:** all crypto vectors and fragmentation tests pass; malformed input cannot panic; LZO gate outcome recorded.

### M2 — LoginService vertical slice

**Deliverables**

- config, tracing, Postgres migrations;
- accounts/credentials/profile/handover repositories;
- minimum typed login packet set;
- synthetic login E2E;
- operator account creation command.

**Exit:** clean DB account flow reaches server selection; handover consume race proves one success; credentials redacted.

### M3 — GameService handover and player bootstrap

**Deliverables**

- GameService hello/auth;
- immutable data catalog with minimum IFF types;
- player/character/inventory/equipment load;
- one channel entry.

**Exit:** real U.S. 852 reaches channel/lobby with starter character and no client crash.

### M4 — lobby and room

**Deliverables**

- room actors and registry;
- create/list/join/leave/settings/ready/chat;
- owner transfer, disconnect cleanup, queue bounds.

**Exit:** synthetic concurrent clients pass state/property tests; real client can create and enter a room.

### M5 — solo practice first playable

**Deliverables**

- match actor state;
- hole/weather/wind/start packets;
- shot/result relay;
- finish/score/Pang/EXP commit;
- restart persistence.

**Exit:** full 12-step real-client release checklist passes for at least one hole; reward replay test passes.

### M6 — multiplayer stroke and records

**Deliverables**

- turn arbitration and multiple participants;
- disconnect/forfeit policy;
- standings/course records;
- load/turn/game timeouts.

**Exit:** two real clients finish a match with consistent results and no duplicate/negative economy changes.

### M7 — inventory/shop depth

**Deliverables**

- broader IFF catalog;
- equipment validation;
- transactional shop;
- consumables/durability as required.

**Exit:** purchases use catalog prices and remain correct under concurrency tests.

### M8 — social/ranking

**Deliverables**

- MessageService, friends/presence/messages;
- mail basics;
- RankingService projection.

**Exit:** service reconnects do not lose durable social state; ranking rebuild matches expected projection.

### M9 — broad parity program

Feature groups are delivered independently with packet matrices, migrations, state specs, and real-client acceptance evidence. “All SuperSS features” is not one milestone.

---

## 24. ADR queue

Create ADRs before implementation for:

1. **ADR-0001:** final project license (recommended decision before source adaptation).
2. **ADR-0002:** U.S. 852 as first compatibility profile.
3. **ADR-0003:** modular monolith vs separate processes.
4. **ADR-0004:** Tokio codec and room actor model.
5. **ADR-0005:** `lzokay` acceptance or alternative.
6. **ADR-0006:** PostgreSQL/SQLx and migration policy.
7. **ADR-0007:** legacy MD5 transport secret handling with Argon2id at rest.
8. **ADR-0008:** packet fixture/provenance policy.
9. **ADR-0009:** client-authoritative shot data and server-side validation/reward boundary.
10. **ADR-0010:** proprietary data mounting and no-assets repository policy.

---

## 25. Open decisions

These do not block documentation but must be resolved by the named milestone:

| Decision | Default recommendation | Deadline |
|---|---|---|
| Final source license | MIT OR Apache-2.0, or dual MIT/Apache-2.0, with clean-room boundary | Before M1 source adaptation |
| Exact U.S. 852 client package/hash | Operator supplies and records hash privately | Before M1 real-client fixtures |
| LZO crate | `lzokay` if all gates pass | M1 |
| Account provisioning UX | CLI command; local auto-create optional | M2 |
| Message server list behavior when absent | Verify whether empty list or stub avoids client hang | M2/M3 capture spike |
| Default GameService port | Use captured/client-compatible mapping; advertised address configurable | M2 |
| IFF minimum set | Derive from player bootstrap packet needs | M3 |
| Tier B reward formula | Simplified, server-side, documented and deterministic | M5 |
| Full public hosting support | Not promised; separate hardening program | Post-M6 |

---

## 26. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Incorrect packet layout crashes client | High | Fixture per packet, real-client gate, unknown bytes preserved |
| LZO incompatibility | High | Dedicated M1 gate and fallback adapter interface |
| License contamination | High | Ignore local clones, provenance ledger, `cargo deny`, license ADR |
| Client-claimed rewards exploited | High | Server-calculated rewards, ledgers, idempotent transactions |
| Scope expands to all legacy features too early | High | Tiered release and vertical exits |
| Room concurrency races | High | Single-owner actor state, deterministic tests |
| Malformed packets exhaust memory | High | frame/decompression/count caps before allocation, fuzzing |
| Region-specific code leaks into domain | Medium | compatibility profile and versioned registry |
| DB becomes hot-path bottleneck | Medium | snapshots and commit boundaries; no shot-relay queries |
| Legacy README claims mislead implementation | Medium | source/capture evidence hierarchy |
| Proprietary assets accidentally committed | High | ignore patterns, staged-file CI scan, no-assets policy |

---

## 27. Definition of done

A feature is done only when all applicable items are true:

- normative requirement and user outcome are identified;
- packet layouts have fixture/provenance metadata;
- parser/encoder and error cases are tested;
- state transition is explicit and deterministic;
- authorization and resource bounds are enforced;
- persistence is transactional/idempotent where relevant;
- logs/metrics are structured and secrets absent;
- real-client behavior is tested when the feature is client-visible;
- docs, config, migrations, progress, and memory are updated;
- format, clippy, tests, SQLx check, and dependency/license audit pass;
- residual unknowns are recorded rather than hidden behind magic constants.

A milestone is not complete because code compiles or a client reaches one screen. It is complete only when its stated exit evidence exists.

---

## 28. Immediate implementation backlog

When implementation is explicitly authorized, start with:

1. Create ADR-0001 through ADR-0005 skeletons and resolve license/LZO decisions.
2. Initialize Cargo workspace and lint/CI policy.
3. Add `pangya-crypto` with attributed oracle tables and PangCrypt vectors.
4. Implement client decrypt/encrypt without LZO.
5. Validate `lzokay` against known server ciphertext/plaintext vectors.
6. Implement custom Tokio `Decoder`/`Encoder` with limits and fragmentation tests.
7. Add minimum U.S. 852 hello/login packet types.
8. Build synthetic TCP login harness before PostgreSQL/domain breadth.
9. Capture/record the exact real-client login ordering.
10. Update [`PROGRESS.md`](PROGRESS.md) after each exit gate.

No gameplay feature work begins before items 1–9 establish a trustworthy wire foundation.
