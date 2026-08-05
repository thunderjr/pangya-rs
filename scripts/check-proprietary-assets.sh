#!/usr/bin/env bash
set -euo pipefail
root=${1:-.}

scan_files() {
  find "$root" \
    -path "$root/.git" -prune -o \
    -path "$root/.pi-subagents" -prune -o \
    -path "$root/target" -prune -o \
    -path "$root/fuzz/target" -prune -o \
    -path "$root/opensource-references" -prune -o \
    -type f "$@" -print
}

if scan_files \( -iname '*.exe' -o -iname '*.dll' -o -iname '*.iff' -o -iname '*.pak' -o -iname '*.pcap' -o -iname '*.pcapng' \) | grep -q .; then
  echo 'proprietary or capture-like asset extension found' >&2
  exit 1
fi

while IFS= read -r binary; do
  case "$binary" in
    "$root"/crates/*/tests/fixtures/*/*.bin) ;;
    *)
      echo "binary fixture outside approved fixture directory: $binary" >&2
      exit 1
      ;;
  esac
  metadata="$(dirname "$binary")/fixture.yaml"
  if [[ ! -f "$metadata" ]]; then
    echo "fixture binary lacks adjacent fixture.yaml: $binary" >&2
    exit 1
  fi
  for field in source_project upstream_revision upstream_url upstream_path license client_version service direction redaction_status expected_behavior sha256_fixture; do
    if ! grep -q "^${field}: ." "$metadata"; then
      echo "fixture metadata lacks mandatory $field: $metadata" >&2
      exit 1
    fi
  done
done < <(scan_files -iname '*.bin')

while IFS= read -r blob; do
  case "$blob" in
    "$root"/crates/*/tests/fixtures/*/*.bin)
      echo "fixture binary exceeds 1 MiB review cap: $blob" >&2
      ;;
    *)
      echo "unapproved binary/blob exceeds 1 MiB: $blob" >&2
      ;;
  esac
  exit 1
done < <(scan_files -size +1M)

if git -C "$root" ls-files 'opensource-references/**' | grep -vE '^opensource-references/(README.md|\.gitignore)$' | grep -q .; then
  echo 'nested reference content is tracked' >&2
  exit 1
fi
