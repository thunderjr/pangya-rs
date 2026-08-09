#!/usr/bin/env bash
set -euo pipefail

# Authors one retail IFF archive, writes the server manifest/tables from that same archive, and
# stages a PAK for the client. Proprietary inputs and outputs remain under local-data/ and are
# never copied into Git.
#
# Two modes, selected by PANGYA_PATCH_MODE:
#
#   latest   (default) Emit a NEW one-entry archive that sits after the retail series and wins by
#            ordering. Every retail archive stays byte-for-byte pristine, so a client never
#            re-downloads a modified base, and a rolling shop update replaces only this one small
#            file. ~730 KB rather than a 1.4 MB rebuilt copy of a retail archive.
#
#   replace  Rebuild an existing retail archive in place. What the first custom shop used. Kept
#            because it is the only mode proven in front of a real client, and because a client
#            that refuses to mount an unknown archive name would need it.
#
# Ordering is what makes `latest` work: the client loads the last archive in the series that
# provides a file, which is why authoring projectg850gb.pak did nothing while projectg851gb.pak
# still supplied `data/pangya_gb.iff`. That was measured, not assumed.

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

BASE_IFF=${PANGYA_BASE_IFF:-local-data/us851-data/pak-iff/pangya_gb.iff}
BASE_PAK=${PANGYA_BASE_PAK:-local-data/custom-shop/projectg851gb-original.pak}
PATCH_MODE=${PANGYA_PATCH_MODE:-latest}
CATALOG=${PANGYA_SHOP_CATALOG:-local-data/custom-shop/catalog.json}
OUTPUT_DIR=${PANGYA_SHOP_OUTPUT_DIR:-local-data/custom-shop}
SERVER_IFF_DIR=${PANGYA_SERVER_IFF_DIR:-$OUTPUT_DIR/iff-gb}
PATCH_DIR=${PANGYA_PATCH_DIR:-local-data/us851}
# In `latest` mode this names the NEW archive, which must sort after every retail one. The retail
# U.S. series ends at projectg851gb.pak.
PAK_NAME=${PANGYA_CLIENT_PAK_NAME:-projectg852gb.pak}

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
