#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
binary=${DGO_BIN:-$repo_root/target/release/dgo}
scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/dirgo-installer-smoke.XXXXXX")
cleanup() {
  case "$scratch_dir" in
    "${TMPDIR:-/tmp}"/dirgo-installer-smoke.*) rm -rf -- "$scratch_dir" ;;
    *) printf 'Refusing to clean unexpected path: %s\n' "$scratch_dir" >&2 ;;
  esac
}
trap cleanup EXIT

case "$(uname -s):$(uname -m)" in
  Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
  Darwin:x86_64) target=x86_64-apple-darwin ;;
  Linux:x86_64|Linux:amd64) target=x86_64-unknown-linux-gnu ;;
  *) printf 'INSTALLER-SMOKE:unsupported host\n' >&2; exit 1 ;;
esac

test -x "$binary"
mkdir -p "$scratch_dir/assets/dirgo-smoke" "$scratch_dir/installed"
cp "$binary" "$scratch_dir/assets/dirgo-smoke/dgo"
tar -C "$scratch_dir/assets" -czf "$scratch_dir/assets/dirgo-$target.tar.gz" dirgo-smoke
(
  cd "$scratch_dir/assets"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "dirgo-$target.tar.gz" > SHA256SUMS
  else
    shasum -a 256 "dirgo-$target.tar.gz" > SHA256SUMS
  fi
)

DIRGO_DOWNLOAD_BASE="file://$scratch_dir/assets" \
DIRGO_INSTALL_DIR="$scratch_dir/installed" \
  sh "$repo_root/install/dirgo-installer.sh" --no-setup
"$scratch_dir/installed/dgo" --version
printf 'INSTALLER-SMOKE:unix:ok\n'
