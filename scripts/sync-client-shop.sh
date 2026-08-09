#!/usr/bin/env bash
set -euo pipefail

# Authors one retail IFF archive, injects it into the actual mounted PAK, writes the server
# manifest/tables from that same archive, and atomically stages the PAK for manual client sync.
# Proprietary inputs and outputs remain under local-data/ and are never copied into Git.

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

BASE_IFF=${PANGYA_BASE_IFF:-local-data/us851-data/pak-iff/pangya_gb.iff}
BASE_PAK=${PANGYA_BASE_PAK:-local-data/custom-shop/projectg850gb-original.pak}
CATALOG=${PANGYA_SHOP_CATALOG:-local-data/custom-shop/catalog.json}
OUTPUT_DIR=${PANGYA_SHOP_OUTPUT_DIR:-local-data/custom-shop}
SERVER_IFF_DIR=${PANGYA_SERVER_IFF_DIR:-$OUTPUT_DIR/iff-gb}
PATCH_DIR=${PANGYA_PATCH_DIR:-local-data/us851}
PAK_NAME=${PANGYA_CLIENT_PAK_NAME:-projectg850gb.pak}

for input in "$BASE_IFF" "$BASE_PAK" "$CATALOG"; do
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

scripts/author-client-iff.py \
  --base-archive "$BASE_IFF" \
  --catalog "$CATALOG" \
  --out-archive "$AUTHORED_IFF" \
  --out-server-iff-dir "$SERVER_IFF_DIR" \
  --replace-in-pak "$BASE_PAK" \
  --out-client-pak "$AUTHORED_PAK" \
  --pak-entry data/pangya_gb.iff \
  --report "$REPORT" >/dev/null

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
expected = report["client_pak_sha256"]
if digest != expected:
    raise SystemExit(f"staged PAK hash mismatch: {digest} != {expected}")

# Write the deployed archive name back into the report. author-client-iff.py only knows its own
# output filename, which carries an "-authored" suffix and exists nowhere in a client's folder;
# the deployed name is decided here, and a server cross-checking the deployment needs that one.
report["client_pak_name"] = payload.name
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
