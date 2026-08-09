# Operator admin panel evidence — 2026-08-09

## Scope

This run proves the complete operator console: migrations 0009 and 0010 apply to an existing
schema, the admin API answers on the `[http]` listener behind a real session, authorisation is
re-read per request rather than frozen at sign-in, account/inventory/equipment mutations hold
every schema invariant, the DB-backed shop overlay changes what the server charges without a
restart, and the built panel is served same-origin by `pangya-server` itself.

Nothing here was run against the live `pangya_retail` database. Every run used a throwaway
`pangya_adminsmoke` database and a second server process on non-colliding ports.

What it does **not** prove: no real U.S. 852 client was driven through any of it. The
client-facing consequences — that a granted item appears in My Room, that an overlay price is
what the client is charged — remain the open gate, and are the same 12-step check
`docs/evidence/REAL_CLIENT_SHOP_2026-08-09.md` and `REAL_CLIENT_EQUIPMENT_2026-08-09.md`
already define.

## Environment

A throwaway PostgreSQL 17 database (`pangya_adminsmoke`) on the existing container, and a
second `pangya-server` process on ports that do not collide with the running one
(login `19103`, game `19201`, http `18081`, client_web `19090`). The production server on
`8080` was left untouched throughout.

## Migration

`sqlx migrate run` applied `0001`–`0010` to an empty database with no errors.

`0009` adds `accounts.role` with a `player` default, `admin_sessions`, and
`admin_audit_events` with an append-only trigger. Existing accounts keep the `player` role, so
applying it to a populated database changes no behaviour until a role is granted.

`0010` adds `shop_offer_overrides` and a singleton `shop_overlay_revision` bumped by a
statement-level trigger. With no rows, the overlay resolves to exactly the catalog's own
answers, so applying it changes no behaviour either.

## Bootstrap

The first admin cannot be created through the API, because the API only grants a role to
someone who already holds one:

```text
$ pangya-server account create --username smokeadmin --nickname smokeadm --secret-stdin
account created: id=1 status=active
$ pangya-server account role --account-id 1 --role admin
role set: id=1 role=admin
```

The secret supplied on stdin was `900150983cd24fb0d6963f7d28e17f72` — the MD5 of `abc`, and
exactly what a real client would send for that password.

## Startup

Enabling `[http.admin_api]` logs the warning it is supposed to, and readiness is unchanged:

```text
WARN the operator admin API is enabled; this listener can now mutate player state
$ curl -s -o /dev/null -w '%{http_code}' /health/ready    → 200
$ curl -s -o /dev/null -w '%{http_code}' /metrics         → 200
```

## The uniform-failure property

Three different reasons for refusing a sign-in produced byte-identical responses:

| Request | Response |
|---|---|
| `GET /admin/v1/auth/me` with no cookie | `401 {"error":"unauthorized"}` |
| `POST /auth/login` `smokeadmin` / wrong password | `401 {"error":"unauthorized"}` |
| `POST /auth/login` unknown username `nobodyhere` | `401 {"error":"unauthorized"}` |

The unknown-username path still runs an Argon2id verification against a decoy hash carrying
the exact policy parameters, so absence is not distinguishable by timing either. The specific
reason (`unknown_username`, `bad_credentials`, `not_an_admin`, `account_inactive`) is logged
and never returned.

## The authenticated path

```text
POST /admin/v1/auth/login  {"username":"smokeadmin","password":"abc"}
  → 200 {"account_id":1,"username":"smokeadmin","role":"admin"}
  → Set-Cookie: pangya_admin_session=<selector>.<secret>; Path=/admin; HttpOnly;
                SameSite=Strict; Max-Age=43200

GET  /admin/v1/auth/me     → 200 {"account_id":1,"username":"smokeadmin","role":"admin"}
GET  /admin/v1/audit       → 200 [{"action":"admin.session.open", …}]
```

The audit row is written in the sign-in transaction, so a session that exists is a session that
was recorded. Its `detail` carries only the masked source prefix — `{"source_prefix":
"127.0.0.0/24"}` — never a raw peer address, matching what `handover_sessions` persists.

## Panel serving

The panel is its own service, flat at `/` on port 5173, and this listener serves no HTML. It
reaches the API by proxying `/admin/v1` and `/health` through to the server, so the browser
sees one origin and the session cookie stays `HttpOnly; SameSite=Strict` with no CORS layer.

| Path | Served by | Result |
|---|---|---|
| `/` | panel | `200 text/html` |
| `/audit`, `/accounts/2` (client-side routes) | panel | `200 text/html` |
| `/assets/index-*.js`, `*.css` | panel | `200`, correct content type |
| `/admin/v1/auth/me` | proxied to server | `200` with the session cookie |
| `/health/ready` | proxied to server | `200` |
| `/admin/ui` on the server listener | — | `404`; the mount is gone |

An earlier revision served the built panel from the server at `/admin/ui`. That is removed: the
proxy already provides the same-origin property it existed for, without a static file server, a
path prefix on every asset URL, a router basepath, or a trailing-slash redirect. See ADR-0016
decision 3.

## Browser run

An unmodified Chromium drove the whole flow against the served build:

- the login screen rendered with a live `server ready` indicator sourced from `/health/ready`;
- signing in as `smokeadmin` / `abc` navigated to the overview, which displayed
  `account id 1`, `username smokeadmin`, `role admin`, and the two `admin.session.open` rows;
- `/audit` loaded directly as a deep link, with the session surviving a full page load;
- signing out redirected to the login screen, and a subsequent direct navigation to
  `/audit` redirected back to login rather than rendering.

PostgreSQL then held `1/2` sessions revoked — the browser's, and not the earlier `curl`
session, which was never signed out. That is the correct result: sign-out revokes one session,
not every session for the account.

## Automated coverage

`crates/pangya-storage/tests/admin.rs`, 12 tests against real PostgreSQL, pin the properties
that decide whether a stale or stolen session can still act:

- a `player`-role account cannot have a session minted for it, enforced in SQL rather than only
  in the handler;
- a wrong digest and an unknown selector both resolve to nothing;
- an expired session stops resolving without being revoked;
- revoking is immediate and idempotent, because a browser can send logout twice;
- **demotion revokes outstanding sessions in the same transaction**;
- **banning an admin stops its live session resolving**, via the per-request status re-read —
  `set_status` never touches `admin_sessions`, so this is what closes that hole;
- audit rows are append-only: both `UPDATE` and `DELETE` are refused by the trigger;
- a non-object audit `detail` is refused by the column CHECK;
- paging is clamped regardless of the requested limit.

`crates/pangya-admin` adds 10 unit tests, including the RFC 1321 MD5 suite (the digest must
match what a real client sends, or the admin path would reach a different stored verifier),
bearer round-trip and malformed-bearer rejection, and a check that the clearing cookie carries
the same attributes as the issued one — a mismatched `Path` would leave the browser holding it.

`crates/pangya-storage/tests/admin.rs` grew to 21 tests covering the rest: account listing
filters, search and every ordering; detail loading and `NotFound`; setting a balance *down*,
which a credit cannot express; credential replacement; empty ledger and match listings on a
fresh account; overlay round-trip with revision bumps, including enabling an unsold item and
refusing an override that inherits both fields; inventory shape rules, consumable stacking and
ownership checks; and equipment version conflict plus the equipped-row delete guard.

`crates/pangya-admin` adds unit tests for the closed sort/status/role/kind vocabularies — an
unknown value is refused rather than silently falling back — and for hex rendering.

Workspace totals with the change: **475 passing, 1 ignored**, up from 445 before. `cargo fmt
--all --check`, `cargo clippy --workspace --all-targets --all-features` and `cargo sqlx prepare
--workspace --check` are all clean.

## Account and inventory mutations

Driven over HTTP against the running server, with PostgreSQL checked after each:

| Action | Result |
|---|---|
| `POST /accounts/2/balance` `{"mode":"set","pang":42}` | `{"pang":42,"points":3200}` — the operation a credit cannot express, since grants refuse to go down |
| `POST …/balance` `{"mode":"grant","pang":1000}` | `1042`, matching `profiles` |
| `PATCH /accounts/4` `{"status":"banned"}` | `204`; the account's handovers and admin sessions were revoked with it |
| `PATCH /accounts/1` `{"status":"banned"}` (self) | `409 cannot_disable_self` |
| `PATCH /accounts/1` `{"role":"player"}` (self) | `409 cannot_demote_self` |
| `POST /accounts/3/password` | `204`; that account then signed in with the new password, and the old one stopped working |
| banning that account mid-session | its **live** session stopped resolving on the next request |
| `POST …/inventory` club set | one row, acquisition key `admin.<uuid>` — distinguishable from a starter grant and a purchase forever after |
| the same consumable granted twice (3 then 4) | one row at quantity 7, not a unique-index violation |
| a club set at quantity 5 | `409 invalid_shape` — `ck_inventory_m7_shape`, refused as a typed error rather than an opaque storage fault |
| an unknown type id | `404` |
| `PUT …/equipment` at version 0 | committed, `version` **1** |
| the same write replayed at version 0 | `409 version_conflict` |
| deleting the equipped row | `409 item_equipped` |
| deleting another account's row | `409 not_owned` |

Every one of those wrote exactly one `admin_audit_events` row. The password reset's row carries
`{}` — the fact it happened, never the password.

## The shop overlay, live

Against the operator's real extracted client tables (`local-data/us851-data/pak-iff/iff`,
6 tables, 7,918 records, 3,109 sold by the client):

```text
before          0x10000061 Papel Training Club Set: client=20000 server=20000 drift=false
PUT /shop/…     {"pang": 77}                                   → revision 2
after           0x10000061 Papel Training Club Set: client=20000 server=77    drift=true
```

And the capability `data.price_override_pang` explicitly cannot provide — offering something
the client's own tables mark unavailable:

```text
PUT /shop/0x14000000  {"enabled": true, "pang": 5}             → revision 4
0x14000000 Comet: client=not sold  server=5
```

**No restart.** The write reloads the overlay and pushes it over a `tokio::watch` before
answering, so an operator who sees a success knows it is in force. Startup logs a warning
whenever any override is active, because the server's prices then differ from the client's.

## Catalog names

`parse_client_iff_bytes` now reads the display name at record offset `0x08`, from the 0x90-byte
base shared by every family. Parsed against the real tables, it independently reproduces values
recorded elsewhere in this repository before this change existed:

| Type ID | Name | Client price |
|---|---|---:|
| `0x10000021` | Air Knight Utility Set | 10,000 |
| `0x1000002b` | Candy Club Set | 7,500 |
| `0x10000061` | Papel Training Club Set | 20,000 |
| `0x140000c9` | Cobra Comet | 1,500 |

The first two match the figures `pangya-data` recorded as empirically verified; the last two are
the exact items the shop and equipment evidence documents authored and equipped. The character
table parses as Nuri, Hana, Azer, Cecilia, Max, Kooh, Arin, Kaz, Lucia, Nell.

**The fingerprint is unchanged.** `crates/pangya-data/tests/catalog.rs` pins
`Catalog::fingerprint()` to a literal, and it still matches — so every historical
`matches.catalog_sha256` and `economy_operations.catalog_sha256` row remains valid.

## Not claimed

- **No real client was driven.** Everything above is server-side. That an operator-granted item
  appears in My Room, and that an overlay price is what the client is actually charged, are the
  open gate.
- **The overlay cannot change what the client displays.** It changes what the server charges
  and permits. An item enabled here that the client does not list is purchasable by the
  protocol but unreachable through the client's shop UI. Changing the display still means
  re-authoring the IFF and having every player re-download it.
- **Eleven of the fourteen wire slot families still do not persist**; the panel says so rather
  than showing empty fields. See `SPEC_DURABLE_PLAYER_STATE.md`.
- **No presence list.** `active_accounts` holds no player data; the status endpoint reports
  connection counts, not who is online. Tracked as `DPS-082`.
- **No TLS.** The listener keeps its loopback default and is reached over the existing tailnet;
  publishing it is a separate hardening decision.
