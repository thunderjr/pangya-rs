# ADR-0004: Tokio codec and bounded room actors

- Status: **Accepted**
- Date: 2026-08-05

## Context

This centralizes wire bounds and avoids shared hot Arc<Mutex<Room>> state.

## Decision

Use a custom bounded tokio-util codec. Mutable room state will be owned by one Tokio task receiving commands through bounded channels.

## Consequences

The decision is normative for the M1 foundation. Reversal requires a superseding ADR and updated evidence.
