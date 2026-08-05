# M2 LoginService vertical-slice evidence — 2026-08-05

## Scope and claim

The local synthetic M2 exit is implemented: typed configuration, bounded
LoginService runtime, operator account creation, PostgreSQL migration/audit,
read-only health/metrics, and synthetic TCP server selection with single-use
handover. This is **not** a GameService/gameplay implementation and is **not** a
claim that a real U.S. 852 client accepts the provisional response order or
handover field.

## Runtime/security evidence

- Explicit state machine covers login, nickname check/set, provisional bounded
  character selection, handover issue, server selection, complete/closed;
  table tests cover valid transitions, rejected-event immutability, bounded
  retries, and disconnect from every state.
- The hello is written before framed processing with an OS-random key `0..=15`.
- M2 intentionally uses direct framed writes and exposes no unused outbound-queue
  setting. Transport and plaintext caps bound every write.
- Credential tasks acquire a semaphore before `spawn_blocking`; an instrumented
  test proves the concurrency cap. `spawn_blocking` work is not cancellable.
  Active workers are tracked with `Notify`; LoginService waits inside its grace
  or returns a typed shutdown timeout, the supervisor adds a checked 250 ms
  cleanup allowance and inspects every drained required-task result, and binary
  runtime teardown has a final one-second bound. A successful signal cannot hide
  a cleanup error/join failure/abort timeout; an earlier failure remains primary.
  An injected over-grace worker proves bounded error and eventual worker exit.
- One total login deadline and cancellation scope covers frame waits, repository
  calls, credential work, and outbound sends. A held account-row TCP test proves
  blocked setup DB work is cancelled by that total deadline.
- Admission now acquires global/source accept budgets and global/per-source
  connection guards synchronously before spawning; JoinSet length and sockets
  are hard-bounded by `global_connections`. Stress TCP evidence proves only the
  configured tasks receive hello and 18 excess sockets close before hello.
- Fixed-capacity weighted windows cover global/source accepts; global/source/
  normalized-username login attempts; global/source/per-connection plaintext
  packet and byte budgets; and global/source connections. Unit reset/capacity
  tests and TCP paths cover each layer.
- Username and canonical 32-hex secret are validated before repository/auth.
  Argon2 runs outside DB transactions. Canonical credentials, CLI secret
  strings, decoded/encoded packet frames, crypto compression/decryption
  temporaries, handover random/encoded/decoded intermediates, pending tokens,
  and outbound session-key buffers zeroize on drop. Stdin/file CLI secret bytes
  are zeroizing from allocation through read, size, UTF-8, and conversion exits.
  A bounded tracing capture from an actual TCP connection proves credential,
  bearer, raw packet body, username/nickname, and raw peer IP absence while
  masked prefix/account ID/opcode remain observable.
- Handover is 60 seconds, source is masked to `/24` or `/56`, digest only is
  persisted, and select-server must match configured ID. Candidate friendly
  codes 5100143 (invalid credential) and 5100107 (duplicate login) are exact-
  encoded/tested but remain real-client acceptance gates.
- Fixed typed I/O and detailed truncated/limit/overflow/terminator/invalid decode
  classes plus crypto/encode/invalid-state/unknown classes, bounded true-
  unknown opcode ranges, distinct credential overload/timeout/operational error,
  exact rate classes, symmetric plaintext packet bytes, typed completed/rejected/
  cancelled/peer-closed termination reasons, and repository fast/slow/error
  latency have live observer paths and nonzero runtime tests. Only the completed
  state is counted as `complete`; service cancellation and peer EOF are distinct.
  Known opcodes in an invalid state are not counted as unknown. Unsupported stored PHC
  policy is operational failure rather than friendly credential mismatch.
- Unavailable nickname checks and duplicate nickname sets share one cumulative
  connection retry bound; TCP tests prove direct and alternating available/
  unavailable checks cannot reset or bypass exhaustion.
- Nickname/starter mutations lock the account row and reject inactive status;
  deterministic ban-first PostgreSQL races prove no post-ban setup mutation.

## PostgreSQL synthetic TCP acceptance

`crates/pangya-server/tests/login_e2e.rs` starts LoginService on ephemeral
loopback against a SQLx-isolated PostgreSQL database. It proves exact hello,
local auto-create, nickname check/set `0x000e`, configured server list,
configured server selection, recovery of the provisional `0x0003` session-key
bearer, one successful repository consume, replay rejection, starter aggregate
row counts, invalid-state close, bad credentials, duplicate login rejection,
timeout, bounded shutdown, every rate layer, pre-spawn connection stress,
malformed/invalid-state/true-unknown metrics, authentication and nickname retry
exhaustion, credential overload/timeout/cancellation, total DB deadline, stored-
PHC operational failure, RAII cleanup, and NeedsStarter. Metric/Debug and bounded runtime
trace capture search for the synthetic credential and generated bearer.

## Configuration/operations evidence

- `config/local.example.toml` and `.env.example` contain placeholders only.
- Axum `/health/live`, `/health/ready`, and optional `/metrics` are read-only;
  health unit tests prove readiness progression and false-before-drain behavior.
- `pangya-server account create` accepts username/nickname as flags but secret
  content only via stdin/named env/mounted file, runs migration, hashes on the
  bounded executor, atomically creates starter state, writes
  `operator_audit_events`, and prints only account ID/status.
- DB connect/migration uses a checked, maximum-32-attempt exponential schedule.
  A supervised continuous DB probe has its own timeout, clears readiness on a
  pending/failed probe, and restores it after injected or real recovery.
- Every allocation/concurrency/rate/retry/duration setting has a nonzero and hard
  upper bound in typed configuration; runtime constructors independently reject
  unsafe direct composition. Starter character allowlists cap at 64; the shared
  domain/storage/config/runtime starter-item cap is 256 and is checked at each
  public PostgreSQL create/grant boundary before allocation or SQL. Starter keys
  cap at 64 bytes, and
  credential operation timeout cannot exceed shutdown grace. Extreme-value
  aggregation tests cover these limits.
- Database URL and account secret files are opened once and read through a
  129-byte sentinel cap, rejecting non-UTF-8 or content above 128 bytes without
  metadata/read races. Configuration tests inject a synthetic secret resolver
  and do not require ambient `DATABASE_URL`.
- Signal success/error and every required task exit use one cleanup path:
  readiness false, cancellation, bounded drain, abort/join, and pool close. Every
  drained task result is inspected: an original error remains primary, while a
  successful signal is replaced by any cleanup task/join/timeout failure.
  Injected supervisor tests cover normal and LoginService-style over-grace paths.
- Operator success audit is in the same transaction as aggregate creation;
  trigger-injected audit failure rolls back all rows. CLI tests query success
  audit rows for stdin, named-env, and mounted-file secret sources.
- Any acknowledged public bind is durably audited before binding; injected audit
  failure prevents public-mode preparation.

## Residual external gates

1. Legally held U.S. 852 response order and actual server-list acceptance.
2. Exact nickname/name limits.
3. Exact `0x000e` unknown result semantics/order.
4. Exact handover token field, encoding, and accepted length.
5. GameService consumption/presence transfer begins in M3.
