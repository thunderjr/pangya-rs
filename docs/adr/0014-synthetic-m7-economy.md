# ADR-0014: synthetic M7 inventory, shop, and equipment checkpoint

- Status: accepted
- Date: 2026-08-07

## Context

M2–M6 established a local synthetic vertical slice through login, game bootstrap, lobby
and rooms, solo practice, and exactly-two stroke play. Each was delivered as clearly
labeled generated protocol behind an external retail gate that remains open.

M7 is the inventory/shop depth milestone. Its spec exit is "purchases use catalog prices
and remain correct under concurrency tests." The economy is the first subsystem where a
protocol defect can silently destroy or fabricate player property, so the durable
guarantees matter more than the wire shape, which is provisional anyway.

## Decision

Deliver M7 as an independent, disabled-by-default synthetic slice that preserves every
M2–M6 semantic and adds no retail claim.

### Boundary and composition

- `[game.economy]` is disabled by default. Enabling it requires `game.enabled` and a
  catalog carrying at least one shop offer including at least one consumable; composition
  fails closed otherwise, before any listener binds.
- Every bound is validated twice: once in configuration and once in `GameService::new`.
  `command_timeout` is nonzero, capped at 60 seconds, and capped by `server.shutdown_grace`
  so an in-flight command cannot outlive the grace that must cover it.
- Economy opcodes are accepted only from an authenticated connection inside a channel.

### Catalog as the sole price authority

Prices, stack limits, durability maxima, and repair rates come from the immutable catalog.
The wire carries a `type_id` and a quantity, never a price. A `type_id` that is not a shop
offer is rejected, including items present in the catalog but not sold.

### Exactly-once by client-chosen operation id

Every mutating command carries an `operation_id`. Replaying it with identical parameters
returns the original commit and moves no balance. Replaying it with different parameters
is `IdempotencyDrift` and commits nothing. This makes a client retry safe across
reconnects and restarts without the server holding session state.

### Optimistic equipment versioning

Equipment changes carry `expected_version`. A stale version is `VersionConflict`. This
resolves concurrent equipment changes without locking a player row for the duration of a
client interaction.

### Failure is never silently successful

Repository storage, arithmetic-overflow, and corrupt-data errors are deliberately **not**
mapped to a wire outcome. They terminate the connection as `EconomyPersistence`. Only
outcomes the server can truthfully assert — refusals it decided, and a deadline it
enforced — are reported to the client. A client can therefore never be told a failed write
succeeded.

### Provisional opcode family

The `0x7f40`/`0x7fc0` families are generated values chosen to avoid colliding with observed
U.S. 852 opcodes. They are placeholders. They will be replaced by reference-derived retail
layouts before any real-client gate is attempted, and they must never be identified as
retail protocol.

## Consequences

- The economy can be shipped disabled with zero effect on the M2–M6 boundary, and its
  storage layer is protocol-agnostic, so the retail pivot replaces only the wire layer.
- Idempotency is a durable property of the storage schema rather than a runtime cache, so
  it survives process restart. The end-to-end test proves this by restarting the service
  and re-reading the balance.
- Reporting a deliberately narrow outcome set costs some client-side diagnosability. That
  is the intended trade: an untruthful success is far worse than an opaque disconnect.
- Ten outcomes plus rate limiting and state gating are each proven over encrypted TCP
  against real PostgreSQL, so the retail pivot changes layouts against a behavior contract
  that is already pinned by tests.

## External gate

Unchanged from M2–M6 and not satisfied by this checkpoint. A legally held U.S. client and
legally supplied data must accept these exchanges before any retail claim. The acquired
client and what could be established about it are recorded in
[`../evidence/US_CLIENT_ACQUISITION_2026-08-07.md`](../evidence/US_CLIENT_ACQUISITION_2026-08-07.md).
