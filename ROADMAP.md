# Dirgo release roadmap

This roadmap is ordered by release risk rather than by feature visibility. A phase is complete only when its acceptance checks pass on supported platforms.

## M0 — trustworthy core (complete)

- Rust 2024 binary, structured CLI, XDG paths, typed config, quiet logging.
- Rebuildable redb filesystem index with ignore rules, project markers, single-writer lock, and atomic publication.
- Separate redb user state for bookmarks, visits, and session navigation.
- Deterministic resolver: paths, bookmarks, unique exact basename, ranked fuzzy candidates.
- Generated Zsh, Bash, and Fish wrappers with an in-shell direct-path fast path.
- Root, repository, recent, bookmark, query, refresh, config path/show, doctor, and stats commands.
- Unit and integration tests for Unicode, spaces, ambiguity, paths, bookmarks, project roots, shell generation, and index replacement.

Exit gate: release build, fmt, clippy with warnings denied, all tests, and manual Zsh/Bash smoke tests pass without `fd`, `fzf`, `zoxide`, or `eza`.

## M1 — picker-quality navigation (complete)

Completion evidence: responsive Ratatui rendering, live Unicode query editing, background high-level Nucleo matching over the full candidate set, navigation and paging, inline-to-fullscreen fallback, lazy debounced preview I/O on a dedicated worker, safe open/copy/editor actions, Ctrl-R atomic refresh, explicit no-color/ASCII modes, TERM=dumb fallback, terminal restoration guards, render/input tests, real PTY live-search/resize/Ctrl-C restoration tests, and a reproducible five-sample median latency gate are implemented.

- [x] High-level Nucleo worker with incremental/background matching.
- [x] Ratatui inline picker, responsive wide/medium/small layouts, lazy debounced preview, and terminal restoration guard.
- [x] Keyboard and action layer: navigate, copy, OS open, editor, preview toggle, paging, refresh, resize, cancel.
- [x] NO_COLOR, ASCII, `TERM=dumb`, and fullscreen behavior.
- [x] First-run indexing transitions directly into the picker.
- [x] Golden render tests and real PTY tests for Ctrl-C, resize, and tiny terminals.

Exit gate: no render-loop matching or preview I/O, first useful paint and search latency measured, and terminal state restored after every tested exit path.

## M2 — learning and navigation sessions (complete)

Completion evidence: configurable frequency/recency/proximity/bookmark/project signals feed one score formula with JSON-visible components. History-backed prefix auto-resolution requires at least five visits, two candidates, a 1,000-point absolute margin, and a 30% relative margin. Duplicate exact names, close scores, fuzzy typos, stale leaders, short prefixes, forced-picker queries, multi-word queries, and smart case are covered by tests. Session history preserves the first origin, isolates shell sessions, truncates forward branches, and skips deleted entries in both directions. Explicit zoxide import fully validates output before an idempotent merge and never runs through a shell.

- [x] Frequency/recency/proximity ranking with explainable score components.
- [x] Conservative measured confidence margin; ambiguous and typo matches still open the picker.
- [x] Complete back/forward branch semantics per `DGO_SESSION_ID`.
- [x] Stale/deleted path handling and bookmark repair UX.
- [x] Safe import from documented `zoxide query --list --score` output.

Exit gate: adversarial ranking suite covers duplicate names, close scores, stale paths, multi-word queries, smart case, and typo cases without unsafe auto-navigation.

## M3 — operations and resilience (complete)

Completion evidence so far: state schema `0 → 1` migration preserves bookmarks; unsupported future schemas fail without overwrite. Invalid index files are timestamp-quarantined then rebuilt; invalid state files are timestamp-backed up then recreated. `doctor` reports integration, storage, missing/stale index, state health, platform actions, and oversized shell startup files. `dgo explain` exposes candidate score components without navigating.

- [x] Schema versions and tested migrations.
- [x] Corrupt index quarantine/rebuild; corrupt state timestamped backup with non-destructive recovery flow.
- [x] Doctor checks permissions, integration, stale index, platform actions, and common slow-hook symptoms.
- [x] Stats, explain, and local benchmark commands.
- [x] Concurrent readers/refresh, interrupted refresh, permission-denied, broken symlink, and symlink-cycle tests.

Exit gate: fault-injection matrix passes and user state is recoverable from every supported migration/corruption scenario.

## M4 — performance and compatibility (complete)

Current evidence: `dgo-fixture` creates an exact, non-overwriting breadth-first dataset up to 1M child directories. Criterion's index-crawl harness and an external cold/warm CLI harness are implemented. On the current macOS ARM64 host (Darwin 25.5.0, Rust 1.89.0, release `dgo 0.1.0`, five warm samples), external runs completed at 10k/100k/500k/1M. The shell PTY matrix passes Zsh, Bash, and Fish on macOS.

| Dataset | Cold refresh | Warm no-match |
| --- | ---: | ---: |
| 10k | 0.30 s | 0.06 s |
| 100k | 5.95 s | 0.18 s |
| 500k | 33.13 s | 0.63 s |
| 1M | 86.87 s | 1.12 s |

The 10k run and Criterion crawl baseline passed. Replacing redundant JSON index values with a schema-3 compact record format reduced a 1M cold refresh from 86.87 s to 46.82 s and context decoding from 826.190 ms to 637.389 ms. The interactive picker now opens before index decoding, then streams records into its background Nucleo matcher; closing it cancels the read walk. A collision-safe exact-basename lookup keeps direct unique navigation on the fast path. The fixture generator can checkpoint and resume bounded batches, which made the 1M gate reproducible under short-lived executors.

The release budget is **at most 100 ms to first paint and at most 100 ms to the first useful result at 1M indexed directories**. Fresh three-sample PTY evidence on the current macOS ARM64 host is:

| Dataset | First paint | First useful result |
| --- | ---: | ---: |
| 100k | 52.628 ms | 35.154 ms |
| 500k | 53.529 ms | 35.209 ms |
| 1M | 55.180 ms | 35.236 ms |

The Zsh/Bash/Fish wrapper matrix passes on both macOS and a clean Debian Linux container, including paths with spaces, quotes, Unicode, `..`, leading dashes, bookmarks, per-shell back/forward navigation, and an interactive ambiguous selection without stdout contamination. The Linux gate pins Tcl/Expect to a UTF-8 locale so the Unicode fixture itself is deterministic.

- [x] Synthetic fixture generator and parameterized Criterion index-crawl benchmark (10k/100k/500k/1M inputs).
- [x] External CLI latency script with environment metadata and reproducible cold/warm methodology.
- [x] Local stage measurements for context/database decoding, picker preparation, fuzzy resolution, and cold index throughput.
- [x] 1M interactive first-paint/first-result PTY measurement meets the explicit 100 ms / 100 ms release budget.
- [x] Complete Zsh/Bash/Fish PTY matrix passes on macOS and Linux.

Exit gate: budgets are met or documented with evidence; README contains only reproduced measurements and identifies hardware, dataset, and method.

## M5 — public release candidate

Current progress: the local release-candidate implementation is complete. CI runs Rust formatting, warnings-denied clippy, tests, release binaries, Criterion compilation, a disposable 100-directory external benchmark smoke, Linux PTY terminal/shell gates, and dependency policy checks on pull requests and `main`. Actions are immutable-SHA pinned and tracked by weekly cooldown updates. The public install surface contains only `dgo`; the fixture generator is feature-gated. The command surface, state-independent completion scripts, dynamic bookmark suggestions, support/privacy documentation, issue forms, offline package and install checks, and one non-publishing local release preflight are complete. CI deliberately records no runner latency claim.

- [x] Complete command help, state-independent completions with lazy bookmark suggestions, action detection, support page, privacy/security docs, changelog, contributing guide, dual license.
- [x] CI on macOS and Linux: fmt, clippy, tests, release build, and bench smoke; cargo-deny policy check.
- [x] Tag-gated native release workflow for four required targets plus aggregate SHA-256 checksums; Homebrew formula template without an invented tap owner.
- [ ] Produce the four archives on a release tag and verify every published checksum before declaring the release complete.
- [x] Launch checklist and privacy-aware issue templates.
- [x] Reproducible VHS demo source and rendered artifact.
- [ ] Manual accessibility/terminal compatibility pass on the release artifacts.
- Verify crates.io package-name availability immediately before publishing; binary remains `dgo` regardless of package name.

Exit gate: clean-machine install and upgrade/rollback drills pass, artifacts match checksums, documentation matches behavior, and there are no P0/P1 findings.

## Remaining path to `v0.1.1`

These are publication operations, not missing product implementation:

1. Commit the reviewed working tree; re-authenticate `gh` for `RudySource`.
2. Push the exact commit and require green macOS/Linux CI plus dependency policy.
3. Verify branch protection, private security advisories, and repository release permissions.
4. Run `cargo publish --dry-run --locked` and confirm the crates.io owner/name state.
5. Complete the manual terminal and accessibility matrix on the built candidate.
6. Create only tag `v0.1.1`; let the tag workflow build all four native archives and `SHA256SUMS`. The immutable `v0.1.0` tag records a stopped Windows gate and has no release artifacts.
7. Independently verify downloads and run clean install, upgrade, and rollback on macOS, Linux, and Windows.
8. Publish the Homebrew formula and crates.io package only after their real URLs, owners, versions, and checksums exist.

## Planned updates after `0.1.1`

### `0.1.x` — stabilization

- Reduce duplicate `syn`, `hashbrown`, and benchmark-only `itertools` versions when possible without silently raising MSRV.
- Convert the first manual Windows runtime matrix into repeatable native CI coverage.
- Fix verified compatibility defects and improve diagnostics without changing the navigation contract or persistent schema unnecessarily.

### `0.2` — portable distribution and recovery

- Maintainer-owned package definitions for Homebrew, Scoop/Winget, AUR, and Nix, each with clean install/upgrade/rollback evidence.
- Explicit export/import backup for bookmarks and history, with schema validation and a no-overwrite preview.
- Guided shell setup/unsetup commands that print changes first and never edit startup files without explicit confirmation.

### `0.3` — organization and ranking

- Bookmark tags, tag-aware filtering, and explainable tag ranking.
- Optional project metadata and saved scopes without adding Git operations or file management.
- User-controlled ranking presets backed by the same visible score breakdown and ambiguity safety gates.

### Later, only with measured demand

- Optional external picker adapters.
- Compact mmap snapshot only if profiling shows redb decoding misses the 1M-entry budget.
- Incremental/watch refresh only if filesystem churn measurements justify its complexity and resource cost.

Explicitly excluded: file management, Git operations, terminal multiplexing, directory listing replacement, plugins that execute arbitrary shell text, telemetry, analytics, and network activity during normal use.
