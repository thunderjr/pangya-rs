# Client patch delivery

How an authored shop reaches a player's machine. Companion to
[`SPEC_SHOP_COVERAGE.md`](SPEC_SHOP_COVERAGE.md) (what is in the shop) and
[`adr/0015-client-patch-web-service.md`](adr/0015-client-patch-web-service.md) (why the patch
service has a listener of its own).

Status legend matches [`PROGRESS.md`](PROGRESS.md): ✅ done · 🟡 partial · ⬜ not started ·
⛔ blocked · 🔬 needs a real-client evidence run.

## The problem

The client renders shop names, prices, currency and listing from its **own** IFF tables inside
its PAK series. Nothing the server says changes that. So a re-authored shop only reaches a player
when the authored archive reaches their disk.

## Two properties that were measured, not assumed

**The last archive in the series wins.** `data/pangya_gb.iff` is present in nearly every one of
the 84 retail archives. Authoring `projectg850gb.pak` changed nothing a player saw, because the
stock `projectg851gb.pak` supplied a later copy. `scripts/extract-client-iff.py --list` reports
the winner. This cost a full round trip to a real client to discover, and it is the single
easiest mistake to repeat.

**The client is not a downloader.** `ProjectG.exe` validates its archives against the server's
`updatelist` and refuses to start on a mismatch, but the retail downloader is `update.exe`, a
separate WinLicense-packed binary the launcher deliberately never runs. Raising
`client_web.patch_number` above the client's own level does not trigger an update — the client
loads, re-requests the update list, and then never offers a login dialog at all. That state is
terminal. **`patch_number` must stay at 851**, and it is a compatibility constant, not a version
carrier.

## The delivery model

```
catalog.json  ─┬─► authored pangya_gb.iff ─┬─► projectg852gb.pak   (client half)
               │                            └─► iff-gb/ + manifest (server half)
               │
               └─► shop-sync-report.json ──► startup cross-check (publish_report)

server: GET /launcher/v1/manifest   →  size + PangYa CRC + SHA-256 per archive
launcher: compare → download → verify all three → back up → atomic rename → launch
```

### PATCH-001 — a new archive, never a rebuilt retail one ✅

`PANGYA_PATCH_MODE=latest` (the default) emits a **one-entry archive numbered past the retail
series** — `projectg852gb.pak` — rather than rebuilding a retail archive in place. Three reasons:

- every retail archive stays byte-for-byte pristine, so a client never re-downloads a mutated
  1.4 MB base to get a shop change;
- our patch is ~730 KB and carries only `data/pangya_gb.iff`;
- rolling updates replace one file we own, so nothing retail is ever overridden and a rollback is
  deleting it.

`deploy.sh` removes any previously-added archive in the `projectg852..859` range before placing
the current one, so switching what is authored — or back to pristine — cannot leave two of ours
in the series at once.

`PANGYA_PATCH_MODE=replace` keeps the older behaviour. It is the only mode proven in front of a
real client, which is why it still exists.

🔬 **Unverified:** that the client mounts an archive numbered past its own patch level. The
ordering property was proven for 850 vs 851; extending it to 852 is reasoned, not measured.

### PATCH-002 — the launcher manifest ✅

`GET /launcher/v1/manifest` on the client-web listener, unauthenticated like the update list it
derives from. Per archive: `size`, `pangya_crc`, `sha256`.

Built from the **same `UpdateList` the client is validated against**, so launcher and client
cannot disagree by construction rather than by discipline. The CRC decides whether the client
starts; the SHA-256 decides whether the transfer arrived intact — a 32-bit checksum is not an
integrity boundary for a network transfer, and the launcher requires all three.

`stale` is re-stat'd per request: a file replaced under a running server leaves the held `File`
pointing at the unlinked old inode, so the server would go on serving old bytes with an old
update list and nothing would report it. The launcher refuses to patch a stale manifest.

### PATCH-003 — the launcher applies it ✅

`pangya-client` `src-tauri/src/patcher.rs`, as a `PlayStage` between the preflight gate and the
install step. Downloads to `.pangya-updates/staging/` **inside** the client folder so the final
rename is same-volume and atomic, backs the displaced archive up to `.pangya-updates/backup/`,
and records what it applied.

Refusals: a stale server view, a running client (exclusive-open probe), any name that is not a
bare `.pak`, the Rugburn shim and its siblings, a body larger than the manifest declared, and a
manifest version it does not understand.

### PATCH-004 — the startup cross-check ✅

`client_web.publish_report` points at `shop-sync-report.json`. At startup the server hashes the
archive the report names and the `manifest.toml` it loaded, and **refuses to start** if either
disagrees — naming which side is stale. This turns the project's most expensive silent failure
(an item the client shows and the server refuses) into a startup error.

### PATCH-005 — range requests and conditional GET ⬜

`patch_file` has no `Accept-Ranges`, no `Range` handling, no `ETag`, no conditional GET. The
routine unit is ~730 KB so this is quality-of-life — but `projectg700gb+.pak` is 1.1 GB, and a
first-time install over a flaky link has no resume.

### PATCH-006 — preflight reports one failure at a time ⬜

The client names only the first bad archive, so a mismatched series surfaces as one corruption
dialog per launch. The launcher can already fetch the manifest; reporting **every** mismatch at
once is a small change to `preflight.rs` and retires a documented trap.

### PATCH-007 — no progress or revert in the UI ⬜

`apply_patches` emits `patch://progress` and `revert_patches` exists as a command, but neither
is surfaced. A large first download looks like a hang, and reverting needs a console.

### PATCH-008 — no operator visibility ⬜

The console cannot show what is published: archives, digests, staleness, or whether the publish
report agrees. Proposed as read-only `GET /admin/v1/client-pak`.

### PATCH-009 — authoring is not driven from the console ⬜

`catalog.json` is edited by hand and `sync-client-shop.sh` run from a shell, then the server
restarted. Deliberately **not** an admin endpoint that shells out: that would put code execution
on the only HTTP surface that mutates player state, and it would still need a restart. The
console editing `catalog.json` with a separate worker doing the authoring is the shape that keeps
one point of control without that.

### PATCH-010 — the flatten question 🔬

`docs/PATCHING.md` in `pangya-client` argues the 41,192-file flatten is a symptom of
`IntegratedPak = "0"` rather than a fix. If a flattened install lets loose files win over mounted
archives, this whole mechanism is inert for such installs.

Partly answered: the 2026-08-09 run replaced an archive and the client rendered the authored
shop, so PAK replacement demonstrably reaches the client in the configuration in use. What
remains untested is whether that holds with `IntegratedPak` set properly and the install
un-flattened.

## Verification

1. `cargo test -p pangya-updater -p pangya-server` — manifest construction, the golden update-list
   XML (byte-identical after adding SHA-256), the publish cross-check refusals.
2. `cargo test --manifest-path src-tauri/Cargo.toml` in `pangya-client` — manifest validation,
   name rejection, stale refusal.
3. Real client: press Play with an authored archive published, confirm the update stage runs
   once, the client starts, and the shop shows the authored prices. **Done 2026-08-09 for the
   four original tables; not repeated since the eight families were added.**
