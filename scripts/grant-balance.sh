#!/usr/bin/env bash
# Credits an account's pang and point ("cookie") balances so it can buy from the in-game shop.
#
# This wraps `pangya-server account grant`, which takes a row lock, refuses rather than wraps on
# overflow, and emits an operator audit line. Updating `profiles` with psql skips all three.
#
# Usage:
#   scripts/grant-balance.sh --account-id 145 --pang 1000000 --points 5000
#   scripts/grant-balance.sh --username rsp3 --pang 1000000
#
# DATABASE_URL must be set, or the config's database section must resolve one. Point the script at
# a non-default config with PANGYA_CONFIG.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
config="${PANGYA_CONFIG:-}"
binary="${PANGYA_SERVER_BIN:-$repo_root/target/release/pangya-server}"

account_id=""
username=""
pang=0
points=0

usage() {
    sed -n '2,13p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit "${1:-1}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --account-id) account_id="${2:?--account-id needs a value}"; shift 2 ;;
        --username) username="${2:?--username needs a value}"; shift 2 ;;
        --pang) pang="${2:?--pang needs a value}"; shift 2 ;;
        --points|--cookies) points="${2:?--points needs a value}"; shift 2 ;;
        -h|--help) usage 0 ;;
        *) echo "unknown argument: $1" >&2; usage ;;
    esac
done

if [[ -z "$account_id" && -z "$username" ]]; then
    echo "one of --account-id or --username is required" >&2
    usage
fi
if [[ "$pang" == 0 && "$points" == 0 ]]; then
    echo "nothing to grant: pass --pang and/or --points" >&2
    usage
fi
if [[ ! -x "$binary" ]]; then
    echo "server binary not found at $binary" >&2
    echo "build it with: cargo build --release -p pangya-server" >&2
    exit 1
fi

# Usernames are normalized on the way in, so resolve one the same way the server would rather
# than assuming the display form matches what is stored.
if [[ -z "$account_id" ]]; then
    if [[ -z "${DATABASE_URL:-}" ]]; then
        echo "resolving --username needs DATABASE_URL; pass --account-id instead" >&2
        exit 1
    fi
    account_id="$(psql "$DATABASE_URL" -tAc \
        "SELECT id FROM accounts WHERE username_normalized = lower('${username//\'/\'\'}')")"
    if [[ -z "$account_id" ]]; then
        echo "no account matches username: $username" >&2
        exit 1
    fi
fi

args=(account grant --account-id "$account_id" --pang "$pang" --points "$points")
if [[ -n "$config" ]]; then
    args=(--config "$config" "${args[@]}")
fi

cd "$repo_root"
exec "$binary" "${args[@]}"
