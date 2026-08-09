---
name: check-references-on-error
description: "On any client error or protocol crash, read opensource-references/ FIRST — never experiment against the client to guess"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: da50bc5f-7a59-4e1c-aeac-da9e5c532a93
  modified: 2026-08-09T01:59:50.746Z
---

When the retail client crashes, disconnects, hangs, or behaves oddly, the **first** action is
to read the vendored servers in `opensource-references/` for how they handle that exact packet
or phase. Do not start changing bytes and re-running the client to see what happens.

Highest-yield places, in order:
- `Acrisio-Filho--SuperSS-Dev/Server Lib/Game Server/PACKET/packet_func_sv.cpp` — `pacoteXXX`
  builders and `packetXXX` opcode handlers; `TYPE/pangya_game_st.h` and
  `Projeto IOCP/TYPE/pangya_st.h` for the structs (all under `#pragma pack(1)`).
- `pangbox--packetdoc/src/packets/**/*.ksy` — field-by-field specs, plus real `examples/*.bin`.
- `pangbox--server/game/{room,packet}/*.go` and `hsreina--pangya-server/.../Game.pas`.

Every fix that has actually moved this client came from a reference, not from a guess. Four
separate causes of the hole-load crash were found this way, and every field-by-field guess
tried against the client cost ~5 minutes per cycle and found nothing.

**Why:** the user has said this twice, and sessions have been lost to guess-and-check loops.

**How to apply:** on an error, first read the references and cite file:line; spawn a research
agent for the sweep and keep working the logs while it runs. Only then change code, one
reference-cited change at a time. See [[reading-the-live-client]] for the measurement
instrument to use when the references genuinely disagree or are silent.
