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

## M1 — picker-quality navigation (current)

Current progress: responsive Ratatui rendering, live Unicode query editing, background high-level Nucleo matching over the full candidate set, navigation and paging, inline-to-fullscreen fallback, lazy debounced preview I/O on a dedicated worker, safe open/copy/editor actions, terminal restoration guards, render/input tests, and a real PTY live-search smoke test are implemented. Ctrl-R refresh behavior, latency benchmarks, and remaining exit-path tests are open.

- [x] High-level Nucleo worker with incremental/background matching.
- [x] Ratatui inline picker, responsive wide/medium/small layouts, lazy debounced preview, and terminal restoration guard.
- Keyboard and action layer: navigate, copy, OS open, editor, preview toggle, paging, resize, cancel.
- NO_COLOR, ASCII, `TERM=dumb`, and fullscreen behavior.
- First-run indexing transitions directly into the picker.
- Golden render tests and real PTY tests for Ctrl-C, resize, and tiny terminals.

Exit gate: no render-loop matching or preview I/O, first useful paint and search latency measured, and terminal state restored after every tested exit path.

## M2 — learning and navigation sessions

- Frequency/recency/proximity ranking with explainable score components.
- Conservative measured confidence margin; ambiguous and typo matches still open the picker.
- Complete back/forward branch semantics per `DGO_SESSION_ID`.
- Stale/deleted path handling and bookmark repair UX.
- Safe import from documented `zoxide query --list --score` output.

Exit gate: adversarial ranking suite covers duplicate names, close scores, stale paths, multi-word queries, smart case, and typo cases without unsafe auto-navigation.

## M3 — operations and resilience

- Schema versions and tested migrations.
- Corrupt index quarantine/rebuild; corrupt state timestamped backup with non-destructive recovery flow.
- Doctor checks permissions, integration, stale index, platform actions, and common slow-hook symptoms.
- Stats, explain, and local benchmark commands.
- Concurrent readers/refresh, interrupted refresh, permission-denied, broken symlink, and symlink-cycle tests.

Exit gate: fault-injection matrix passes and user state is recoverable from every supported migration/corruption scenario.

## M4 — performance and compatibility

- Synthetic fixture generator and Criterion benchmarks at 10k/100k/500k/1M directories.
- External CLI latency script with environment metadata and reproducible cold/warm methodology.
- Profile startup, database decoding, matcher ingestion, allocations, and index throughput before optimizing.
- Zsh, Bash, and Fish PTY integration matrix on macOS and Linux, including spaces, quotes, Unicode, `..`, `-`, bookmarks, and sessions.

Exit gate: budgets are met or documented with evidence; README contains only reproduced measurements and identifies hardware, dataset, and method.

## M5 — public release candidate

- Complete command help, completions with dynamic suggestions, action detection, support page, privacy/security docs, changelog, contributing guide, dual license.
- CI on macOS and Linux: fmt, clippy, tests, release build, bench smoke, cargo-deny.
- `cargo-dist` artifacts and checksums for four required targets; Homebrew tap template without invented owner.
- Reproducible VHS demo, launch checklist, issue templates, and manual accessibility/terminal compatibility pass.
- Verify crates.io package-name availability immediately before publishing; binary remains `dgo` regardless of package name.

Exit gate: clean-machine install and upgrade/rollback drills pass, artifacts match checksums, documentation matches behavior, and there are no P0/P1 findings.

## Post-1.0 candidates

- Optional external picker adapters.
- Bookmark tags and tag-aware ranking.
- Additional package managers (AUR, Nix) after maintainers are available.
- Compact mmap snapshot only if profiling shows redb decoding misses the 1M-entry budget.

Explicitly excluded: file management, Git operations, terminal multiplexing, directory listing replacement, plugins that execute arbitrary shell text, telemetry, analytics, and network activity during normal use.
