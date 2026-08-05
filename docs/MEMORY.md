# PangYa-RS session memory

> Durable handoff for the next coding session
>
> Last updated: **2026-08-05**
>
> Read first: [`PROGRESS.md`](PROGRESS.md), then [`SPEC.md`](SPEC.md)

## One-paragraph state

This repository is still in **planning only**. There is no `Cargo.toml`, Rust source, migration, Docker stack, or test suite. The original TypeScript-oriented research was replaced with a Rust/U.S.-852 plan. Sixteen upstream repositories are locally cloned as shallow, ignored reference checkouts under `opensource-references/`; their exact revisions and license boundaries are in `opensource-references/README.md`. The next implementation work is M1 protocol foundation: license ADR, Cargo workspace, attributed PangCrypt vectors/oracle tables, permissive LZO compatibility spike, and a bounded Tokio codec. Do not start gameplay or persistence breadth before those wire proofs.

---

## Durable decisions in the proposed baseline

These are proposed in the spec and should be changed only deliberately (prefer an ADR):

- First client target: **PangYa U.S. 852.00 / GB.852**.
- Project type: **clean-room Rust rewrite**, not a mechanical translation.
- Runtime: **Tokio**.
- Framing: custom `tokio-util::codec::{Decoder, Encoder}` with `bytes`.
- Durable store: **PostgreSQL via SQLx**.
- Redis: **not used in the first vertical slice**.
- Deployment: one **modular-monolith** binary with separate Login/Game TCP listeners; preserve crate boundaries for later process split.
- Concurrency: one bounded task/actor owns each room/match; avoid shared `Arc<Mutex<Room>>` hot state.
- Packet strategy: hand-reviewed, state-aware Rust packet types with a fixture per implemented opcode; PacketDoc is evidence, not blindly generated truth.
- Data strategy: safe explicit little-endian IFF parser; operator mounts proprietary game data read-only.
- Shot model: client computes trajectory/result; server owns identity, membership, ordering, score, validation, inventory, currency, rewards and persistence.
- LZO candidate: **`lzokay` 2.x** (pure Rust, MIT), still gated on PangCrypt vectors and real-client acceptance.
- Error policy: `thiserror` in libraries, `anyhow` only at binary composition boundary, no production `unwrap`/`expect`.
- Implementation must remain safe Rust initially (`unsafe_code = "forbid"`).

---

## Important unresolved decisions

1. **Final project license** — decide MIT, Apache-2.0, dual MIT/Apache-2.0, or another intentional option before adapting upstream code. This is ADR-0001 and the main blocker.
2. **Exact U.S. 852 client package/hash** — operator must identify it privately before captures; never commit the client.
3. **LZO result** — prove `lzokay`; do not assume compatible solely from format name.
4. **Account provisioning** — spec recommends CLI plus local-profile-only auto-create.
5. **Exact advertised ports/packet order** — verify with the chosen U.S. 852 client/capture during M1/M2.

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
- Port with ISC attribution and add a hash assertion.

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

When asked to start implementation, do this order:

1. Read `docs/PROGRESS.md` and this file.
2. Check `git status --short --branch`; do not stage nested clones.
3. Ask/confirm the final project license if still unresolved.
4. Create ADR-0001 through ADR-0005 skeletons.
5. Initialize Rust 2024 workspace and lint policy.
6. Add `THIRD_PARTY_NOTICES.md` and `docs/PROVENANCE.md`.
7. Create only `pangya-crypto` and `pangya-protocol` first.
8. Port PangCrypt fixtures with attribution before algorithm code.
9. Port oracle tables and verify their hash.
10. Implement client crypto and pass vectors.
11. Spike `lzokay` decompression of known server vectors.
12. Implement the bounded Tokio codec and fragmentation/property/fuzz tests.
13. Update `PROGRESS.md` immediately after each evidence gate.

Do **not** begin with a full schema, IFF breadth, room UI, shop, guild, or all opcodes.

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
