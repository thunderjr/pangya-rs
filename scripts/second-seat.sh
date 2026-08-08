#!/usr/bin/env bash
# Puts a headless client in the second seat of a versus room, beside a real one.
#
# A versus hole needs two players and two real clients cannot be driven on one desktop, so a
# person drives one and this fills the other over the same retail wire. It mints its own bearer,
# because a handover is single use and a fresh one is needed for every attempt.
#
# Usage:
#   scripts/second-seat.sh --username rsp5 --room 3          # join the room a real client made
#   scripts/second-seat.sh --username rsp5 --host            # host, and print the room number
#
# DATABASE_URL must be set. Point the script at a non-default config with PANGYA_CONFIG, and at
# a non-default GameService with PANGYA_GAME (default 127.0.0.1:20201). RUST_LOG=debug prints
# every frame in order, which is the fastest way to see what the real client did not answer.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
config="${PANGYA_CONFIG:-}"
server="${PANGYA_SERVER_BIN:-$repo_root/target/release/pangya-server}"
client="${PANGYA_CLIENT_BIN:-$repo_root/target/release/pangya-test-client}"
game="${PANGYA_GAME:-127.0.0.1:20201}"

username=""
room=""
host=0
strokes=2

usage() {
    sed -n '2,14p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit "${1:-1}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --username) username="${2:?--username needs a value}"; shift 2 ;;
        --room) room="${2:?--room needs a value}"; shift 2 ;;
        --host) host=1; shift ;;
        --strokes) strokes="${2:?--strokes needs a value}"; shift 2 ;;
        -h|--help) usage 0 ;;
        *) echo "unknown argument: $1" >&2; usage ;;
    esac
done

if [[ -z "$username" ]]; then
    echo "--username is required" >&2
    usage
fi
if [[ -z "$room" && "$host" == 0 ]]; then
    echo "give either --room to join one or --host to open one" >&2
    usage
fi
for binary in "$server" "$client"; do
    if [[ ! -x "$binary" ]]; then
        echo "binary not found at $binary" >&2
        echo "build both with: cargo build --release -p pangya-server -p pangya-test-client" >&2
        exit 1
    fi
done
if [[ -z "${DATABASE_URL:-}" ]]; then
    echo "DATABASE_URL is required to resolve the account and mint a bearer" >&2
    exit 1
fi

# Usernames are normalized on the way in, so resolve one the way the server would.
account_id="$(psql "$DATABASE_URL" -tAc \
    "SELECT id FROM accounts WHERE username_normalized = lower('${username//\'/\'\'}')")"
if [[ -z "$account_id" ]]; then
    echo "no account matches username: $username" >&2
    exit 1
fi

server_args=(account handover --account-id "$account_id")
if [[ -n "$config" ]]; then
    server_args=(--config "$config" "${server_args[@]}")
fi

cd "$repo_root"
# The bearer reaches the client through the environment only: it is a credential, and a command
# line is visible to every process on the machine.
PANGYA_HANDOVER="$("$server" "${server_args[@]}" 2>/dev/null | tail -1)"
export PANGYA_HANDOVER
if [[ -z "$PANGYA_HANDOVER" ]]; then
    echo "no bearer was issued for account $account_id" >&2
    exit 1
fi

client_args=(--game "$game" --account-id "$account_id" --username "$username" --strokes "$strokes")
if [[ "$host" == 1 ]]; then
    client_args+=(--host)
else
    client_args+=(--room "$room")
fi

exec "$client" "${client_args[@]}"
