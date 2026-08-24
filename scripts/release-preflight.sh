#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/release-preflight.sh [--require-fish]

Runs reproducible, non-publishing checks for a Dirgo release candidate. It does
not create tags, upload artifacts, modify shell startup files, or publish a
crate. --require-fish turns an unavailable Fish shell into a failure.
EOF
}

require_fish=0
while (($#)); do
  case "$1" in
    --require-fish) require_fish=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) printf 'Unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/dirgo-release-preflight.XXXXXX")
cleanup() { rm -rf "$scratch_dir"; }
trap cleanup EXIT

printf '%s\n' '== Dirgo release preflight =='
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release --bin dgo
sh -n install/dirgo-installer.sh
DGO_BIN="$repo_root/target/release/dgo" scripts/installer-smoke.sh
cargo build --release --bin dgo-fixture --features benchmark-tools
cargo bench --bench index_pipeline --no-run
cargo package --allow-dirty --no-verify --offline
git diff --check

install_root="$scratch_dir/install-root"
cargo install --path . --locked --offline --root "$install_root"
if [[ ! -x "$install_root/bin/dgo" ]] || [[ -e "$install_root/bin/dgo-fixture" ]]; then
  printf '%s\n' 'Default cargo install must expose dgo and must not expose dgo-fixture' >&2
  exit 1
fi
if [[ $(find "$install_root/bin" -maxdepth 1 -type f | wc -l | tr -d ' ') != 1 ]]; then
  printf '%s\n' 'Default cargo install produced an unexpected executable set' >&2
  exit 1
fi
printf '%s\n' 'PACKAGE:default-install-surface:ok'

package_files="$scratch_dir/package-files.txt"
cargo package --allow-dirty --no-verify --offline --list > "$package_files"
for required_file in \
  Cargo.toml CHANGELOG.md LICENSE-APACHE LICENSE-MIT README.md SECURITY.md SUPPORT.md \
  CONTRIBUTING.md docs/dirgo-demo.tape docs/assets/dirgo-demo.gif \
  docs/assets/dirgo-wordmark.png \
  install/dirgo-installer.sh install/dirgo-installer.ps1 \
  scripts/demo-setup.sh src/lib.rs src/main.rs; do
  if ! grep -Fxq "$required_file" "$package_files"; then
    printf 'Release archive is missing required file: %s\n' "$required_file" >&2
    exit 1
  fi
done
if grep -Eq '(^target/|\.redb$|\.env$|(^|/)id_rsa$|\.(pem|key)$)' "$package_files"; then
  printf '%s\n' 'Release archive contains a forbidden build, database, or secret-like path' >&2
  exit 1
fi

if ! command -v expect >/dev/null 2>&1; then
  printf '%s\n' 'PTY-GATES:expect is required for release preflight' >&2
  exit 1
fi
export DGO_BIN="$repo_root/target/release/dgo"
expect scripts/pty-picker-smoke.exp
expect scripts/pty-terminal-gates.exp
expect scripts/pty-shell-matrix.exp
DGO_FIXTURE_BIN="$repo_root/target/release/dgo-fixture" \
  scripts/benchmark-cli.sh --directories 100 --samples 1

blocked_path="$scratch_dir/blocked"
printf blocked > "$blocked_path"

for shell in zsh bash; do
  XDG_CONFIG_HOME="$blocked_path" XDG_CACHE_HOME="$blocked_path" XDG_STATE_HOME="$blocked_path" \
    target/release/dgo completions "$shell" > "$scratch_dir/dgo.$shell"
  XDG_CONFIG_HOME="$blocked_path" XDG_CACHE_HOME="$blocked_path" XDG_STATE_HOME="$blocked_path" \
    target/release/dgo init "$shell" > "$scratch_dir/init.$shell"
  "$shell" -n "$scratch_dir/dgo.$shell"
  "$shell" -n "$scratch_dir/init.$shell"
done

if command -v fish >/dev/null 2>&1; then
  XDG_CONFIG_HOME="$blocked_path" XDG_CACHE_HOME="$blocked_path" XDG_STATE_HOME="$blocked_path" \
    target/release/dgo completions fish > "$scratch_dir/dgo.fish"
  XDG_CONFIG_HOME="$blocked_path" XDG_CACHE_HOME="$blocked_path" XDG_STATE_HOME="$blocked_path" \
    target/release/dgo init fish > "$scratch_dir/init.fish"
  fish -n "$scratch_dir/dgo.fish"
  fish -n "$scratch_dir/init.fish"
  printf '%s\n' 'SHELL-SYNTAX:fish:ok'
elif ((require_fish)); then
  printf '%s\n' 'SHELL-SYNTAX:fish:required but not installed' >&2
  exit 1
else
  printf '%s\n' 'SHELL-SYNTAX:fish:skipped (fish is not installed)'
fi

printf '%s\n' 'PREFLIGHT:local-gates:ok'
