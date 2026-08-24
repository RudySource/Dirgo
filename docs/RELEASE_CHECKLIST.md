# Release checklist

This checklist is a publication gate, not a promise of future work. Do not create a Git tag, GitHub release, Homebrew formula, or crates.io publication until every applicable item is evidenced in the release record.

Release status on 2026-08-24: `v0.1.3` is the reviewed patch candidate. Its complete local preflight passes with 70 unit and 22 Unix CLI integration tests, package/default-install checks, and Zsh/Bash/Fish PTY gates. The differential review has no open P0/P1 finding. Native Ubuntu 22.04, macOS, Windows, dependency-policy CI, tag publication, downloaded-archive drills, and the Homebrew checksum update remain mandatory gates on the exact candidate. GitHub Release `v0.1.2` and the maintained `RudySource/homebrew-tap` formula remain the current public distribution until those gates complete. crates.io additionally requires an external registry credential.

## 0.1.3 patch delta

- [x] Terminal-facing output escapes control characters and bidirectional overrides; redirected stdout retains exact shell/machine path semantics.
- [x] History, session transitions, and abandoned shell-session records have tested hard bounds and backward-compatible state decoding.
- [x] Concurrent visit increments and competing bookmark renames preserve their invariants inside serialized write transactions.
- [x] Doctor, config-path, and support recovery flows work with malformed configuration or unavailable storage.
- [x] CLI help and README cover the actual new-user, picker, platform, recovery, uninstall, and contributor workflows.
- [ ] Protected native CI passes on the exact candidate commit.
- [ ] The `v0.1.3` tag workflow publishes four version-matched archives and checksums; independent download/install drills pass.
- [ ] The maintained Homebrew tap installs `0.1.3` using the public archive checksums.

## Code and package

- [x] A clean Linux build passes on the declared MSRV (Rust 1.89); raising dependency versions must not silently raise MSRV without updating `Cargo.toml`, README, and CI together. Evidence: CI run `32674908733` on `1240cdf`.
- [x] `scripts/release-preflight.sh --require-fish` passes on the release commit; it covers formatting, strict clippy, tests, release build, Criterion compilation, offline package assembly, completion syntax, PTY picker/terminal/shell gates, and a disposable benchmark smoke.
- [x] `cargo build --release --bin dgo`, the feature-gated fixture build, and package verification pass locally; a default `cargo install` exposes only `dgo`, and `cargo publish --dry-run --locked` passes from a clean worktree.
- [x] The four binary targets have public archives and matching SHA-256 checksums: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, and `x86_64-pc-windows-msvc`. Evidence: Release run `32677282624`; all four downloaded files pass independent `shasum -a 256 -c SHA256SUMS`.
- [x] Public archives execute on macOS ARM64, macOS Intel through Rosetta, native Windows release runners, and clean Debian 12 amd64. The macOS archive lifecycle passed `0.1.1 → 0.1.2 → rollback 0.1.1 → restore 0.1.2`; the first maintained Homebrew formula installs and links `dirgo 0.1.2` from the remote tap. Homebrew upgrade is not applicable until a second formula version exists.

## Behaviour and compatibility

- [x] The direct streaming picker meets its recorded 1M PTY budget on a host without fixture-generation limits: 55.180 ms first paint and 35.236 ms first useful result against a 100/100 ms budget.
- [x] The Zsh, Bash, and Fish PTY matrix passes on macOS and Linux. A skipped shell is not a pass.
- [x] Terminal gates cover `TERM=dumb`, no-color, no-Unicode, tiny viewport, resize, Ctrl-C, restoration, and the plain numbered selector used as the screen reader-compatible fallback. No claim is made that this replaces a human assistive-technology study.

## Security and communication

- [x] GitHub authentication is valid and repository settings are readable from the release workstation.
- [x] `cargo deny check` passes and the dependency-policy CI job is green; Dependabot alert 1 is fixed by `lru 0.18.2`.
- [x] `SECURITY.md`, `SUPPORT.md`, README installation instructions, version, changelog, and release-note source agree on 0.1.2. Published checksums remain tag-gated below.
- [x] GitHub private vulnerability reporting, vulnerability alerts, Dependabot security updates, secret scanning, and push protection are enabled before publishing.
- [x] Tagged release `v0.1.2` includes concise release notes, known limitations, four native archives, and aggregate checksums. Superseded `v0.1.1` remains immutable and carries an explicit Linux compatibility warning.

## Release sequence

Run these steps in order. Stop at the first failed gate.

1. Re-authenticate GitHub if `gh auth status` is not green, then verify the repository and private security-advisory setting.
2. Commit the complete release candidate, push it, and wait for macOS, Linux, Windows, and dependency-policy CI to pass on that exact commit.
3. Run `cargo publish --dry-run --locked` and verify that the `dirgo` crate name is still publishable for this owner.
4. Perform the manual accessibility/terminal pass from the built commit, including the plain fallback.
5. Create and push only the matching annotated tag (`v0.1.3` for package version `0.1.3`). The tag-gated workflow builds and tests all four targets before creating the GitHub release. `v0.1.0` is retained as a failed gate tag; `v0.1.1` is retained as a superseded release whose Linux install drill exposed its glibc 2.39 requirement. No historical tag may be moved or reused.
6. Download every release asset plus `SHA256SUMS`; verify hashes independently on macOS, Linux, and Windows.
7. Exercise clean install, upgrade, and rollback with the downloaded archives. Record commands, host details, and results in the release notes.
8. Update `packaging/homebrew/dirgo.rb` with the real version and published checksums; publish it only in the maintained `RudySource/homebrew-tap` after the archive drills pass.
9. Publish to crates.io only after the GitHub artifacts and documentation are final. Never reuse or move the Git tag after publication.
