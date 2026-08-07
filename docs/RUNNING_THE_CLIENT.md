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
| LoginService handshake, login, server list | Real U.S. opcodes and layouts, MD5 client secret handled. Plausible but **unverified** against a client. |
| GameService bootstrap | Retail packets are implemented in `pangya-protocol` but **not yet wired into the runtime**, which still emits the synthetic family. A real client will fail here. |
| Lobby, rooms, match | Synthetic `0x7f**` families only. A real client cannot use them. |

So today this gets you as far as testing login and server selection. Bootstrap wiring is
the next step; see `PROGRESS.md`.

## 1. Obtain and extract the client

The client is not distributed here. Community preservation archives carry the final U.S.
build; the highest available is `PangYa_Client_US_851.zip` (2,362,368,969 bytes). Its patch
level is 851 while the protocol version it speaks is `852.00` — both are correct, see
`evidence/US_CLIENT_ACQUISITION_2026-08-07.md`.

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
import struct, hashlib, sys
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

## 4. Configure the server

Start from `config/local.example.toml`. The parts that matter here:

```toml
[game]
enabled = true

[data]
catalog_required_m3 = true
iff_directory = "./local-data/us851-data/iff"
manifest = "manifest.toml"
```

Set `DATABASE_URL`, run the migrations, and create an account. The client sends its
password as an MD5 digest, which the server treats as a transport secret and stores under
Argon2id; see `CONFIGURATION.md` for the account-creation command and its secret sources.

Keep every listener on loopback. Any non-loopback bind requires
`--acknowledge-public-bind` and is not what you want for client testing.

## 5. Redirect the client

The client hardcodes the retail endpoints, so it needs Rugburn — a drop-in replacement for
`ijl15.dll` that disables GameGuard and rewrites Winsock connections. It lists U.S. 431–852
as supported, which covers this build. It is vendored at
`opensource-references/pangbox--rugburn` and is built with MinGW32 or Visual Studio.

1. Back up the client's existing `ijl15.dll`.
2. Copy Rugburn's `ijl15.dll` into the client directory.
3. Run `ProjectG.exe` once to generate `rugburn.json`.
4. Add `PortRewrites` entries sending the login and game ports to `127.0.0.1` on the ports
   your config binds.

Run `ProjectG.exe` directly. The launcher and updater are not involved and should not be
used — there is no patch server to talk to.

## 6. What to report back

Because none of this is client-verified yet, the useful signal is exactly where it stops:

- Does the client reach the server-selection screen? That exercises the login layouts.
- If it errors during handover, the code matters. `11` is a server version mismatch, `1`
  and `9` send it back to LoginService, `3` means it could not reach LoginService at all.
  These are enumerated in `protocol/US852_RETAIL_BOOTSTRAP.md`.
- If it hangs on the loading screen, the bootstrap sequence is incomplete rather than
  malformed — that is the expected failure today.

Packet-body logging stays off; `logging.packet_bodies = true` is rejected. Report opcodes
and observed behavior rather than captures, and never commit a capture.
