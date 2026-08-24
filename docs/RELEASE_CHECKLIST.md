# Release checklist

This checklist is a publication gate, not a promise of future work. Do not create a Git tag, GitHub release, Homebrew formula, or crates.io publication until every applicable item is evidenced in the release record.

Release status on 2026-08-24: `v0.1.3` is the current public release. Its complete local preflight passes with 70 unit and 22 Unix CLI integration tests, package/default-install checks, and Zsh/Bash/Fish PTY gates. Protected CI run `32679864525` passed on release commit `dd7497d` across Ubuntu 22.04, macOS, Windows, and dependency policy. Tag workflow `32680020828` published four native archives plus `SHA256SUMS`; independent downloads, checksum verification, macOS ARM/Intel execution, clean Debian 12 execution, and the archive lifecycle drill passed. The maintained `RudySource/homebrew-tap` formula passed strict audit, real `0.1.2 → 0.1.3` upgrade, and `brew test`. The only unpublished channel is crates.io, which requires an external owner credential and is not required to use the GitHub or Homebrew release.

## 0.1.3 patch delta

- [x] Terminal-facing output escapes control characters and bidirectional overrides; redirected stdout retains exact shell/machine path semantics.
- [x] History, session transitions, and abandoned shell-session records have tested hard bounds and backward-compatible state decoding.
- [x] Concurrent visit increments and competing bookmark renames preserve their invariants inside serialized write transactions.
- [x] Doctor, config-path, and support recovery flows work with malformed configuration or unavailable storage.
- [x] CLI help and README cover the actual new-user, picker, platform, recovery, uninstall, and contributor workflows.
- [x] Protected native CI passes on exact commit `dd7497d` (run `32679864525`).
- [x] Tag workflow `32680020828` published four version-matched archives and checksums; independent download/install drills passed.
- [x] The maintained Homebrew tap installs and tests `0.1.3` using the public archive checksums (tap commit `2c421f6`).

## Code and package

- [x] A clean Linux build passes on the declared MSRV (Rust 1.89); raising dependency versions must not silently raise MSRV without updating `Cargo.toml`, README, and CI together. Evidence: CI run `32674908733` on `1240cdf`.
- [x] `scripts/release-preflight.sh --require-fish` passes on the release commit; it covers formatting, strict clippy, tests, release build, Criterion compilation, offline package assembly, completion syntax, PTY picker/terminal/shell gates, and a disposable benchmark smoke.
- [x] `cargo build --release --bin dgo`, the feature-gated fixture build, and package verification pass locally; a default `cargo install` exposes only `dgo`, and `cargo publish --dry-run --locked` passes from a clean worktree.
- [x] The four binary targets have public archives and matching SHA-256 checksums: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, and `x86_64-pc-windows-msvc`. Evidence: Release run `32680020828`; all four downloaded files pass independent `shasum -a 256 -c SHA256SUMS`.
- [x] Public archives execute on macOS ARM64, macOS Intel through Rosetta, native Windows release runners, and clean Debian 12 amd64. The current macOS archive lifecycle passed `0.1.2 → 0.1.3 → rollback 0.1.2 → restore 0.1.3`; Homebrew upgraded the linked binary from `0.1.2` to `0.1.3`, then passed strict audit and `brew test`.

## Behaviour and compatibility

- [x] The direct streaming picker meets its recorded 1M PTY budget on a host without fixture-generation limits: 55.180 ms first paint and 35.236 ms first useful result against a 100/100 ms budget.
- [x] The Zsh, Bash, and Fish PTY matrix passes on macOS and Linux. A skipped shell is not a pass.
- [x] Terminal gates cover `TERM=dumb`, no-color, no-Unicode, tiny viewport, resize, Ctrl-C, restoration, and the plain numbered selector used as the screen reader-compatible fallback. No claim is made that this replaces a human assistive-technology study.

## Security and communication

- [x] GitHub authentication is valid and repository settings are readable from the release workstation.
- [x] `cargo deny check` passes and the dependency-policy CI job is green; Dependabot alert 1 is fixed by `lru 0.18.2`.
- [x] `SECURITY.md`, `SUPPORT.md`, README installation instructions, version, changelog, and release-note source agree on the supported `0.1.x` line and current release `0.1.3`.
- [x] GitHub private vulnerability reporting, vulnerability alerts, Dependabot security updates, secret scanning, and push protection are enabled before publishing.
- [x] Tagged release `v0.1.3` includes concise release notes, known limitations, four native archives, and aggregate checksums. Superseded releases remain immutable; `v0.1.1` carries an explicit Linux compatibility warning.

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
