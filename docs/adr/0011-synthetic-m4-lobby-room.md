# ADR-0011: synthetic M4 lobby and room checkpoint

- Status: Accepted for local synthetic M4
- Date: 2026-08-05

## Context

M3 reaches one generated local GameService channel, but the repository has no
legally validated U.S. 852 room opcodes, layouts, ordering, or create/enter
acceptance evidence. M4 still needs a locally testable concurrency and authority
boundary without presenting invented packets as retail protocol facts.

Room state also needs a single bounded owner. Sharing mutable rooms directly
between connection tasks would make admission, owner transfer, disconnect
cleanup, queue saturation, and shutdown ordering difficult to prove.

## Decision

1. Reserve the provisional local-only opcode families `0x7f00..=0x7f08`
   client-to-server and `0x7f80..=0x7f84` server-to-client. They are generated
   PangYa-RS contracts, not observed or accepted U.S. 852 values.
2. Run one bounded lobby registry task for room discovery, room-ID allocation,
   and the one-room-per-connection index. Run one bounded actor task as the sole
   mutable owner of each room.
3. Separate normal room commands from priority disconnect/shutdown control.
   Lobby commands, actor commands, actor events, per-connection room events,
   rates, deadlines, room count, occupancy, and shutdown are all bounded.
4. Derive sender, caller, membership, account, nickname, and ownership from the
   authenticated connection. Client packets never supply a sender identity.
5. Keep room passwords process-local and ephemeral: zeroize parsed input, retain
   only a random-salted SHA-256 digest, and compare candidates in constant time.
   Public summaries and snapshots expose only a protected boolean.
6. Treat known room opcodes in the wrong state as protocol errors regardless of
   unknown-opcode policy. For truly unknown post-channel opcodes, `capture`
   retains only a bounded ring of `(state, opcode, payload length, SHA-256)`;
   raw payloads are never retained.
7. Keep rooms process-local and non-durable. This checkpoint has no match start,
   loading, shot/gameplay, scoring, persistence, economy, or rewards behavior.
   M5 does not begin as a consequence of this decision.

## Consequences

The local synthetic M4 exit can prove authoritative create/list/join/leave,
password admission, capacity races, settings, ready state, chat, kick, owner
transfer, disconnect cleanup, queue/rate limits, metrics redaction, and bounded
shutdown over the existing M3 TCP and PostgreSQL bootstrap.

This does **not** complete the real M4 exit. Exact room opcodes, packet fields,
packet order, room-list/create/enter semantics, and successful room creation and
entry with a legally held U.S. 852 client remain external gates. Replacing the
`0x7f00` family with evidence-backed layouts requires fixture/provenance updates
and a superseding or narrowing decision; synthetic evidence alone is insufficient.
