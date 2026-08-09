#!/usr/bin/env bash
set -euo pipefail

# Authors one retail IFF archive, writes the server manifest/tables from that same archive, and
# stages a PAK for the client. Proprietary inputs and outputs remain under local-data/ and are
# never copied into Git.
#
# Two modes, selected by PANGYA_PATCH_MODE:
#
#   replace  (default) Rebuild an existing retail archive in place. The only mode that has ever
#            reached a real client's shop.
#
#   latest   Emit a NEW one-entry archive numbered past the retail series, hoping to win by
#            ordering. Cheaper in principle — every retail archive stays pristine and a shop
#            update ships ~730 KB instead of a rebuilt 1.4 MB base — but see below: it does not
#            work on the U.S. 852 client.
#
# Two ordering facts, both measured in front of a real client, that read as one rule but are not:
#
#   1. Among the archives the client mounts, the LAST one to provide a file wins. Authoring
#      projectg850gb.pak did nothing because the stock projectg851gb.pak still supplied
#      `data/pangya_gb.iff`. (2026-08-09)
#
#   2. The client does NOT mount an archive numbered past its own patch level. Serving a pristine
#      projectg851gb.pak alongside an authored projectg852gb.pak put the shop back to retail
#      prices: 852 was published, hashed and served, and the client ignored it. (2026-08-10)
#
# So the mount set is bounded by the patch level, and `latest` can only ever win inside that
# bound — where, by definition, a retail archive already exists to be replaced. That leaves
# `replace` as the mode that works, and it is why PANGYA_CLIENT_PAK_NAME defaults per mode:
# rebuilding must target the LAST archive the client mounts, which is projectg851gb.pak.
#
# Raising client_web.patch_number to lift the bound is not an option — the client then
# re-requests the update list forever and never offers a login dialog. See
# docs/SPEC_CLIENT_PATCH_DELIVERY.md.

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

BASE_IFF=${PANGYA_BASE_IFF:-local-data/us851-data/pak-iff/pangya_gb.iff}
BASE_PAK=${PANGYA_BASE_PAK:-local-data/custom-shop/projectg851gb-original.pak}
PATCH_MODE=${PANGYA_PATCH_MODE:-replace}
CATALOG=${PANGYA_SHOP_CATALOG:-local-data/custom-shop/catalog.json}
OUTPUT_DIR=${PANGYA_SHOP_OUTPUT_DIR:-local-data/custom-shop}
SERVER_IFF_DIR=${PANGYA_SERVER_IFF_DIR:-$OUTPUT_DIR/iff-gb}
PATCH_DIR=${PANGYA_PATCH_DIR:-local-data/us851}
# The default follows the mode, because the two modes want opposite ends of the series and a
# single default would be silently wrong for one of them. `replace` must rebuild the LAST archive
# the client mounts (projectg851gb.pak); `latest` must land past the series end.
if [[ "${PANGYA_PATCH_MODE:-replace}" == "latest" ]]; then
  PAK_NAME=${PANGYA_CLIENT_PAK_NAME:-projectg852gb.pak}
else
  PAK_NAME=${PANGYA_CLIENT_PAK_NAME:-projectg851gb.pak}
fi

required_inputs=("$BASE_IFF" "$CATALOG")
if [[ "$PATCH_MODE" == "replace" ]]; then
  required_inputs+=("$BASE_PAK")
elif [[ "$PATCH_MODE" != "latest" ]]; then
  printf 'unknown PANGYA_PATCH_MODE %s (latest|replace)\n' "$PATCH_MODE" >&2
  exit 2
fi

for input in "${required_inputs[@]}"; do
  if [[ ! -f "$input" ]]; then
    printf 'missing required input: %s\n' "$input" >&2
    exit 2
  fi
done

mkdir -p "$OUTPUT_DIR" "$PATCH_DIR"
rm -rf "$SERVER_IFF_DIR"

AUTHORED_IFF="$OUTPUT_DIR/pangya_gb.iff"
AUTHORED_PAK="$OUTPUT_DIR/${PAK_NAME%.pak}-authored.pak"
REPORT="$OUTPUT_DIR/shop-sync-report.json"

author_args=(
  --base-archive "$BASE_IFF"
  --catalog "$CATALOG"
  --out-archive "$AUTHORED_IFF"
  --out-server-iff-dir "$SERVER_IFF_DIR"
  --pak-entry data/pangya_gb.iff
  --report "$REPORT"
)
if [[ "$PATCH_MODE" == "latest" ]]; then
  # One entry, nothing borrowed from a retail archive.
  author_args+=(--out-pak "$AUTHORED_PAK")
else
  author_args+=(--replace-in-pak "$BASE_PAK" --out-client-pak "$AUTHORED_PAK")
fi

scripts/author-client-iff.py "${author_args[@]}" >/dev/null

# The patch web service serves PATCH_DIR. Install only after every authoring/server-manifest step
# succeeds, so a failed run leaves the previous client payload intact.
install_tmp="$PATCH_DIR/.${PAK_NAME}.partial"
cp "$AUTHORED_PAK" "$install_tmp"
mv -f "$install_tmp" "$PATCH_DIR/$PAK_NAME"

python3 - "$REPORT" "$PATCH_DIR/$PAK_NAME" <<'PY'
import hashlib
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
payload = pathlib.Path(sys.argv[2])
digest = hashlib.sha256(payload.read_bytes()).hexdigest()
# `latest` records the one-entry archive under authored_pak_sha256; `replace` records the
# rebuilt retail archive under client_pak_sha256. Exactly one is set per run.
expected = report["client_pak_sha256"] or report["authored_pak_sha256"]
if digest != expected:
    raise SystemExit(f"staged PAK hash mismatch: {digest} != {expected}")

# Write the deployed archive name back into the report. author-client-iff.py only knows its own
# output filename, which carries an "-authored" suffix and exists nowhere in a client's folder;
# the deployed name is decided here, and a server cross-checking the deployment needs that one.
# Normalise both fields onto the pair a consumer needs: what was staged, and under what name.
# `latest` records its one-entry archive under authored_pak_sha256 and `replace` records its
# rebuilt retail archive under client_pak_sha256; anything cross-checking a deployment should not
# have to know which mode produced the file it is looking at.
report["client_pak_name"] = payload.name
report["client_pak_sha256"] = digest
pathlib.Path(sys.argv[1]).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

print(json.dumps({
    "client_pak": str(payload),
    # The name it is DEPLOYED as, which is what a server has to hash — not the authoring
    # tool's output filename, which carries an "-authored" suffix and exists nowhere in a
    # client's folder.
    "client_pak_name": payload.name,
    "client_pak_sha256": digest,
    "server_iff": report["server_iff"],
    "offers": report["offers"],
}, indent=2, sort_keys=True))
PY
