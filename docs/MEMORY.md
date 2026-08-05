# PangYa-RS session memory

> Durable handoff for the next coding session
>
> Last updated: **2026-08-05**
>
> Read first: [`PROGRESS.md`](PROGRESS.md), then [`SPEC.md`](SPEC.md)

## One-paragraph state

M1 and M2 remain intact. M3's **opt-in local synthetic handover/player-bootstrap exit is complete**: immutable bounded manifest catalog, opaque minimum Character/ClubSet/Ball records, coherent repeatable-read PlayerSnapshot, authoritative single-use Game handover, catalog validation, segmented bootstrap, channel state, pre-spawn/rate/deadline/presence bounds, optional readiness, redacted metrics/tracing, generated fuzz fixtures, and real-PostgreSQL Login-to-Game evidence. This is not real-client or real-IFF compatibility: U.S. 852 login order, token field, Game hello/opcode layouts/order, channel semantics, and all IFF headers/record sizes remain open. Evidence is in [`evidence/M3_SYNTHETIC_GAME_BOOTSTRAP_2026-08-05.md`](evidence/M3_SYNTHETIC_GAME_BOOTSTRAP_2026-08-05.md).

---

## Durable decisions in the proposed baseline

These are accepted in ADR-0001 through ADR-0005 and should change only through a superseding ADR:

- First client target: **PangYa U.S. 852.00 / GB.852**.
- Project type: **clean-room Rust rewrite**, not a mechanical translation.
- Runtime: **Tokio**.
- Framing: custom `tokio-util::codec::{Decoder, Encoder}` with `bytes`.
- Durable store: **PostgreSQL via SQLx**.
- Redis: **not used in the first vertical slice**.
- Deployment: one **modular-monolith** binary with separate Login/Game TCP listeners; preserve crate boundaries for later process split.
- Concurrency: one bounded task/actor owns each room/match; avoid shared `Arc<Mutex<Room>>` hot state.
- Packet strategy: hand-reviewed, state-aware Rust packet types with a fixture per implemented opcode; PacketDoc is evidence, not blindly generated truth.
- Data strategy: safe explicit little-endian IFF parser; operator mounts proprietary game data read-only. Local M3 uses ADR-0010's versioned synthetic header and interprets only `u32 type_id`; real layouts remain externally gated.
- Shot model: client computes trajectory/result; server owns identity, membership, ordering, score, validation, inventory, currency, rewards and persistence.
- LZO implementation: **`lzokay` 2.0.1** (pure Rust, MIT), proven for known PangCrypt vectors, generated round trips, and one independent go-lzo decode; real-client acceptance remains open.
- Error policy: `thiserror` in libraries, `anyhow` only at binary composition boundary, no production `unwrap`/`expect`.
- Persistence: PostgreSQL 17 through SQLx 0.8.6, embedded forward-only migrations, named constraints, explicit transactions, and committed offline metadata (ADR-0006).
- M2 names: ASCII edge trim + lowercase normalized key; usernames `[a-z0-9_]{3,32}`, nicknames `[a-z0-9_-]{3,16}`; store display/key separately pending client validation.
- Credentials: exactly 32 ASCII hex transport bytes canonicalized lowercase, then Argon2id v19 (`m=19456,t=2,p=1`, canonical 16-byte salt, 32-byte output) in PHC form; verification rejects every extension/downgrade/malformed shape; never hash inside DB transactions (ADR-0007).
- Handovers: UUID selector plus 256 random bits, SHA-256 digest only at rest, 60-second default, row lock and constant-time digest comparison, single use, revoke on ban/disable; persist only canonical IPv4 `/24` or IPv6 `/56` source prefixes, never raw peer IPs.
- Starter grants serialize on the profile row, insert the aggregate only when wholly absent, perform no writes on exact replay, and reject stable-key/type/quantity/equipment drift; full IFF validation remains M3.
- All static production repository SQL uses checked SQLx macros and committed offline metadata; dynamic SQL is test-only failure injection/assertion DDL.
- Implementation must remain safe Rust initially (`unsafe_code = "forbid"`).

---

## Important unresolved decisions

1. **Exact U.S. 852 client package/hash** — operator must identify it privately before captures; never commit the client.
2. **Real-client acceptance** — validate generated server packets through U.S. 852 login/channel entry; do not infer this from local or independent decoder evidence.
3. **Exact login ordering** — record the legally held U.S. 852 client packet order without committing proprietary captures or secrets.
4. **Name limits** — M2 ASCII normalization/limits are provisional until the chosen client validates input behavior.
5. **Exact advertised ports/packet order/token field** — verify with the chosen client/capture before claiming compatibility.
6. **Real IFF and GameService layouts** — validate legally held Character/ClubSet/Ball headers, record sizes, Game hello/auth/bootstrap/channel fields and ordering; never infer them from local synthetic M3.

---

## Reference clones

Location: `opensource-references/`

Parent Git behavior:

- `opensource-references/.gitignore` ignores all child clone content.
- `opensource-references/README.md` and `.gitignore` are the only parent-tracked files intended there.
- Builds/CI must never depend on local clone directories.

Core servers:

- `Acrisio-Filho--SuperSS-Dev` — MIT, broadest feature inventory, snapshot `485ba29b4831`.
- `K4T--Py_Source_US` — GPL-3.0, U.S. 852/824 behavior, `dcfb75a84be8`.
- `hex-agon--alter-pangya` — no license found, modern U.S. 852 behavior only, `5afc41b39260`.
- `pangbox--server` — primarily ISC, typed Go packets/room actor/Minibox, `91d8a5a4f3be`.
- `hsreina--pangya-server` — Apache-2.0, FreshUp historical flow, `1720cdceafcd`.
- `juanangel123--pangya-server` — MIT, minimal PHP decomposition, `96010b0007e9`.
- `luismk--Pangya-Server-Community` — root MIT but **`Server/` AGPL-3.0**, current JP breadth, `a0e30985d271`.

Protocol/tooling:

- `pangbox--packetdoc` — ISC, 286 `.ksy` files and 260 service/direction packet definitions, `d61f583a3e67`.
- `pangbox--pangcrypt` — ISC own code, oracle/algorithm/vectors, GPL Go LZO dependency, `2bf7a1d36591`.
- `pangbox--pangfiles` — ISC, PAK/XTEA/CRC/updatelist, `4311f2199d5e`.
- `pangbox--rugburn` — ISC plus separate Intel IJL terms, client redirect, `7158511d52b6`.
- `pangbox--pantrant` and `pangbox--wireshark-dissector` — ISC traffic analysis.
- `retreev--PangLib` — AGPL-3.0 file-format behavior.

Exact manifest and update command: [`../opensource-references/README.md`](../opensource-references/README.md).

---

## Protocol facts to retain

### Hello/key

- Plaintext hello comes first.
- Key range is `0x00..=0x0f`; PangCrypt rejects `>= 0x10`.
- Key byte position varies by service/region: U.S. Login example index 6; U.S. Game example last byte.

### Client → server

- 5-byte transport header.
- Salt at byte 0.
- `u16 LE` length at bytes 1–2, excludes first 4 bytes.
- Byte 3 zero/padding.
- Byte 4 oracle-derived.
- Client body uncompressed.
- Decrypt forward from index 8: byte XOR byte four positions behind.
- Strip first 5 bytes; plaintext starts with `u16 LE` opcode.

Primary code: `opensource-references/pangbox--pangcrypt/client.go`.

### Server → client

- 8-byte transport header.
- Salt at byte 0.
- `u16 LE` length at 1–2, total size minus 3.
- Byte 3 is XOR of the two oracle table bytes.
- Bytes 4–7 encode original-size metadata; byte 7 receives private-table XOR.
- Plain packet is LZO1X-compressed before encryption.
- Encrypt backward from end to index 10: byte XOR byte four positions behind.

Primary code: `opensource-references/pangbox--pangcrypt/server.go`.

### Oracle

- `cryptTable` is `[2][0x1000]u8` (two 4,096-byte tables).
- PangCrypt says it was extracted from U.S. 852 `ProjectG.exe`.
- Ported with ISC attribution. Locked SHA-256: combined `003d0b42f9fc1e2fb3b9dc37d23bbe1ff018669ea81bf0068abfaea4942b7133`; table 0 `6eee0700c7096c57a992b1cf787e06d2b661cfb0fd8871c481e089c3a55fabfe`; table 1 `89a66b67ca44457bc782c85cffff41abb45930c723983fd03bd9f2bc20b331d7`.

### PacketDoc counts

Excluding each direction's `index.ksy`:

- Login client/server: **7 / 6**.
- Game client/server: **98 / 141**.
- Message client/server: **8 / 2**.
- Common types: **14**.
- All `.ksy`: **286**.

PacketDoc is WIP and has unknown fields. One observed inconsistency: GameService server index imports `010e` but maps opcode `0x010c` to a `010e`-named type. Fixture-check every mapping.

---

## Highest-value source files

### Crypto/packet foundation

- `opensource-references/pangbox--pangcrypt/common.go`
- `opensource-references/pangbox--pangcrypt/client.go`
- `opensource-references/pangbox--pangcrypt/server.go`
- `opensource-references/pangbox--pangcrypt/client_test.go`
- `opensource-references/pangbox--pangcrypt/server_test.go`
- `opensource-references/pangbox--packetdoc/src/packets/**/index.ksy`
- `opensource-references/pangbox--packetdoc/src/packets/common/version.ksy`

### Login/service ordering

- `opensource-references/pangbox--server/login/conn.go`
- `opensource-references/hex-agon--alter-pangya/login-server/src/main/kotlin/`

### Shot and room authority

- `opensource-references/hex-agon--alter-pangya/game-server/src/main/kotlin/work/fking/pangya/game/packet/handler/match/MatchPlayerShotCommitPacketHandler.kt`
- `.../MatchPlayerShotSyncPacketHandler.kt`
- `.../room/match/PracticeMatchDirector.kt`
- `opensource-references/pangbox--server/game/server/conn.go`
- `opensource-references/pangbox--server/game/room/room.go`

### Persistence/data

- `opensource-references/pangbox--server/migrations/0001_initial.sql`
- `opensource-references/pangbox--server/queries/`
- `opensource-references/retreev--PangLib/PangLib.IFF/IffFile.cs`
- `opensource-references/pangbox--pangfiles/crypto/pyxtea/`
- `opensource-references/pangbox--pangfiles/pak/`

### Feature breadth

- `opensource-references/Acrisio-Filho--SuperSS-Dev/Server Lib/Game Server/GAME/`
- `opensource-references/Acrisio-Filho--SuperSS-Dev/Server Lib/Game Server/PANGYA_DB/`
- `opensource-references/luismk--Pangya-Server-Community/Server/GameServer/Game/GameModes/`

---

## Licensing boundary

Treat this as a hard engineering rule until ADR-0001 says otherwise:

- **May adapt with notices:** MIT/ISC/Apache-2.0 sources such as SuperSS, Pangbox own code, hsreina, juanangel.
- **Behavior-only for a permissive rewrite:** GPL `Py_Source_US`; AGPL `PangLib`; AGPL `Pangya-Server-Community/Server`.
- **No copying:** `alter-pangya` and `SuperSS-Dev-Docker` because no repository license was found in the snapshots.
- **Mixed:** Rugburn contains separately licensed Intel IJL artifacts; do not vendor/redistribute them.
- **LZO:** PangCrypt's Go LZO dependency and several Rust LZO ports are GPL. `lzokay` is the permissive candidate.
- Keep `THIRD_PARTY_NOTICES.md` and `docs/PROVENANCE.md` from the first source adaptation onward.

This is not legal advice; it is the conservative implementation policy.

---

## Why no physics engine first

Modern source shows the client sends shot parameters and result coordinates/state, and servers broadcast or consume them. The server must still own the deterministic match state and validate order/ranges. The correct first-playable boundary is:

- **Client:** trajectory simulation, visual ball flight, result synchronization.
- **Server:** authenticated actor, room membership, active turn, hole sequence, score, item authorization, server-derived rewards, persistence.

Never persist client-claimed Pang/EXP directly. Simplified Tier-B rewards are acceptable only if calculated server-side and documented.

---

## Next-session checklist

1. Read `docs/PROGRESS.md`, the M2 evidence file, and this file; check unstaged status.
2. Preserve the fixture/provenance, SQLx offline, real-PostgreSQL, and no-proprietary-assets boundaries.
3. Preserve the completed synthetic M2 runtime/config/CLI/health/TCP/PostgreSQL evidence; do not infer external compatibility from it.
4. Obtain the exact U.S. 852 build/hash privately and validate order, `0x000e`, name limits, server-list acceptance, and token field/length without committing it.
5. Begin GameService/bootstrap only as M3 scope.

---

## Useful commands

Inspect project state:

```bash
git status --short --branch
find docs -maxdepth 1 -type f -print | sort
```

Inspect clone revisions without updating:

```bash
for dir in opensource-references/*/.git; do
  repo=${dir%/.git}
  git -C "$repo" show -s --format='%H %cI %s' HEAD
done
```

Recount PacketDoc definitions:

```bash
python3 - <<'PY'
from pathlib import Path
root = Path('opensource-references/pangbox--packetdoc/src/packets')
for service in ('loginservice', 'gameservice', 'messageservice'):
    for direction in ('client', 'server'):
        files = [p for p in (root / service / direction).glob('*.ksy') if p.name != 'index.ksy']
        print(service, direction, len(files))
PY
```

Future mandatory Rust verification:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo test --doc --workspace --locked
cargo deny check
```

---

## Pitfalls already identified

- Do not repeat the old “only one playable server” claim; source ecosystem changed and playability was not independently tested.
- Do not repeat “all physics are client-side” without the server-authority nuance.
- Do not trust repository `pushed_at` as default-branch HEAD date; the manifest records local HEAD revision dates.
- Do not treat a root license as automatically covering nested directories; community `Server/` has AGPL despite root MIT.
- Do not run Kaitai compiler directly and assume PacketDoc's custom `#pragma.template` corpus is production-ready.
- Do not require byte-identical LZO output if a valid stream is client-compatible; do require known-vector decompression and real-client acceptance.
- Do not store plaintext passwords or bare MD5 digests. Hash the normalized client transport secret with Argon2id.
- Do not put DB work in aim/shot relay handlers.
- Do not use unbounded Tokio channels or reserve buffers from untrusted lengths before caps.
- Do not commit clients, extracted IFF/PAK data, packet captures with credentials, or nested clone contents.

---

## Memory maintenance rule

Keep this file concise enough to serve as a startup handoff. Update it only for durable decisions, verified protocol facts, changed reference revisions, new high-value source locations, or a materially different next action. Daily task detail belongs in `PROGRESS.md` or the issue tracker.
