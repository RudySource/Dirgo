# Changelog

All notable changes follow Keep a Changelog and Semantic Versioning.

## [Unreleased]

## [0.2.0] - 2026-08-24

### Added

- Add `dgo setup` for previewable, idempotent Zsh, Bash, and Fish onboarding with explicit confirmation, timestamped backups, atomic startup-file updates, local receipts, repair, and managed-block removal.
- Add one-command macOS/Linux and Windows installers that detect the platform, verify SHA-256 before installation, avoid administrator privileges, and ask before changing shell or user `PATH` configuration.
- Publish stable installer asset names alongside immutable versioned archives and attach GitHub artifact attestations to every release asset.
- Exercise the Unix installer end to end on macOS and Linux CI and the PowerShell installer on native Windows CI.

### Changed

- Reduce first-run onboarding to one paste and one reviewable confirmation; an index is still built automatically on the first search.
- Route `setup` through generated shell wrappers and expose it in Zsh, Bash, and Fish completions.

### Security

- Refuse non-interactive shell-file changes without explicit `--yes`, preserve symlinked dotfile layouts, reject malformed or duplicate managed blocks, and abort if the startup file changes between preview and write.
- Pin the release provenance action to an immutable commit and keep installer downloads HTTPS-only unless an explicit test mirror is provided.

## [0.1.3] - 2026-08-24

### Security

- Escape terminal control characters and bidirectional overrides in every human-facing candidate, preview, diagnostic, bookmark, and error path while preserving the original filesystem path for navigation and machine-readable output.

### Fixed

- Make `dgo doctor` explain malformed configuration instead of failing before diagnostics, and keep `dgo config path` plus `dgo support` available when configuration or storage is broken.
- Add concise descriptions to the top-level and nested Clap help so `dgo --help` explains actual workflows instead of listing blank command names.
- Update navigation counters and bookmark-renaming collision checks inside single redb write transactions so concurrent invocations cannot lose visits or overwrite a bookmark.

### Changed

- Bound persistent history to 50,000 rows with batched strength/recency pruning, retain at most 256 transitions per shell session, and prune abandoned session records in batches.
- Expand the README with verified shell persistence guidance, picker keys, platform limits, troubleshooting, uninstall, security behavior, and contributor onboarding.

## [0.1.2] - 2026-08-24

### Fixed

- Build the Linux GNU archive on Ubuntu 22.04 and reject binaries requiring symbols newer than glibc 2.35. The superseded 0.1.1 Linux archive was built on Ubuntu 24.04 and required glibc 2.39, so it could not run on Debian 12.

## [0.1.1] - 2026-08-24

### Fixed

- Kept the Unix shell integration suite Unix-only and added a native Windows CLI smoke suite, so the four-platform release gate compiles and exercises tests appropriate to each operating system.
- Upgraded Ratatui/Crossterm and the transitive `lru` dependency to patched versions, removed the obsolete `paste` exception, and avoided a fullscreen teardown cursor-position timeout.

### Added

- Initial Rust core with XDG configuration, redb index/state separation, parallel crawling, conservative resolution, bookmarks, history, project detection, generated shell wrappers, diagnostics, and tests.
- Live project-local crawling, atomic refresh preservation tests, and real Zsh/Bash wrapper coverage.
- Ratatui picker foundation with responsive layouts, terminal-mode guards, fullscreen fallback, render tests, and PTY smoke coverage.
- Live Unicode query editing backed by a high-level Nucleo worker, with basename/path modes and background candidate injection.
- Debounced asynchronous directory preview with project-marker detection and a bounded, non-recursive top-level listing.
- Cross-platform open, clipboard, and editor actions with capability-aware TUI shortcuts and no shell interpolation.
- Ctrl-R atomic index refresh plus PTY gates for Ctrl-C terminal restoration and tiny-terminal resize.
- Explicit `--no-color` and `--no-unicode` compatibility modes with ASCII-safe picker rendering.
- Reproducible local M1 latency measurement for warm first paint and first useful live result.
- Explainable candidate score components with configurable frequency, recency, proximity, bookmark, and project weights.
- Conservative history-backed prefix auto-resolution with absolute and relative confidence margins; duplicate exact names, fuzzy typos, close scores, stale paths, and forced-picker queries remain interactive.
- Browser-style per-shell navigation history that preserves the first origin, isolates sessions, truncates abandoned forward branches, and skips deleted entries.
- Explicit, validated, idempotent zoxide score import without a runtime dependency or shell execution.
- State schema migration, non-destructive timestamped storage recovery, expanded doctor diagnostics, and `dgo explain` ranking introspection.
- Local `dgo bench` measurements plus fault-injection coverage for interrupted refresh, concurrent readers, unreadable roots, broken symlinks, and symlink cycles.
- Deterministic non-overwriting fixture generator, Criterion index-crawl harness, external cold/warm CLI benchmark harness, and PTY shell compatibility matrix with explicit Fish skips.
- Compact index record format (schema 3) removes redundant JSON fields and adds a collision-safe exact-basename lookup; older disposable indexes rebuild automatically while a future unknown index requires explicit refresh.
- Interactive picker now opens before decoding a large index, streams candidate construction directly into its background matcher, and stops the index walk promptly when closed.
- macOS/Linux CI now verifies formatting, warnings-denied clippy, tests, release binaries, benchmark-harness smoke, and dependency policy.
- Resumable, checkpointed fixture batches make the 1M interactive performance gate reproducible without adopting arbitrary existing or symlinked directories.
- Cross-platform PTY coverage now exercises Zsh, Bash, and Fish on Linux in a deterministic UTF-8 locale.
- Current cargo-deny policy validates advisories, licenses, sources, and bans without advisory exceptions.
- Tag-gated native release automation builds and tests four targets, packages documentation and licenses, verifies SHA-256 checksums, and publishes only after every matrix job succeeds.
- Reproducible VHS source and a rendered terminal demo cover refresh, exact navigation, bookmarks, and ambiguous picker selection.
- Shell command substitution now forces the picker to a stdout-safe fullscreen backend, preventing Crossterm's cursor-position request from prefixing the selected destination; all three real wrapper PTY gates exercise the regression.
- The benchmark fixture executable is feature-gated so normal `cargo install dirgo` installs only the public `dgo` command.
- Windows open and clipboard actions use native commands with the selected path passed as data, never interpolated into a shell command.
- Bookmark repairs canonicalize relative inputs and preserve existing metadata; renames refuse to overwrite another bookmark.
- Missing index schema markers trigger normal quarantine/rebuild recovery, and unsupported non-UTF-8 indexed paths are skipped instead of being stored lossily.
- `dgo doctor` reports an unhealthy index with recovery guidance instead of aborting before the remaining diagnostics.
- CI and release actions are pinned to immutable revisions and tracked by weekly dependency updates with a seven-day cooldown.
- Demo fixtures use a neutral public temp prefix so rendered assets do not expose a machine-specific macOS TMPDIR identifier.
- Shareable benchmark reports omit the machine hostname and use a neutral `/tmp` fixture prefix.
- The tag workflow publishes version-matched, privacy-aware release notes with installation, verification, and known limitations.

### Changed

- Replaced the undocumented external-tool prototype architecture with a single-binary design.
- Raised the declared MSRV to Rust 1.89 to match the locked dependency graph and clean Linux builds.
