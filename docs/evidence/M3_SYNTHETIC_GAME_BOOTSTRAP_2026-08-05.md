# M3 synthetic GameService bootstrap evidence — 2026-08-05

## Claim boundary

The opt-in local synthetic M3 slice is implemented. It does **not** claim real
U.S. 852 GameService or IFF compatibility and includes no proprietary data.

## Catalog evidence

- Versioned TOML manifest with exact filename, lowercase SHA-256, kind, count,
  binding, version, and record size.
- The catalog root is opened once as a directory capability; manifest/member
  opens remain capability-relative with no ambient joined-path reopen. Regular-file
  checks, symlink/component escape rejection, bounded one-open reads, file/manifest
  caps, checked exact arithmetic, no trailing bytes, duplicate kinds, and type-ID
  duplicates both within and across Character/ClubSet/Ball are rejected.
- Required Character/ClubSet/Ball families; only LE `u32 type_id` interpreted and
  all remaining bytes retained as opaque immutable records.
- Generated golden fixtures and provenance metadata, typed failure tests,
  arbitrary-byte property coverage, and a bounded `iff-parser` fuzz target in CI.
- Starter cross-check occurs before listeners; PlayerSnapshot cross-check occurs
  before bootstrap output.

## Storage evidence

- `PlayerRepository::load_player_snapshot(AccountId)` uses a read-only
  repeatable-read transaction and checked SQLx queries.
- Bounded `LIMIT cap+1` character/inventory reads prevent unbounded bootstrap
  allocation. Active account, complete profile, nickname, owner IDs, selected
  character, equipment references, quantities/ranges, and uniqueness are checked.
- A test-only synchronization checkpoint pauses immediately after the first SELECT
  establishes the repeatable-read snapshot. A second transaction then commits
  profile and equipment generation changes before later projections continue; the
  paused read returns both old values and a subsequent read returns both new values.
  Real PostgreSQL tests also cover happy projection, ban coherence/post-ban
  rejection, and valid-but-incomplete persisted setup rejection.

## Runtime/protocol evidence

- Explicit `AwaitHandover -> AwaitChannel -> InChannel` state flow.
- Bearer parsing/consume is atomic for target Game; claimed identity is ignored as
  authority and must match the consumed account. Bearers zeroize and never enter
  logs, metrics, debug output, or errors.
- Pre-spawn global/source admission, fixed-capacity accept/auth/packet/weighted-byte
  windows, per-connection budgets, auth/idle deadlines, duplicate presence RAII,
  and bounded cancellation/drain.
- Generated four-byte hello golden asserts only the key-final-byte property.
- Typed synthetic opcodes `0x0002`, `0x0004`, `0x0070`, `0x0072`, `0x0073`,
  `0x004d`, `0x004e`; inventory segmentation is capped at 50.
- Real PostgreSQL TCP tests cover Login-to-bearer-to-Game consume, snapshot/catalog
  bootstrap, three inventory segments, channel entry, expired/wrong/replay/
  concurrent/mismatch/banned/invalid/catalog-invalid/malformed outcomes,
  duplicate presence and RAII release; global/source accept and source connection;
  global/source auth, packet, and byte windows; per-connection packet and byte
  windows; wrong channel; known invalid-state and true-unknown opcodes; in-channel
  idle timeout; cancellation cleanup followed by reconnect on the same service;
  connection-task capacity; and shutdown grace. Runtime trace capture searches for
  the bearer and credential.

## Composition evidence

- `game.enabled=false` preserves M2 behavior by default.
- Enablement requires `data.catalog_required_m3`, directory, and manifest.
- Catalog load/cross-check completes under a timeout before any listener binds.
  The underlying filesystem operation is noncancellable, but one detached standard
  thread owns it; timeout does not retain Tokio's blocking pool/runtime or delay
  runtime/process teardown.
- When enabled, catalog and Game listener join readiness and unified supervision;
  when disabled, neither is required. Disabled `game.bind` is excluded from bind
  validation, while `game.advertise` remains validated for Login output.
- Enabled Login/Game runtimes receive deterministic floor-half/remainder partitions
  of every configured process-total connection, accept, auth, packet, and byte
  quota; partitions sum exactly to the configured total. Disabled Game leaves the
  full total with Login.

## External gates

1. Legally held U.S. 852 IFF headers, binding/version values, and record sizes.
2. Real GameService hello bytes and packet layouts/order.
3. Real bootstrap/channel semantics and acceptance.
4. Real client Login-to-Game handover field and token acceptance.
5. Any gameplay, rooms, economy, or M4+ behavior.
