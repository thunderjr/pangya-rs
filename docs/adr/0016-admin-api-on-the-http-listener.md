# ADR-0016: serve the operator admin API from the `[http]` listener

- Status: accepted
- Date: 2026-08-09
- Related: [ADR-0007](0007-legacy-secret-argon2id.md) (credential policy),
  [ADR-0015](0015-client-patch-web-service.md) (the separate-listener rule this does *not* follow)

## Context

Every operator action today is a CLI invocation — `pangya-server account create|grant|handover`
— or raw `psql`. Neither scales to "show me every account's balance" or "give this player a
club set", and hand-written SQL bypasses the row locks, overflow refusals and audit rows the
domain layer exists to enforce. `PROGRESS.md` has named "the operator admin surface" as Tier C
work since the M7 checkpoint.

The panel itself is a separate Vite project outside this repository. What this ADR settles is
where its API lives, how it authenticates, and what a mutating HTTP surface is allowed to do.

ADR-0015 established that the client patch contract gets a listener of its own, deliberately
**not** extra routes on `[http]`, because "health, readiness, and metrics must not be"
reachable by whatever machine runs the client. That reasoning is about *audience*, and the
admin API's audience is the operator — the same audience `[http]` already serves.

## Decision

1. **A new crate, `pangya-admin`**, holds the API. It depends on `pangya-storage`, which is why
   it is not folded into `pangya-observability`: that crate would otherwise acquire a database
   dependency it has no reason to hold.

2. **Mounted on the existing `[http]` listener**, nested at `/admin/v1`. One listener, one
   audience. `pangya-observability` holds the prefix constant and knows nothing about
   `pangya-admin`; a test in `pangya-server` pins it to the crate's own `ADMIN_PREFIX` so the
   duplication cannot drift.

3. **The panel is a separate service and this listener serves no HTML.** `pangya-admin` runs
   flat on port 5173 and proxies `/admin/v1` (and `/health`) through to this listener, so the
   browser still sees one origin. That is what lets the session cookie work with no CORS layer,
   no `tower-http` dependency and no `SameSite` relaxation — the three things a genuinely
   cross-origin panel would have forced.

   An earlier revision of this ADR had the server serve the built panel from `/admin/ui`. That
   is gone: it bought the same same-origin property the proxy already provides, while adding a
   static file server, a path prefix on every asset URL, a router basepath, and a
   trailing-slash redirect to the server. In a compose stack where the panel is its own
   service anyway, none of that earns its keep.

4. **Disabled by default** under `[http.admin_api]`, like `[game]` and `[client_web]`. Enabling
   it logs a warning naming what changed, because this is the first HTTP surface in the project
   that can mutate player state. The `--acknowledge-public-bind` gate is unchanged: the
   listener stays on loopback and is reached over the existing tailnet forwarder.

5. **Authority is a role on the existing account.** `accounts.role` is `player` or `admin`; an
   operator signs in with the same credentials a player signs in with. There is no second
   identity system and no second credential policy to keep correct. The first admin is created
   out of band with `pangya-server account role`, because the API can only grant a role to
   someone who already holds one.

6. **The browser sends the password, not its MD5 digest.** The wire protocol's legacy transport
   secret is an MD5 digest (ADR-0007), so the endpoint derives it server-side and feeds the
   same `CredentialPolicy` LoginService uses. MD5 here is a format conversion whose output is
   the *input* to Argon2id; it is never treated as a password hash. Sending the digest from the
   browser would gain nothing — it is password-equivalent to whoever holds it — and would put a
   second hashing implementation in JavaScript.

7. **Sessions reuse the handover bearer construction.** Nonsecret UUID selector, 256 OS-random
   bits, only the SHA-256 digest persisted, constant-time comparison. Two bearer schemes in one
   codebase is one more than anyone can keep correct.

8. **Authorisation is resolved per request, not per session.** `resolve_admin_session` joins
   `accounts` and re-reads `role` and `status` every time, and `set_account_role` revokes
   outstanding sessions in the same transaction that demotes. Demoting or banning an account
   therefore takes effect on its next request rather than at the session's expiry.

9. **Sign-in failures are uniform.** Unknown username, wrong password, wrong role and inactive
   account all return the same `401`, and an unknown username still pays the Argon2id
   verification cost against a decoy hash so absence is not distinguishable by timing. The
   specific reason is logged, not returned.

10. **Admin mutations get their own audit table.** `admin_audit_events` is append-only by
   trigger, carries an open action vocabulary and a JSONB detail, and is written in the same
   transaction as the mutation it records. `operator_audit_events` keeps its closed two-value
   CHECK and its meaning: "the binary did this". The new table means "a signed-in human did
   this".

11. **The economy ledgers are not touched.** `economy_operations` and its three ledgers are
    append-only and cross-checked by `enforce_economy_ledger_authority`. Admin item grants will
    write `inventory_items` plus `admin_audit_events`, not a synthetic `purchase` row, so the
    player-economy ledger keeps meaning "things the player did".

## Consequences

- The operator gets a real console, and every mutation through it is attributable to an account
  and recorded in a row that cannot be edited away.
- A demoted or banned operator loses access immediately, at the cost of one join per request.
  That join is against two indexed primary keys and is far cheaper than the Argon2id
  verification it replaces on every request after the first.
- `serve_admin` changed shape, so `pangya-observability` now serves with
  `into_make_service_with_connect_info`. Health and metrics are unaffected; the admin API needs
  the peer address to derive the same masked prefix the login path persists.
- Adding `md-5` to the dependency baseline. It is RustCrypto, MIT OR Apache-2.0, and the same
  family as the already-present `sha2`. Hand-rolling it in a repository with this provenance
  discipline would have been worse.
- This ADR does **not** claim the admin API is safe to expose publicly. It has no TLS of its
  own and relies on the loopback bind plus an already-encrypted tailnet. Publishing it is a
  separate hardening decision.

## Alternatives rejected

- **A listener of its own, mirroring ADR-0015.** That ADR's reason was audience separation from
  the *client's* machine. Here the audience is identical to health and metrics.
- **Serving the built panel from this listener.** Tried, then removed — see decision 3.
- **Letting the panel call the server's origin directly.** Would need CORS with credentials and
  a relaxed `SameSite`, to avoid one proxy rule.
- **A Node/Bun backend talking to Postgres directly.** Fastest to build, and it bypasses every
  row lock, overflow refusal, trigger and audit row the domain layer enforces. The panel would
  have been able to corrupt state no in-game path can reach.
- **A separate operator identity table.** A second credential policy to keep correct, and admin
  actions attributable to an identity with no relationship to the accounts they act on.
- **Widening `operator_audit_events`.** Its `action` CHECK is a closed set for a reason. The
  panel grows verbs faster than a migration cadence can track, and merging the two would have
  cost the CLI ledger its meaning.
