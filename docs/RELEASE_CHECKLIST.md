# Release checklist

This checklist is a publication gate, not a promise of future work. Do not create a Git tag, GitHub release, Homebrew formula, or crates.io publication until every applicable item is evidenced in the release record.

Local audit status on 2026-08-24: implementation review has no open P0/P1 finding; default installation exposes only `dgo`; Windows MSVC type-check, actionlint, cargo-deny, macOS/Linux shell evidence, and the non-publishing preflight are available. Working-tree/history secret scans pass, commit identity uses GitHub noreply, and the rendered demo contains no user-specific path or author metadata. Items remain unchecked below when they require the final clean commit, remote CI, native release artifacts, manual accessibility work, or publication credentials.

## Code and package

- [x] A clean Linux build passes on the declared MSRV (Rust 1.89); raising dependency versions must not silently raise MSRV without updating `Cargo.toml`, README, and CI together. Evidence: CI run `32674908733` on `1240cdf`.
- [x] `scripts/release-preflight.sh --require-fish` passes on the release commit; it covers formatting, strict clippy, tests, release build, Criterion compilation, offline package assembly, completion syntax, PTY picker/terminal/shell gates, and a disposable benchmark smoke.
- [x] `cargo build --release --bin dgo`, the feature-gated fixture build, and package verification pass locally; a default `cargo install` exposes only `dgo`, and `cargo publish --dry-run --locked` passes from a clean worktree.
- [ ] The four binary targets have archives and SHA-256 checksums: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, and `x86_64-pc-windows-msvc`.
- [ ] Clean-machine install, upgrade, and rollback are exercised for each installer that is actually enabled. Do not claim a package manager that has no maintained owner or tap.

## Behaviour and compatibility

- [x] The direct streaming picker meets its recorded 1M PTY budget on a host without fixture-generation limits: 55.180 ms first paint and 35.236 ms first useful result against a 100/100 ms budget.
- [x] The Zsh, Bash, and Fish PTY matrix passes on macOS and Linux. A skipped shell is not a pass.
- [x] Terminal gates cover `TERM=dumb`, no-color, no-Unicode, tiny viewport, resize, Ctrl-C, restoration, and the plain numbered selector used as the screen reader-compatible fallback. No claim is made that this replaces a human assistive-technology study.

## Security and communication

- [x] GitHub authentication is valid and repository settings are readable from the release workstation.
- [x] `cargo deny check` passes and the dependency-policy CI job is green; Dependabot alert 1 is fixed by `lru 0.18.2`.
- [x] `SECURITY.md`, `SUPPORT.md`, README installation instructions, version, changelog, and release-note source agree on 0.1.0. Published checksums remain tag-gated below.
- [x] GitHub private vulnerability reporting, vulnerability alerts, Dependabot security updates, secret scanning, and push protection are enabled before publishing.
- [ ] A tagged release includes concise release notes, known limitations, and the exact verification evidence.

## Release sequence

Run these steps in order. Stop at the first failed gate.

1. Re-authenticate GitHub if `gh auth status` is not green, then verify the repository and private security-advisory setting.
2. Commit the complete release candidate, push it, and wait for both macOS/Linux CI jobs and dependency policy to pass on that exact commit.
3. Run `cargo publish --dry-run --locked` and verify that the `dirgo` crate name is still publishable for this owner.
4. Perform the manual accessibility/terminal pass from the built commit, including the plain fallback.
5. Create and push only the matching annotated tag (`v0.1.0` for package version `0.1.0`). The tag-gated workflow builds and tests all four targets before creating the GitHub release.
6. Download every release asset plus `SHA256SUMS`; verify hashes independently on macOS, Linux, and Windows.
7. Exercise clean install, upgrade, and rollback with the downloaded archives. Record commands, host details, and results in the release notes.
8. Substitute the real version and published checksums into `packaging/homebrew/dirgo.rb.template`; publish it only in a tap with a confirmed maintainer.
9. Publish to crates.io only after the GitHub artifacts and documentation are final. Never reuse or move the Git tag after publication.
