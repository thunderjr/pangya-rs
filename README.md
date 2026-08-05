# PangYa-RS

A clean-room, safe-Rust server compatibility project targeting the preserved
PangYa U.S. 852.00 client. M2 provides a local synthetic LoginService slice and
M3 adds an opt-in local synthetic GameService handover/player bootstrap. Neither
is a claim of real-client compatibility; rooms/gameplay remain unimplemented.

Licensed under either MIT or Apache-2.0 at your option.

## Legal and assets notice

PangYa and related names and marks belong to their respective owners. This
project is independent, unofficial, and is not affiliated with or endorsed by
those owners. No game client, executable, artwork, audio, IFF/PAK data, packet
capture containing personal data, or other proprietary asset is included or
may be contributed. Operators must supply legally obtained client/data files.

See `docs/PROVENANCE.md` and `THIRD_PARTY_NOTICES.md`.

## Local M2/M3 operations

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
Synthetic M3 remains disabled by default. To enable it, mount a legally obtained
read-only IFF directory, create the versioned manifest described in
`docs/data/M3_SYNTHETIC_CATALOG.md`, then set `game.enabled=true` and
`data.catalog_required_m3=true`. See `docs/CONFIGURATION.md`,
`docs/protocol/M2_SYNTHETIC_LOGIN_FLOW.md`, and
`docs/protocol/M3_SYNTHETIC_GAME_FLOW.md`. Admin endpoints are read-only:
`/health/live`, `/health/ready`, and optional `/metrics`.

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
