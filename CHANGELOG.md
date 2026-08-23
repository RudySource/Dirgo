# Changelog

All notable changes follow Keep a Changelog and Semantic Versioning.

## [Unreleased]

### Added

- Initial Rust core with XDG configuration, redb index/state separation, parallel crawling, conservative resolution, bookmarks, history, project detection, generated shell wrappers, diagnostics, and tests.
- Live project-local crawling, atomic refresh preservation tests, and real Zsh/Bash wrapper coverage.
- Ratatui picker foundation with responsive layouts, terminal-mode guards, fullscreen fallback, render tests, and PTY smoke coverage.
- Live Unicode query editing backed by a high-level Nucleo worker, with basename/path modes and background candidate injection.
- Debounced asynchronous directory preview with project-marker detection and a bounded, non-recursive top-level listing.
- Cross-platform open, clipboard, and editor actions with capability-aware TUI shortcuts and no shell interpolation.

### Changed

- Replaced the undocumented external-tool prototype architecture with a single-binary design.
