# Differential security and release review

Date: 2026-08-24
Baseline: `284a316` (`main`; content-equivalent privacy rewrite of the original baseline)
Reviewed state: complete working-tree release candidate against the baseline

## Executive summary

**Product release decision: PASS for the `v0.1.3` candidate.** No open P0 or P1
security or correctness finding is known in the reviewed tree. The complete
local preflight passes with the new terminal-injection and bounded-state
regressions. Native macOS/Linux/Windows/dependency-policy CI remains a hard gate
on the exact commit before tagging. GitHub Release `v0.1.2`, its four verified
native archives, and the maintained Homebrew formula remain public while the
patch candidate is verified. crates.io publication remains an external
credential operation rather than an unfinished repository task.

| Severity | Found | Open |
| --- | ---: | ---: |
| P0 critical | 0 | 0 |
| P1 high | 8 | 0 |
| P2 medium | 14 | 0 |
| P3 low | 2 | 0 |

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
The collision check and rename now share one write transaction, so concurrent
processes cannot bypass the same invariant.

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

### R15 — released dependency graph contained vulnerable `lru` (P2)

GitHub Dependabot identified GHSA-rhfx-m35p-ff5j in `lru 0.12.5`, pulled by
Ratatui 0.29. Ratatui is now 0.30.2 with default features disabled except for
the required Crossterm backend; this selects patched `lru 0.18.2`. Direct
Crossterm was aligned to 0.29, and the obsolete cargo-deny exception for the
removed `paste` dependency was deleted.

### R16 — dependency update blocked fullscreen picker teardown (P1)

Ratatui 0.30 changed `Terminal::clear()` to preserve the cursor via a DSR
position query. In a fullscreen shell picker this unnecessary teardown query
timed out and discarded an otherwise valid selection. Fullscreen cleanup now
skips the redundant clear because leaving the alternate screen restores the
original display; the real PTY picker and Zsh/Bash/Fish wrapper matrix pass.

### R17 — Unix-only integration fixture blocked the Windows release (P1)

The first tag correctly stopped before publication because `tests/cli.rs`
imported Unix permissions APIs unconditionally. That shell-focused suite is now
explicitly Unix-only, while a separate native Windows integration suite covers
version execution, refresh, exact query output, and diagnostics. The failed
`v0.1.0` tag is retained without a GitHub Release; the corrected release is
versioned `0.1.1` rather than silently moving a public tag.

### R18 — pinned GitHub Actions runtime was deprecated (P2)

The immutable checkout pin still targeted the Node 20 generation, which GitHub
was already force-running on Node 24 with a deprecation warning. Checkout,
artifact upload, and artifact download now use current Dependabot-resolved
major versions pinned to exact official commit SHAs. Actionlint and the full
native CI matrix validate the updated workflow surface before tagging.

### R19 — Linux release archive required glibc 2.39 (P1)

The first successful GitHub release built its GNU/Linux binary on Ubuntu 24.04.
Although native CI and packaging passed, the downloaded archive failed on a
clean Debian 12 container because it required `GLIBC_2.39`. Release builds now
run on Ubuntu 22.04 and inspect the final dynamic symbol table, rejecting any
binary that requires a glibc symbol newer than 2.35. The immutable `v0.1.1`
release is explicitly marked superseded; the corrected release is `v0.1.2`.

### R20 — untrusted filenames could inject terminal controls (P1)

Indexed UTF-8 directory names were rendered directly by the Ratatui picker,
plain fallback, diagnostics, and several human-facing commands. A directory
created by an extracted archive or repository could therefore emit ANSI/OSC
controls or invisible bidirectional overrides when a user searched for it.
Human-facing output now escapes C0/C1 controls and bidi overrides. Direct TTY
path output is escaped, while redirected stdout and shell command substitution
retain the exact path required by the navigation protocol. Unit, render, and
CLI regressions cover Unicode preservation, OSC/ANSI bytes, bidi text, and the
ambiguous non-TTY path.

### R21 — persistent navigation state grew without a limit (P2)

Every newly visited directory, shell transition, and generated shell-session ID
could remain in redb indefinitely. History is now capped at 50,000 rows and
pruned to 45,000 by recency and visit strength. Each session retains its latest
256 transitions, while session records prune from 256 to 192 without evicting
the session currently being written. Legacy session JSON remains readable via a
defaulted timestamp field, so the schema stays backward-compatible.

### R22 — diagnostics depended on valid configuration (P2)

`dgo doctor` and `dgo config path` loaded the configuration before they could
diagnose it. A malformed TOML file therefore hid its own location and prevented
the recovery command from running. Config-path and support output now bypass
storage/config loading, while Doctor reports the parse error safely, continues
independent checks with defaults, and exits non-zero after completing.

### R23 — generated CLI help omitted command purpose (P3)

Clap listed every subcommand without descriptions, forcing a new user back to
the README. Top-level and nested workflows now have concise, tested help text;
the README also documents persistent shell setup, exact picker keys, platform
limits, troubleshooting, uninstall, security behavior, and contributor setup.

### R24 — concurrent state updates used split read/write transactions (P2)

Navigation read the existing visit counter before opening its write transaction,
so simultaneous invocations could both increment the same old value and lose a
visit. Bookmark rename checked the destination name in the same split pattern,
allowing a competing rename to violate the no-overwrite invariant. Both
operations now perform read-check-update inside one serialized redb write
transaction. Barrier-driven thread regressions prove all 320 concurrent visits
are retained and exactly one competing rename succeeds.

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
- Filesystem text crosses a display boundary before terminal rendering; raw
  paths are emitted only when stdout is redirected for shell or machine use.
- State retention is bounded and batch-pruned in serialized redb write
  transactions; the current shell session is protected during pruning.

## Blast radius

| Boundary | Producers | Consumers | Failure impact | Controls |
| --- | --- | --- | --- | --- |
| stdout destination protocol | `__resolve`, picker | three shell wrappers | wrong `cd` or command failure | stderr UI, fullscreen on redirected stdout, PTY matrix |
| filesystem index | crawler/refresh | resolver, picker, repo, stats | stale/wrong candidates | lock, validation, atomic rename, quarantine/rebuild |
| persistent state | bookmarks/history/session/import | ranking, recent, back/forward | user data loss | schema gate, transactions, backup recovery, collision checks |
| process execution | open/copy/editor/zoxide | OS helpers | command injection or wrong target | fixed executable/args, no shell, stdin clipboard |
| terminal display | paths, query, preview entries | TUI, fallback, diagnostics | ANSI/OSC injection or bidi spoofing | centralized escaping, TTY-aware raw-path boundary, regressions |
| navigation retention | visits and shell sessions | ranking, recent, back/forward | unbounded state or loss of active session | hard caps, batch pruning, protected current session, legacy decode |
| release pipeline | tag workflow | four platform archives | compromised or mismatched release | immutable actions, tag/version gate, locked builds, checksums |

## Test coverage and evidence

The final local preflight covers formatting, warnings-denied Clippy across all
targets/features, unit and integration tests, release builds for the public and
feature-gated developer binaries, Criterion compilation, offline package
assembly, default install surface, package-content policy, generated shell
syntax, PTY restoration and wrappers, and an external benchmark smoke.

The `v0.1.3` candidate run passed 70 macOS unit tests and 22 CLI integration tests. The
additional Linux-only non-UTF-8 filesystem regression passed in a clean
`rust:1.89-bookworm` container.

Additional evidence includes:

- native Windows CI: 56 unit tests, three Windows CLI integration tests, strict
  Clippy, and the MSVC release binary build;
- clean Debian Rust 1.89 locked release build and Zsh/Bash/Fish PTY matrix;
- cargo-deny 0.20 advisories, bans, licenses, and sources checks;
- actionlint 1.7.12 on CI and release workflows;
- gitleaks scans of the complete working tree and Git history: no leaks found;
- macOS ARM64 and x86_64 release binaries executing the matching Dirgo version;
- 1M-directory PTY results of 55.180 ms first paint and 35.236 ms first useful
  result, within the 100/100 ms release budget;
- rendered VHS demo inspected frame by frame.
- malformed-config Doctor/config-path recovery, terminal-control rendering, and
  bounded history/session regressions;
- release run `32677282624`: four locked native test/build jobs plus gated
  checksum generation and GitHub publication;
- independently downloaded `v0.1.2` checksums, native macOS ARM/Intel execution,
  clean Debian 12 amd64 execution, and native Windows staged-binary execution;
- remote `RudySource/homebrew-tap` install of `dirgo 0.1.2`, followed by linked
  binary version/help checks and an archive upgrade/rollback drill.

The tests are strong around the highest-risk boundaries. They do not replace a
human screen-reader evaluation or clean-machine installer drills.

## History analysis

`git log main` begins with baseline commit `284a316`. Its author metadata was
rewritten before release to use the canonical GitHub noreply identity; the code
tree is unchanged from the original baseline. Release preparation and the
dependency/TUI security fix are retained as separate reviewable commits.
There is no prior security-fix lineage or mature blame history from which to
infer invariants. Searches for removed validation, escaping, schema, lock, and
permission checks were therefore reviewed against the complete implementation;
no unexplained removal remains.

## Remaining publication gates

Push the exact `v0.1.3` candidate, require all protected native CI contexts, then
let the immutable tag workflow publish and independently verify all archives.
Update the maintained Homebrew formula only from those public checksums. Publish
to crates.io after authenticating this workstation with an authorized registry
token. No token is stored in the repository or local environment.

Any code, dependency, workflow, or release-document change after this review
invalidates the PASS until the affected gates are rerun.

## Recommended follow-up

- Reduce duplicate `syn`, `hashbrown`, and benchmark-only `itertools` versions when the
  dependency graph permits, without raising MSRV accidentally.
- Expand native Windows coverage when parent-shell PowerShell/cmd integration is
  introduced; 0.1.2 intentionally ships archive CLI support without those wrappers.
- Keep package-manager expansion maintainer-owned; never publish unowned taps,
  buckets, or invented checksums.

## Review methodology

The review followed a differential security workflow: working-tree inventory,
complete baseline diff, high-risk caller tracing, adversarial path/process/state
analysis, history and removed-check searches, dependency policy, platform
type-checking, executable PTY tests, packaging inspection, and release-document
consistency review. Findings were fixed in place and retained here with severity,
impact, control, and verification evidence.
