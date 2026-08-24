<div align="center">

# Dirgo

### Go anywhere. Instantly.

A fast, fuzzy directory navigator for your terminal.

</div>

![Dirgo terminal demo](docs/assets/dirgo-demo.gif)

```console
$ dgo punk
~/Developer/Projects/Punk
```

Unlike history-only jumpers, Dirgo indexes the filesystem and can find directories you have never visited. History then improves ranking without becoming a discovery requirement.

> [!IMPORTANT]
> Dirgo `0.1.2` supersedes `0.1.1`. The earlier Linux archive accidentally required glibc 2.39; `0.1.2` is built and gated against glibc 2.35 for compatibility with current mainstream distributions.

## What works now

- one Rust binary named `dgo`;
- parallel filesystem indexing with `ignore`, Git ignore support, configurable roots, and excludes;
- crash-safe index publication and a single-refresh lock;
- separate redb databases for disposable index data and persistent user state;
- direct paths, exact bookmarks, unique exact basenames, smart-case fuzzy candidates, and conservative ambiguity handling;
- configurable frequency, recency, proximity, bookmark, and project ranking signals with per-candidate score components in JSON output;
- bookmarks, visits, project-root detection, repository search, recent directories, and session back/forward state;
- generated Zsh, Bash, and Fish wrappers with a no-process direct-path fast path;
- responsive Ratatui picker with live Unicode query editing, background Nucleo matching, inline/fullscreen fallback, keyboard selection, lazy debounced directory preview, terminal restoration, and non-TTY fallback;
- safe OS open, clipboard, and editor actions without shell interpolation;
- JSON query output, config inspection, doctor, stats, and state-independent Zsh/Bash/Fish completions with lazy bookmark suggestions;
- no telemetry, analytics, network calls, `fd`, `fzf`, `zoxide`, or `eza` dependency at runtime.

The picker opens immediately and updates matches in the background as the query changes. For large indexes it decodes and injects records on a cancellable worker, while a compact collision-safe lookup preserves unique exact-basename navigation without scanning the whole index. Its preview reads at most 20 top-level entries on a separate worker after the selection settles. `Ctrl-R` closes the picker, rebuilds the index atomically, and reports the result.

## Install

With Homebrew:

```bash
brew install RudySource/tap/dirgo
```

Download the archive for your platform and `SHA256SUMS` from the [latest GitHub Release](https://github.com/RudySource/Dirgo/releases/latest). Verify the checksum before extracting:

```bash
# Linux
sha256sum --check SHA256SUMS

# macOS
shasum -a 256 -c SHA256SUMS
```

On Windows, compare `Get-FileHash -Algorithm SHA256 <archive>` with `SHA256SUMS`. Extract `dgo` (`dgo.exe` on Windows) into a directory on `PATH`.

To build from source instead, Rust 1.89 or newer is required. This is the actual minimum supported by the locked dependency graph and is verified by the Linux clean-build gate.

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

Install matching command completion after the wrapper. Generating completion text never opens or writes Dirgo state; bookmark-name suggestions are read lazily only while completing bookmark arguments.

```zsh
source <(dgo completions zsh)
```

```bash
source <(dgo completions bash)
```

```fish
dgo completions fish | source
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
dgo --no-color           # preserve hierarchy without ANSI color
dgo --no-unicode         # use ASCII-only picker symbols
```

On the first search, a missing index is built automatically. `dgo refresh` remains useful after filesystem changes.

## Resolution safety

Dirgo automatically resolves only:

1. an existing explicit path;
2. an exact bookmark;
3. a unique exact basename.
4. a basename prefix backed by at least five visits when two or more prefix candidates exist and the leader clears both a 1,000-point and 30% score margin.

Fuzzy typos, duplicate exact names, close scores, stale paths, short prefixes, and forced `?` queries always require selection. This is deliberate: opening a picker once is safer than silently changing to the wrong project. Ranked-prefix responses expose their measured confidence and source in JSON.

## Commands

```text
dgo [QUERY]...                  resolve or choose a directory
dgo init <zsh|bash|fish>        print shell integration
dgo refresh                    rebuild the filesystem index
dgo query <QUERY> [--json]     machine-readable resolution
dgo explain <QUERY>            inspect ranked candidates and score components
dgo bench [--query TEXT]       measure local context, picker, and fuzzy-search work
dgo root                       find the nearest project root
dgo repo [QUERY]               choose among project roots
dgo recent [QUERY]             choose from Dirgo history
dgo back | forward             navigate the current shell session
dgo bookmarks                  list bookmarks
dgo bookmark add NAME          create a bookmark
dgo bookmark remove NAME       remove a bookmark
dgo bookmark rename OLD NEW    rename a bookmark
dgo import zoxide              explicitly import local zoxide scores
dgo config path | show         inspect configuration
dgo doctor                     check local health
dgo stats                      show local-only statistics
```

If a bookmark target was deleted or moved, Dirgo reports both repair commands. Re-running `dgo bookmark add NAME --path NEW_DIRECTORY` updates the bookmark in place; `dgo bookmark remove NAME` removes it. Back/forward navigation automatically skips deleted session entries.

`dgo import zoxide` is optional and explicit. It invokes `zoxide query --list --score` directly without a shell, validates the complete output before writing, accepts only finite bounded scores and existing absolute directories, and merges with `max(existing visits, imported score)`. Imported rows receive no synthetic recency, and repeating the command is idempotent. Zoxide is never required for normal Dirgo operation.

If a redb file is invalid, Dirgo preserves it beside the original as a timestamped `.corrupt.<timestamp>` file. The disposable index is then rebuilt; state starts empty only after its preserved backup is written. A newer unknown persistent-state schema is never overwritten. A newer disposable-index schema requires an explicit `dgo refresh` before replacement. `dgo doctor` also warns when the active shell startup file exceeds 1 MiB, a common cause of slow or unstable terminal startup.

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

When a Dirgo upgrade changes the disposable index format, the next use rebuilds that index atomically. Persistent bookmarks, visit history, and sessions are not discarded.

## Machine-readable contract

```bash
dgo query punk --json
```

Unresolved JSON responses include a `score_breakdown` for every candidate so ranking decisions can be inspected without guessing at hidden weights.

Resolved paths are written to stdout without decoration. Diagnostics and selector UI use stderr. Exit codes currently used by the resolver are `0` success, `3` no match, and `4` ambiguous/cancelled; Clap uses `2` for invalid arguments.

UTF-8 paths with spaces, quotes, Unicode, brackets, emoji, and leading dashes are supported. Paths containing a newline or non-UTF-8 Unix bytes are intentionally rejected at the shell boundary because its text command-substitution protocol cannot transport them safely. NUL is not valid in paths. A direct existing path handled by the shell wrapper does not cross this protocol.

## Privacy and security

Dirgo works entirely locally. It contains no telemetry or analytics and makes no network request during normal use. The index contains local filesystem paths, so protect the XDG cache/state directories as you would other local application data.

Filesystem paths are never interpolated into shell commands or passed through `eval`. See [SECURITY.md](SECURITY.md) for reporting guidance.

## Performance

The architecture keeps direct paths in the shell and performs no `chpwd` index scan. On the current Apple Silicon macOS host (Darwin 25.5.0, Rust 1.89.0, optimized `dgo 0.1.0`), a real PTY probe produced these three-sample results from an already-built isolated index:

| Indexed directories | First picker paint | First useful result |
| ---: | ---: | ---: |
| 100,000 | 52.628 ms | 35.154 ms |
| 500,000 | 53.529 ms | 35.209 ms |
| 1,000,000 | 55.180 ms | 35.236 ms |

The release budget is 100 ms for each metric at 1M directories. These are local measurements, not a promise that every machine will produce identical timings. The probe enters an initial query in a real pseudo-terminal and observes explicit first-paint and first-result markers emitted by the release binary.

Build the public binary and the feature-gated developer fixture tool, then run the external harness for each dataset size. The fixture tool is not installed by a normal `cargo install dirgo`. The harness creates isolated XDG configuration and state, refuses to overwrite an existing fixture, records OS/Rust/binary metadata, measures one cold index build, and then measures warm CLI work. The generated fixture contains exactly the requested number of child directories (the indexed root itself is additional).

```bash
cargo build --release --bin dgo
cargo build --release --bin dgo-fixture --features benchmark-tools
scripts/benchmark-cli.sh --directories 10000
scripts/benchmark-cli.sh --directories 100000
scripts/benchmark-cli.sh --directories 500000
scripts/benchmark-cli.sh --directories 1000000
```

Pass `--report /absolute/new-file.txt` to retain one run's stdout evidence. The harness refuses to overwrite either fixtures or reports.

For profiler-driven index crawl measurements, Criterion uses the same deterministic fixture layout. Set `DIRGO_BENCH_DIRECTORIES` to one of the four sizes; optionally set `DIRGO_BENCH_FIXTURE` to a pre-created fixture to avoid including fixture creation in the run.

```bash
DIRGO_BENCH_DIRECTORIES=10000 cargo bench --bench index_pipeline
```

The warm picker latency smoke can be reproduced locally with:

```bash
DGO_BIN="$PWD/target/release/dgo" expect scripts/measure-picker-latency.exp
```

It enforces the M1 targets for first paint and first useful live result, but is intentionally not a shared-runner CI gate.

The PTY shell matrix checks Zsh, Bash, and (when installed) Fish against spaces, quotes, Unicode, `..`, leading dashes, bookmarks, and per-shell back/forward navigation. A host without Fish reports an explicit skip rather than a pass; release evidence requires a Fish-equipped macOS or Linux runner.

```bash
DGO_BIN="$PWD/target/release/dgo" expect scripts/pty-shell-matrix.exp
```

To measure the streaming interactive path at a chosen fixture size, first create a fixture and then run the dedicated PTY probe. It creates its own index and state and removes them afterwards; the supplied fixture remains unchanged.

```bash
target/release/dgo-fixture --output /private/tmp/dirgo-100k --directories 100000
DGO_BIN="$PWD/target/release/dgo" DGO_FIXTURE_ROOT=/private/tmp/dirgo-100k \
  expect scripts/measure-streaming-picker-latency.exp
```

Large fixture creation can be split into repeatable batches. Only a matching Dirgo progress marker can be resumed; arbitrary existing directories and symlinked roots are rejected.

```bash
target/release/dgo-fixture --output /private/tmp/dirgo-1m \
  --directories 1000000 --batch-size 200000
target/release/dgo-fixture --output /private/tmp/dirgo-1m \
  --directories 1000000 --batch-size 200000 --resume
```

For a lightweight measurement against your own current index, use `dgo bench --query punk --samples 5`. It reports context load plus median **non-interactive fallback** candidate construction (without counting a synthetic record clone) and fuzzy-resolution work. The interactive picker streams candidate construction into its background matcher after its first frame, but this command remains useful for finding heavy fallback and resolver work. Neither is a cross-machine performance claim.

Before creating a release candidate, run the non-publishing local preflight. It checks formatting, warnings, tests, release binaries, Criterion compilation, offline package assembly, the default one-command install surface, generated completion syntax, PTY picker/terminal/shell restoration gates, a disposable benchmark smoke, and accidental whitespace errors. It never creates tags, uploads artifacts, or changes shell startup files. Use `--require-fish` on a host where Fish release evidence is required.

```bash
scripts/release-preflight.sh
```

## Project documents

- [Baseline audit](docs/BASELINE_AUDIT.md)
- [Domain language](CONTEXT.md)
- [Release checklist](docs/RELEASE_CHECKLIST.md)
- [Differential security and release review](DIFFERENTIAL_REVIEW_REPORT.md)
- [Contributing](CONTRIBUTING.md)
- [Changelog](CHANGELOG.md)
- [Support](SUPPORT.md)

## License

Dirgo is licensed under either Apache License 2.0 or MIT, at your option.
