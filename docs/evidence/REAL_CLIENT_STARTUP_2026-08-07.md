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

## 5. Where it stops, and how it was narrowed

### The client is WinLicense-protected

Attaching `cdb` prints the protector's own banner and then the process is terminated:

```
---        WinLicense Professional           ---
---      (c)2012 Oreans Technologies         ---
```

followed by `ntdll!NtTerminateProcess`. That explains every earlier debugging dead end, and two
things follow from it:

- **The crash report's symbol names mean nothing.** Every frame resolves to
  `RFSingleton<RFWindowFactoryManager>::_singleton+0x…` or `RFMatrixTransform2DPoint+0x…` with
  offsets in the hundreds of kilobytes, because the image is packed with randomized section
  names and those are simply the nearest preceding exports. An earlier reading of them as
  "the fault is in the window/render subsystem" was wrong.
- **A debugger cannot be used**, so first-chance information has to come from inside the
  process.

### The technique that worked: a vectored handler inside Rugburn

Rugburn is already injected as `ijl15.dll`, so a `AddVectoredExceptionHandler(1, …)` in its
`DllMain` sees every exception first-chance, returns `EXCEPTION_CONTINUE_SEARCH` so behaviour is
unchanged, and the protector never notices. For an MSVC throw
(`0xE06D7363`) the record carries `magic`, the thrown object, and its `ThrowInfo`, so the handler
can walk `ThrowInfo → CatchableTypeArray → CatchableType → TypeDescriptor+8` for the RTTI name
and print any readable ASCII the object's fields point at. Every dereference is guarded by
`VirtualQuery` first, so a bad pointer cannot fault the handler.

That produced the answer immediately:

```
[veh] C++ throw #1 at 75ff3184 params=3
[veh]   magic=19930520 obj=0019df70 throwinfo=00d7358c
[veh]   rtti-type -> ".?AVWAppException@@"
[veh]       points-at -> "Cannot open file."
[veh]       points-at -> "chat.bin"
```

### Three loose client files were missing

The client opens `chat.bin`, `nick.bin`, and `bbh.bin` **as real files in its own directory**,
not through its PAK filesystem. All three exist inside the PAK series — `pangfiles` extracts
them to `data/` — but a bare-name lookup does not find them there, and placing them under
`data/` on disk does not satisfy it either. Copying the three into the client root does. Each
one was found by fixing the previous and re-reading the handler's output, so the chain is
`chat.bin → nick.bin → bbh.bin`. Adding the other eight top-level extracted files changes
nothing, so three is the minimum and the maximum that matters.

With them in place the C++ exception is gone entirely and the client gets measurably further —
its crash report now includes an `Inst Dir` line it never reached before.

### What remains

A deterministic access violation, at the same address on every run:

```
[av] #1 at 00888d08 accessing 00000030
[av]   eax=0d1f4790 ebx=00000000 ecx=00000000 edx=0d1f4790
[av]   esi=0d5e0fd0 edi=00000000
```

`ecx` is zero and the instruction reads `[ecx+0x30]`: a method called on a null `this`. The
object in `esi` has a vtable at `0x00d06c94`, and the object `eax`/`edx` both point at is
entirely zero-filled — something that should have been constructed was not. No filename or
other string appears anywhere on the stack, so unlike the previous failure this one is not a
missing file.

Additionally ruled out for **this** failure, each by direct observation:

| Hypothesis | How it was ruled out |
|---|---|
| Theme JPEG decoding | Serving an empty theme document, so the client downloads no images at all, produces the identical AV at the identical address |
| Loose files shadowing PAK content | Reducing the root additions from 11 files to only the three the client demands produces the identical AV |
| The translation catalog's content | An empty catalog body produces the identical AV |
| Hypervisor detection | The guest now reports `HypervisorPresent: False` with real-looking SMBIOS strings; unchanged |

Getting further needs static analysis of an unpacked image (the faulting address is
`ProjectG.exe+0x488d08`), which is a different kind of work from anything above and is not
attempted here.

Also still eliminated from the original 20-second exit, and unchanged by any of the above:
audio, all three HTTP prerequisites, `IntegratedPak`, GameGuard, Rugburn's cosmetic US 852
patches, DNS, Direct3D availability, the client's graphics settings, client file integrity, the
launcher argument, and `.dat` string-table alignment.

**No socket is ever opened to LoginService**, so no part of the game protocol has been exercised
by a real client yet.

## 6. Test-harness note

macOS's Application Firewall gates incoming connections per application. The unsigned
`pangya-server` is not on its allow list, so a remote peer completed the TCP handshake and
then received nothing, while the same listener answered correctly on loopback —
`python3` is explicitly allowed, which is why a Python server on the same address worked and
made this confusing. Resolved without changing the host's security posture by keeping the
server on loopback and publishing it with `scripts/tailnet-forward.py`. This is harness
scaffolding; the server's loopback default is unchanged.
