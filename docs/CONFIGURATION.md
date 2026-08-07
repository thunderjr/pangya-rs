# Server configuration reference

Precedence is `typed defaults < optional TOML < PANGYA__... environment <
explicit CLI flags`. Nested environment keys use `__`, for example
`PANGYA__LOGIN__BIND=127.0.0.1:11000`. See
`config/local.example.toml` for every current section.

## Security boundaries

- The database URL value is never a CLI/TOML value. `[database].url_env` names
  an environment variable (default `DATABASE_URL`); if absent, `secret_file`
  may name a mounted file containing the URL. Secret files are opened once and
  bounded to 128 UTF-8 bytes; oversized or raced content is rejected.
- Account secret content is read by `account create` from stdin, a named
  environment variable, or a mounted file. It is never accepted as an argument.
- Loopback is the default for LoginService, optional synthetic M3 GameService, and admin
  HTTP. Any enabled non-loopback listener in any profile requires
  `--acknowledge-public-bind`; before binding, startup must durably record
  `public_bind_enabled/success` or fail closed. When `game.enabled=false`,
  `game.bind` is inert and is excluded from parsing, zero-port, duplicate-bind,
  and public-bind checks. `game.advertise` remains validated because LoginService
  still emits the advertised Game endpoint.
- `login.auto_create_accounts` defaults false and validation rejects it outside
  `server.profile = "local"`.
- `logging.packet_bodies = true` is rejected in M2. The GameService
  `protocol.unknown_opcode_policy` accepts `disconnect`, `ignore`, or `capture`;
  capture retains only bounded opcode/state/length metadata and a SHA-256 digest,
  never the raw payload or a public audit record.

## Current readiness boundary

Base readiness requires validated configuration, successful embedded migrations,
bound LoginService/admin listeners, and a continuously successful bounded
`SELECT 1` database probe. Each probe has a timeout covering pool acquisition
and query; pending/failing probes clear readiness and recovery restores it.
Shutdown clears readiness before listener cancellation. `game.enabled=false`
preserves this M2 boundary. When enabled, `data.catalog_required_m3=true`,
`iff_directory`, and a relative `manifest` are mandatory; bounded catalog load,
starter cross-check, and GameService binding must all succeed before readiness.
`[game.solo_practice]` is a disabled-by-default, synthetic/local-only M5 mode and does
not claim retail compatibility. Enabling it requires `game.enabled=true` and a catalog
Course record exactly matching `course_id`; startup resolves the catalog par and exact
catalog fingerprint into immutable runtime configuration. `loading_timeout` is nonzero,
representable as `u32` milliseconds, and capped at 300 seconds. `commit_timeout` is
nonzero, capped at 60 seconds, and cannot exceed `server.shutdown_grace`. `max_strokes`
is `1..=30`, `startup_recovery_limit` is `1..=10000`, and
`shot_packets_per_window` is `1..=1000000`. Startup abort recovery is completed under
the commit timeout before any Game listener binds; failure prevents readiness.

`[game.stroke_two]` is the independent disabled-by-default synthetic M6 exactly-two,
one-hole mode. It resolves `course_id` and the catalog fingerprint before listener bind.
`loading_timeout`, `turn_timeout`, and `game_timeout` must be nonzero and exactly
representable as `u32` milliseconds; loading is capped at 300 seconds, turn/game at one
hour, and `turn_timeout` cannot exceed `game_timeout`. `commit_timeout` is capped at 60
seconds and by `server.shutdown_grace`; `max_strokes`, recovery, and shot-budget bounds
match solo practice. Its outbound queue must hold at least three events, the actual
standings/own-balance/finished terminal burst. If either synthetic mode is enabled,
startup performs generic incomplete-match recovery exactly once using the larger enabled
recovery cap and timeout before any listener binds.

`[game.economy]` is the independent disabled-by-default synthetic M7 inventory/shop
boundary and claims no retail compatibility. Enabling it requires `game.enabled=true` and
a catalog that carries at least one shop offer, of which at least one is a consumable;
composition fails closed otherwise. `command_timeout` is the repository command deadline:
nonzero, capped at 60 seconds, and additionally capped by `server.shutdown_grace`, so an
in-flight economy command can never outlive the grace that must cover it.
`commands_per_window` is `1..=1000000` and bounds economy commands per connection within
the shared rate window. `page_size` is `1..=50`, the protocol's maximum shop-page entry
count. `max_purchase_quantity` is `1..=99` and is enforced against the wire before any
repository work. Economy opcodes are accepted only from an authenticated connection that
has entered a channel; when the section is disabled the opcodes still decode and receive
an explicit disabled result rather than closing the connection.

Catalog loading occurs before any listener bind. The blocking filesystem work is
noncancellable, but it runs on one detached standard thread rather than Tokio's
blocking pool; timeout therefore cannot retain the Tokio runtime or delay runtime/
process teardown. See [`data/M3_SYNTHETIC_CATALOG.md`](data/M3_SYNTHETIC_CATALOG.md).
`[login].advertise` remains validated/reserved and is not encoded into an M2 packet.

Validation reports all independently detected errors, including malformed or
duplicate/zero-port binds, nonrepresentable advertised IPv4/fixed-width fields,
any public bind without acknowledgement, nonlocal auto-create, zero/inconsistent
limits, unknown client profiles, missing DB secret sources, and starter
inconsistency. M2 transport still requires `security.malformed_strike_cap = 1`:
encrypted client frames cannot be safely resynchronized after malformed input,
so the first malformed or invalid-state transport strike is observed and closed.
GameService unknown post-channel opcodes use the separately bounded policy,
strike count, and metadata-digest capture capacity. Global, masked-source,
normalized-username, and per-connection count/weighted-byte budgets use
fixed-capacity windows. All allocation, concurrency, retry, rate, frame, actor
queue, and duration values have hard upper bounds. Game-specific bounds include
rooms (4096), lobby command/event queues (8192 each), normal room commands (4096),
room control commands (64), per-connection outbound room events (4096), room
commands per window (10000), chat messages per window (1000), unknown strikes
(32), and unknown captures (4096). The nonzero GameService command timeout cannot
exceed process shutdown grace. The provisional character allowlist caps at 64
entries and starter items at 256; the starter-item cap is also enforced before
allocation or SQL at every PostgreSQL create/grant boundary. Stable keys cap at
64 bytes. Credential operation timeout must not exceed process shutdown grace.
LoginService uses direct bounded framed writes; GameService additionally uses its
bounded room actor and per-connection event queues. When GameService is enabled, each configured
process-total `global_connections`, `global_accepts_per_window`,
`global_logins_per_window` (Login login/Game handover auth),
`global_packets_per_window`, and `global_bytes_per_window` must be at least two.
Each total is split deterministically: Game receives integer floor-half and Login
receives the remainder, so the two quotas sum exactly to (and never inflate) the
configured process total. With GameService disabled, Login receives the complete
total. Per-source and per-connection limits remain service-local; the per-source
connection cap is clamped to each service's connection partition. GameService
reuses the remaining validated source, window, codec, authentication deadline,
idle deadline, and shutdown bounds. `data.load_timeout` is nonzero and capped at
60 seconds.
Configuration `Debug` redacts the resolved database URL and secret-file path.
