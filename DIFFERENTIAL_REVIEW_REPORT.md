# Differential security and release review

Date: 2026-08-24
Baseline: `284a316` (`main`; content-equivalent privacy rewrite of the original baseline)
Reviewed state: complete working-tree release candidate against the baseline

## Executive summary

**Local decision: PASS.** No open P0 or P1 security or correctness finding is
known in the reviewed tree. The code is suitable to become the `0.1.0` release
candidate after it is committed. Public release is still gated by evidence that
cannot exist in a dirty local tree: green remote CI on the exact commit, native
artifacts and checksum verification, Windows runtime validation, clean-machine
install/upgrade/rollback drills, GitHub security-advisory configuration, and the
final publication dry run.

| Severity | Found | Open |
| --- | ---: | ---: |
| P0 critical | 0 | 0 |
| P1 high | 4 | 0 |
| P2 medium | 9 | 0 |
| P3 low | 1 | 0 |

## What changed

The baseline was a small foundation commit. The reviewed candidate adds the
complete Rust CLI/TUI product: XDG configuration, redb index and state stores,
parallel crawling, ranking, bookmarks and history, session navigation, safe
Zsh/Bash/Fish wrappers, terminal picker, platform actions, diagnostics,
benchmarks, PTY automation, CI, native release packaging, policy files, support
material, and a reproducible demo.

The tracked baseline diff is over 3,300 added Rust/documentation lines, plus the
reviewed untracked release automation, benchmark, packaging, and documentation
files. Because the repository has one baseline commit, the review treats the
entire candidate as security-sensitive rather than sampling only recent hunks.

## Resolved findings

### R1 — MSRV contradicted the locked dependency graph (P1)

The manifest and CI claimed Rust 1.85 while locked dependencies required a newer
compiler. `Cargo.toml`, README, CI, and the release checklist now consistently
declare Rust 1.89; a clean Debian Rust 1.89 release build passed.

### R2 — streaming picker bypassed corrupt-index recovery (P1)

The large-index path opened redb directly and did not share the regular query
path's quarantine/rebuild flow. Both now use `open_index_with_recovery`; invalid
disposable data is preserved under a collision-safe timestamped name before a
rebuild.

### R3 — inline TUI contaminated the shell stdout protocol (P1)

Ratatui's inline viewport caused Crossterm 0.28 to write `ESC[6n` to stdout.
Inside command substitution this prefixed the selected path and made `cd` fail.
Redirected stdout now forces the alternate-screen backend. The real
Zsh/Bash/Fish PTY matrix selects an ambiguous result through each wrapper and
asserts the resulting working directory.

### R4 — bookmark operations could persist or destroy the wrong data (P1)

`bookmark add --path ./relative` stored a cwd-dependent path, and renaming to an
existing name silently replaced that bookmark. Adds now canonicalize and
validate the destination, in-place repairs preserve metadata, and rename
collisions fail without modifying either bookmark.

### R5 — resumable benchmark fixture accepted a symlink root (P2)

A crafted marker behind a symlink could redirect developer-only fixture writes.
Resume requires a real directory identified with `symlink_metadata`; a test
proves the target remains untouched.

### R6 — Linux PTY evidence depended on implicit Tcl encoding (P2)

Expect could create a byte-different Unicode fixture on a locale-free runner and
then appear to hang. The gates select UTF-8 before fixture creation, export a
deterministic locale, diagnose missing fixtures, and handle early Fish EOF.

### R7 — normal installation exposed a developer fixture binary (P2)

`dgo-fixture` was a public package binary. It now requires the non-default
`benchmark-tools` feature. The release preflight performs an offline default
`cargo install` and asserts that the only installed executable is `dgo`.

### R8 — Windows actions used Linux helper commands (P2)

The Windows artifact type-checked, but Open and Copy searched for `xdg-open`,
`wl-copy`, or `xclip`. Windows now uses `explorer.exe` and a fixed PowerShell
`Set-Clipboard` command. The path crosses stdin as UTF-8 data and is never
interpolated into executable shell text.

### R9 — incomplete indexes looked valid and path encoding was lossy (P2)

An existing redb without a schema marker was accepted as an empty index, and a
non-UTF-8 Unix directory could collide after lossy string conversion. Missing
markers are recoverable corruption; indexed paths are stored only when valid
UTF-8. The documented shell boundary explicitly rejects newline and non-UTF-8
paths while the wrapper's direct existing-path fast path remains native.

### R10 — project markers drifted under symlinked configured roots (P2)

Directory records were canonical but marker parents were lexical paths, so
project metadata could disappear. Marker parents are now canonicalized before
joining scan output; a symlink-root regression test covers it.

### R11 — GitHub Actions used mutable references (P2)

CI and release workflows referenced floating action tags. Every action is now
pinned to an immutable official repository commit. Dependabot tracks weekly
GitHub Actions updates with a seven-day cooldown; actionlint validates both
workflows.

### R12 — repository/support artifacts contained release noise (P3)

Finder's `.DS_Store` was removed. Placeholder donation addresses were replaced
with actionable, privacy-aware support guidance. Historical baseline wording
was corrected without deleting useful design or audit records.

### R13 — publication metadata exposed local identity details (P2)

The original baseline commit used a personal email domain, and the rendered demo
captured a machine-specific macOS `TMPDIR` identifier. Before release, the
content-equivalent baseline was rewritten to the canonical GitHub noreply identity
`80245370+RudySource@users.noreply.github.com`. Demo generation now uses a neutral
`/private/tmp/dirgo-demo.*` prefix; all 14 sampled frames and GIF metadata were
rechecked. Working-tree and Git-history gitleaks scans found no secret.

### R14 — shareable benchmark reports captured host identity (P2)

The external benchmark used `uname -a` and inherited macOS `$TMPDIR`, which can
embed a hostname and a per-user temporary-directory identifier in `--report`
output. Reports now record only `uname -srm`, and generated fixture/sandbox paths
use a neutral `/tmp/dirgo-*` prefix.

## Adversarial analysis

- Generated wrappers quote command substitution and invoke `builtin cd --`;
  filesystem paths are never evaluated as shell source.
- Paths with spaces, quotes, Unicode, brackets, emoji, and leading dashes cross
  the supported UTF-8 protocol as data. Newline and non-UTF-8 indexed paths are
  rejected explicitly.
- Editor configuration accepts one executable only. Open/editor pass one path
  argument; clipboard passes bytes through stdin. No action uses `sh -c`,
  `cmd /c`, `eval`, or path interpolation.
- Zoxide import executes a fixed argument vector, validates the complete UTF-8
  response before mutation, rejects relative/non-finite/unbounded values, and
  ignores stale paths.
- Unknown persistent-state schemas return without overwrite. Recoverable state
  is moved to a collision-safe backup before empty recreation.
- Refresh uses a single-writer lock, process-specific temporary redb, validation,
  and atomic publication. Inaccessible roots preserve the previous snapshot.
- Symlink traversal is opt-in; broken links, cycles, symlinked fixture roots, and
  symlinked configured roots have regression coverage.
- Normal runtime behavior has no network, telemetry, plugin execution, or update
  channel.

## Blast radius

| Boundary | Producers | Consumers | Failure impact | Controls |
| --- | --- | --- | --- | --- |
| stdout destination protocol | `__resolve`, picker | three shell wrappers | wrong `cd` or command failure | stderr UI, fullscreen on redirected stdout, PTY matrix |
| filesystem index | crawler/refresh | resolver, picker, repo, stats | stale/wrong candidates | lock, validation, atomic rename, quarantine/rebuild |
| persistent state | bookmarks/history/session/import | ranking, recent, back/forward | user data loss | schema gate, transactions, backup recovery, collision checks |
| process execution | open/copy/editor/zoxide | OS helpers | command injection or wrong target | fixed executable/args, no shell, stdin clipboard |
| release pipeline | tag workflow | four platform archives | compromised or mismatched release | immutable actions, tag/version gate, locked builds, checksums |

## Test coverage and evidence

The final local preflight covers formatting, warnings-denied Clippy across all
targets/features, unit and integration tests, release builds for the public and
feature-gated developer binaries, Criterion compilation, offline package
assembly, default install surface, package-content policy, generated shell
syntax, PTY restoration and wrappers, and an external benchmark smoke.

The final run passed 61 macOS unit tests and 18 CLI integration tests. The
additional Linux-only non-UTF-8 filesystem regression passed in a clean
`rust:1.89-bookworm` container.

Additional evidence includes:

- Windows MSVC `cargo check --locked --offline --bin dgo`;
- clean Debian Rust 1.89 locked release build and Zsh/Bash/Fish PTY matrix;
- cargo-deny 0.20 advisories, bans, licenses, and sources checks;
- actionlint 1.7.12 on CI and release workflows;
- gitleaks scans of the complete working tree and Git history: no leaks found;
- macOS ARM64 and x86_64 release binaries executing `dgo 0.1.0`;
- 1M-directory PTY results of 55.180 ms first paint and 35.236 ms first useful
  result, within the 100/100 ms release budget;
- rendered VHS demo inspected frame by frame.

The tests are strong around the highest-risk boundaries. They do not replace a
native Windows runtime pass, screen-reader evaluation, remote repository policy
inspection, or clean-machine installer drills.

## History analysis

`git log main` contains one baseline commit (`284a316`). Its author metadata was
rewritten before release to use the canonical GitHub noreply identity; the code
tree is unchanged from the original baseline.
There is no prior security-fix lineage or mature blame history from which to
infer invariants. Searches for removed validation, escaping, schema, lock, and
permission checks were therefore reviewed against the complete implementation;
no unexplained removal remains.

## Remaining publication gates

1. Commit the reviewed tree and re-authenticate `gh` for `RudySource`.
2. Push that exact commit and require green macOS/Linux CI plus dependency policy.
3. Verify private security advisories and repository protection/settings.
4. Run the crates.io dry run and confirm package ownership/name availability.
5. Complete the manual terminal/accessibility pass.
6. Tag exactly `v0.1.0`; download all four native archives and `SHA256SUMS`.
7. Verify hashes and execute macOS/Linux/Windows clean install, upgrade, and
   rollback drills before publishing package-manager metadata.

Any code, dependency, workflow, or release-document change after this review
invalidates the PASS until the affected gates are rerun.

## Recommended follow-up

- Upgrade Ratatui when a compatible release removes transitive unmaintained
  `paste`; then remove the explicit cargo-deny advisory exception.
- Reduce duplicate `syn`, `unicode-width`, and `windows-sys` versions when the
  dependency graph permits, without raising MSRV accidentally.
- Add native Windows PTY/runtime automation after the first manual Windows gate.
- Keep package-manager expansion maintainer-owned; never publish placeholder
  taps, buckets, or checksums.

## Review methodology

The review followed a differential security workflow: working-tree inventory,
complete baseline diff, high-risk caller tracing, adversarial path/process/state
analysis, history and removed-check searches, dependency policy, platform
type-checking, executable PTY tests, packaging inspection, and release-document
consistency review. Findings were fixed in place and retained here with severity,
impact, control, and verification evidence.
