# Real U.S. 852 practice hole against the homelab deployment — 2026-08-09

## Scope

First end-to-end run of a real client against the containerised homelab deployment rather than a
developer machine: startup contract, login, handover, room creation, a settled practice hole, and
the disconnect that followed it.

This is the first evidence of `[client_web]` being served from that deployment at all — it had
been off since the stack was created — and the first run driven from an ordinary Windows machine
on the tailnet rather than the QA VM.

- Client: U.S. 852, unmodified, on a physical Windows machine
- Account: 1
- Server: `pangya-server` container on homelab, tailnet-only publishing
- Server policy: `protocol.unknown_opcode_policy = "disconnect"`
- Client data: all 84 PAK archives and 29 `extrares/` files byte-identical to
  `PangYa_Client_US_851.zip`

Everything below is read from the server's own structured log. No capture was taken and none may
be; opcodes and observed behaviour only.

## Two startup failures, both silent on the server

Neither produced a single diagnostic server-side. Both were found by inspection, not by logs.

### 1. Another service holding the client web port — "string load failed"

`[client_web]` binds the upstream default `8090`. On homelab that port belonged to an unrelated
application, published on loopback and fronted on the tailnet by `tailscale serve`, i.e. TLS. The
client issues **plain HTTP** to whatever address it is pointed at, so `GET /Translation/Read.aspx`
drew `400 Client sent an HTTP request to an HTTPS server`. The client reports that as
**"string load failed"** and exits.

The failure is indistinguishable on screen from the service being absent entirely, and
`pangya-server` never sees the request. The other application moved off the port.

### 2. Authored PAK archives — "…has been corrupted"

The archives served to the client were synced from a working tree that
`scripts/sync-client-shop.sh` writes into, so three of them were authored rather than retail:

| Archive | Authored | Retail |
|---|---|---|
| `projectg700gb+.pak` | 1,131,865,968 | 1,131,135,933 |
| `projectg850gb.pak` | 2,355,310 | 1,625,279 |
| `projectg851gb.pak` | 730,082 | 690,312 |

A fourth, `projectg852gb.pak`, is a byte-identical duplicate of `projectg851gb.pak` and is not in
the retail series at all; it inflated the generated updatelist from 14,560 to 14,728 bytes.

**The client validates the series in order and names only the first archive that fails**, so this
surfaced as one `File Error: <name>.pak file has been corrupted.` at a time — 850 first, then 851
once 850 was replaced. Chasing them individually is the wrong shape; the whole set has to be
diffed against `PangYa_Client_US_851.zip`, whose central directory carries every archive's CRC-32
without needing extraction.

After restoring all three from the zip: 84 archives, updatelist exactly **14,560 bytes**,
translation catalog **24,324 bytes** — both matching `REAL_CLIENT_STARTUP_2026-08-07.md`.

The catalog under `data.iff_directory` was re-extracted from the corrected chain and compared
table by table; all six were already byte-identical, so no catalog change was needed.

## The run

Server-side timeline, opcodes as logged:

| Time | Event |
|---|---|
| 20:40:17 | `account authenticated`, account 1 — LoginService |
| 20:40:19 | `handover authenticated` — GameService |
| 20:41:03 | `retail room created`, room 1 |
| 20:41:33 | `retail match roster`, `players: 1` |
| 20:42:08 | shot commit, `shot_subtype: 1`, 73-byte payload |
| 20:42:34 | shot commit, `shot_subtype: 0`, 64-byte payload |
| 20:42:58 | shot commit, `shot_subtype: 1`, 73-byte payload → `0x0055` relayed |
| 20:42:58 | `0x001c` in → `0x006e` out — turn ended |
| 20:43:06 | `0x0031` in → **`0x0065`, `0x0066`, `0x0095` out** |
| 20:43:10 | `0x0006` in — accepted, no reply |
| 20:43:17 | `0x002f` in → **connection closed, `reason: "protocol"`** |

`0x0031` → `0x0065` then `0x0066` with standings, followed by `0x0095` carrying the pang balance,
is the settlement sequence `RUNNING_THE_CLIENT.md` §7.1 documents. **The hole was played and
settled.** Nine `retail equipment update decoded` records appeared earlier in the session.

## What ended it

Eleven seconds after settlement the client sent **`0x002f`**, which this server does not handle.
With `unknown_opcode_policy = "disconnect"` that closed the connection.

`0x002f` is the **user information request** — the post-round statistics and profile path.
[`protocol/US852_SUBSYSTEM_GAPS.md`](../protocol/US852_SUBSYSTEM_GAPS.md) §378 records it as
triggering a thirteen-packet response burst, and lists it as backlog item 4 together with the
`0x0006`/`0x0031` statistics submit that immediately preceded it. `0x0006` was accepted silently
at 20:43:10; `0x002f` is the one with no handler.

So the practice path is complete through settlement, and the first thing the client does *after*
settling is ask a question the server cannot answer.

Two follow-on symptoms, both consequences of that disconnect rather than separate faults:

- The client twice requested **`/Report/ReportError.aspx`** on the client web listener, which has
  no route and returned 404. That endpoint appears nowhere in `opensource-references/`; it is the
  client's own error-reporting path. Answering it with a bare 200 would stop the retry.
- Its reconnect attempt reached LoginService and was closed on `reason: "protocol"` after a single
  inbound `0x000b`.

## What this establishes

- The full startup HTTP contract works from a containerised deployment with the client tree
  bind-mounted read-only, not just from a developer checkout.
- A practice hole plays and settles for a real client against that deployment.
- `patch_number = 851` against a client at patch level 851 downloads nothing, as intended.

## What it does not

- Nothing beyond settlement. The session ends there every time until `0x002f` is handled.
- Nothing about versus. `retail match roster` reported `players: 1`.
- Nothing about the `IntegratedPak` question in `RUNNING_THE_CLIENT.md` §6 — the client used here
  was already configured by hand and that value was not varied.

## Next

1. Handle or capture `0x002f`. Setting `protocol.unknown_opcode_policy = "capture"` for one run
   collects it and anything behind it as bounded metadata; implement the batch, then flip back to
   `disconnect` and re-verify. Do not leave capture on.
2. Serve `/Report/ReportError.aspx` as a no-op 200.
3. Identify the LoginService `0x000b` the reconnect died on.
