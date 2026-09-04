# Changelog

All notable changes follow Keep a Changelog and Semantic Versioning.

## [Unreleased]

## [0.8.0] - 2026-09-04

### Added

- Add separately opt-in Workflow Intelligence with exact one- and two-command transition evidence, canonical project/session isolation, and deterministic `NEXT` suggestions in Zsh, Bash, Fish, and PowerShell 7+.
- Add saved 2–8 step workflows plus `dgo workflows enable|disable|status|next|list|show|save|rename|remove|clear-learned|export` management commands.
- Add a bounded Workspace Palette Workflows source after Tasks and before Git, with full-sequence preview and one-step insertion.

### Changed

- Migrate command history atomically from schema v2 to v3 without rewriting retained events or aggregates; learned transitions are rebuildable while saved workflows remain user-owned.
- Replace detached per-selection Palette preview threads with one session-owned latest-request worker that joins on close.
- Extend `dgo doctor` and `dgo stats` with workflow enablement, schema, rebuild, and bounded learned/saved counts without printing command text or project paths.
- Make interactive `dgo --version` render cached state immediately, then start or observe one detached refresh when the last successful check is stale or missing. Redirected version output remains exactly one line and performs no update-state work.
- Treat successful release knowledge, cache freshness, and refresh scheduling as separate states so `dgo --version`, navigation notices, and `dgo doctor` report the same facts.

### Fixed

- Keep a known newer stable release visible even when its cached result is stale instead of replacing it with a generic stale label.
- Retry failed fetches after a 15-minute bounded backoff and failed process starts after at most 60 seconds instead of suppressing checks for 24 hours.
- Use a five-minute exclusive attempt lease so concurrent shells start at most one update checker and clock rollback or malformed state cannot create an unlimited delay.
- Classify update-check lock contention portably on Unix and Windows.

### Security

- Keep workflow inference separately disabled by default, require existing command-history consent, and never introduce a run/execute/retry path.
- Require evidence from at least three observations and two distinct sessions; reject privacy gaps, cross-project/session transitions, controls, bidi overrides, likely credentials, and configured deny-patterns.
- Publish versioned workflow JSONL atomically with private permissions, redact project paths by default, reject symlink destinations, and require `--force` before replacement.
- Keep update cache and attempt state bounded, private, atomic where published, and resistant to symlink or non-file state paths; malformed notification markers no longer silently disable checking.

## [0.7.1] - 2026-09-01

### Added

- Add a copy-ready Command Prompt bootstrap command for the existing verified PowerShell installer, while keeping PowerShell 7+ as the supported interactive Windows shell.

### Changed

- Publish each GitHub Release with the curated notes for that exact version from this changelog instead of a bare generated commit comparison.
- Validate release-note extraction and Windows installation documentation in the release preflight so future releases cannot silently lose their user-facing changelog.

## [0.7.0] - 2026-09-01

### Added

- Add `dgo palette` and an `Alt+P` Workspace Palette for files, project tasks, Git branches and worktrees, Compose services, bookmarks, and indexed projects.
- Add live All/Files/Tasks/Git/Compose/Places source switching over one bounded snapshot, plus debounced lazy previews and adaptive color/Unicode fallbacks.
- Add `dgo roots list|add|remove` with comment-preserving atomic configuration edits, focused-root diagnostics, and optional deferred refresh.
- Add ordered path-segment search for `/` and `\\`, including omitted intermediate folders and focused roots below normally ignored parents.
- Add cached update status to interactive `dgo --version` while preserving exact one-line output in pipes, plus existing notification on/off controls.

### Changed

- Extend `dgo doctor` and `dgo stats` with root, focused-root, Palette-provider, and local update-state reporting.
- Collect Palette providers concurrently under per-source time and item budgets; filtering and source switching never rerun discovery.

### Security

- Return versioned Palette action frames that either navigate literally or replace the editor buffer; Task, Compose, and Git branch selections never autoexecute.
- Quote structured Git commands per shell, reject controls and newlines at the shell boundary, and avoid `eval` and `Invoke-Expression` for Palette results.
- Publish update cache and notification state atomically with private permissions and reject symlink targets.
- Keep focused roots explicit and narrow; Dirgo does not automatically crawl all of `Library`, `AppData`, `/`, system folders, or ignored heavy directories.

### Fixed

- Clear the Palette query buffer after navigation so a bookmark query cannot become the next accidental command.
- Keep release-version labels derived from Cargo metadata instead of hard-coding the previous minor version in picker tests.
- Offer options immediately after a completed leaf command and keep an exact option such as `git commit -m` visible as a safe trailing-space insertion.
- Show a contextual path for directory suggestions so equal basenames remain distinguishable in the live panel.

## [0.6.0] - 2026-08-29

### Added

- Add the opt-in Context Engine with completed-command events containing available cwd, project root, exit status, duration, and shell session context.
- Add project-scoped, success-aware command ranking with neutral treatment for legacy unknown outcomes and bounded cwd/session boosts.
- Add `dgo suggestions history status`, scoped `list`, event `inspect`, scoped `clear`, and versioned JSONL `export` commands.

### Changed

- Migrate command history atomically to schema v2 while retaining legacy command counts and timestamps without inventing missing context.
- Capture completed commands through bounded stdin frames in Zsh, Bash 4+, Fish, and PowerShell 7+, preserving existing prompt and completion hooks.

### Security

- Reject likely secrets and leading-space private-history commands before opening history storage; keep history independently opt-in and local.
- Refuse unknown future schemas and symlink database/export targets, preserve malformed v1 databases as private recovery copies, and omit paths from exports unless `--include-paths` is explicit.
- Publish exports through a private same-directory temporary file and require `--force` before replacing an existing destination.

## [0.5.1] - 2026-08-27

### Fixed

- Keep the Unix-only project-cache symlink test import out of Windows builds so the strict cross-platform Clippy gate passes.

## [0.5.0] - 2026-08-26

### Added

- Add project-scoped `PROJ` suggestions for npm, pnpm, Yarn, and Bun scripts; Cargo workspace packages, binaries, examples, and features; simple Make targets, Just recipes, and Compose services.
- Add compact source-aware descriptions to project commands across Zsh, Bash, Fish, and the native PowerShell predictor.
- Add a bounded local project-command cache with content fingerprints, atomic snapshots, background refresh, and per-project invalidation.
- Open the current directory immediately with `dgo --open`, accept full directory paths directly, and surface existing paths outside the index inside the interactive picker.

### Changed

- Prefer commands declared by the current project over matching global command-history entries while preserving deterministic paging and deduplication.
- Recognize Just and Compose files as project-root markers and isolate malformed optional manifests instead of suppressing other project commands.
- Restyle the shared suggestion picker and Zsh live panel with responsive terminal-native chrome, explicit actions, accessible text labels, and color/Unicode fallbacks.

### Fixed

- Keep the Zsh redraw guard recoverable when a line is interrupted during delayed expansion, remove owned `POSTDISPLAY` content literally without duplicating panels, and preserve project source labels in wide rows.

### Security

- Parse manifests as bounded data without invoking package managers, Cargo, Make, Just, Docker, shell completion scripts, or manifest-defined commands.
- Keep manifest bodies out of suggestion descriptions, restrict inserted task identifiers to portable text, and store at most 64 private local project snapshots.

## [0.4.0] - 2026-08-26

### Added

- Add opt-in, local shell-native suggestions for indexed directories, navigation history, executables, filesystem entries, and separately opt-in command history.
- Add a debounced, selectable 5–12 row live panel for Zsh, safe `Ctrl+F` insertion, and a textual `Shift+Tab` picker; accepting a suggestion never submits the command line.
- Enrich the native Fish and Bash 4+ completion menus while preserving their line editors and existing completion ownership.
- Add a compiled PowerShell 7.4.x PSReadLine predictor with inline and ListView modes, a bounded per-session worker, command-history feedback, and a safe `Ctrl+F` fallback on other PowerShell 7 versions.
- Complete `dgo sug` to `dgo suggestions`, `dgo --upd` to `dgo --update`, and discover bounded executable names from `PATH` without invoking them.
- Add a versioned, bounded NDJSON suggestion protocol, local privacy filters, retention controls, diagnostics, and independent history clearing.
- Add real-shell PTY coverage for Zsh, Bash, and Fish plus native Windows integration, predictor packaging, and installer smoke coverage.

### Changed

- Add parent-shell navigation, setup, completions, and suggestion integration for PowerShell 7+ on Windows.
- Package the PowerShell predictor with Windows archives and install it beside `dgo.exe` after checksum verification.
- Make the suggestion list readable without color or Unicode and identify every source with a text label.

### Security

- Keep suggestions disabled by default, keep command-history collection independently disabled by default, and reject likely credentials, control characters, bidirectional overrides, oversized frames, and stale text edits.
- Send command buffers over standard input or inherited private files instead of process arguments, environment values, shell evaluation, or command interpolation.

## [0.3.1] - 2026-08-25

### Added

- Add `dgo --update`, which selects Homebrew, Cargo, Scoop, or the verified release installer from the active executable path.
- Check for stable GitHub releases in a detached daily background task and show a cached, non-blocking update notice on later navigation commands.
- Add persistent `dgo update-notifications off|on` controls for the update notice.

### Changed

- Make Shift-Up/Shift-Down the primary visible directory-content scrolling controls while retaining Ctrl-B/Ctrl-F as alternatives.

### Fixed

- Exercise real shifted-arrow terminal sequences in the PTY release gate, including scrolling to both boundaries without moving the selected directory.

## [0.3.0] - 2026-08-24

### Added

- Allow `--open`, `--finder`, `--code`, `--copy`, and `--print` both before and after a directory query, including through the generated Zsh, Bash, and Fish wrappers.
- Render and optionally publish `RudySource/homebrew-tap` updates automatically from the checksums of a successfully published GitHub release.
- Publish the official `RudySource/scoop-bucket` as the supported Scoop installation source.
- Scroll long directory previews reliably with Ctrl-B/Ctrl-F paging, plus shifted navigation keys in terminals that preserve those modifiers, without moving the result selection.

### Changed

- Bound the inline picker to useful content height and apply the configured preview and height preferences.
- Keep GitHub releases successful when the optional cross-repository Homebrew token is not configured, with an explicit workflow notice and manual update path.

### Fixed

- Return the cursor to the picker's origin after inline teardown so the next shell prompt does not leave a large block of empty terminal rows.
- Keep up to 200 sorted preview entries and report any omitted remainder instead of silently hiding entries beyond the old 20-item limit.
- Reject conflicting hidden-resolver action flags even when they appear on opposite sides of the positional separator.
- Restore the visible cursor from the terminal guard even if picker teardown exits through an error path.

## [0.2.1] - 2026-08-24

### Changed

- Add a repository-hygiene gate that rejects tracked secrets, personal machine paths, private contact data, and internal agent artifacts before CI or release packaging.
- Publish the completed 0.2 release status and separate optional future updates from current product requirements.
- Update the security policy to reflect the supported public release line and private GitHub reporting flow.

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
