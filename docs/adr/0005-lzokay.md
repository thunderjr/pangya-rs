# ADR-0005: lzokay 2.x adoption

- Status: **Accepted with real-client gate open**
- Date: 2026-08-05

## Context

Known PangCrypt vectors, local round trips, and one independent decompression
probe pass. Real-client acceptance remains external.

## Decision

Use lzokay 2.x for the bounded protocol foundation. Independent evidence is
recorded in [`../evidence/LZO_INDEPENDENT_2026-08-05.md`](../evidence/LZO_INDEPENDENT_2026-08-05.md).
Do not claim production or real-client compatibility until a legally held U.S.
852 client accepts generated traffic through login/channel entry.

## Consequences

The decision is normative for the M1 foundation. The independent GPL decoder was
used only as an external one-time developer tool; it is not linked, vendored, or
distributed. Reversal requires a superseding ADR and updated evidence.
