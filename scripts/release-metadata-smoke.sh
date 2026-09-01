#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/dirgo-release-metadata.XXXXXX")
cleanup() { rm -rf "$scratch_dir"; }
trap cleanup EXIT

cat > "$scratch_dir/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

## [9.8.7] - 2030-01-02

### Added

- First release note.
- Second release note.

## [9.8.6] - 2030-01-01

- Older note.
EOF

scripts/extract-release-notes.sh 9.8.7 "$scratch_dir/CHANGELOG.md" > "$scratch_dir/notes.md"

cat > "$scratch_dir/expected.md" <<'EOF'
## Dirgo 9.8.7

### Added

- First release note.
- Second release note.
EOF

diff -u "$scratch_dir/expected.md" "$scratch_dir/notes.md"

if scripts/extract-release-notes.sh 1.2.3 "$scratch_dir/CHANGELOG.md" > /dev/null 2>&1; then
  printf '%s\n' 'Missing release section was accepted.' >&2
  exit 1
fi

cmd_install='powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Invoke-RestMethod '\''https://github.com/RudySource/Dirgo/releases/latest/download/dirgo-installer.ps1'\'' | Invoke-Expression"'
if ! grep -Fq "$cmd_install" README.md; then
  printf '%s\n' 'README is missing the copy-ready CMD installer command.' >&2
  exit 1
fi

if ! grep -Fq -- '--notes-file release-notes.md' .github/workflows/release.yml; then
  printf '%s\n' 'Release workflow is not publishing curated CHANGELOG notes.' >&2
  exit 1
fi

printf '%s\n' 'RELEASE-METADATA:ok'
