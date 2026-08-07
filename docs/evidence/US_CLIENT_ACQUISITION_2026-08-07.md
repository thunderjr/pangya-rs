# U.S. client acquisition and version characterization — 2026-08-07

## Claim boundary

This file records which PangYa client artifact this project targets and what could and
could not be established about its version by static inspection. It makes no claim that any
packet layout in this repository has been accepted by that client. No client bytes, PAK
contents, or IFF records are committed; the artifact lives only in the gitignored
`local-data/` tree.

## Acquired artifact

| Property | Value |
|---|---|
| Source | `inventory.pangya.golf` public share, `Clients/US/` |
| File | `PangYa_Client_US_851.zip` |
| Size | 2,362,368,969 bytes (verified) |
| SHA-256 | `7218fa160225e8b254914c583f90abca5400c3644c65874279a02ce9e3e237f9` |
| Contents | 210 files, 2,675,925,850 bytes uncompressed |

This is the highest U.S. build in the archive. The full U.S. set is 242a, 300, 318, 431,
500a, 524, 538, 612, 622, 627, 633, 806, 851. No archive entry is named 852, in any region;
EU stops at 500 and the GB installer set stops at `PangYa_Setup_GB.R7.806.Inst.exe`.

## Installed patch level

The client carries a `projectg700gb+.pak` base plus incremental patches `701`–`730` and
`801`–`851`, every one suffixed `gb`. The highest is `projectg851gb.pak`.

This matches this repository's own patch corpus exactly: `pangya.wiki/PATCH_HISTORY.md`
records the final corpus page as `GB.R7.851.00`, dated 2016-04-27. The acquired client is
therefore the final GB/U.S. Season 8 client at its last content patch.

## What static inspection could not establish

`ProjectG.exe` is packed. Its PE section table carries randomized names (`umlaceax`,
`jgiahknf`, two blank) with virtual sizes far exceeding raw sizes, and it has no
`VS_VERSIONINFO` resource — `.rsrc` contains only `MANIFEST_RESOURCE_ID`. Two checks that
would otherwise have settled the version were therefore unavailable:

- **PangCrypt oracle tables.** `INITIAL_RESEARCH.md` records that PangCrypt's tables were
  extracted from a U.S. 852 `ProjectG.exe`. Neither `ORACLE[0]` nor `ORACLE[1]` from
  `crates/pangya-crypto/src/oracle.rs` appears in the binary at any offset, at full length
  or as a 16-byte prefix. This is consistent with packing — the tables are materialized at
  runtime — and is **not** evidence that the client differs.
- **Version resource.** Stripped, as above.

`PangyaUS.ini` is not UTF-8 text. `update.cln` is the updater and carries only the node
names it expects from a live patch server (`packetversion`, `version`), so the client's
version is served, not stored.

## Reconciliation with the `US_852` profile label

`INITIAL_RESEARCH.md` fixes the target as "GB.852 / US 852.00" on five independent grounds:
alter-pangya targets GB.852; `Py_Source_US` lists GB.852 and GB.824; PacketDoc carries a
`us_852` discriminator beside `us_824`; Rugburn lists U.S. 852 as supported; PangCrypt's
tables came from a U.S. 852 `ProjectG.exe`.

These label the client's protocol/build generation. The archive and the wiki patch corpus
label the same artifact by its final content patch, 851. The two numbering axes are not in
conflict, and Rugburn treating "PangYa US (431–852)" as one continuous supported range
points the same way.

Accordingly `CompatibilityProfile::US_852` is retained as the internal profile label — it
names the protocol generation, is corroborated five ways, and is the discriminator every
vendored reference uses. The artifact this project actually targets and tests against is the
one recorded above.

**This reconciliation is inference from converging sources, not proof.** It is falsifiable
by the client itself: the first retail layout exchange that the client accepts or rejects
tests it directly. Until then no layout in this repository may be described as
client-verified.

## Tooling implications

- `ijl15.dll` is present, so Rugburn's documented install path applies unchanged: back up
  `ijl15.dll`, replace it, and use `rugburn.json` `PortRewrites` to redirect Winsock2 to a
  local listener.
- Course and item data live in the `gb`-suffixed PAK series and require extraction before
  the `pangya-data` catalog can load anything real. `pangbox--pangfiles` is the vendored
  reference for that format.
- `ProjectG.exe` is a Windows x86 binary. Extraction and parsing are host-agnostic; running
  the client is not.
