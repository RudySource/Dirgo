<div align="center">

# Dirgo

### Go anywhere. Instantly.

A fast, fuzzy directory navigator for your terminal.

</div>

```console
$ dgo punk
~/Developer/Projects/Punk
```

Unlike history-only jumpers, Dirgo indexes the filesystem and can find directories you have never visited. History then improves ranking without becoming a discovery requirement.

> [!IMPORTANT]
> Dirgo is under active development toward `0.1.0`. The trustworthy core and live Ratatui search loop work; actions, production benchmarks, and release artifacts are tracked in [ROADMAP.md](ROADMAP.md) and are not claimed as complete.

## What works now

- one Rust binary named `dgo`;
- parallel filesystem indexing with `ignore`, Git ignore support, configurable roots, and excludes;
- crash-safe index publication and a single-refresh lock;
- separate redb databases for disposable index data and persistent user state;
- direct paths, exact bookmarks, unique exact basenames, smart-case fuzzy candidates, and conservative ambiguity handling;
- bookmarks, visits, project-root detection, repository search, recent directories, and session back/forward state;
- generated Zsh, Bash, and Fish wrappers with a no-process direct-path fast path;
- responsive Ratatui picker with live Unicode query editing, background Nucleo matching, inline/fullscreen fallback, keyboard selection, lazy debounced directory preview, terminal restoration, and non-TTY fallback;
- safe OS open, clipboard, and editor actions without shell interpolation;
- JSON query output, config inspection, doctor, stats, and shell completion stubs;
- no telemetry, analytics, network calls, `fd`, `fzf`, `zoxide`, or `eza` dependency at runtime.

The picker owns the full candidate set and updates matches in the background as the query changes. Its preview reads at most 20 top-level entries on a separate worker after the selection settles. Action shortcuts and measured latency gates remain M1 work.

## Build and install locally

Rust 1.85 or newer is required.

```bash
cargo build --release
install -m 755 target/release/dgo ~/.local/bin/dgo
```

Load the wrapper for your shell:

```zsh
eval "$(dgo init zsh)"
```

```bash
eval "$(dgo init bash)"
```

```fish
dgo init fish | source
```

The wrapper is essential: a child process cannot change its parent shell's working directory. Existing paths such as `..`, `./src`, `~/Projects`, and `-` are handled directly by the shell builtin without starting the Rust binary.

## 30-second start

```bash
dgo refresh              # build the index explicitly
dgo punk                 # unique exact directory name
dgo                      # choose from the index
dgo . api                # restrict candidates to the current tree
dgo ? api                # always ask when names collide
dgo +work                # bookmark the current directory
dgo @work                # navigate to it
dgo root                 # nearest project root
dgo repo punk            # indexed project roots only
dgo recent               # Dirgo navigation history
dgo back                 # per-shell navigation session
dgo --open punk          # open in the OS file browser
dgo --code punk          # open in the configured editor
dgo --copy punk          # copy the selected path
dgo --print punk         # print without navigating
```

On the first search, a missing index is built automatically. `dgo refresh` remains useful after filesystem changes.

## Resolution safety

Dirgo automatically resolves only:

1. an existing explicit path;
2. an exact bookmark;
3. a unique exact basename.

Fuzzy and duplicate-name matches require selection. This is deliberate: opening a picker once is safer than silently changing to the wrong project.

## Commands

```text
dgo [QUERY]...                  resolve or choose a directory
dgo init <zsh|bash|fish>        print shell integration
dgo refresh                    rebuild the filesystem index
dgo query <QUERY> [--json]     machine-readable resolution
dgo root                       find the nearest project root
dgo repo [QUERY]               choose among project roots
dgo recent [QUERY]             choose from Dirgo history
dgo back | forward             navigate the current shell session
dgo bookmarks                  list bookmarks
dgo bookmark add NAME          create a bookmark
dgo bookmark remove NAME       remove a bookmark
dgo bookmark rename OLD NEW    rename a bookmark
dgo config path | show         inspect configuration
dgo doctor                     check local health
dgo stats                      show local-only statistics
```

Compatibility forms retained from the original prototype documentation include `--refresh`, `-r`, `--doctor`, `--bookmarks`, `--forget NAME`, `+name`, and `@name`.

## Configuration

Dirgo follows XDG environment variables and reads:

```text
${XDG_CONFIG_HOME:-~/.config}/dirgo/config.toml
```

Example:

```toml
schema_version = 1
roots = ["~/Developer", "~/Projects"]

ignore = [".git", "node_modules", "Library", ".cache", "target", "dist"]
respect_gitignore = true
follow_symlinks = false

[ranking]
frequency = 1.0
recency = 0.85
proximity = 0.55
bookmarks = 1.25
projects = 0.30

[ui]
preview = true
accent = "cyan"
icons = "auto"
height_percent = 70

[actions]
editor = "auto"
```

`actions.editor` accepts one executable name or path, not a shell command line. In the picker, supported actions are shown in the footer and use `Ctrl-O`, `Ctrl-Y`, and `Ctrl-E`.

The disposable index is stored below `XDG_CACHE_HOME`; bookmarks, visits, and sessions are stored separately below `XDG_STATE_HOME`.

## Machine-readable contract

```bash
dgo query punk --json
```

Resolved paths are written to stdout without decoration. Diagnostics and selector UI use stderr. Exit codes currently used by the resolver are `0` success, `3` no match, and `4` ambiguous/cancelled; Clap uses `2` for invalid arguments.

Paths with spaces, quotes, Unicode, brackets, emoji, and leading dashes are supported. Paths containing a newline are intentionally rejected at the shell boundary because command substitution cannot transport them safely. NUL is not valid in Unix paths.

## Privacy and security

Dirgo works entirely locally. It contains no telemetry or analytics and makes no network request during normal use. The index contains local filesystem paths, so protect the XDG cache/state directories as you would other local application data.

Filesystem paths are never interpolated into shell commands or passed through `eval`. See [SECURITY.md](SECURITY.md) for reporting guidance.

## Performance

The architecture keeps direct paths in the shell and performs no `chpwd` index scan. No benchmark numbers are published yet. Reproducible 10k/100k/500k/1M fixtures and release performance gates are specified in [ROADMAP.md](ROADMAP.md); measurements will be published only after those tools land.

## Project documents

- [Baseline audit](docs/BASELINE_AUDIT.md)
- [Domain language](CONTEXT.md)
- [Release roadmap](ROADMAP.md)
- [Contributing](CONTRIBUTING.md)
- [Changelog](CHANGELOG.md)
- [Support](SUPPORT.md)

## License

Dirgo is licensed under either Apache License 2.0 or MIT, at your option.
