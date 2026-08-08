# Real U.S. 852 client startup: what it requires, and where it still stops

Date: **2026-08-07**

First session in which the acquired U.S. client was actually executed against this server.
Everything below was observed, not inferred.

## Environment

| Part | Value |
|---|---|
| Client | `PangYa_Client_US_851`, run as `ProjectG.exe` from `C:\pangya\us851` |
| Client self-report | `Pangya (Version: 1.xx, Packet Version: 2016110200)`, protocol `852.00` |
| Host | Windows 11 Pro under QEMU/KVM, 3 vCPU, 4 GiB, VirtIO GPU DOD, `dxdiag` D3D enabled through feature level 12_1 |
| Shim | Rugburn built from `pangbox--rugburn` with i686 MinGW; identifies the client as **US 852** |
| Server | `pangya-server` on macOS, PostgreSQL 17, reached over Tailscale |
| Client file integrity | All 210 game files verified byte-for-byte against the source install (only `ijl15.dll` differs, by design) |

## 1. The client will not start without three HTTP responses

This is the session's main finding. It triggers the conditional in `SPEC.md` §13.4 and is
implemented in `pangya-updater` under ADR-0015.

| Missing | Observed failure |
|---|---|
| `GET /Translation/Read.aspx` | Modal "string load failed." then exit |
| `GET …/S4_Patch/updatelist` | Modal "Plesae re-install the game or run the update program first." (client's typo) |
| `GET …/S4_Patch/extracontents/extracontents.xml` → theme document → all 30 images | Silent exit |

Each was isolated by satisfying the previous one and observing the next failure appear. The
requests are made in that order, before any socket to LoginService.

Two layout details cost real time and are worth recording:

- The theme `url` attribute is passed to WinINet **verbatim**. A leading-slash path produces
  `InternetOpenUrlA(/new/Service/…) // (no rewrite rules matched)` and the fetch fails, so the
  attribute must be an absolute URL built from an operator-advertised address.
- Rugburn's `$0` is the **first capture group**, not the whole match.

## 2. Two host prerequisites the server cannot supply

- **An audio device must exist.** With none, Miles Sound System raises a modal repeatedly and
  the client cannot proceed. Fixed permanently by giving the VM an Intel HDA controller with a
  null QEMU backend (`-audiodev none`); nothing needs to be audible.
- **`HKLM\SOFTWARE\WOW6432Node\Ntreev USA\Pangya\IntegratedPak` must exist.** Retail's updater
  writes it. Procmon showed the value queried, `NAME NOT FOUND` returned, and the "re-install"
  dialog created immediately afterwards — *before* any `updatelist` request. Setting it to the
  string `0` let the client accept the update list and mount all 84 archives.

## 3. What now works against `pangya-rs`

With `[client_web]` enabled and pointed at the client directory, the real client:

- issued **33 HTTP requests** to this server and had every one answered;
- mounted its complete PAK series — about **130,000 file operations** across all 84 archives,
  where before it opened none.

Endpoint responses observed from the client's machine:

| Path | Status | Bytes |
|---|---:|---:|
| `/Translation/Read.aspx` | 200 | 24,324 |
| `/new/Service/S4_Patch/updatelist` | 200 | 14,560 |
| `/S4_Patch/updatelist` | 200 | 14,560 |
| `/pangya/season4/patch/updatelist` | 200 | 14,560 |
| `…/extracontents/extracontents.xml` | 200 | 220 |
| `…/extracontents/default/pangya_default.xml` | 200 | 1,894 |
| `…/extracontents/default/main_bg.jpg` | 200 | 308,938 |
| `…/extracontents/default/readme.txt` | 404 | 0 |
| `/metrics` | 404 | 0 |
| `/health/ready` | 404 | 0 |

The last three matter: a file not named in the theme document is refused, and the admin
surface is absent from the client-reachable listener.

### Byte equality of the update list

The generated `updatelist` for the real 84-file directory is **byte-identical** to the output
of an independent implementation of the same format and cipher, with the same
`patch_version`. A committed golden fixture pins the same property in CI.

## 4. Two server defects this exposed

Both were real and are fixed.

- **Solo practice could not resolve a course from a real catalog.** The client's `Course.iff`
  record is a presentation row — identifier, display and Korean names, map directory, short
  name, a length-prefixed property XML filename, one float — and carries **no par**. The
  real-client parser therefore never set one, so `one_hole_course` failed and startup aborted
  with the generic "catalog loading or validation failed". Par is now operator-declared via
  `course_par`, validated against the catalog for course existence, with a distinct error that
  names the problem. `GameService` no longer re-derives par from a catalog that has none.
- **A listener that died during startup reported nothing.** Every listener and service-composition
  failure collapsed into "required runtime task exited". The typed cause is now logged first;
  that is how the catalog defect above was identified at all.

## 5. Getting the client to its login screen

Three more requirements had to be found. Each was identified by satisfying the previous one and
reading what failed next.

### The client is WinLicense-protected, so diagnose from inside the process

Attaching `cdb` prints the protector's banner and then the process is terminated:

```
---        WinLicense Professional           ---
---      (c)2012 Oreans Technologies         ---
```

Two consequences. First, the crash report's symbol names are meaningless: the image is packed
with randomized section names, so every frame resolves to the nearest preceding export plus an
offset in the hundreds of kilobytes. Second, a debugger is unusable.

What works instead is a first-chance vectored exception handler inside Rugburn, which is already
injected as `ijl15.dll`. `AddVectoredExceptionHandler(1, …)` in its `DllMain` sees every
exception before the client's own handler and returns `EXCEPTION_CONTINUE_SEARCH`, so behaviour
is unchanged and the protector never notices. For an MSVC throw (`0xE06D7363`) the record carries
the thrown object and its `ThrowInfo`, so the handler can walk
`ThrowInfo → CatchableTypeArray → CatchableType → TypeDescriptor+8` for the RTTI name and print
any readable ASCII the object's fields point at, guarding every dereference with `VirtualQuery`.
It answered immediately:

```
[veh]   rtti-type -> ".?AVWAppException@@"
[veh]       points-at -> "Cannot open file."
[veh]       points-at -> "chat.bin"
```

The same handler, extended to dump the unpacked image out of memory, made the later
access-violation site disassemblable offline.

### Requirement: the client's resources must be loose files, not PAK entries

`chat.bin` is inside the PAK series, and the client still could not open it. Placing it under
`data/` on disk did not help; placing it in the client root did, and the failure moved to
`nick.bin`, then `bbh.bin`. Disassembling the next fault site showed the same shape: the client
builds `"[s_pointer" + name + ".png"` for fifteen cursor images, calls a loader, and dereferences
the result with no null check. Those files are also PAK-resident, and again only a loose copy
satisfied it.

Every resource this client asks for is a **bare file name**, and the whole extracted `data/`
tree flattens without a single collision — 41,192 files, 41,192 distinct base names. Flattening
all of them into the client directory removed the entire class of failure at once, and the client
then rendered its 3D login scene.

That the namespace is flat and collision-free is what makes this safe rather than a hack; it is
evidently the shape the client expects.

### Requirement: the advertised patch number must not exceed the client's own

With `patch_number = 9999` the client loads, renders its scene, re-requests the `updatelist`,
and then sits there permanently with no login dialog. With `patch_number = 851`, matching the
client's own patch level, the login dialog appears. The client treats a higher number as "an
update is pending" and refuses to offer login.

## 6. The real client authenticates

With all of the above, the U.S. 852 client reached its login screen and logged in. Server side:

```
connection accepted, connection_id: 1, service: "login", client_profile: "us_852"
account authenticated, service: "login", account_id: 1
packet, service: "login", direction: "in",  opcode: 1
packet, service: "login", direction: "out", opcode: 1
```

This is the first time any real U.S. 852 protocol has been exchanged with this server. It
exercises, end to end and for real: the U.S. 852 hello, the client frame decode and decrypt path,
the login packet layout, MD5 transport-secret canonicalization with Argon2id verification, and
local auto-create.

The client then displayed its **Character Creation** screen, meaning it understood the
"needs character" outcome. Selecting Nuri (`0x04000000`, the identifier the shipped
`[starter]` policy allows) and confirming persisted the character:

```
 id | account_id | item_type_id
----+------------+--------------
  1 |          1 |     67108864
```

So `SPEC.md` §19.6 steps 1 and 2 now pass against the real client, and step 3 partially does:
first-character setup is accepted and durable.

### Where it stops now

Immediately after the character is confirmed the connection closes with
`reason: "protocol"`, and a fresh login still shows Character Creation rather than advancing.
The character row is written but the account's setup state is not advanced, so the login result
keeps reporting "needs character". That is a server-side defect in the retail setup flow, not a
client problem, and it is the next thing to fix. It is recorded as blocker 14.

Two operational findings from the same session:

- `security.login_timeout` was 15 seconds, which closed the connection while the character
  screen was still open. Interactive first-time setup needs a far longer allowance; the run
  above used 300 seconds.
- The client reads its mouse through DirectInput, so it ignores `SetCursorPos` entirely and its
  in-engine cursor does not follow it. Automating its UI needs relative `SendInput` deltas;
  pinning the cursor into a corner with a large negative delta first gives absolute positioning,
  after which engine coordinates equal client-area pixels.

## 7. Test-harness note

macOS's Application Firewall gates incoming connections per application. The unsigned
`pangya-server` is not on its allow list, so a remote peer completed the TCP handshake and
then received nothing, while the same listener answered correctly on loopback —
`python3` is explicitly allowed, which is why a Python server on the same address worked and
made this confusing. Resolved without changing the host's security posture by keeping the
server on loopback and publishing it with `scripts/tailnet-forward.py`. This is harness
scaffolding; the server's loopback default is unchanged.
