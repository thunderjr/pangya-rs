# ADR-0003: Modular-monolith baseline

- Status: **Accepted**
- Date: 2026-08-05

## Context

This avoids premature distributed-system complexity without preventing later separation.

## Decision

Start with one deployable server while enforcing service and dependency boundaries through workspace crates.

## Consequences

The decision is normative for the M1 foundation. Reversal requires a superseding ADR and updated evidence.
