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
- Command history is disabled by default, stored locally with user-only access,
  and removable independently from navigation history.
- No provider uses a network connection, telemetry, or an AI service.
- Each shell uses its native affordances. Feature presentation may differ when
  the host shell does not expose equivalent APIs.

## Supported presentation

| Shell | Presentation |
| --- | --- |
| Zsh | Inline suffix and explicit list through ZLE |
| Fish | Native dynamic completions without replacing built-in autosuggestions |
| Bash | Readline completion and an explicit picker |
| PowerShell 7.2+ | PSReadLine predictor inline and list views |
| Windows PowerShell 5.1 | Completion-only compatibility mode |

WSL uses the corresponding Linux shell adapter. `cmd.exe` is not supported.

## Data flow

1. The adapter captures the text before and after the cursor using its native
   editing API.
2. It sends a versioned request to a per-session worker or a bounded one-shot
   process.
3. Local providers produce candidates and the engine sanitizes, deduplicates,
   ranks, and limits them.
4. The adapter discards stale responses and applies an edit only when the
   expected buffer suffix still matches.
5. The shell renders the result and remains solely responsible for submission.

## Accessibility and terminal behavior

Suggestions inherit terminal colors and do not depend on color, dim text, or
Nerd Fonts alone. Explicit lists identify sources with text labels such as
`DIR`, `NAV`, `HIST`, `PATH`, and `FILE`. Reduced-capability environments use
ASCII or completion-only fallbacks and never receive cursor-positioning
overlays.
