# U.S. 852 issue #1 evidence capture plan

> **Status: capture procedure, not protocol evidence.** Run only after issue #45's loading
> disconnect is fixed. This plan neither changes a packet layout nor makes a retail claim.

Issue #1 is blocked by three incompatible claims, recorded in its queue disposition:

- `0x0042` has no PacketDoc layout; pangbox leaves it TODO, while SuperSS only suggests `u8`
  count followed by `u32` values (`opensource-references/Acrisio-Filho--SuperSS-Dev/Server Lib/Game
  Server/GAME/versus_base.cpp:1548-1593`).
- C2S `0x0019` is three `f32`s in PacketDoc, but S2C `0x0060` is 12 bytes there and 16 bytes
  (connection ID plus three `f32`s) in pangbox. The current branch emits the latter
  (`crates/pangya-protocol/src/us852_match.rs`, `RetailCometRelief`).
- PacketDoc says the three `0x0014` meter inputs have no response and identifies `0x0058` as
  the response to `0x0015`; the issue table previously associated `0x0014` with `0x0058`.

The existing observer records only metadata (`GameObserver::frame`), so this branch adds a
separate, opt-in capture for precisely these decrypted GameService payloads:

| Direction | Opcode | Why |
|---|---:|---|
| C2S | `0x0014` | the complete meter sequence |
| C2S | `0x0015` | power-toggle sample, if the client sends one |
| C2S | `0x0019` | comet-relief request |
| C2S | `0x0042` | aiming-arrow body |
| S2C | `0x0058` | possible power-toggle announcement |
| S2C | `0x0060` | comet-relief announcement |

It is **not** a general body logger. Authentication (`0x0002`), handovers, chat, account data,
assets, and every other opcode remain metadata-only. A selected body over 1,024 bytes is logged
only as an over-limit warning and length; it is not dumped. The expected SuperSS arrow maximum is
1,021 bytes (`1 + 255 * 4`), so a conforming count-plus-words observation fits the limit.

## 1. Preparation and server command

On the Linux server, use an ignored retail configuration that has the usual real-client catalog,
`game.retail_bootstrap = true`, enabled `game.stroke_two`, a one-hole room/course matching that
configuration, and the issue #45 fix. Do not put credentials, bearer values, PAK paths, or client
files in the capture directory.

```bash
cd /private/tmp/pangya-rs-issue-1
git status --short --branch
cargo build --release --locked -p pangya-server -p pangya-test-client

# PANGYA_CONFIG names the existing ignored retail TOML and DATABASE_URL is already supplied by
# the shell/secret manager. This creates local-only evidence with restrictive permissions.
export PANGYA_CONFIG="$PWD/local-data/run/retail.toml"
export CAPTURE_DIR="$PWD/local-data/captures/issue-1-$(date -u +%Y%m%dT%H%M%SZ)"
umask 077
install -d -m 700 "$CAPTURE_DIR"

# Do not enable logging.packet_bodies. The explicit environment override is the six-frame
# allowlist above; JSON makes direction/opcode/length/hex mechanically extractable.
export PANGYA__LOGGING__US852_ISSUE_1_CAPTURE=true
export PANGYA__LOGGING__FILTER='info,pangya_observability=info'
export PANGYA__LOGGING__FORMAT=json
export PANGYA__PROTOCOL__UNKNOWN_OPCODE_POLICY=capture
set -o pipefail
./target/release/pangya-server --config "$PANGYA_CONFIG" serve \
  2>&1 | tee "$CAPTURE_DIR/server.jsonl"
```

Keep that terminal running. `capture` is for unrelated unknown metadata only; it does not expose
raw unknown bodies. After the run, unset the three `PANGYA__LOGGING__...` variables and return the
normal policy to `disconnect` before any normal play.

In a second Linux terminal, after the real host has created its room, add the normal retail-wire
second seat. The script mints its bearer through the environment rather than the command line.

```bash
cd /private/tmp/pangya-rs-issue-1
DATABASE_URL="$DATABASE_URL" PANGYA_CONFIG="$PANGYA_CONFIG" \
  scripts/second-seat.sh --username <second-seat-username> --room <room-number> --strokes 0
```

Use two real clients instead if visual receipt by the other player is needed; the checked-in
Windows harness supports per-process targeting. Do not capture a packet trace, process list, or
shell history containing credentials.

## 2. Exact Windows UI sequence

Copy the checked-in harness to the VM if necessary, then start/sign in two distinct local test
accounts. The second account can instead be the Linux second seat above.

```powershell
. C:\tools\pangya-client.ps1
$host = Start-PangyaClientAt -Path 'C:\pangya\us851'
Invoke-PangyaSignIn -ProcessId $host -Id '<host-id>' -Password '<host-password>' -Nickname '<host-nick>'
Set-PangyaTarget -ProcessId $host | Out-Null
Open-PangyaMultiplay
New-PangyaDefaultVersusRoom -LongTimer
# Read the room number from the host's room header and use it in second-seat.sh.
# Once the other seat is present, have it Ready; leave the host as the room master.
Start-PangyaRoomMatch
```

Wait for the host to receive its first turn (`0x0090`/visible active meter). Do **not** start the
shot meter until each of the following isolated actions has completed; wait two seconds after each
so the log boundary is unambiguous.

1. **Aiming arrow (`0x0042`).** With the host still the active player and no meter visible, run:

   ```powershell
   Send-PangyaArrow -Direction Up
   Send-PangyaArrow -Direction Right
   Send-PangyaArrow -Direction Down
   Send-PangyaArrow -Direction Left
   ```

   This emits real extended DirectInput cursor keys, not `SendKeys`. Record the UI before and
   after the four visible shot-arrow/power-spin-or-curve selections. Stop this sub-run if the
   client does not visibly accept the sequence; do not call an absent `0x0042` proof.

2. **Meter (`0x0014`, and `0x0015` if offered).** First click the active player's visible
   Power Shot control once **only if it is enabled**, then wait two seconds. This is the only UI
   action expected to produce the optional `0x0015`. Next run exactly one normal meter cycle:

   ```powershell
   Invoke-PangyaObservedShotMeter -PowerX 300 -ImpactX 185 -Trace
   ```

   It sends the three space transitions and waits for the ball animation. Preserve every selected
   log row from the first `0x0014` through the next turn-start frame, not merely `0x0058`.

3. **Comet relief (`0x0019`).** On the host's next turn, aim at the nearest visible water/out of
   bounds area and run the same observed meter helper. When the client's **Move Comet/relief**
   dialog appears, choose one clearly different legal drop location, confirm it once, and wait
   for the ball to settle. Do not retry the confirmation: this capture needs one request/response
   pair. If no relief dialog appears, stop and report the UI precondition rather than fabricating
   a packet.

The host must complete only these one-off actions; then end the run by closing the two clients and
stopping the server. The capture is invalid if the host disconnected before the active-turn UI,
which is the issue #45 prerequisite.

## 3. Extract, redact, and decide

The new rows have `capture="us852_issue_1"`, a direction, hexadecimal opcode, payload length,
and lowercase `payload_hex`. Extract only them into a local evidence file:

```bash
jq -c 'select(.fields.capture == "us852_issue_1") |
  {direction: .fields.direction, opcode: .fields.opcode,
   payload_len: .fields.payload_len, payload_hex: .fields.payload_hex}' \
  "$CAPTURE_DIR/server.jsonl" > "$CAPTURE_DIR/issue-1-frames.jsonl"
cat "$CAPTURE_DIR/issue-1-frames.jsonl"
```

Before sharing, retain just this extracted file and redact each transient connection-ID word from
an S2C body as `CONNECTION_ID_LE32` **only after recording its byte offset and length locally**.
Do not publish server logs, original encrypted frames, salts, IP/source prefixes, account names,
room numbers, screenshots with account names, bearer values, client executables, PAK/IFF files,
or the raw full configuration. A payload here contains no credential by construction; the redaction
still keeps ephemeral identifiers out of a public issue comment.

| Observation | Decision it makes | Required result |
|---|---|---|
| First C2S `0x0042` | Establishes the actual U.S. 852 client arrow body. | Record its exact length/bytes. A `1 + 4*N` shape corroborates the SuperSS hint only; it is not sufficient to guess a relay. Record every immediate S2C row and whether the second real client visibly changes. |
| C2S `0x0019` and following S2C `0x0060` | Establishes the client relief request and tests the branch's 16-byte candidate. | Request must be 12 bytes; record whether `0x0060` is emitted, its exact length, its bytes, and whether the client continues normally. |
| From first C2S `0x0014` through next turn | Separates meter state from power-toggle semantics. | Preserve all three `0x0014` bodies and every S2C frame in order. A `0x0058` must be paired with a preceding `0x0015`, not merely a nearby `0x0014`. If the UI had no enabled Power Shot control, report `0x0015` as not observed. |

A run against this branch can **falsify** its candidate (`0x0060` length 16 or no safe response) if
the real client rejects/misparses it, but it cannot positively distinguish 12 from 16 merely by
logging bytes this same server generated. Positive 12-vs-16 resolution requires the same selected
frame record from an independently operated, legally accessible U.S. 852 reference peer (or a
prior legally held decrypted reference capture), with its provenance and client build/hash recorded
outside Git. Likewise, observing that this branch emits no answer to `0x0042` is evidence of the
current gap, not proof that retail has no relay. Do not change either layout until that independent
comparison exists.

## 4. Expected local frame boundaries

The capture has no transport headers: `payload_len` is the decrypted bytes after the two-byte
opcode. The expected candidate boundaries are therefore:

| Direction/opcode | Current candidate boundary | Interpretation |
|---|---:|---|
| C2S `0x0042` | unknown, capped at 1,024 capture bytes | body is the evidence target |
| C2S `0x0014` | 5 | `u8` sequence + `f32` meter value |
| C2S `0x0015` | 1 | closed power level |
| S2C `0x0058` | 5 | current candidate: `u32` connection ID + level |
| C2S `0x0019` | 12 | three `f32` coordinates |
| S2C `0x0060` | 16 current branch / 12 PacketDoc conflict | the unresolved decision |

No capture result closes #1 by itself. It must be attached to the issue with the redacted selected
rows, UI action/time ordering, client build/hash held privately, and an explicit comparison to the
cited PacketDoc/pangbox conflict.
