#!/usr/bin/env bash
set -euo pipefail

# The worker half of console-driven shop publishing.
#
# The admin console decides WHAT the shop is — `shop_offer_overrides`, edited live — and enqueues
# a rendered `catalog.json`. It deliberately cannot author anything itself: authoring reads
# proprietary client archives, writes a multi-megabyte PAK and stages it into a served tree, and
# none of that belongs behind an admin cookie on the surface that also mutates player state.
#
# This script is what does it. It claims the queued request over the admin API, authors the
# client tables from the exact document the operator approved, ships the result, and reports the
# outcome back so the console can say the client is current.
#
#   scripts/publish-shop.sh                 claim one request and publish it, then exit
#   scripts/publish-shop.sh --watch         poll until interrupted
#   scripts/publish-shop.sh --dry-run       claim nothing; report what a publish would author
#
# Required:
#   PANGYA_ADMIN_URL        e.g. http://homelab.orca-fujita.ts.net:27431
#   PANGYA_ADMIN_USER       an account whose role is `admin`
#   PANGYA_ADMIN_PASSWORD   its password
#
# Optional:
#   PANGYA_PUBLISH_DEPLOY   command run after authoring to ship it (default: the homelab deploy)
#   PANGYA_POLL_SECONDS     --watch interval, default 15
#
# The document is authored, not trusted: `author-client-iff.py` re-validates every offer against
# the client's own tables and refuses anything it cannot author faithfully. A console that asked
# for something impossible produces a `failed` row with the reason, not a broken shop.

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

ADMIN_URL=${PANGYA_ADMIN_URL:-}
ADMIN_USER=${PANGYA_ADMIN_USER:-}
ADMIN_PASSWORD=${PANGYA_ADMIN_PASSWORD:-}
POLL_SECONDS=${PANGYA_POLL_SECONDS:-15}
# Defaults to the homelab stack's deploy, which is the only consumer today. Anything that stages
# the authored output and restarts the server works.
DEPLOY_COMMAND=${PANGYA_PUBLISH_DEPLOY:-$HOME/homelab/pangya/deploy.sh}
# Console publishes get their own output directory. Sharing one with a hand-authored set would
# let a console publish silently overwrite an operator's staged work, and vice versa.
OUTPUT_DIR=${PANGYA_SHOP_OUTPUT_DIR:-local-data/custom-shop/console}

WATCH=0
DRY_RUN=0
for argument in "$@"; do
  case "$argument" in
    --watch)   WATCH=1 ;;
    --dry-run) DRY_RUN=1 ;;
    -h|--help) sed -n '3,30p' "$0"; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$argument" >&2; exit 2 ;;
  esac
done

die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }
step() { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }

[ -n "$ADMIN_URL" ]      || die "PANGYA_ADMIN_URL is required"
[ -n "$ADMIN_USER" ]     || die "PANGYA_ADMIN_USER is required"
[ -n "$ADMIN_PASSWORD" ] || die "PANGYA_ADMIN_PASSWORD is required"
command -v curl >/dev/null || die "curl is required"
command -v jq   >/dev/null || die "jq is required"

COOKIES="$(mktemp)"
WORKDIR="$(mktemp -d)"
# The cookie jar holds a live operator session and the workdir holds the claimed document; both
# are removed however this exits, including on the signal --watch is normally stopped with.
cleanup() { rm -rf "$COOKIES" "$WORKDIR"; }
trap cleanup EXIT INT TERM

api() {
  local method=$1 path=$2
  shift 2
  curl -sS --fail-with-body -b "$COOKIES" -c "$COOKIES" \
    -X "$method" -H 'Content-Type: application/json' \
    "$ADMIN_URL/admin/v1$path" "$@"
}

login() {
  # Credentials go in the body, never the URL: a query string reaches access logs and shell
  # history, and this password also signs a player in.
  jq -n --arg u "$ADMIN_USER" --arg p "$ADMIN_PASSWORD" \
    '{username: $u, password: $p}' > "$WORKDIR/login.json"
  api POST /auth/login --data @"$WORKDIR/login.json" >/dev/null \
    || die "admin login failed for $ADMIN_USER"
}

# Reports a failure back to the console before exiting, so a queued request never sits in
# `running` with no explanation. Falls back to a plain exit if the report itself fails.
fail_request() {
  local id=$1 detail=$2
  printf '\033[1;31mpublish %s failed:\033[0m %s\n' "$id" "$detail" >&2
  jq -n --arg d "$detail" '{status: "failed", detail: $d}' > "$WORKDIR/finish.json"
  api POST "/shop/publish/$id/finish" --data @"$WORKDIR/finish.json" >/dev/null || true
}

publish_once() {
  local claimed id document_sha
  claimed="$WORKDIR/claim.json"
  # 204 means an empty queue, which is the normal state, not a failure.
  if ! api POST /shop/publish/claim -o "$claimed" -w '%{http_code}' > "$WORKDIR/code" 2>/dev/null; then
    die "claim request failed"
  fi
  if [ "$(cat "$WORKDIR/code")" = "204" ]; then
    return 1
  fi

  id=$(jq -r '.id' "$claimed")
  document_sha=$(jq -r '.document_sha256' "$claimed")
  step "claimed publish $id (overlay revision $(jq -r '.overlay_revision' "$claimed"))"

  mkdir -p "$OUTPUT_DIR"
  # `.document` is raw JSON text, so it is written out verbatim rather than re-emitted by jq:
  # the digest below is over these exact bytes. `-j` and not `-r`, because `-r` appends a
  # newline the console never hashed and every publish would fail the check two lines down.
  jq -j '.document' "$claimed" > "$OUTPUT_DIR/catalog.json"

  local actual
  actual=$(shasum -a 256 "$OUTPUT_DIR/catalog.json" | cut -d' ' -f1)
  if [ "$actual" != "$document_sha" ]; then
    fail_request "$id" "document digest mismatch: authored $actual, console approved $document_sha"
    return 0
  fi

  step "authoring $(jq -r '.offers | length' "$OUTPUT_DIR/catalog.json") offers"
  if ! PANGYA_SHOP_CATALOG="$OUTPUT_DIR/catalog.json" \
       PANGYA_SHOP_OUTPUT_DIR="$OUTPUT_DIR" \
       scripts/sync-client-shop.sh > "$WORKDIR/author.log" 2>&1; then
    fail_request "$id" "authoring refused: $(tail -c 1500 "$WORKDIR/author.log")"
    return 0
  fi

  local report pak_name pak_sha
  report="$OUTPUT_DIR/shop-sync-report.json"
  pak_name=$(jq -r '.client_pak_name' "$report")
  pak_sha=$(jq -r '.client_pak_sha256' "$report")
  [ "$pak_name" != "null" ] && [ "$pak_sha" != "null" ] \
    || { fail_request "$id" "authoring wrote no client archive name or digest"; return 0; }

  step "shipping $pak_name"
  if ! PANGYA_SHOP=console $DEPLOY_COMMAND > "$WORKDIR/deploy.log" 2>&1; then
    fail_request "$id" "deploy refused: $(tail -c 1500 "$WORKDIR/deploy.log")"
    return 0
  fi

  jq -n --arg n "$pak_name" --arg s "$pak_sha" \
    '{status: "published", client_pak_name: $n, client_pak_sha256: $s}' > "$WORKDIR/finish.json"
  api POST "/shop/publish/$id/finish" --data @"$WORKDIR/finish.json" >/dev/null \
    || die "published $pak_name but could not report it; the console will show this stuck in running"
  printf '\n\033[1;32mpublished\033[0m %s (%s)\n' "$pak_name" "${pak_sha:0:16}"
  return 0
}

login

if [ "$DRY_RUN" = "1" ]; then
  step "what a publish would author"
  api GET /shop/publish | jq '{overlay_revision, pending_offer_count, document_sha256, client_behind, in_flight, tables}'
  exit 0
fi

if [ "$WATCH" = "1" ]; then
  step "watching $ADMIN_URL every ${POLL_SECONDS}s"
  while true; do
    # The session outlives most polls but not all of them; a re-login per round is cheap next to
    # an authoring run and removes expiry as a failure mode.
    login
    publish_once || true
    sleep "$POLL_SECONDS"
  done
fi

if ! publish_once; then
  printf 'nothing queued\n'
fi
