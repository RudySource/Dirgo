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

New domain terms belong in `CONTEXT.md`. Add an ADR only for a hard-to-reverse, surprising trade-off.

## Pull requests

Include behavior-focused tests, list exact verification commands, and do not add performance claims without a reproducible fixture, method, and result.
