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
build_root="${CARGO_TARGET_DIR:-$repo_root/target}"
if [[ "$build_root" != /* ]]; then
  build_root="$repo_root/$build_root"
fi
scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/dirgo-release-preflight.XXXXXX")
cleanup() { rm -rf "$scratch_dir"; }
trap cleanup EXIT

printf '%s\n' '== Dirgo release preflight =='
scripts/repository-hygiene.sh
crate_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
manifest_version=$(sed -n "s/^[[:space:]]*ModuleVersion = '\([^']*\)'.*/\1/p" powershell/DirgoPredictor/DirgoPredictor.psd1)
if [[ -z "$crate_version" || "$manifest_version" != "$crate_version" ]]; then
  printf 'PowerShell module version %s does not match crate version %s\n' "$manifest_version" "$crate_version" >&2
  exit 1
fi
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release --bin dgo
dotnet restore powershell/DirgoPredictor/DirgoPredictor.csproj --locked-mode
dotnet build powershell/DirgoPredictor/DirgoPredictor.csproj --configuration Release --no-restore
dotnet list powershell/DirgoPredictor/DirgoPredictor.csproj package --vulnerable --include-transitive
sh -n install/dirgo-installer.sh
sh -n scripts/render-homebrew-formula.sh
sh -n scripts/render-scoop-manifest.sh
DGO_BIN="$build_root/release/dgo" scripts/installer-smoke.sh
cargo build --release --bin dgo-fixture --features benchmark-tools
cargo bench --bench index_pipeline --no-run
cargo bench --bench suggestions --no-run
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
  packaging/homebrew/dirgo.rb.template packaging/scoop/dirgo.json.template \
  scripts/demo-setup.sh scripts/render-homebrew-formula.sh \
  scripts/render-scoop-manifest.sh scripts/repository-hygiene.sh \
  scripts/pty-suggestions-smoke.exp scripts/pty-zsh-live-completion.exp \
  scripts/windows-suggestions-smoke.ps1 \
  powershell/DirgoPredictor/DirgoPredictor.cs \
  powershell/DirgoPredictor/DirgoPredictor.csproj \
  powershell/DirgoPredictor/DirgoPredictor.psd1 \
  powershell/DirgoPredictor/packages.lock.json \
  src/lib.rs src/main.rs; do
  if ! grep -Fxq "$required_file" "$package_files"; then
    printf 'Release archive is missing required file: %s\n' "$required_file" >&2
    exit 1
  fi
done

formula_sums="$scratch_dir/formula-SHA256SUMS"
formula="$scratch_dir/dirgo.rb"
printf '%064d  dirgo-v9.8.7-aarch64-apple-darwin.tar.gz\n' 1 > "$formula_sums"
printf '%064d  dirgo-v9.8.7-x86_64-apple-darwin.tar.gz\n' 2 >> "$formula_sums"
printf '%064d  dirgo-v9.8.7-x86_64-unknown-linux-gnu.tar.gz\n' 3 >> "$formula_sums"
scripts/render-homebrew-formula.sh v9.8.7 "$formula_sums" "$formula"
if grep -q '@[A-Z_]*@' "$formula" || ! grep -q 'releases/download/v9.8.7' "$formula"; then
  printf '%s\n' 'Rendered Homebrew formula contains stale placeholders or version data' >&2
  exit 1
fi
if command -v ruby >/dev/null 2>&1; then
  ruby -c "$formula"
fi
scoop_manifest="$scratch_dir/dirgo.json"
printf '%064d  dirgo-v9.8.7-x86_64-pc-windows-msvc.zip\n' 4 >> "$formula_sums"
scripts/render-scoop-manifest.sh v9.8.7 "$formula_sums" "$scoop_manifest"
if grep -q '@[A-Z_]*@' "$scoop_manifest" || ! grep -q 'releases/download/v9.8.7' "$scoop_manifest"; then
  printf '%s\n' 'Rendered Scoop manifest contains stale placeholders or version data' >&2
  exit 1
fi
if command -v jq >/dev/null 2>&1; then
  jq -e '.version == "9.8.7" and .architecture."64bit".hash == "0000000000000000000000000000000000000000000000000000000000000004"' "$scoop_manifest" >/dev/null
fi
if grep -Eq '(^target/|^for_social/|\.redb$|\.env$|(^|/)(AGENTS|CLAUDE|CONTEXT|ROADMAP)\.md$|(^|/)id_(rsa|ed25519)$|\.(pem|key|p12|pfx)$|(_AUDIT|_REVIEW_REPORT)\.md$)' "$package_files"; then
  printf '%s\n' 'Release archive contains a forbidden build, private, agent, or secret-like path' >&2
  exit 1
fi

if ! command -v expect >/dev/null 2>&1; then
  printf '%s\n' 'PTY-GATES:expect is required for release preflight' >&2
  exit 1
fi
export DGO_BIN="$build_root/release/dgo"
expect scripts/pty-picker-smoke.exp
expect scripts/pty-terminal-gates.exp
expect scripts/pty-shell-matrix.exp
DGO_BASH_MAJOR="${BASH_VERSINFO[0]}" expect scripts/pty-suggestions-smoke.exp
expect scripts/pty-zsh-live-completion.exp
DGO_FIXTURE_BIN="$build_root/release/dgo-fixture" \
  scripts/benchmark-cli.sh --directories 100 --samples 1

blocked_path="$scratch_dir/blocked"
printf blocked > "$blocked_path"

for shell in zsh bash; do
  XDG_CONFIG_HOME="$blocked_path" XDG_CACHE_HOME="$blocked_path" XDG_STATE_HOME="$blocked_path" \
    "$build_root/release/dgo" completions "$shell" > "$scratch_dir/dgo.$shell"
  XDG_CONFIG_HOME="$blocked_path" XDG_CACHE_HOME="$blocked_path" XDG_STATE_HOME="$blocked_path" \
    "$build_root/release/dgo" init "$shell" > "$scratch_dir/init.$shell"
  "$shell" -n "$scratch_dir/dgo.$shell"
  "$shell" -n "$scratch_dir/init.$shell"
done

if command -v fish >/dev/null 2>&1; then
  XDG_CONFIG_HOME="$blocked_path" XDG_CACHE_HOME="$blocked_path" XDG_STATE_HOME="$blocked_path" \
    "$build_root/release/dgo" completions fish > "$scratch_dir/dgo.fish"
  XDG_CONFIG_HOME="$blocked_path" XDG_CACHE_HOME="$blocked_path" XDG_STATE_HOME="$blocked_path" \
    "$build_root/release/dgo" init fish > "$scratch_dir/init.fish"
  fish -n "$scratch_dir/dgo.fish"
  fish -n "$scratch_dir/init.fish"
  printf '%s\n' 'SHELL-SYNTAX:fish:ok'
elif ((require_fish)); then
  printf '%s\n' 'SHELL-SYNTAX:fish:required but not installed' >&2
  exit 1
else
  printf '%s\n' 'SHELL-SYNTAX:fish:skipped (fish is not installed)'
fi

if command -v pwsh >/dev/null 2>&1; then
  XDG_CONFIG_HOME="$blocked_path" XDG_CACHE_HOME="$blocked_path" XDG_STATE_HOME="$blocked_path" \
    "$build_root/release/dgo" completions powershell > "$scratch_dir/dgo-completions.ps1"
  XDG_CONFIG_HOME="$blocked_path" XDG_CACHE_HOME="$blocked_path" XDG_STATE_HOME="$blocked_path" \
    "$build_root/release/dgo" init powershell > "$scratch_dir/init.ps1"
  pwsh -NoLogo -NoProfile -Command \
    "[void][scriptblock]::Create((Get-Content -Raw -LiteralPath '$scratch_dir/dgo-completions.ps1')); [void][scriptblock]::Create((Get-Content -Raw -LiteralPath '$scratch_dir/init.ps1'))"
  printf '%s\n' 'SHELL-SYNTAX:powershell:ok'
else
  printf '%s\n' 'SHELL-SYNTAX:powershell:skipped (pwsh is not installed; Windows CI is required)'
fi

printf '%s\n' 'PREFLIGHT:local-gates:ok'
