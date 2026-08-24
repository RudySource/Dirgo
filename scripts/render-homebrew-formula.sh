#!/usr/bin/env bash
set -euo pipefail

if (($# < 2 || $# > 3)); then
  printf '%s\n' 'Usage: scripts/render-homebrew-formula.sh VERSION SHA256SUMS [OUTPUT]' >&2
  exit 2
fi

version=${1#v}
checksums=$2
output=${3:-}
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]]; then
  printf 'Invalid release version: %s\n' "$version" >&2
  exit 2
fi
if [[ ! -f $checksums ]]; then
  printf 'Checksum file does not exist: %s\n' "$checksums" >&2
  exit 2
fi

checksum_for() {
  local target=$1 archive checksum count
  archive="dirgo-v${version}-${target}.tar.gz"
  checksum=$(awk -v archive="$archive" '$2 == archive { print $1 }' "$checksums")
  count=$(awk -v archive="$archive" '$2 == archive { count++ } END { print count + 0 }' "$checksums")
  if [[ $count != 1 || ! $checksum =~ ^[0-9a-f]{64}$ ]]; then
    printf 'Expected one valid SHA-256 entry for %s\n' "$archive" >&2
    return 1
  fi
  printf '%s' "$checksum"
}

macos_arm=$(checksum_for aarch64-apple-darwin)
macos_intel=$(checksum_for x86_64-apple-darwin)
linux_intel=$(checksum_for x86_64-unknown-linux-gnu)
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
template="$repo_root/packaging/homebrew/dirgo.rb.template"
rendered=$(sed \
  -e "s/@VERSION@/$version/g" \
  -e "s/@SHA_MACOS_ARM@/$macos_arm/g" \
  -e "s/@SHA_MACOS_INTEL@/$macos_intel/g" \
  -e "s/@SHA_LINUX_INTEL@/$linux_intel/g" \
  "$template")

if [[ -n $output ]]; then
  printf '%s\n' "$rendered" > "$output"
else
  printf '%s\n' "$rendered"
fi
