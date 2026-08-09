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

## Three properties that were measured, not assumed

**The last archive in the series wins.** `data/pangya_gb.iff` is present in nearly every one of
the 84 retail archives. Authoring `projectg850gb.pak` changed nothing a player saw, because the
stock `projectg851gb.pak` supplied a later copy. `scripts/extract-client-iff.py --list` reports
the winner. This cost a full round trip to a real client to discover, and it is the single
easiest mistake to repeat.

**The client does not mount an archive numbered past its own patch level.** The obvious
consequence of the rule above — add `projectg852gb.pak` and it wins, leaving every retail archive
pristine — is false. A one-entry 852 was authored, published, hashed and served correctly, with
851 restored to its pristine 690,312 bytes; the shop went straight back to retail prices
(2026-08-10). The mount set is bounded by the patch level, so an archive that sorts last is not
necessarily an archive that is read.

Those two together are narrower than they look: *the winner is the last archive the client
mounts*, and the mount set ends at the client's own patch level. Anything that wins by ordering
must therefore be a retail archive rebuilt in place, because inside that bound every slot is
already occupied.

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

### PATCH-001 — rebuild the last archive the client mounts ✅

`PANGYA_PATCH_MODE=replace` (the default) rebuilds `projectg851gb.pak` in place, replacing its
`data/pangya_gb.iff` entry. The pristine copy is kept at `projectg851gb-original.pak`, which is
both the authoring base and the rollback.

**This is not the design that was wanted.** `PANGYA_PATCH_MODE=latest` emits a one-entry
`projectg852gb.pak` instead, leaving every retail archive byte-for-byte pristine so a client
never re-downloads a mutated 1.4 MB base and a shop change ships ~730 KB. It was built, deployed
and measured, and the client ignored it — see the second measured property above. The mode is
kept because the code is written and a future client build may bound its mount set differently,
but it must not be selected without a fresh evidence run. ⛔

`deploy.sh` still sweeps `projectg852..859` before placing the authored archive, so a switch
between modes — or back to pristine — cannot leave a stale one of ours in the series.

The cost of `replace` is honest and small: a shop change re-ships 1.4 MB rather than 730 KB, and
exactly one retail archive is no longer the file the client shipped with.

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

### PATCH-009 — authoring is driven from the console ✅

The console renders the `catalog.json` and a worker outside the server authors it. Nothing shells
out from the HTTP surface that mutates player state.

```
shop_offer_overrides ──► GET  /admin/v1/shop/publish          what a publish would author
                    └──► POST /admin/v1/shop/publish          queue it (one at a time)
                                    │
publish-shop.sh ────────────────────┤ POST /shop/publish/claim      → document + digest
  author-client-iff.py              │ author, stage, deploy
  homelab deploy.sh                 │
                                    └ POST /shop/publish/{id}/finish → archive name + SHA-256
```

The queued row carries the **exact document bytes** the operator approved, and the worker refuses
to author unless its own SHA-256 of what it wrote matches. That is why `shop_publish_requests.document`
is `TEXT` and not `JSONB`: `jsonb` reorders keys and drops whitespace, so a normalising column
would fail the digest check on every publish. It caught a real defect on its first run — `jq -r`
appends a newline the console never hashed.

One request may be outstanding at a time (`uq_shop_publish_active`); a second is refused as
`publish_in_flight` rather than racing two workers over one client tree. A failure keeps the
worker's reason, so the console can show why rather than sending an operator to a log on a machine
they may not have open.

**Verified 2026-08-10.** An override of `0x10000000` to 777 Pang showed client 1 / server 777 /
drift; after a console publish the client's own table read 777 with no drift. Restoring it to 1
and republishing produced `cff2b454425c67f8…`, byte-identical to the hand-authored archive — so
the console-driven and hand-run paths agree by construction, not by discipline.

One property to know: the document is rendered from the **server's current catalog** with the
overlay applied, and that catalog is the last published set. Prices therefore ratchet — clearing
an override does not restore the retail price, it keeps whatever was last published, because that
is now what the client's own tables say. Restoring retail means overriding to the retail value
explicitly, or deploying `PANGYA_SHOP=pristine`.

Deployment default: `deploy.sh` selects the console's set as soon as one exists, so a routine
deploy cannot silently revert a console publish back to a hand-authored one.

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
3. `cargo test -p pangya-storage --test admin` — the publish queue: one request at a time, a
   claim that cannot be claimed twice, a resolved request that cannot be rewritten, and the
   exact-bytes round trip the worker's digest check depends on.
4. `scripts/publish-shop.sh --dry-run` — what a publish would author, without claiming anything.
5. Real client: press Play with an authored archive published, confirm the update stage runs
   once, the client starts, and the shop shows the authored prices. **Done 2026-08-09 for the
   four original tables; not repeated since the eight families were added, nor since the
   2026-08-10 revert from `latest` back to `replace`.**
