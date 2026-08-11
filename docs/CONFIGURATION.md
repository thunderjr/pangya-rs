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
- `logging.packet_bodies = true` is rejected in M2. `logging.us852_issue_1_capture = true`
  is the sole exception: it logs only the six non-credential match-preview frames listed in
  [`evidence/US852_ISSUE_1_CAPTURE_PLAN.md`](evidence/US852_ISSUE_1_CAPTURE_PLAN.md), bounded
  to 1,024 payload bytes, for the one-off #1 evidence run. The GameService
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

`game.retail_bootstrap` selects the reference-derived U.S. 852 bootstrap sequence instead
of the synthetic one. It requires `game.enabled` and defaults to false. A real client cannot
use the synthetic bootstrap, and the retail sequence is derived from vendored references
rather than verified against a client, so neither is a retail-compatibility claim. See
[`protocol/US852_RETAIL_BOOTSTRAP.md`](protocol/US852_RETAIL_BOOTSTRAP.md) and
[`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md).

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

## Course par with a real client catalog

`game.solo_practice.course_par` and `game.stroke_two.course_par` default to `0`, meaning
"take par from the catalog". Only the generated catalogs can satisfy that: a real U.S. client
`Course.iff` record is a presentation row — identifier, display and Korean names, map
directory, short name, a length-prefixed property XML filename, one float — and carries no
par at all. Per-hole par lives in the course's own data inside the PAK series.

So with `manifest_version = 3` an enabled mode requires an explicit `course_par` in `1..=10`,
and startup fails naming that requirement rather than inventing a number. The catalog is
still what makes the declared value meaningful: a par declared for a course the client does
not have is rejected. A declared par also overrides a generated catalog's own, so the two
schemas share one code path.

## Client patch web service

`[client_web]` serves the static HTTP contract a retail client needs *before* it will open
any socket: a string catalog, the XTEA-encrypted patch `updatelist`, and the theme documents
plus the images they name. It is disabled by default. See
[`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md) for the client-side setup and ADR-0015 for
why it exists.

- It is a **separate listener** from `[http]`, deliberately. The patch surface must be
  reachable by the machine running the client; health, readiness, and metrics must not be.
  Both keep the loopback default, and any non-loopback bind still needs
  `--acknowledge-public-bind`. `client_web.bind` participates in the duplicate-bind and
  nonzero-port checks with every other listener.
- `advertise` is separate from `bind` for the same reason as `login.advertise`: the client
  resolves it on its own machine and passes it to the OS HTTP client verbatim, so a
  container-internal or wildcard address is unusable there. It becomes the absolute base URL
  in `extracontents.xml`.
- `client_directory` is required when enabled and is validated at startup. `entries = "paks"`
  lists only the PAK series, which is what the client needs to mount its data; `"all"` mirrors
  a retail patch server and also lists executables and DLLs, which means a locally replaced
  client file is listed with the server's checksum rather than the one on disk.
- `region` selects the `updatelist` XTEA key (`us`, `jp`, `th`, `eu`, `id`, `kr`).
- `translation_catalog` points at the **plaintext** catalog XML; the service base64-encodes it
  as the client expects. Omitting it serves an empty body, which the client accepts before
  falling back to its own `.dat` strings. The catalog is client content and is
  operator-supplied.
- `theme_directory` is optional. Notice, lobby, and loading entries are derived from the retail
  file-naming convention, and a file the generated theme document does not name is refused
  rather than served, so no path built from request text reaches the filesystem. Theme images
  are capped at 4 MiB each and the catalog at 4 MiB.
- Content is built once at startup, on a blocking worker, before the listener binds. Building
  the update list checksums the whole client directory — about eight seconds for the U.S.
  series — so a misconfigured directory is a startup error rather than a client-visible 404.
  Startup logs `client patch web service ready` with the generated list's size.

## Operator admin API and panel

`[http.admin_api]` is the authenticated operator console API, nested on the existing `[http]`
listener at `/admin/v1` rather than getting a listener of its own. That is the opposite of
`[client_web]`'s decision, for the opposite reason: the patch surface has to be reachable by
the *client's* machine, whereas the admin surface has exactly the audience health and metrics
already have. See ADR-0016.

**This listener serves no HTML.** The panel is a separate service — `pangya-admin`, flat on
port 5173 — that proxies `/admin/v1` and `/health` through to here. The browser therefore sees
one origin, which is what lets the session cookie stay `HttpOnly; SameSite=Strict` with no CORS
layer.

It is **disabled by default**, and enabling it logs a warning at startup naming what changed.
This is the only HTTP surface in the project that can mutate player state.

- `enabled` — off by default. When on, `/admin/v1/*` answers.
- `session_lifetime` — how long an issued session stays valid. Validation caps it at `24h`;
  beyond that a forgotten browser tab becomes a standing credential. Default `12h`.
- `logins_per_window` / `rate_window` — sign-in attempts permitted per masked source prefix
  per window. `logins_per_window = 0` is refused, because no operator could then sign in.
The cookie is issued `HttpOnly; SameSite=Strict; Path=/admin`. `Path=/admin` is tighter than
`/`: the panel is served at the root by its own service, so the cookie travels only with the
proxied API calls and never with a request for an asset.

Point the panel at this listener with `PANGYA_ADMIN_API` — `http://127.0.0.1:8080` on a
developer's machine, or the server's service name in a compose stack.

### Bootstrapping the first operator

The API can grant the admin role, but only to an operator who already holds it, so the first
one is created out of band:

```bash
pangya-server account create --username <name> --nickname <nick> --secret-stdin
pangya-server account role --account-id <id> --role admin
```

Sign-in takes the account's **password**, not the 32-hex transport secret the game client
sends; the server derives that secret itself and verifies it against the same Argon2id record
LoginService uses (ADR-0007).

### What it does not promise

The listener has no TLS of its own. It keeps the loopback default, and any non-loopback bind
still requires `--acknowledge-public-bind`. Reaching it from another machine is expected to go
over the existing tailnet, not over a public interface.

Authorisation is re-read from the database on every request, so demoting or banning an account
takes effect on its next request rather than at its session's expiry; `account role` also
revokes that account's outstanding sessions in the same transaction.

### The shop overlay

`shop_offer_overrides` (migration 0010) is the only way to change what the server sells without
a restart. The catalog is parsed once at startup from the client's own IFF tables and is
immutable; `data.price_override_pang` can reprice rows the client already sells, but
deliberately cannot make an unsold row sellable. The overlay can.

- It changes what the server **charges and permits**, never what the client **displays**. The
  client renders shop names, prices and listing from its own tables inside the PAK, so an item
  enabled here that the client does not list is purchasable by the protocol but unreachable
  through the client's shop UI, and a repriced item shows one figure and charges another. The
  `/shop` endpoint returns both values and a `drift` flag so the panel can say so.
- A write reloads the overlay and publishes it over a `tokio::watch` **before answering**, so a
  success means the change is in force rather than queued.
- Startup logs a warning whenever any override is active, because the server's prices then
  differ from the client's.
- It resolves against the catalog, so it can only reach type ids the client's own tables
  contain. An override for an unknown id is refused rather than stored inert.
- A zero price is refused: zero is how the client's tables spell "unavailable", so an override
  meaning "free" would be indistinguishable from one meaning "not sold". Use `enabled = false`.
