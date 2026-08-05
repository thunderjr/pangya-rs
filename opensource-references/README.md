# Open-source references

Local, shallow clones used to research the PangYa Rust rewrite. These are **reference inputs**, not vendored dependencies and not part of the parent repository's source tree. Each clone retains its own `.git` directory and upstream license.

- Snapshot date: **2026-08-05**
- Clone mode: `git clone --depth 1 --no-tags`
- Parent-repository policy: child clones are ignored by `opensource-references/.gitignore`; this manifest records reproducible URLs and revisions.
- Update a clone with: `git -C opensource-references/<directory> fetch --depth 1 origin <branch> && git -C opensource-references/<directory> reset --hard FETCH_HEAD`

## Snapshot manifest

| Category | Upstream | Local directory | Branch / revision | Revision date | Observed license | Intended use |
|---|---|---|---|---|---|---|
| Full server | [Acrisio-Filho/SuperSS-Dev](https://github.com/Acrisio-Filho/SuperSS-Dev) | `Acrisio-Filho--SuperSS-Dev` | `master` / `485ba29b4831` | 2026-07-17 | MIT | Broadest feature inventory; packet/gameplay/database behavior |
| Deployment | [Acrisio-Filho/SuperSS-Dev-Docker](https://github.com/Acrisio-Filho/SuperSS-Dev-Docker) | `Acrisio-Filho--SuperSS-Dev-Docker` | `main` / `fb60c077fa33` | 2024-05-28 | **No repository license found** | Docker topology and JP client configuration only |
| US S8 server | [K4T/Py_Source_US](https://github.com/K4T/Py_Source_US) | `K4T--Py_Source_US` | `master` / `dcfb75a84be8` | 2020-08-11 | GPL-3.0 | US GB.852 packet and feature behavior; no code copying into a permissive rewrite |
| Modern server | [hex-agon/alter-pangya](https://github.com/hex-agon/alter-pangya) | `hex-agon--alter-pangya` | `master` / `5afc41b39260` | 2025-09-27 | **No repository license found** | Clean architecture and US GB.852 behavior; factual reference only |
| Server proof of concept | [pangbox/server](https://github.com/pangbox/server) | `pangbox--server` | `master` / `91d8a5a4f3be` | 2023-07-15 | ISC, with noted BSD-3-Clause/Apache-2.0 files | Typed packet models, room actor model, Minibox, DB schema |
| Packet corpus | [pangbox/packetdoc](https://github.com/pangbox/packetdoc) | `pangbox--packetdoc` | `master` / `d61f583a3e67` | 2023-07-04 | ISC | Primary opcode and binary-layout corpus (Kaitai Struct) |
| Transport crypto | [pangbox/pangcrypt](https://github.com/pangbox/pangcrypt) | `pangbox--pangcrypt` | `master` / `2bf7a1d36591` | 2021-07-14 | ISC for project code; current Go LZO dependency is GPL-2.0 | Crypto oracle tables, framing algorithm, golden vectors |
| File formats | [pangbox/pangfiles](https://github.com/pangbox/pangfiles) | `pangbox--pangfiles` | `master` / `4311f2199d5e` | 2024-06-23 | ISC | PAK, XTEA, CRC, updatelist, and file-format behavior |
| Client shim | [pangbox/rugburn](https://github.com/pangbox/rugburn) | `pangbox--rugburn` | `master` / `7158511d52b6` | 2025-05-28 | Mixed: ISC plus Intel IJL redistributable terms | Supported client matrix and local network redirection |
| Packet analysis | [pangbox/wireshark-dissector](https://github.com/pangbox/wireshark-dissector) | `pangbox--wireshark-dissector` | `master` / `88a3aa0a1402` | 2023-06-13 | ISC | Manual capture inspection |
| Packet analysis | [pangbox/pantrant](https://github.com/pangbox/pantrant) | `pangbox--pantrant` | `master` / `76b81e83cc8f` | 2022-03-03 | ISC | PCAP/cassette workflows and traffic comparison |
| Legacy server | [hsreina/pangya-server](https://github.com/hsreina/pangya-server) | `hsreina--pangya-server` | `develop` / `1720cdceafcd` | 2021-10-16 | Apache-2.0 | FreshUp login/training/chat flow and `pang.dll` API |
| Crypto ABI sample | [hsreina/pang.dll-sample](https://github.com/hsreina/pang.dll-sample) | `hsreina--pang.dll-sample` | `master` / `fc3175092a7f` | 2016-01-07 | Apache-2.0 | Historical crypto ABI contract |
| Minimal server | [juanangel123/pangya-server](https://github.com/juanangel123/pangya-server) | `juanangel123--pangya-server` | `master` / `96010b0007e9` | 2020-06-15 | MIT | Minimal service decomposition and US 851 startup flow |
| Current C# server | [luismk/Pangya-Server-Community](https://github.com/luismk/Pangya-Server-Community) | `luismk--Pangya-Server-Community` | `main` / `a0e30985d271` | 2026-08-04 | Root MIT; **`Server/` is AGPL-3.0** | Current JP feature taxonomy; server code is copyleft reference only |
| File formats | [retreev/PangLib](https://github.com/retreev/PangLib) | `retreev--PangLib` | `master` / `8162ce129f0a` | 2024-03-30 | AGPL-3.0 | IFF/DAT/PAK/PET/SBIN/UCC/update-list format behavior only |

## Licensing boundary

This project should use a **clean-room, behavior-first rewrite** unless its final license is intentionally made compatible with every copied source. In particular:

1. Protocol facts, packet captures, interoperability tests, and independently authored data layouts may inform the implementation.
2. Do not copy from unlicensed repositories (`alter-pangya`, `SuperSS-Dev-Docker`).
3. Do not copy GPL/AGPL implementation code into a permissively licensed Rust crate. This applies to `Py_Source_US`, `Pangya-Server-Community/Server`, `PangLib`, and GPL LZO ports.
4. MIT/ISC/Apache-2.0 material may be adapted only with required notices and a provenance record.
5. Rugburn contains separately licensed Intel IJL files; do not redistribute those binaries from this repository.
6. This summary is an engineering safeguard, not legal advice. Review the upstream license files before distribution.

## Additional dependency research

The Rust transport plan uses [`lzokay`](https://crates.io/crates/lzokay) as the leading LZO candidate because version 2.x is a pure-Rust, MIT-licensed compressor/decompressor. It is not cloned here because it should be consumed as a normal Cargo dependency. Adoption remains gated on PangCrypt-vector and real-client compatibility tests.
