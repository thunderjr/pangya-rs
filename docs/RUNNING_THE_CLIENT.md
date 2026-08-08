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
| Client reaches its login screen | **Verified.** Requires the resource flattening and `patch_number` settings below. |
| LoginService login | **Verified.** The client authenticates: `account authenticated`, opcode `0x0001` in and out. |
| First-character setup | **Partly verified.** Character Creation is shown and the chosen character is accepted and persisted, but the setup state does not advance, so a returning login repeats it. Blocker 14 in `PROGRESS.md`. |
| Server list, GameService handover onward | **Unverified** — gated on blocker 14. |
| GameService auth + bootstrap | Complete when `game.retail_bootstrap = true`, proven end to end over encrypted TCP in CI, **unverified** against a client. |
| Rooms, one scored hole | Routed and proven over TCP in CI, **unverified** against a client. |

Everything past first-character setup is therefore still gated on blocker 14.

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

**`patch_number` must not exceed the client's own patch level.** With a higher number the client
loads, renders its scene, re-checks the `updatelist`, and then never offers a login dialog at
all — it believes an update is pending. For this build set `patch_number = 851`.

**Raise `security.login_timeout` for first-time setup.** The shipped 15 seconds closes the
connection while the client's own Character Creation screen is still open. Something like
`300s` is appropriate while setting an account up.

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

## 6. Client-side prerequisites that are not the server's

These are properties of the client install and the machine running it. All were found the hard way.

### An audio device must exist

PangYa initialises Miles Sound System during startup. On a host with **no** audio device at
all it shows a modal "Miles Sound System" error repeatedly and cannot proceed. A virtual
device is enough — nothing needs to be audible. Under QEMU, an HDA controller with a null
backend does it:

```
-audiodev none,id=snd0 -device intel-hda -device hda-output,audiodev=snd0
```

### Every resource must be a loose file in the client directory

This is the big one. The client asks for resources by **bare file name** — `chat.bin`,
`[s_pointer1.png`, and so on — and does not find them inside its PAK series. Putting them under
`data/` on disk does not help either; they have to be directly in the client directory.

Fortunately the whole extracted tree flattens cleanly: 41,192 files with 41,192 distinct base
names, no collisions. Flatten all of it into the client directory:

```bash
mkdir -p /tmp/flat
cd local-data/us851-data/data
find . -type f -exec sh -c 'ln "$1" "/tmp/flat/$(basename "$1")"' _ {} \;
COPYFILE_DISABLE=1 tar -cf /tmp/flat.tar -C /tmp/flat .
# then, in the client directory on Windows:
#   tar.exe -xf flat.tar -C C:\pangya\us851
```

Without this the client throws `WAppException("Cannot open file.")` for the first `.bin` it
wants, and after that a null-pointer access violation for the first cursor image.

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

With everything above in place the client renders its login screen, authenticates, and reaches
Character Creation. Choosing a character is accepted and persisted. Immediately afterwards the
connection closes with `reason: "protocol"`, and a fresh login shows Character Creation again
rather than advancing — the account's setup state is never marked complete. That is a server-side
defect, tracked as blocker 14 in `PROGRESS.md`.

### Automating the client's UI

The client reads its mouse through DirectInput, so it ignores `SetCursorPos` completely and its
in-engine cursor will not follow it. Synthetic clicks therefore do nothing to its own widgets,
even though keyboard input works. Drive it with **relative** `SendInput` deltas instead: push the
cursor into a corner with a large negative delta to pin it, then move by the target offset. After
pinning, engine coordinates equal client-area pixels, so a widget at screen `(x, y)` is reached by
moving `(x - client_left, y - client_top)`.

### If you need to know what the client is failing on

The client is protected with WinLicense, so a debugger is terminated on attach and the packed
image's symbol names are meaningless. Diagnose from inside the process: Rugburn is already
injected as `ijl15.dll`, so a first-chance `AddVectoredExceptionHandler` in its `DllMain` can read
the RTTI type name out of an MSVC throw's `ThrowInfo` and print any strings the thrown object
points at, guarding each dereference with `VirtualQuery` and returning
`EXCEPTION_CONTINUE_SEARCH` so nothing changes. It can also dump the unpacked image out of memory
so a faulting address can be disassembled offline. See
`evidence/REAL_CLIENT_STARTUP_2026-08-07.md`.

Packet-body logging stays off; `logging.packet_bodies = true` is rejected. Report opcodes and
observed behavior rather than captures, and never commit a capture.

### Scripted client automation

`C:\tools\pangya-client.ps1` on the Windows VM wraps the whole startup path — launch, login,
nickname, character confirmation, server and channel selection — so none of it has to be
rediscovered per session. Dot-source it and call the functions:

```powershell
. C:\tools\pangya-client.ps1
Start-PangyaClient                       # kills any running client, launches, anchors the window
Invoke-PangyaLogin -Id 'user' -Password 'secret'
Set-PangyaNickname -Nickname 'Someone'   # first login only
Confirm-PangyaCharacter                  # first login only, when the roster is shown
Select-PangyaServer                      # double-clicks the first server row
Invoke-PangyaDoubleClick 494 222         # enters the channel from the right-hand pane
```

Four things in that script are load-bearing, and each one cost a debugging session:

- **The window is moved to the screen origin.** Because the engine cursor is driven by relative
  deltas from a corner pin, the OS cursor ends up at the same coordinates as the engine cursor.
  At the window's default placement that point is outside the client, so the click activates
  another window and the client dismisses its modal login dialog. Anchoring keeps every click
  inside the window. All coordinates in the script are client-area pixels, so they hold wherever
  the window ends up.
- **Keystrokes use SendInput scan codes, not `SendKeys`.** `SendKeys` targets the foreground
  window and will silently type somewhere else.
- **Double clicks must fit inside the OS double-click time.** A settle delay between the two taps
  pushes the second press past 500 ms and the list row only ever selects, so the server or
  channel never opens.
- **Wait for the list to actually render before clicking a row.** Clicking row one before the
  server list arrives selects a blank row, and the client reports that as `Server is full` — the
  same message it shows for a genuinely full server, which makes this easy to misread as a
  protocol defect.

PowerShell also needs `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned`; without it
dot-sourcing fails and the calls afterwards look like they ran but did nothing.

Because the login-to-game handover is single-use and short-lived, run the flow briskly: several
minutes of manual poking between login and server selection expires it, and GameService then
rejects the auth at `stage: "handover_consume"`.

### Funding an account for shop testing

`scripts/grant-balance.sh` credits pang and points through the server's own
`account grant` command, which takes a row lock, refuses rather than wraps on overflow, and emits
an operator audit line. Updating `profiles` with `psql` skips all three.

```bash
export DATABASE_URL=postgres://...
scripts/grant-balance.sh --username rsp3 --pang 5000000 --points 10000
scripts/grant-balance.sh --account-id 147 --pang 1000000
```

The lobby header reads its balances from the two frames that close the bootstrap (`0x0095` pang,
`0x0096` points), so a grant applied while the client is connected shows up on the next login
rather than immediately.

### Detecting "Server is full."

`Test-PangyaServerFull` in `C:\tools\pangya-client.ps1` watches the red label under the server
list and returns whether it appeared. It samples for a few seconds because the label blinks.

Worth knowing: that one message covers two unrelated causes. It is what a real client shows when
the server list is malformed — a missing channel-count byte per entry produced it for a long time
— and it is *also* what it shows when a list row is clicked before the list has rendered, which
is an automation artifact rather than a server fault. `Enter-PangyaChannel` calls the check on its
last attempt so a run that fails reports the client's own diagnosis instead of a generic timeout.

### Where the client's item tables come from

The client resolves `pangya_gb.iff` — a ZIP holding `ClubSet.iff`, `Ball.iff`, `Item.iff`,
`Part.iff` and the rest — through its **PAK chain**, not from a loose file. The install carries a
long series (`projectg700gb+.pak` through `projectg820gb.pak`), and a later PAK supersedes the
same-named entry in an earlier one, so the winning copy is whichever the newest PAK provides.

A catalog built from an older copy loads and validates cleanly, and is still wrong: it silently
lacks every item added since. The symptom is a purchase refused with
`stage: "not_in_catalog"` for an id that sits inside a family's range but is absent from the
table. `pangbox/pangfiles` (`pak`) implements the overlay.

Note also that the shop's names, prices and listing are rendered from the client's own tables.
`data.price_override_pang` changes what the server *charges*, never what the client *displays*.

### Extracting the client's item tables

```bash
scripts/extract-client-iff.py --client-dir local-data/us851 --region us --list
scripts/extract-client-iff.py --client-dir local-data/us851 --out local-data/us851-data/pak-iff
```

The script reads each PAK's trailer and file table (XTEA-deciphering entry metadata and paths
where flagged), decompresses the entry with the client's LZ77 variant, and keeps the copy from
the newest PAK. Point `data.iff_directory` at a directory holding the unzipped tables plus a
manifest built from their real counts, versions and record sizes.

Do not skip this and use a loose `pangya_gb.iff`: an older copy loads and validates cleanly while
silently lacking every item added since.

### The notice dialog trap

A stray click on the player-list pane opens a "You have left a message" notice. While it is up it
swallows every later click, so a run fails somewhere unrelated and looks like a server fault.
`Invoke-PangyaClick` and `Invoke-PangyaDoubleClick` therefore clear a pending notice before every
click; `Dismiss-PangyaNotice` passes `-SkipNoticeCheck` on its own click so it cannot recurse.

Wiring the check into one high-level step is not enough — that was the first attempt, and any
other step still lost its clicks.
