# M2/M3 configuration reference

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
