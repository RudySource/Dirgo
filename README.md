

<div align="center">

<img src="docs/assets/dirgo-wordmark-rounded.png" width="620" alt="Dirgo — fast directory navigation for the terminal">

<table width="100%">
  <tr>
    <td align="center">
      <strong>💚 Support Dirgo</strong><br>
      If Dirgo saves you time, <a href="https://github.com/RudySource/Dirgo">star it on GitHub</a>,
      <a href="https://github.com/RudySource/Dirgo/issues/new/choose">share feedback</a>, or
      <a href="CONTRIBUTING.md">contribute a fix</a>.<br><br>
      <a href="#support-dirgo"><strong>Support development with BTC or USDT TRC20 →</strong></a>
    </td>
  </tr>
</table>

<h1>Dirgo</h1>

<p><strong>Go anywhere. Instantly.</strong></p>

<p>A fast, local-first directory navigator with fuzzy search and a responsive terminal picker.</p>

<p>
  <a href="https://github.com/RudySource/Dirgo/actions/workflows/ci.yml"><img alt="Build status" src="https://img.shields.io/github/actions/workflow/status/RudySource/Dirgo/ci.yml?branch=main&style=flat-square&label=build&labelColor=071b3a&color=20bf55"></a>
  <a href="https://github.com/RudySource/Dirgo/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/RudySource/Dirgo?style=flat-square&label=release&labelColor=071b3a&color=20bf55"></a>
  <a href="https://github.com/RudySource/Dirgo/releases"><img alt="Release downloads" src="https://img.shields.io/github/downloads/RudySource/Dirgo/total?style=flat-square&label=downloads&labelColor=071b3a&color=20bf55"></a>
  <a href="https://www.rust-lang.org"><img alt="Rust 1.89 or newer" src="https://img.shields.io/badge/Rust-1.89%2B-20bf55?style=flat-square&logo=rust&logoColor=white&labelColor=071b3a"></a>
  <a href="LICENSE-MIT"><img alt="MIT or Apache 2.0 license" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-20bf55?style=flat-square&labelColor=071b3a"></a>
</p>

<p>
  <a href="#install">Install</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#interactive-picker">Picker</a> ·
  <a href="#commands">Commands</a> ·
  <a href="#configuration">Configuration</a> ·
  <a href="#support-dirgo">Support</a> ·
  <a href="#privacy-and-security">Security</a> ·
  <a href="#release-status-and-roadmap">Roadmap</a>
</p>

</div>

Dirgo indexes your filesystem, finds directories you have never visited, and
uses local history to improve ranking over time.

| ⚡ **55 ms** | 🗂️ **1M directories** | 🔒 **Zero telemetry** | 🐚 **Four shells** |
| :---: | :---: | :---: | :---: |
| First picker paint¹ | Tested index size | Local data only | Zsh · Bash · Fish · PowerShell |

<p align="center">
  <img src="docs/assets/dirgo-demo.gif" width="860" alt="Dirgo terminal demo showing fuzzy directory search and navigation">
</p>

<p align="center"><sub>Type a fragment. See live matches. Press Enter. You are there.</sub></p>

## Why Dirgo

| | Capability | What it means for you |
| --- | --- | --- |
| **Discover** | Filesystem index | Find new and existing directories without remembering full paths. |
| **Decide safely** | Conservative resolution | Ambiguous names open the picker instead of silently choosing the wrong project. |
| **Stay fast** | Rust + background matching | The UI opens immediately while candidates stream in. |
| **Keep control** | Local-first storage | No account, cloud service, telemetry, or network request during normal use. |

One `dgo` binary; no runtime dependency on `fd`, `fzf`, `zoxide`, or `eza`.

## Install

<p align="center">
  <img src="docs/assets/dirgo-install.gif" width="860" alt="Installing Dirgo, connecting Zsh, and making the first directory jump">
</p>

### macOS and Linux

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/RudySource/Dirgo/releases/latest/download/dirgo-installer.sh | sh
```

Detects the platform, verifies SHA-256, installs to `~/.local/bin`, then asks
before connecting the shell.

> [!TIP]
> Prefer Homebrew? One paste does the same onboarding: `brew install rudysource/tap/dirgo && dgo setup`

### Windows

```powershell
irm https://github.com/RudySource/Dirgo/releases/latest/download/dirgo-installer.ps1 | iex
```

Verifies SHA-256, installs `dgo.exe` with its predictor, and asks before changing
the user `PATH`. Open PowerShell 7+ and run `dgo setup`.

Or use the official Scoop bucket:

```powershell
scoop bucket add rudysource https://github.com/RudySource/scoop-bucket
scoop install rudysource/dirgo
```

> [!NOTE]
> Windows support targets PowerShell 7+ and Windows Terminal. Legacy Windows
> PowerShell 5.1 and `cmd.exe` are not supported.

### What setup changes

`dgo setup` previews one managed block for Zsh, Bash, Fish, or PowerShell. On
approval it creates a timestamped backup and updates the profile atomically.

```bash
dgo setup --dry-run     # preview only
dgo setup               # connect or repair
dgo setup --remove      # remove only Dirgo's managed block
```

It never uses `sudo` or silently edits a non-interactive shell. Automation must
opt in with `dgo setup --yes`.

<details>
<summary><strong>Manual install and checksum verification</strong></summary>

Download the archive for your platform and `SHA256SUMS` from the [latest GitHub Release](https://github.com/RudySource/Dirgo/releases/latest).

```bash
# Linux
sha256sum --check SHA256SUMS

# macOS
shasum -a 256 -c SHA256SUMS
```

On Windows, compare `Get-FileHash -Algorithm SHA256 <archive>` with
`SHA256SUMS`. Keep `DirgoPredictor/<version>` beside `dgo.exe`.

Release assets include GitHub attestations: `gh attestation verify <archive> --repo RudySource/Dirgo`.

</details>

<details>
<summary><strong>Build from source</strong></summary>

Rust 1.89 or newer is required.

```bash
git clone https://github.com/RudySource/Dirgo.git
cd Dirgo
cargo build --release --locked
install -m 755 target/release/dgo ~/.local/bin/dgo
dgo setup
```

For the native PowerShell 7.4 predictor, install .NET 8 and run:

```powershell
dotnet build powershell/DirgoPredictor/DirgoPredictor.csproj --configuration Release --locked-mode
```

</details>

## Quick start

```bash
dgo refresh       # index your configured roots
dgo punk          # jump by directory name
dgo               # browse everything
dgo . api         # search only below the current directory
dgo ? api         # always ask when names collide

dgo +work         # bookmark the current directory
dgo @work         # jump to the bookmark
dgo repo punk     # search project roots
dgo recent        # browse Dirgo history
dgo back          # move back in this shell session
```

The first search creates the index. Run `dgo refresh` after filesystem changes.

## Interactive picker

The picker opens for ambiguous queries or bare `dgo`.

| Key | Action |
| --- | --- |
| `↑` / `↓`, `Ctrl-K` / `Ctrl-J` | Move the selection |
| `Home` / `End`, `PageUp` / `PageDown` | Move through long lists |
| `Enter` | Go to the selected directory |
| `Tab` | Toggle the directory preview |
| `Shift-↑` / `Shift-↓` | Scroll the directory contents without moving the selection |
| `Ctrl-R` | Rebuild the index atomically |
| `Ctrl-O` / `Ctrl-Y` / `Ctrl-E` | Open, copy, or launch the configured editor |
| `Esc` / `Ctrl-C` | Close without navigating |

Compatibility: `NO_COLOR=1`, `--no-unicode`, or `TERM=dumb`.

## Commands

```text
dgo [QUERY]...                  resolve or choose a directory
dgo setup                      connect or repair shell integration
dgo refresh                    rebuild the filesystem index
dgo query <QUERY> [--json]     resolve without navigating
dgo explain <QUERY>            show candidates and score components
dgo root                       print the nearest project root
dgo repo [QUERY]               search indexed project roots
dgo recent [QUERY]             search Dirgo navigation history
dgo back | forward             navigate this shell session
dgo bookmarks                  list bookmarks
dgo bookmark add NAME          create or repair a bookmark
dgo bookmark rename OLD NEW    rename a bookmark
dgo bookmark remove NAME       remove a bookmark
dgo import zoxide              import validated local zoxide scores
dgo config path | show         inspect configuration
dgo doctor                     diagnose the installation
dgo stats                      show local index statistics
dgo support                    show support and security guidance
dgo suggestions enable         enable local shell suggestions
dgo suggestions status         inspect suggestion privacy settings
dgo suggestions history enable opt in to filtered command history
dgo suggestions history clear  erase stored command suggestions
dgo --update                   install the latest stable release
dgo update-notifications off   disable new-version notices
dgo update-notifications on    enable new-version notices
```

Run `dgo --help` or `dgo <command> --help` for the complete interface.

### Shell-native suggestions

Suggestions are local and opt-in. Enable them, then reload the shell:

```bash
dgo suggestions enable
```

Dirgo merges directories, filesystem entries, executables, known
subcommands/options, navigation history, and optional filtered command history.
The built-in catalog covers Git, Docker, Cargo, package managers, kubectl,
cloud CLIs, and other common developer tools. For example, `git ch` and
`docker compose u` offer `checkout` and `up`.

- **Zsh:** a paged panel follows the editable line after a 30 ms debounce. Use
  arrows or Page Up/Down, `Tab` to insert, and `Esc` to close. The selected
  description moves from a side preview to a compact row on narrow terminals.
- **PowerShell 7.4.x + PSReadLine 2.2.2+:** suggestions use native ListView;
  `F2` switches PSReadLine's prediction view.
- **Fish and Bash 4+:** Dirgo enriches native `Tab` completion.

`Ctrl+F` inserts the best result; `Shift+Tab` opens the source-labelled picker.
Insertion never submits or executes the command. Other PowerShell 7 versions
retain navigation and `Ctrl+F`.

Command history stays off until `dgo suggestions history enable`. Likely
credentials and unsafe terminal text are rejected before storage; use
`history disable` or `history clear` at any time.

Unknown installed tools still appear as `PATH` commands. Add data-only metadata
for a private CLI in `completions/*.toml` beside `dgo config path`:

```toml
name = "acme"
description = "Company developer tool"
aliases = ["a"]

[[subcommands]]
name = "deploy"
description = "Deploy the current service"

[[subcommands.options]]
name = "--production"
description = "Deploy to production"
```

Dirgo bounds these files and never runs the tool, `--help`, or a completion
script while you type.

`dgo --update` uses the detected Homebrew, Cargo, Scoop, or installer source.
The optional daily version check is detached and never blocks navigation.

Action flags work on either side of the query:

```bash
dgo --finder "project name"
dgo "project name" --finder
```

The same applies to `--open`, `--code`, `--copy`, and `--print`.

### Resolution safety

Automatic navigation requires an explicit path, exact bookmark, unique exact
basename, or strongly dominant visited prefix. Everything ambiguous opens the
picker.

## Configuration

Dirgo reads:

```text
${XDG_CONFIG_HOME:-~/.config}/dirgo/config.toml
```

<details>
<summary><strong>Complete configuration example</strong></summary>

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

[suggestions]
enabled = false
command_history = false
live_panel = true
native_completions = true
debounce_ms = 30
native_timeout_ms = 80
max_results = 8
retention_entries = 10000
retention_days = 180
deny_patterns = []
```

`actions.editor` accepts one executable, never a command line. Suggestion and
command-history enablement are separate. Visible results are clamped to 5–12
rows; debounce and native predictor work have explicit time budgets.

</details>

The rebuildable index lives below `XDG_CACHE_HOME`; bookmarks and history stay
below `XDG_STATE_HOME`. Persistent state is bounded and pruned.

## Platform support

| Platform | Distribution | Navigation support |
| --- | --- | --- |
| macOS Apple Silicon | One-command installer, Homebrew, archive | Zsh, Bash, Fish |
| macOS Intel | One-command installer, archive | Zsh, Bash, Fish |
| Linux x86_64 GNU | One-command installer, archive; glibc 2.35+ | Zsh, Bash, Fish |
| Windows x86_64 MSVC | PowerShell installer, Scoop, archive | PowerShell 7+ navigation and insertion; native predictor on 7.4.x |

WSL uses its Zsh, Bash, or Fish adapter. PowerShell 5.1 and `cmd.exe` are not supported.

## Performance

Apple Silicon macOS, optimized build, three-sample PTY run:

| Indexed directories | First picker paint | First useful result |
| ---: | ---: | ---: |
| 100,000 | 52.628 ms | 35.154 ms |
| 500,000 | 53.529 ms | 35.209 ms |
| 1,000,000 | 55.180 ms | 35.236 ms |

¹ Reproducible local measurements, not universal guarantees. The 1M release
budget is 100 ms for both metrics.

Run a lightweight benchmark against your own index:

```bash
dgo bench --query punk --samples 5
```

## Privacy and security

- No telemetry, analytics, account, cloud sync, or network call during normal use.
- Suggestions and command-history collection are independently disabled by default.
- Paths cross the shell boundary as data and are never interpolated into executable shell text.
- Human-facing output escapes terminal controls and bidirectional overrides from untrusted filenames.
- Index publication is atomic; recovery preserves corrupted data and never overwrites unknown schemas.

> [!WARNING]
> The local index contains filesystem paths. Remove personal paths from logs and screenshots.

Report vulnerabilities privately through the process in [SECURITY.md](SECURITY.md).

<details>
<summary><strong>Machine-readable contract</strong></summary>

```bash
dgo query punk --json
```

Resolved paths use stdout; diagnostics and UI use stderr. Exit codes: `0`
success, `3` no match, `4` ambiguous/cancelled, `2` invalid arguments.

UTF-8 paths with spaces, quotes, Unicode, emoji, and leading dashes are supported.
Newline-containing and non-UTF-8 Unix paths are rejected at the shell boundary.

</details>

## Technology

| Layer | Technology | Role |
| --- | --- | --- |
| Core | Rust 2024 edition | CLI, indexing, ranking, state, and cross-platform actions |
| Terminal UI | Ratatui + Crossterm | Responsive picker and terminal restoration |
| Matching | Nucleo | Fast, cancellable fuzzy matching |
| Storage | redb | Transactional local index and persistent state |
| Integration | Zsh, Bash, Fish, PowerShell | Parent-shell navigation, completions, and safe suggestions |
| Windows prediction | C# / PSReadLine subsystem | Native inline and list suggestions through a bounded local worker |
| Packaging | Shell, PowerShell, Homebrew | Verified, low-friction installation |

## Release status and roadmap

Version 0.4 completes Dirgo's shell-native suggestion layer. Version 0.5 adds
project-scoped commands on top of that existing engine.

| Track | State | Product outcome |
| --- | :---: | --- |
| Navigation and picker | **Ready** | Indexed discovery, conservative fuzzy resolution, bookmarks, recents, project roots, preview, and per-shell back/forward navigation |
| Safety and privacy | **Ready** | Local-only operation, opt-in command history, secret filtering, bounded storage and protocols, safe insertion without execution |
| Distribution | **Ready** | Verified installers and archives for macOS, Linux, and Windows, plus Homebrew and Scoop packaging |
| Shell intelligence · 0.4 | **Current release** | Zsh live panel, Bash/Fish native completions, PowerShell predictor, command catalog, descriptions, paging, and custom data-only command specs |
| Project commands · 0.5 | **Next** | Suggest useful commands from the project the user is currently in, not from a global generic list alone |

### Dirgo 0.5 · Project Commands

Dirgo will read a small set of local manifests as bounded data, combine declared
tasks with its command catalog, and insert the selected command—never execute it.

<p align="center"><strong>Current directory → project root → declared task → one safe insertion</strong></p>

| Priority | 0.5 deliverable | Definition of done |
| :---: | --- | --- |
| **P0** | Project-scoped provider | Reuse existing project-root detection; show labelled project results only inside that project |
| **P0** | JavaScript tasks | Read bounded `package.json` scripts and emit the detected npm, pnpm, Yarn, or Bun form without invoking it |
| **P0** | Rust targets | Read packages, binaries, examples, and features from `Cargo.toml`; complete concrete Cargo forms without calling Cargo |
| **P0** | Local cache | Fingerprint manifests, invalidate only changed projects, and keep parsing off the input path |
| **P1** | More manifests | Conservatively parse simple Make/Just tasks and Compose services; ignore dynamic or ambiguous definitions |
| **P1** | Ranking and detail | Prefer exact task prefixes and project relevance; show source and description in the current compact UI |
| **Gate** | Release confidence | Preserve bounded, cancellable, insertion-only behavior and pass the full OS/shell release matrix |

#### Release boundary

0.5 requires production-ready `package.json` and `Cargo.toml` workflows,
deterministic cache invalidation, and no manifest work on the keystroke path.
Make, Just, and Compose ship only if they meet the same gates.

Out of scope: cloud/AI/telemetry, automatic execution, arbitrary completion
scripts, a mandatory daemon, command exit tracking, and unrelated packaging work.

## Support Dirgo

Dirgo is free and open source. Voluntary donations help fund maintenance,
release infrastructure, platform support, and future improvements.

<table width="100%">
  <tr>
    <td align="center">
      <strong>₿ Bitcoin</strong><br>
      <sub>Network: Bitcoin · Asset: BTC</sub><br><br>
      <img src="https://img.shields.io/badge/Wallet-BTC-f7931a?style=for-the-badge&logo=bitcoin&logoColor=white" alt="Bitcoin wallet"><br><br>
      <strong>Wallet address</strong>
      <pre><code>bc1qk9n84av9f8lj6xcg6sknq2h6r6r495y226zqje</code></pre>
    </td>
  </tr>
  <tr>
    <td align="center">
      <strong>₮ USDT</strong><br>
      <sub>Network: TRON · Token standard: TRC20 · Asset: USDT</sub><br><br>
      <img src="https://img.shields.io/badge/Wallet-USDT_TRC20-26a17b?style=for-the-badge&logo=tether&logoColor=white" alt="USDT wallet on TRON TRC20"><br><br>
      <strong>Wallet address</strong>
      <pre><code>TLuVmVQ1XuYYUWc4bu1AYmJt5a8vTRDNbY</code></pre>
    </td>
  </tr>
</table>

> [!IMPORTANT]
> Copy the complete address from the matching block and verify its first and
> last characters before sending. For USDT, use the TRON network and TRC20 token
> standard only. Cryptocurrency transfers are irreversible. Donations are
> voluntary and do not purchase priority support, roadmap influence, or service
> guarantees.

You can also support Dirgo without donating: [star the repository](https://github.com/RudySource/Dirgo),
[report a focused issue](https://github.com/RudySource/Dirgo/issues/new/choose), or
[contribute code and documentation](CONTRIBUTING.md).

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| `dgo` prints a path but does not change directory | Run `dgo setup`, open a new terminal, and confirm `type dgo` reports a function. |
| A terminal starts slowly or closes | Run `command dgo doctor`, then `command dgo setup --remove` if the managed block is involved. |
| A new or moved directory is missing | Run `dgo refresh`. |
| Configuration is invalid | Run `dgo config path`, repair the file, then run `dgo doctor`. |
| A query is ambiguous in a pipe or CI | Use `dgo query <query> --json` and handle exit codes `3` and `4`. |
| The picker is unreadable | Try `NO_COLOR=1`, `--no-unicode`, or `TERM=dumb`. |

For environment-safe support information, run `dgo support` or read [SUPPORT.md](SUPPORT.md).

## Contributing

Rust 1.89 is the supported toolchain. Start with:

```bash
cargo build --locked
cargo test --all-features --locked
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for development checks and pull-request expectations.

## License

Dirgo is available under your choice of the [MIT license](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE).

<div align="center">

**Built for speed. Designed for trust.**

[Releases](https://github.com/RudySource/Dirgo/releases) · [Changelog](CHANGELOG.md) · [Donate](#support-dirgo) · [Security](SECURITY.md) · [Support](SUPPORT.md)

</div>
