# Related repositories

This project is one half of a pair. The split is by audience: this repository is written for
operators and protocol work, the other for players.

## [`thunderjr/pangya-client`](https://github.com/thunderjr/pangya-client) — the installer

A Tauri Windows app that points a legally held U.S. 852 client at a `pangya-rs` server: choose the
client folder, fill in the server address, install, play. It automates §1, §5 and §6 of
[`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md), which remains the specification it implements.

| Document | Owns |
|---|---|
| [`docs/PATCHING.md`](https://github.com/thunderjr/pangya-client/blob/main/docs/PATCHING.md) | How the client is patched: DLL side-load, inline API hooks, runtime memory patches, and the registry profile. **Authoritative** for the patch mechanism. |
| [`patches/README.md`](https://github.com/thunderjr/pangya-client/blob/main/patches/README.md) | The Rugburn `AllowMultipleInstances` patch — moved here from this repo — and how to build the DLL. |
| [`docs/INDEX.md`](https://github.com/thunderjr/pangya-client/blob/main/docs/INDEX.md) | That repository's map. |

## Who owns what

| Concern | Repository |
|---|---|
| Every server config key, port, and its bounds | **this one** — [`CONFIGURATION.md`](CONFIGURATION.md) is authoritative |
| The wire protocol, packet families, and what the server answers | **this one** |
| Extracting the client's item tables into a server catalog | **this one** (`scripts/extract-client-iff.py`, `sync-client-shop.sh`) |
| Driving a client's UI to verify *server* behaviour | **this one** — `scripts/windows/pangya-client.ps1` is QA automation, not client tooling, despite the name |
| The manual client procedure, as specification | **this one** — [`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md) |
| Rewriting the client's hardcoded endpoints (`rugburn.json`) | `pangya-client` |
| Building and shipping the Rugburn `ijl15.dll` | `pangya-client` |
| The client's registry profile | `pangya-client` |
| Validating and launching a client install | `pangya-client` |

Neither repository restates the other's facts. Port numbers and config-key names live here; the
installer cites them rather than duplicating them.

## An open disagreement

[`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md) §6 sets the registry value `IntegratedPak` to
`"0"`, reading it as "no integrated pak, the series is separate files", and then works around the
resulting `WAppException("Cannot open file.")` by flattening all 41,192 extracted resources into
the client directory as loose files.

The installer takes the opposite reading: `projectg700gb+.pak` **is** the integrated base archive —
32,389 entries, 1.1 GB, holding the whole `data/` tree including the `chat.bin` the client dies on
— so `"0"` prevents it from mounting and the flatten is a symptom rather than a fix. A registry
export for this same build in
[`juanangel123/pangya-server`](https://github.com/juanangel123/pangya-server) names the archive
rather than `"0"`.

**Neither reading has been tested against a real client.** Until it is, §6 here and
`docs/PATCHING.md` there disagree on purpose, and both say so.
