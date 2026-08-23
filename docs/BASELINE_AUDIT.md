# Baseline audit

Date: 2026-08-22

## Found

- `README.md` documents an alpha Zsh implementation backed by `fd`, `fzf`, optional `zoxide`, and flat files.
- The documented command vocabulary already includes direct paths, global/local search, forced selection, bookmarks, actions, refresh, and doctor.
- The README explicitly preserves a cheap direct-path path and avoids expensive `chpwd` work.

## Not found

- No Zsh implementation, Rust source, tests, configuration, license, or release automation was present.
- At audit time, the directory was not a Git repository and contained no implementation. It was subsequently initialized on `main`; commit `284a316` preserves that supplied-document baseline for differential review using canonical GitHub noreply author metadata.

## Migration decision

The old command vocabulary is the compatibility surface. Its implementation claims are not treated as working functionality because their source is absent. Dirgo is rebuilt as one Rust binary with generated shell wrappers; `fd`, `fzf`, `zoxide`, and `eza` are no longer runtime requirements.

## Product corrections to the supplied brief

- `dgo query` is the stable machine-readable boundary; normal shell navigation uses hidden `__resolve` so human output cannot contaminate command substitution.
- Newline-containing paths are rejected at the shell boundary because command substitution cannot carry them safely. Spaces, quotes, Unicode, brackets, emoji, and leading dashes remain supported.
- An ambiguous fuzzy match never navigates automatically. Initial releases auto-resolve only explicit paths, exact bookmarks, and unique exact basenames; confidence-based history auto-resolution ships only after measured tuning.
- Index and user state are physically separate. Index recovery may rebuild automatically; state recovery must retain a backup and stop with an actionable error.
- No release performance number is publishable until measured on generated 10k/100k/500k/1M fixtures.
