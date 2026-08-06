# PangYa-RS

A clean-room, safe-Rust server compatibility project targeting the preserved
PangYa U.S. 852.00 client. M2-M5 provide the local synthetic login, bootstrap,
room, and one-hole solo slices. M6 adds an opt-in exactly-two-ready-player,
one-hole stroke flow with actor-owned turns, truthful forfeits, standings,
atomic settlement, and course records. These are generated synthetic/non-retail
contracts, not real-client compatibility: exact U.S. 852 room/match opcodes,
layouts, order, Course/IFF interpretation, and one-/two-client acceptance remain
external gates.

Licensed under either MIT or Apache-2.0 at your option.

## Legal and assets notice

PangYa and related names and marks belong to their respective owners. This
project is independent, unofficial, and is not affiliated with or endorsed by
those owners. No game client, executable, artwork, audio, IFF/PAK data, packet
capture containing personal data, or other proprietary asset is included or
may be contributed. Operators must supply legally obtained client/data files.

See `docs/PROVENANCE.md` and `THIRD_PARTY_NOTICES.md`.

## Local M2/M3/M4/M5/M6 operations

```bash
cp config/local.example.toml config/local.toml
cp .env.example .env
# Set a real local-only DATABASE_URL in your shell or ignored .env.
export DATABASE_URL='postgres://USER:PASSWORD@127.0.0.1:5432/pangya'
cargo run -p pangya-server -- --config config/local.toml serve

# Read silently; history/process arguments contain only the variable name.
IFS= read -r -s PANGYA_ACCOUNT_SECRET && printf '\n'
printf '%s\n' "$PANGYA_ACCOUNT_SECRET" | \
  cargo run -p pangya-server -- --config config/local.toml account create \
  --username local_user --nickname local_nick --secret-stdin
unset PANGYA_ACCOUNT_SECRET
```

Enter exactly 32 hexadecimal characters at the silent prompt.
Synthetic GameService remains disabled by default. To enable the M3 bootstrap,
M4 local rooms, and optional M5/M6 modes, mount a legally obtained read-only IFF
directory, create the versioned manifest described in
[`docs/data/M3_SYNTHETIC_CATALOG.md`](docs/data/M3_SYNTHETIC_CATALOG.md), and set
`game.enabled=true` plus `data.catalog_required_m3=true`. M5 and M6 each require
a manifest Course record matching their configured course ID and explicit
`game.solo_practice.enabled=true` or `game.stroke_two.enabled=true`; both remain
disabled by default. See
[`docs/CONFIGURATION.md`](docs/CONFIGURATION.md),
[`docs/protocol/M2_SYNTHETIC_LOGIN_FLOW.md`](docs/protocol/M2_SYNTHETIC_LOGIN_FLOW.md),
[`docs/protocol/M3_SYNTHETIC_GAME_FLOW.md`](docs/protocol/M3_SYNTHETIC_GAME_FLOW.md),
[`docs/protocol/M4_SYNTHETIC_LOBBY_ROOM_FLOW.md`](docs/protocol/M4_SYNTHETIC_LOBBY_ROOM_FLOW.md),
and
[`docs/protocol/M5_SYNTHETIC_SOLO_FLOW.md`](docs/protocol/M5_SYNTHETIC_SOLO_FLOW.md),
and
[`docs/protocol/M6_SYNTHETIC_STROKE_FLOW.md`](docs/protocol/M6_SYNTHETIC_STROKE_FLOW.md).
The M6 decision and checkpoint evidence are
[`docs/adr/0013-synthetic-m6-two-player-stroke.md`](docs/adr/0013-synthetic-m6-two-player-stroke.md)
and
[`docs/evidence/M6_SYNTHETIC_STROKE_2026-08-05.md`](docs/evidence/M6_SYNTHETIC_STROKE_2026-08-05.md).
Current status is in [`docs/PROGRESS.md`](docs/PROGRESS.md). Admin endpoints are
read-only: `/health/live`, `/health/ready`, and optional `/metrics`.

## Local validation

The fuzz manifest is excluded from the root workspace, so audit and fuzz it
explicitly. These are the exact bounded commands:

```bash
cargo deny --locked check
cargo deny --manifest-path fuzz/Cargo.toml --config fuzz/deny.toml --locked check
cargo +nightly fuzz build
cargo +nightly fuzz run client-codec -- -seed=1 -max_total_time=5
cargo +nightly fuzz run server-decompression -- -seed=1 -max_total_time=5
cargo +nightly fuzz run packet-reader -- -seed=1 -max_total_time=5
cargo +nightly fuzz run iff-parser -- -seed=1 -max_total_time=5
```

Each fuzz run is deterministic in seed and bounded in wall-clock fuzz time.
