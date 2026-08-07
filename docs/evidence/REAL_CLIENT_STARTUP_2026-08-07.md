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

## 5. Where it stops

The client exits about **20 seconds** in, consistently, throwing a C++ exception
(`0xE06D7363`) from inside `ProjectG.exe`. It writes its own `exception.log`, `stack.log`, and
`exception.dmp`. **No socket is ever opened to LoginService**, so no part of the game protocol
has been exercised by a real client yet.

Two things about that crash report are worth stating so they are not over-read:

- **The symbol names in it mean nothing.** Every frame resolves to
  `RFSingleton<RFWindowFactoryManager>::_singleton+0x…` or
  `RFMatrixTransform2DPoint+0x…` with offsets in the hundreds of kilobytes. `ProjectG.exe` is
  packed with randomized section names, so those are simply the nearest exports before a large
  unpacked region. They are not evidence that the fault is in the window or render subsystem,
  and an earlier reading of them as such was wrong.
- **The minidump holds only stack memory.** The throwing frame's `_CxxThrowException` arguments
  show an exception object carrying a `std::string` of 23 characters, but the heap buffer it
  points at is not in the dump, so the message itself is not recoverable from it.

Attaching a debugger does not currently help: under `cdb` the client raises a stream of
first-chance privileged-instruction (`0xC0000096`), illegal-instruction (`0xC000001D`), and
`int 1` faults before reaching the original failure. That is the packer's anti-debugging, so
getting a first-chance report for the real throw needs anti-anti-debug tooling, not just a
debugger.

Eliminated as causes, each by direct observation:

| Hypothesis | How it was ruled out |
|---|---|
| Audio device | Fixed; Miles no longer errors, crash unchanged |
| Any of the three HTTP prerequisites | All satisfied; 33/33 requests answered |
| `IntegratedPak` | Set; client now mounts every PAK |
| GameGuard | Rugburn patches it out; no GG module or process present in the client |
| Rugburn's cosmetic US 852 patches | A rebuild with only the GameGuard patches crashes identically |
| DNS | `Clear-DnsClientCache` then run: the client resolves nothing |
| Direct3D availability | `dxdiag`: D3D enabled, feature levels to 12_1, `d3d9`/`d3d9on12`/`d3d10warp` all present and loaded |
| Client graphics settings | Windowed and fullscreen, 800×600 and 1024×768, lowest quality preset, software T&L, shadows and effects off — all identical |
| Missing theme images | All 30 served and fetched successfully |
| Client file corruption | Full name-and-size manifest diff against the source install |
| Missing launcher argument | Rugburn sets `PANGYA_ARG`; also passed explicitly on the command line |
| A `.dat` string-table mismatch | `english.dat` and `korea.dat` both read successfully and both hold exactly 3,994 index-aligned entries |

The remaining work is client-side reverse engineering of a packed, anti-debugged binary and is
recorded as an open blocker in `PROGRESS.md`, not as a server defect.

## 6. Test-harness note

macOS's Application Firewall gates incoming connections per application. The unsigned
`pangya-server` is not on its allow list, so a remote peer completed the TCP handshake and
then received nothing, while the same listener answered correctly on loopback —
`python3` is explicitly allowed, which is why a Python server on the same address worked and
made this confusing. Resolved without changing the host's security posture by keeping the
server on loopback and publishing it with `scripts/tailnet-forward.py`. This is harness
scaffolding; the server's loopback default is unchanged.
