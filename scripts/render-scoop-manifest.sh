#!/usr/bin/env bash
set -euo pipefail

if (($# != 3)); then
  printf 'Usage: %s <vVERSION> <SHA256SUMS> <output.json>\n' "$0" >&2
  exit 2
fi

tag=$1
checksums=$2
output=$3
version=${tag#v}

if [[ "$tag" != v* || ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]]; then
  printf 'Invalid release tag: %s\n' "$tag" >&2
  exit 2
fi

asset="dirgo-$tag-x86_64-pc-windows-msvc.zip"
checksum=$(awk -v asset="$asset" '$2 == asset || $2 == "*" asset { print $1; exit }' "$checksums")
if [[ ! "$checksum" =~ ^[0-9a-fA-F]{64}$ ]]; then
  printf 'Missing or invalid checksum for %s\n' "$asset" >&2
  exit 1
fi
checksum=$(printf '%s' "$checksum" | tr '[:upper:]' '[:lower:]')

template="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/packaging/scoop/dirgo.json.template"
sed \
  -e "s/@VERSION@/$version/g" \
  -e "s/@WINDOWS_SHA256@/$checksum/g" \
  "$template" > "$output"

if grep -q '@[A-Z_]*@' "$output"; then
  printf 'Rendered Scoop manifest contains unresolved placeholders\n' >&2
  exit 1
fi

if command -v jq >/dev/null 2>&1; then
  jq -e . "$output" >/dev/null
fi
