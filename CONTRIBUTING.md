# Contributing to Dirgo

Dirgo accepts focused changes that make directory navigation faster, safer, or easier without turning it into a file manager or shell framework.

## Development

```bash
cargo build
cargo test --all-features
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo bench --bench index_pipeline --no-run
cargo deny check
```

Test a shell integration without modifying shell startup files:

```zsh
cargo build
PATH="$PWD/target/debug:$PATH"
eval "$(dgo init zsh)"
```

Use temporary XDG directories and a small configured root for filesystem tests. Never run performance fixtures against a user's real home directory.

The CI benchmark smoke uses only a disposable 100-directory fixture. It validates the harness and release binary on macOS and Linux; it is not a latency target or published performance claim.

## Architecture

- `index` owns the disposable filesystem snapshot and project detection.
- `state` owns persistent bookmarks, visits, and session navigation.
- `search` ranks candidates and applies the auto-resolution safety policy.
- `shell` emits thin wrappers; filesystem-derived paths are data, never shell code.
- `app` coordinates commands and keeps stdout clean for shell/machine callers.

Keep user-facing terminology consistent across the CLI help, README, and code. Add an ADR only for a hard-to-reverse, surprising trade-off.

## Pull requests

Include behavior-focused tests, list exact verification commands, and do not add performance claims without a reproducible fixture, method, and result.

## Releases

Run `scripts/release-preflight.sh`, then push an annotated version tag that exactly matches `Cargo.toml`. The release workflow publishes attested native archives and only then updates `RudySource/homebrew-tap` from the released checksums when the optional tap token is configured.

Optional repository secret `HOMEBREW_TAP_TOKEN` must be a fine-grained token scoped only to `RudySource/homebrew-tap` with repository Contents read/write permission. Without it, the release remains successful and reports that the tap update was skipped. The normal `GITHUB_TOKEN` remains read-only outside Dirgo and cannot update the separate tap repository.

crates.io versions are immutable. Never move an already published tag or try to
upload the same version again; increment the patch version instead.

### One-time maintainer setup

Authenticate GitHub CLI and Cargo without putting either token in shell history:

```sh
gh auth login -h github.com
cargo login
```

Create a fine-grained GitHub token with Contents read/write access only to
`RudySource/homebrew-tap`, then store it as a Dirgo Actions secret:

```sh
gh secret set HOMEBREW_TAP_TOKEN --repo RudySource/Dirgo
gh secret list --repo RudySource/Dirgo
```

### Example: publish 0.3.1

First set `version = "0.3.1"` in `Cargo.toml`, update `Cargo.lock`, and move the
completed notes from `Unreleased` to a dated `0.3.1` section in `CHANGELOG.md`.
Then run:

```sh
scripts/release-preflight.sh --require-fish

git status --short
git add Cargo.toml Cargo.lock CHANGELOG.md CONTRIBUTING.md README.md \
  scripts/pty-picker-smoke.exp src/app.rs src/cli.rs src/index.rs src/lib.rs \
  src/paths.rs src/shell.rs src/tui.rs src/update.rs tests/cli.rs
git diff --cached --check
git diff --cached
git commit -m "Release Dirgo 0.3.1"

cargo publish --dry-run --locked
git push origin main

git tag -a v0.3.1 -m "Dirgo v0.3.1"
git push origin v0.3.1
```

Pushing the tag starts `.github/workflows/release.yml`. It builds and tests all
four targets, publishes the GitHub Release and checksums, and updates Homebrew.
Wait for that workflow before publishing the crate:

```sh
gh run watch "$(gh run list --repo RudySource/Dirgo --workflow release.yml --branch v0.3.1 --limit 1 --json databaseId --jq '.[0].databaseId')" --repo RudySource/Dirgo --exit-status
gh release view v0.3.1 --repo RudySource/Dirgo

cargo publish --locked
cargo info dirgo@0.3.1
```

The Scoop bucket has `checkver`/`autoupdate`; its Excavator workflow checks for
new GitHub releases every four hours. Trigger it immediately when needed:

```sh
gh workflow run excavator.yml --repo RudySource/scoop-bucket
gh run watch "$(gh run list --repo RudySource/scoop-bucket --workflow excavator.yml --limit 1 --json databaseId --jq '.[0].databaseId')" --repo RudySource/scoop-bucket --exit-status
```

Finally verify the public installation routes:

```sh
cargo install dirgo --version 0.3.1 --locked
brew update
brew info rudysource/tap/dirgo
gh release view v0.3.1 --repo RudySource/Dirgo
```
