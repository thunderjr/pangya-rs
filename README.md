# PangYa-RS

A clean-room, safe-Rust server for the preserved PangYa U.S. 852.00 client.

An unmodified retail client logs in, sets a nickname and first character, picks the server and
channel, reaches the lobby, opens the shop and buys an item, and creates and joins rooms. The
protocol it speaks is the real U.S. 852 one, derived from the vendored `pangbox` references and
corrected against the client itself.

A two-player versus full card plays and settles over that same wire — in the test suite, and
between two instances of `pangya-test-client` against the release binary. Putting a real client
in one of those two seats is the open gate.

The project also carries a generated `0x7f**` synthetic protocol from before a client was
available. It is redundant in places and is being removed; see
[`docs/RETAIL_CONTRACT.md`](docs/RETAIL_CONTRACT.md) for the retail surface, the synthetic
inventory, and the removal plan.

Licensed under either MIT or Apache-2.0 at your option.

## Legal and assets notice

PangYa and related names and marks belong to their respective owners. This project is
independent, unofficial, and is not affiliated with or endorsed by those owners. No game
client, executable, artwork, audio, IFF/PAK data, packet capture containing personal data, or
other proprietary asset is included or may be contributed. Operators must supply legally
obtained client and data files.

`scripts/check-proprietary-assets.sh` enforces this mechanically: it rejects any `.exe`,
`.dll`, `.iff`, `.pak`, `.pcap`, or `.pcapng` anywhere it scans, requires every binary
fixture to sit in an approved directory beside a `fixture.yaml` carrying full provenance, caps
any tracked blob at 1 MiB, and fails if nested reference-repository content is staged. It runs
in CI. `local-data/` — where the client and everything extracted from it live — is gitignored
and pruned from the scan. See `docs/SPEC.md` §15.8, [`docs/PROVENANCE.md`](docs/PROVENANCE.md),
and [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

## What works against a real client

Each of these was driven from an unmodified U.S. 852 client. The evidence is in
[`docs/PROGRESS.md`](docs/PROGRESS.md), blockers 13-26.

| Stage | State |
|---|---|
| Startup: string catalog, patch `updatelist`, theme content | 33 HTTP requests answered, all 84 PAK archives mounted |
| Login | authenticates over encrypted TCP |
| Nickname and first-character setup | accepted and persisted from a clean database |
| Server list, selection, handover | the whole LoginService state machine, closing with `reason: "complete"` |
| GameService auth and bootstrap | retail hello, `0x0002` auth, full handover reply announcing `852.00`, rosters, equipment, inventory, channel list, balances |
| Channel entry and lobby | avatar and menu bar render; login bonus, recent-player history, and ten documented session opcodes answered |
| Shop and My Room | catalog-priced purchase: balance debited, item in inventory, bought clubs rendered on the character |
| Rooms | create, join, leave, list, ready, and a 341-byte member census that re-sends whenever the roster changes |
| A two-player versus full card | **server side only.** Proven over TCP by `game_retail_two_players_play_and_settle_full_card` and by two `pangya-test-client` seats against the release binary; the real client's acceptance of it is the open gate |

## What is not implemented

- **Room chat, settings changes, and kick.** No handler. The roster itself does stay live: a
  membership or ready change re-sends the census.
- **Multi-mode matches.** Tournament, battle, and chat rooms are accepted and then run as
  versus; their mode-specific rules and wire flows are not implemented.
- **Retail give-up.** A concession has no route; a forfeit can only arrive as a disconnect or
  a turn timeout.
- **Retail consume, repair, and durable equipment changes.** `0x0020` reports the equipment
  the server actually holds rather than acknowledging the requested change, so anything it
  cannot store reverts. Nothing in the client catalog is durable, so there is nothing to
  repair.
- **Social, ranking, friends, mail, guilds, caddies, mascots.** None.
- **Server-side physics.** By design: the client computes trajectory and the server relays it.
  The server owns turn order, stroke count, scoring, and settlement.

Nothing in the shop is priced by this server. Prices come from the client's own tables, and
`data.price_override_pang` changes only what the server charges, never what the client shows.

## Running it

You need Rust (MSRV 1.93.0), PostgreSQL 17, and — to reach anything past a socket — a legally
obtained client. [`docs/RUNNING_THE_CLIENT.md`](docs/RUNNING_THE_CLIENT.md) is the full
operator procedure: acquiring and flattening the client tree, extracting its item tables from
the PAK chain, writing the catalog manifest, and pointing Rugburn at a local instance.

**Setting up the client side is now a Windows app.**
[`thunderjr/pangya-client`](https://github.com/thunderjr/pangya-client) points a client at a
server without the manual procedure — pick the folder, fill in the address, install, play. The
manual path above remains authoritative and is what operators and QA use.
[`docs/RELATED.md`](docs/RELATED.md) explains which repository owns what.

```bash
cp config/retail-local.example.toml config/local.toml

# The URL value itself lives only in this environment variable or a secret file.
export DATABASE_URL='postgres://USER:PASSWORD@127.0.0.1:5432/pangya'

cargo run -p pangya-server -- --config config/local.toml serve
```

Create an account. The client sends its password as an MD5 digest, which the server treats as
a transport secret and stores under Argon2id.

```bash
# Read silently; history and process arguments contain only the variable name.
IFS= read -r -s PANGYA_ACCOUNT_SECRET && printf '\n'
printf '%s\n' "$PANGYA_ACCOUNT_SECRET" | \
  cargo run -p pangya-server -- --config config/local.toml account create \
  --username local_user --nickname local_nick --secret-stdin
unset PANGYA_ACCOUNT_SECRET
```

Enter exactly 32 hexadecimal characters at the silent prompt. `scripts/grant-balance.sh` funds
an account through the server's own `account grant` command, which takes a row lock, refuses
rather than wraps on overflow, and writes an operator audit row.

`config/retail-local.example.toml` is the configuration an unmodified client needs, with the
settings that fail silently called out at the top of the file. The ones that matter most:

- **`[client_web]`** must be enabled and reachable from the client's machine. The client
  fetches a string catalog, an XTEA-encrypted patch `updatelist`, and theme documents **before
  it opens any socket**, and aborts startup without them. It is deliberately a separate
  listener from `[http]`, so the client-reachable surface carries no health, readiness, or
  metrics endpoint. `client_web.patch_number` must not exceed the client's own patch level.
- **`game.retail_bootstrap = true`**. Off, the client reads the following frame at the wrong
  offset and reports "Server is full".
- **`data.iff_directory`** must point at the PAK-extracted tables. A superseded copy validates
  cleanly and silently lacks every item added since.
- **`security.login_timeout`**. The shipped 15 seconds closes the connection while the
  client's own first-time setup screens are open.

Keep every listener on loopback; any non-loopback bind requires `--acknowledge-public-bind`.
Admin endpoints on `[http]` are read-only: `/health/live`, `/health/ready`, and optional
`/metrics`. Packet-body logging cannot be enabled — `logging.packet_bodies = true` is rejected.

`config/local.example.toml` is the same server without the retail deltas, for working on the
synthetic path.

## Documentation

| Document | What it holds |
|---|---|
| [`docs/RETAIL_CONTRACT.md`](docs/RETAIL_CONTRACT.md) | what the real client artifacts establish, every retail opcode handled and emitted with its handler and provenance, the synthetic inventory, and the removal plan |
| [`docs/PROGRESS.md`](docs/PROGRESS.md) | the status ledger and the numbered blockers, open and resolved |
| [`docs/RUNNING_THE_CLIENT.md`](docs/RUNNING_THE_CLIENT.md) | the operator procedure for pointing a real client at a local instance, and the client-side prerequisites no server can supply |
| [`docs/SPEC.md`](docs/SPEC.md) | normative scope, architecture, security, testing strategy, and the real-client release checklist |
| [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md) | every configuration key with its bounds |
| [`docs/PROVENANCE.md`](docs/PROVENANCE.md) | what was adapted from where, under which license |
| [`docs/protocol/`](docs/protocol/) | per-family protocol contracts, retail and synthetic |
| [`docs/adr/`](docs/adr/) | the decisions, with their reasoning |
| [`docs/evidence/`](docs/evidence/) | dated evidence files behind the claims above |

Protocol facts are attributed at their definitions. Layouts adapted from `pangbox/server`,
`pangbox/packetdoc`, `pangbox/pangcrypt`, and `pangbox/pangfiles` are ISC licensed and cited
by file. `hex-agon/alter-pangya` carries no license grant and is used as a behavioral
reference only — protocol facts, never code.

## Local validation

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features   # needs DATABASE_URL
cargo deny --locked check
bash scripts/check-proprietary-assets.sh
```

The fuzz manifest is excluded from the root workspace, so audit and fuzz it explicitly. These
are the exact bounded commands:

```bash
cargo deny --manifest-path fuzz/Cargo.toml --config fuzz/deny.toml --locked check
cargo +nightly fuzz build
cargo +nightly fuzz run client-codec -- -seed=1 -max_total_time=5
cargo +nightly fuzz run server-decompression -- -seed=1 -max_total_time=5
cargo +nightly fuzz run packet-reader -- -seed=1 -max_total_time=5
cargo +nightly fuzz run iff-parser -- -seed=1 -max_total_time=5
```

Each fuzz run is deterministic in seed and bounded in wall-clock fuzz time.
