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

**Live panel**: A bounded list rendered below the editable command line while
the user types. It is presentation only: selection and insertion remain
separate from execution.

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
  previous request and stale responses are never rendered.
- The panel normally shows eight rows, grows to at most twelve rows, shrinks to
  five rows in a small terminal, and never consumes more than one third of the
  terminal height.
- Up and Down move the active row, Tab inserts it, Escape dismisses the panel,
  and Enter submits only the text already visible in the shell buffer. Enter
  never accepts an uninserted candidate.
- Sources are understandable without color: `CMD`, `SUB`, `OPT`, `DIR`, `FILE`,
  `HIST`, and `NAV`.
- The renderer applies a row-level diff and never redraws an unchanged panel.
- `NO_COLOR`, ASCII mode, multiline prompts, terminal resize, paste, reverse
  search, vi mode, suspended jobs, and terminal restoration are explicit test
  cases rather than best-effort behavior.

## Completion catalog

The fast catalog contains executable names from `PATH`, shell builtins,
aliases, functions, known static subcommands and options, files, directories,
navigation history, and opted-in command history. Executable directories are
indexed once and refreshed when their metadata changes. Shell adapters publish
their session-local builtins, aliases, and functions without persisting the
user's command buffer.

The native lane delegates context-aware completion to the registered Zsh,
Bash, Fish, or PowerShell completion system. Native work is serialized per
session, cancelled when obsolete, limited to 80 milliseconds, and protected by
a circuit breaker. Dynamic native results remain in memory; only static
metadata keyed by provider, executable path, version, and modification time may
be persisted.

Ranking is streaming and bounded. Providers emit scored candidates into a
deduplicating top-K collector, so a command with thousands of completions does
not require sorting or rendering the entire set.

## Supported presentation

| Shell | Presentation |
| --- | --- |
| Zsh | Full automatic live panel through `zle-line-pre-redraw`; existing widgets and keymaps are preserved |
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
3. Providers produce candidates and the engine sanitizes, deduplicates, ranks,
   and limits them with a bounded top-K collector.
4. The adapter discards stale responses, updates the live panel only when its
   model changed, and applies an edit only when the expected buffer suffix
   still matches.
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
