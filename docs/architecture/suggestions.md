# Shell-native suggestions

Dirgo suggestions help people complete commands and paths while leaving the
interactive shell in control of editing and execution. Dirgo can propose and
insert text, but only the user can execute the resulting command.

## Language

**Suggestion**: A ranked text edit that may be inserted into the current shell
buffer.

**Insertion**: Applying a suggestion to the editable shell buffer without
submitting it.

**Execution**: Submitting the current shell buffer. Dirgo suggestions never
perform this action.

**Provider**: A local source of suggestions, such as the directory index,
navigation history, command history, executables, or the filesystem.

**Completion provider**: A source of context-aware command, subcommand, option,
or argument candidates. It may be backed by Dirgo's static index or by the
interactive shell's registered completion system.

**Live panel**: A virtualized list anchored immediately below the editable
command line while the user types. It is presentation only: selection and
insertion remain separate from execution.

**Shell adapter**: Integration that translates between a shell's native editing
API and Dirgo's suggestion protocol.

**Suggestion session**: A worker owned by one interactive shell session. It
ends when its communication channel closes and is never a global daemon.

**Navigation history**: Dirgo's existing record of visited directories.

**Command history**: Previously submitted shell commands. It is separate from
navigation history and is used only after explicit opt-in.

## Product contract

- The shell owns its buffer, prompt, key handling, and command execution.
- Accepting a suggestion only inserts text. It never submits the line.
- A failed, slow, or unavailable suggestion worker must not delay normal input.
- Request text crosses process boundaries through standard input, never command
  arguments, environment variables, `eval`, or `Invoke-Expression`.
- Command history is disabled by default, stored in the user's local state
  directory (mode `0600` on Unix), and removable independently from navigation
  history.
- Dirgo providers do not use telemetry, an AI service, or a network connection.
  A user-installed native completion script may itself run helper commands or
  access the network, just as it can when the user invokes that shell's normal
  completion UI. Native providers are time-bounded, cancellable, and
  configurable.
- Each shell uses its native affordances. Feature presentation may differ when
  the host shell does not expose equivalent APIs.

## Live completion contract

- The panel becomes eligible after the first non-whitespace character and is
  hidden when there are no useful candidates.
- Input is debounced for 30 milliseconds. A newer buffer snapshot cancels the
  previous request and stale responses are never rendered. The snapshot tracks
  both `LBUFFER` and `RBUFFER`, so moving the cursor without changing text also
  invalidates the previous result and insertion preserves the suffix.
- The panel starts with three rows and expands to at most six when the user
  navigates. It never consumes more than one third of the terminal height.
  Zsh renders it through `POSTDISPLAY`, so it follows the editable buffer
  instead of occupying a terminal-bottom status area or covering the prompt.
- Safe catalog descriptions follow the selected candidate. At 92 columns and
  above they occupy a muted right-hand preview column without increasing panel
  height; below that threshold one cell-width-aware, truncated detail line
  appears beneath the active row when the terminal has at least 18 rows. On
  shorter terminals the detail is suppressed to preserve the one-third height
  budget. Source-only placeholders such as `PATH` and `DIR` are suppressed.
- Up and Down move the active row, Page Up and Page Down move by one viewport,
  Tab inserts the active row, Escape dismisses the panel, and Enter submits only
  the text already visible in the shell buffer. Enter never accepts an
  uninserted candidate.
- The header reports the visible range and exact result count. The Zsh adapter
  fetches ranked results in pages of 96, prefetches near the loaded boundary,
  and renders only the current viewport. Reaching a boundary may synchronously
  drain the already-started bounded request so queued navigation cannot starve
  the asynchronous ZLE callback.
- A watched ZLE file descriptor receives no generation or payload bytes until
  ranking has completed. The backend builds one frame in memory and publishes
  its generation, total, page, and optional descriptions together, so a slow
  request cannot make the editor callback block while waiting for the
  remainder. Descriptions are requested explicitly by the Zsh adapter, leaving
  the existing Bash and Fish completion tuple unchanged.
- Sources are understandable without color: `CMD`, `SUB`, `OPT`, `DIR`, `FILE`,
  `HIST`, and `NAV`.
- The renderer applies a row-level diff and never redraws an unchanged panel.
- `NO_COLOR`, ASCII mode, multiline prompts, terminal resize, paste, reverse
  search, vi mode, suspended jobs, and terminal restoration are explicit test
  cases rather than best-effort behavior.

## Completion catalog

The fast catalog contains executable names from `PATH`, Dirgo's own Clap graph,
a compiled pack of common third-party command trees, optional user TOML specs,
files, directories, navigation history, and opted-in command history. Root
commands and aliases use an in-memory index; nested matching streams into the
bounded top-K collector. Executable discovery is limited to 128 PATH entries,
4,096 files per entry, and 8,192 unique commands.

The compiled pack covers stable command nouns for Git, Docker and Compose,
Cargo/Rustup, npm/pnpm/Yarn/Bun, kubectl, GitHub CLI, Homebrew, Go, Helm,
Terraform, Podman, major cloud/build/deployment CLIs, and common system tools.
User specs are loaded from `completions/*.toml` beside `config.toml`. Loading is
limited to 64 regular files, 256 KiB per file, eight tree levels, 2,048 nodes
total, and 256 children or options per node. Unknown fields, control text,
symlinks, malformed files, and over-limit trees are ignored as optional
enrichment; they cannot prevent the suggestion worker from starting.

The shell-native lane remains available through each shell's normal completion
UI for tools outside the static catalog. Dirgo never invokes arbitrary native
completion scripts or a tool's `--help` path on the per-keystroke hot path.
PowerShell predictor requests and Dirgo worker calls are time-bounded; stale
responses are discarded.

The normal completion path ranks through a streaming, deduplicating top-K
collector. The Zsh catalog view computes an exact, deterministic ordering,
returns it in pages of 96, and never renders the entire set. Later pages are
loaded only as the user approaches them; walking the whole list is possible
without paying its full shell-memory cost up front.

## Supported presentation

| Shell | Presentation |
| --- | --- |
| Zsh | Full automatic live panel through `zle-line-pre-redraw`, anchored after the editable buffer with `POSTDISPLAY`; existing widgets and keymaps are preserved |
| Fish | Built-in live autosuggestion plus its native completion pager; Dirgo enriches candidates without replacing Fish's editor |
| Bash 4+ | Context-complete explicit list and insertion through Readline; no per-character key rebinding |
| PowerShell 7+ | `Ctrl+F` insertion; PowerShell 7.4.x gets automatic native PSReadLine `ListView` prediction |

WSL uses the corresponding Linux shell adapter. Windows PowerShell 5.1 and
`cmd.exe` are not supported.

## Data flow

1. The adapter captures the text before and after the cursor using its native
   editing API.
2. It sends a versioned request to a per-session worker. The fast lane returns
   cached local candidates first while the native lane runs under a separate
   deadline.
3. Providers produce candidates and the engine sanitizes, deduplicates, and
   ranks them. The normal path uses bounded top-K; a catalog page returns an
   exact total plus one deterministic slice.
4. The adapter discards stale responses, prefetches catalog pages near the
   loaded boundary, updates the live panel only when its model changed, and
   applies an edit only when the expected buffer suffix still matches.
5. The shell renders the result and remains solely responsible for submission.

The PowerShell predictor starts its worker before the first request and waits
for a readiness frame without blocking input. A published index or command
history change replaces the worker's immutable engine snapshot between frames;
an in-flight frame always completes against one coherent snapshot.

## Accessibility and terminal behavior

Suggestions inherit terminal colors and do not depend on color, dim text, or
Nerd Fonts alone. Explicit lists identify sources with text labels such as
`CMD`, `SUB`, `OPT`, `DIR`, `FILE`, `HIST`, and `NAV`. Reduced-capability
environments use ASCII or completion-only fallbacks and never receive
cursor-positioning overlays.
