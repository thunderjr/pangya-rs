# ADR-0006: PostgreSQL and SQLx forward migrations

- Status: **Accepted**
- Date: 2026-08-05

## Context

M2 needs transactional aggregate creation, database-enforced normalized-name
uniqueness, row locking for single-use handovers, and reproducible schema setup.
The workspace MSRV is Rust 1.93. SQLx 0.9 requires Rust 1.94.

## Decision

Use PostgreSQL 17 and SQLx 0.8.6 with bounded pools, explicit transactions,
embedded migrations, and committed offline query metadata. Migrations are
forward-only: once released, a migration is never edited or rolled back; fixes
are new migrations. PostgreSQL is the acceptance database, not SQLite or mocks.
CI starts an empty PostgreSQL service, runs migrations and integration tests,
and checks `cargo sqlx prepare --workspace --check`.

Repository APIs expose domain DTOs and typed errors, never SQLx row structs.
Named uniqueness constraints are mapped to friendly domain failures. Every
static production repository statement uses SQLx's checked `query!`,
`query_as!`, or `query_scalar!` macro; dynamic SQL is limited to test-only DDL
and table-driven black-box assertions.

Starter grants serialize on the profile row. The first grant inserts the entire
starter aggregate; a replay locks and exactly compares the persisted character,
item keys/types/quantities, equipment selection, and setup state. An identical
replay performs no writes. Configuration drift fails rather than mutating or
partially repairing a prior grant.

## Consequences

Local and CI builds can compile with `SQLX_OFFLINE=true`. Schema changes require
a forward migration, PostgreSQL tests, regenerated `.sqlx` metadata, and an
offline metadata check. Before the first release, review fixes may update the
single unreleased M2 migration; after release, fixes are new migrations. SQLx
must remain on 0.8.x until the project MSRV permits a reviewed upgrade.
