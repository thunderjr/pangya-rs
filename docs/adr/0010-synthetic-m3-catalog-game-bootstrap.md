# ADR-0010: synthetic M3 catalog and GameService bootstrap

- Status: Accepted for local synthetic M3
- Date: 2026-08-05

## Context

M2 produces a single-use Game-target handover, but legally supplied U.S. 852 IFF
record sizes and GameService packet layouts are not available in this repository.
Inventing those layouts and calling them compatible would be unsafe and misleading.

## Decision

The M3 local slice is explicitly synthetic:

1. Operators enable it with `game.enabled=true`, `data.catalog_required_m3=true`,
   an IFF directory, and a versioned relative manifest.
2. The immutable catalog accepts exactly the required Character, ClubSet, and
   Ball families. Each generated/test file has an eight-byte LE header
   (`count:u16`, `binding:u16`, `version:u32`) and manifest-sized records. Only
   each record's first `u32` type ID is interpreted; all remaining bytes are opaque.
3. Canonical paths must remain below the mounted root. Files are regular,
   hash-locked, bounded, exactly sized, and duplicate-free.
4. GameService consumes the M2 bearer for target Game, ignores the claimed
   identity except to require equality with the consumed authoritative account,
   loads a coherent active/complete PlayerSnapshot, validates it against the
   catalog, emits bounded minimal bootstrap packets, and enters one synthetic
   channel.
5. The four-byte hello and opcodes `0x0002`, `0x0004`, `0x0070`, `0x0072`,
   `0x0073`, `0x004d`, and `0x004e` are local synthetic contracts. Only the
   observed behavioral property that the negotiated hello key is the final byte
   is treated as externally informed.
6. GameService is disabled by default. M2 readiness and behavior are unchanged
   while disabled. When enabled, catalog load and the Game listener join readiness.

## Consequences

This provides end-to-end local safety, persistence, segmentation, abuse-control,
and lifecycle evidence without shipping proprietary data. It does not establish
real-client acceptance. Legally held U.S. 852 IFF files and packet captures remain
external gates, and any resulting layout changes require a new evidence-backed ADR.
