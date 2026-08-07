# Running the U.S. client against this server

Operator guide for pointing a legally held PangYa U.S. client at a local `pangya-rs`
instance. Nothing described here is committed to the repository: the client, its PAK
archives, and everything extracted from them live only under the gitignored `local-data/`
tree, and `scripts/check-proprietary-assets.sh` fails the build if any of it is staged.

## Current state — read this first

The client is a Windows x86 binary, so it must run on Windows (or Wine/a VM). The server
builds and runs anywhere Rust and PostgreSQL do.

| Stage | State |
|---|---|
| Client startup: string catalog, patch `updatelist`, theme content | **Verified against the real client.** It performs 33 HTTP requests against this server, accepts all of them, and mounts its full 84-file PAK series. |
| Client reaches its login screen | **Blocked.** The client exits about 20 seconds in, throwing a C++ exception, without ever opening a socket. See [Where it stops](#7-where-it-stops-today). |
| LoginService handshake, login, server list | Real U.S. opcodes and layouts, MD5 client secret handled. Plausible but **unverified** — no client has reached it. |
| GameService auth + bootstrap | Complete when `game.retail_bootstrap = true`, proven end to end over encrypted TCP in CI, **unverified** against a client. |
| Rooms, one scored hole | Routed and proven over TCP in CI, **unverified** against a client. |

Everything from the login screen onward is therefore still gated on the startup blocker.

## 1. Obtain and extract the client

The client is not distributed here. Community preservation archives carry the final U.S.
build; the highest available is `PangYa_Client_US_851.zip` (2,362,368,969 bytes). Its patch
level is 851 while the protocol version it speaks is `852.00` — both are correct, see
`evidence/US_CLIENT_ACQUISITION_2026-08-07.md`. The client's own crash log confirms both,
and adds a packet version of `2016110200`.

Unpack the archive somewhere outside the repository, or under `local-data/`.

## 2. Extract the catalog

Course and item data live in the `gb`-suffixed PAK series. The vendored
`pangbox--pangfiles` reference builds a CLI that reads them. macFUSE is not needed for
extraction, so build with the `nofuse` tag:

```bash
cd opensource-references/pangbox--pangfiles
CGO_ENABLED=0 go build -tags nofuse -o /tmp/pang ./cmd/pang

/tmp/pang pak-extract -region us -o local-data/us851-data \
  $(ls local-data/us851/projectg700gb+.pak local-data/us851/projectg7*.pak \
       local-data/us851/projectg8*.pak | sort -t g -k3 -V)
```

Order matters: the set is one incremental archive and later members override earlier ones.

The catalog lands at `local-data/us851-data/data/pangya_gb.iff`. **That file is itself a ZIP
container** holding 39 per-family tables. Standard `unzip` skips its entries as "volume
label" because of how their attributes are set, so extract it with any permissive ZIP
reader, for example:

```bash
python3 -c "
import zipfile, os
z = zipfile.ZipFile('local-data/us851-data/data/pangya_gb.iff')
os.makedirs('local-data/us851-data/iff', exist_ok=True)
for n in z.namelist():
    open('local-data/us851-data/iff/' + n, 'wb').write(z.read(n))
"
```

## 3. Write the catalog manifest

The loader takes an explicit manifest so every file is hash-pinned and bounded. Use
`manifest_version = 3`, the real-client schema. Compute each table's header values and
digest:

```bash
python3 -c "
import struct, hashlib
for n in ['Character','ClubSet','Ball','Item','Part','Course']:
    d = open(f'local-data/us851-data/iff/{n}.iff','rb').read()
    c,b,v = struct.unpack_from('<HHI', d, 0)
    print(n, 'count=%d binding=%d version=%d record_size=%d' % (c,b,v,(len(d)-8)//c),
          hashlib.sha256(d).hexdigest())
"
```

Then write `local-data/us851-data/iff/manifest.toml` with one `[[files]]` block per table:

```toml
manifest_version = 3

[[files]]
filename = "Character.iff"
sha256 = "<digest>"
kind = "character"      # character | club_set | ball | consumable | character_part | course
count = 10
binding = 0
version = 13
record_size = 380
```

`Item.iff` maps to `kind = "consumable"` and `Part.iff` to `kind = "character_part"`.
`character`, `club_set`, and `ball` are required; the rest are optional.

Note the loader keeps the `.iff` extension out of the repository entirely — the asset guard
rejects that extension anywhere it scans, and `local-data/` is pruned from the scan.

### Course par is operator-declared

The client's `Course.iff` record is a presentation row: identifier, display and Korean
names, map directory, short name, a length-prefixed property XML filename, and one float.
**It carries no par.** Per-hole par lives in the course's own data inside the PAK series.

So with a real-client catalog you must declare par yourself:

```toml
[game.solo_practice]
enabled = true
course_id = 671088640   # 0x28000000, "BLUE LAGOON"
course_par = 4
```

The catalog is still what makes the value meaningful: a par declared for a course the client
does not have is rejected at startup. Leaving `course_par = 0` with a real-client catalog
fails startup with a message naming the problem, rather than guessing a number.

Real U.S. catalog identifiers, for reference: characters `0x04000000`–`0x04000009`, club sets
from `0x10000000`, balls from `0x14000000`, courses from `0x28000000`. The `[starter]`
defaults in `config/local.example.toml` (`67108864` and `268435456`) are the first character
and the first club set, so they validate against the real catalog unchanged.

## 4. Configure the server

Start from `config/local.example.toml`. The parts that matter here:

```toml
[game]
enabled = true
# Required for a real client; the synthetic bootstrap it replaces is not retail-compatible.
retail_bootstrap = true

[data]
catalog_required_m3 = true
iff_directory = "./local-data/us851-data/iff"
manifest = "manifest.toml"

# The static HTTP contract the client needs before it will start at all.
[client_web]
enabled = true
bind = "127.0.0.1:8090"
advertise = "127.0.0.1:8090"    # must be reachable from the client's machine
region = "us"
client_directory = "./local-data/us851"
entries = "paks"
translation_catalog = "./local-data/us851-data/translation_us.xml"
theme_directory = "./local-data/us851/extrares"
```

Set `DATABASE_URL`, run the migrations, and create an account. The client sends its
password as an MD5 digest, which the server treats as a transport secret and stores under
Argon2id; see `CONFIGURATION.md` for the account-creation command and its secret sources.

Keep every listener on loopback. Any non-loopback bind requires
`--acknowledge-public-bind`.

Startup logs `client patch web service ready` with the size of the generated update list.
For the unmodified U.S. series with `entries = "paks"` that is 14,560 bytes.

### The translation catalog

`translation_catalog` points at the **plaintext** catalog XML; the service base64-encodes it
the way the client expects. The document is a flat sequence:

```xml
<TEXT><KEY>101</KEY><DEV></DEV><SERVICE>Disconnected from server…</SERVICE></TEXT>
```

This content is the client's own localized strings, so it is operator-supplied and not
shipped here. Omitting `translation_catalog` serves an empty body, which the client accepts;
it then falls back to its own `.dat` strings and any string it expects only from the service
is missing.

## 5. Redirect the client

The client hardcodes the retail endpoints, so it needs Rugburn — a drop-in replacement for
`ijl15.dll` that disables GameGuard and rewrites Winsock connections. It lists U.S. 431–852
as supported, which covers this build; it identifies this client as US 852. It is vendored
at `opensource-references/pangbox--rugburn` and builds with MinGW32:

```bash
docker run --rm -v "$PWD/opensource-references/pangbox--rugburn":/src -w /src debian:bookworm \
  bash -c 'apt-get update -qq && apt-get install -y -qq g++-mingw-w64-i686 make && make'
```

1. Back up the client's existing `ijl15.dll`.
2. Copy Rugburn's `out/ijl15.dll` into the client directory.
3. Write `rugburn.json`. Its JSON parser rejects empty objects and arrays, so every key you
   include needs at least one entry. Point both the HTTP rewrites and the login port at this
   server:

```json
{
  "UrlRewrites": {
    "http://[a-zA-Z0-9:.]+/(.*)": "http://<server>:8090/$0"
  },
  "PortRewrites": [
    { "FromPort": 10803, "ToPort": 10103, "ToAddr": "<server>" },
    { "FromPort": 10103, "ToPort": 10103, "ToAddr": "<server>" }
  ]
}
```

`$0` is the **first capture group**, not the whole match. A single catch-all rewrite that
preserves the path is enough, because `[client_web]` serves every path the client asks for.
The client dials **10803**; 10103 is included because other builds in the family use it.

Run `ProjectG.exe` directly, from the client directory. The launcher and updater are not
involved; Rugburn sets `PANGYA_ARG` so the updater check is skipped.

## 6. Two host prerequisites that are not the server's

These are properties of the machine running the client. Both were found the hard way.

### An audio device must exist

PangYa initialises Miles Sound System during startup. On a host with **no** audio device at
all it shows a modal "Miles Sound System" error repeatedly and cannot proceed. A virtual
device is enough — nothing needs to be audible. Under QEMU, an HDA controller with a null
backend does it:

```
-audiodev none,id=snd0 -device intel-hda -device hda-output,audiodev=snd0
```

### `IntegratedPak` must exist in the registry

Retail's updater writes `HKLM\SOFTWARE\WOW6432Node\Ntreev USA\Pangya\IntegratedPak`. A copied
install has no such value, and without it the client shows "Plesae re-install the game or run
the update program first." (the typo is the client's) and exits — **before** it fetches the
`updatelist`, so it is not a server-side problem.

```powershell
New-ItemProperty -Path 'HKLM:\SOFTWARE\WOW6432Node\Ntreev USA\Pangya' `
  -Name 'IntegratedPak' -Value '0' -PropertyType String -Force
```

`0` means "no integrated pak", which is correct for a client whose PAK series is separate
files. With it set, the client accepts the update list and mounts all 84 archives.

### If the client cannot reach a listener that is definitely bound

On macOS the Application Firewall gates *incoming* connections per application. A freshly
built, unsigned `pangya-server` is not on its allow list, so a remote peer completes the TCP
handshake and then never receives a byte — while the same listener answers correctly on
loopback. Either allow the binary in System Settings, or keep the server on loopback and
publish it with `scripts/tailnet-forward.py`, which forwards through the interpreter:

```bash
./scripts/tailnet-forward.py <this-host-ip> 10103 20201 8090
```

## 7. Where it stops today

With everything above in place the client:

- fetches the string catalog, the `updatelist`, `extracontents.xml`, the theme document, and
  all 30 theme images from this server — 33 requests, all answered;
- mounts its complete PAK series (about 130,000 file operations);
- then, roughly 20 seconds in, throws a C++ exception and exits.

It writes its own crash report next to the executable — `exception.log`, `stack.log`, and
`exception.dmp`. No socket is ever opened to LoginService, so nothing about the game protocol
has been exercised yet.

Do not read the symbol names in that report as a location. `ProjectG.exe` is packed with
randomized section names, so every frame resolves to the nearest export plus an offset in the
hundreds of kilobytes; they say nothing about which subsystem failed. Attaching `cdb` does not
help either — the packer's anti-debugging raises streams of privileged-instruction,
illegal-instruction, and `int 1` faults long before the real throw.

Ruled out as causes: the audio device, all three HTTP prerequisites, `IntegratedPak`,
GameGuard (disabled, and confirmed not loaded), Rugburn's cosmetic US 852 patches (a build
with only the GameGuard patches crashes identically), DNS (no lookups are made), Direct3D
availability (`dxdiag` reports D3D enabled with feature levels through 12_1), the client's
own graphics quality settings, missing theme images, and client file integrity (every game
file was verified byte-for-byte against the source install).

The most useful thing to report if you get further is the crash log plus what changed.

Packet-body logging stays off; `logging.packet_bodies = true` is rejected. Report opcodes and
observed behavior rather than captures, and never commit a capture.
