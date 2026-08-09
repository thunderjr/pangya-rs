---
name: reading-the-live-client
description: "The PangYa client is packed on disk; disassemble it by reading a running process over the winvm MCP, not the .exe"
metadata: 
  node_type: memory
  type: project
  originSessionId: da50bc5f-7a59-4e1c-aeac-da9e5c532a93
  modified: 2026-08-08T21:47:00.516Z
---

`local-data/us851/ProjectG.exe` is packed — weird section names, and the bytes at a
crash address do not decode. Disassembling the file is wasted effort. Instead, read the
**running** process: `OpenProcess(0x0410)` + `ReadProcessMemory` via `Add-Type` in
`mcp__winvm-gui__PowerShell`, return the bytes base64, and disassemble locally with
Python `capstone` (installed). Its symbols are useless — one export, so every stack frame
resolves to `RFMatrixTransform2DPoint` — but `exception.log` prints raw frame addresses,
which is enough to walk the call chain this way.

A background `Start-Job` polling ReadProcessMemory every 100 ms survives the crash and
captures the last state before the process dies; that is how the hole-load crash was
narrowed to a per-player record lookup with a zero key.

**Why:** several sessions were lost guessing at protocol fields from crash symptoms.

**How to apply:** when the client dies at a fixed address, disassemble the live process
before changing any packet. See [[winvm-mcp-recovery]].
