# M2 domain and PostgreSQL foundation evidence — 2026-08-05

## Scope

This slice implements M2 domain value objects, security primitives, PostgreSQL
schema/repositories, and real-database acceptance tests. It deliberately does
**not** implement LoginService runtime state handling, synthetic TCP login E2E,
server selection, rate limiting, configuration composition, tracing, or the
operator account CLI. It does not start GameService/gameplay. M2 therefore
remains in progress.

## M2 name policy pending real-client validation

Until a legally held U.S. 852 client validates its input limits, M2 applies this
explicit policy:

- ASCII whitespace is trimmed only at the two ends;
- normalized uniqueness keys are ASCII lowercase;
- usernames are 3–32 ASCII alphanumeric/underscore bytes;
- nicknames are 3–16 ASCII alphanumeric/underscore/hyphen bytes;
- display spelling and normalized keys are stored separately;
- PostgreSQL named unique constraints are authoritative under races.

Changing these limits after client evidence requires a forward migration if
persisted values/constraints are affected.

## Implemented checklist

- [x] Private checked account/character/inventory/equipment/item-type IDs.
- [x] Domain-only account, profile, credential, starter, equipment and handover DTOs.
- [x] Runtime/SQL/wire-neutral repository contracts and typed errors.
- [x] Strict canonical MD5-hex input and exact Argon2id-v19 PHC policy, including rejection of extra/missing/downgraded parameters and noncanonical salt/output shapes.
- [x] Redacted credential/bearer formatting and errors.
- [x] UUID + 256-bit URL-safe handover bearer, SHA-256 storage digest, constant-time comparison, and canonical IPv4 `/24` or IPv6 `/56` source-prefix minimization.
- [x] PostgreSQL 17 schema with named normalized-name uniqueness, ownership FKs and numeric/range checks.
- [x] Explicit account/starter/status/handover transactions and `FOR UPDATE` consume serialization.
- [x] Profile-row-serialized starter creation: exact replay is write-free and any stable-key/type/quantity/equipment drift is rejected; IFF validation remains M3.
- [x] Every static production repository statement uses checked SQLx macros; dynamic SQL exists only in black-box test DDL/assertions.
- [x] Embedded forward-only migrations and SQLx 0.8.6 offline metadata.
- [x] Empty migration and repository/race/constraint tests against PostgreSQL.
- [ ] LoginService runtime/state machine and bounded blocking hash execution.
- [ ] Synthetic LoginService E2E and server-list flow.
- [ ] Operator account creation CLI and final config/tracing composition.
- [ ] Real U.S. 852 login-order/field validation.

## Database acceptance coverage

`crates/pangya-storage/tests/postgres.rs` uses `#[sqlx::test]` isolated real
PostgreSQL databases. It covers empty embedded migration; successful aggregate;
duplicate normalized username; database-triggered rollback at account,
credential, profile, character, inventory, equipment, and setup mutations;
normalized nickname race; write-free starter replay/concurrency plus all
configuration-drift cases; handover wrong digest/target, exact-expiry, replay,
and concurrent consume; direct ban revocation/reactivation and a concurrent
ban/consume race; privacy-minimized source-prefix roundtrip/storage; status plus
revocation rollback; and direct check-constraint violations for balance sign,
item-type range, quantity, digest length, source-prefix canonicality, and status.

## Security boundary

Argon2 work is never performed by the repository or inside its transactions.
The storage test PHC string is synthetic. No bearer or credential is formatted
by diagnostics. Handover lookup uses the UUID selector, locks the row, and only
then performs a `subtle` constant-time digest comparison in Rust. Raw peer IPs
are masked in the domain before repository input and cannot satisfy the database
prefix constraint unless represented as canonical `/24` or `/56` networks.
