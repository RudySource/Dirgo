

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

Dirgo finds directories by intent, not by memory. It indexes the filesystem so you can jump to projects you have never visited, then uses local history to improve ranking over time.

| ⚡ **55 ms** | 🗂️ **1M directories** | 🔒 **Zero telemetry** | 🐚 **Four shells** |
| :---: | :---: | :---: | :---: |
| First picker paint¹ | Tested index size | Local data only | Zsh · Bash · Fish · PowerShell |

<p align="center">
  <img src="docs/assets/dirgo-demo.gif" width="860" alt="Dirgo terminal demo showing fuzzy directory search and navigation">
</p>

<p align="center"><sub>Type a fragment. See live matches. Press Enter. You are there.</sub></p>

> [!TIP]
> Unlike history-only jumpers, Dirgo can discover a directory before you have visited it. Existing paths such as `..`, `./src`, `~/Projects`, and `-` still use the shell's direct fast path.

## Why Dirgo

| | Capability | What it means for you |
| --- | --- | --- |
| **Discover** | Filesystem index | Find new and existing directories without remembering full paths. |
| **Decide safely** | Conservative resolution | Ambiguous names open the picker instead of silently choosing the wrong project. |
| **Stay fast** | Rust + background matching | The UI opens immediately while candidates stream in. |
| **Keep control** | Local-first storage | No account, cloud service, telemetry, or network request during normal use. |

Dirgo ships as one `dgo` binary. It has no runtime dependency on `fd`, `fzf`, `zoxide`, or `eza`.

## Install

Install the binary, connect your shell once, then jump by intent instead of typing full paths.

<p align="center">
  <img src="docs/assets/dirgo-install.gif" width="860" alt="Installing Dirgo, connecting Zsh, and making the first directory jump">
</p>

| **01 · Install** | **02 · Connect** | **03 · Go** |
| :---: | :---: | :---: |
| Use the verified release installer or Homebrew. | `dgo setup` shows the shell change before applying it. | Run `dgo <name>` from anywhere. |

<p align="center"><sub>The animation shows the Homebrew route on macOS. The release installer and manual packages are below.</sub></p>

### macOS and Linux

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/RudySource/Dirgo/releases/latest/download/dirgo-installer.sh | sh
```

The installer detects your platform, verifies the release checksum, installs to `~/.local/bin`, then shows the exact shell change before asking for confirmation.

> [!TIP]
> Prefer Homebrew? One paste does the same onboarding: `brew install rudysource/tap/dirgo && dgo setup`

### Windows

```powershell
irm https://github.com/RudySource/Dirgo/releases/latest/download/dirgo-installer.ps1 | iex
```

The PowerShell installer verifies SHA-256 before copying `dgo.exe` and its
predictor module, then asks before changing your user `PATH`. Open a new
PowerShell 7+ session and run `dgo setup` to connect parent-shell navigation.

Prefer Scoop? Add the official bucket and install Dirgo:

```powershell
scoop bucket add rudysource https://github.com/RudySource/scoop-bucket
scoop install rudysource/dirgo
```

> [!NOTE]
> Windows support targets PowerShell 7+ and Windows Terminal. Legacy Windows
> PowerShell 5.1 and `cmd.exe` are not supported.

### What setup changes

`dgo setup` manages one clearly marked block in your Zsh, Bash, Fish, or
PowerShell profile. Before writing, it previews the target and content. On
approval it creates a timestamped backup and replaces the file atomically.

```bash
dgo setup --dry-run     # preview only
dgo setup               # connect or repair
dgo setup --remove      # remove only Dirgo's managed block
```

It never uses `sudo`, sends telemetry, or silently edits a shell file in a non-interactive session. Automation requires explicit consent with `dgo setup --yes`.

<details>
<summary><strong>Manual install and checksum verification</strong></summary>

Download the archive for your platform and `SHA256SUMS` from the [latest GitHub Release](https://github.com/RudySource/Dirgo/releases/latest).

```bash
# Linux
sha256sum --check SHA256SUMS

# macOS
shasum -a 256 -c SHA256SUMS
```

On Windows, compare `Get-FileHash -Algorithm SHA256 <archive>` with the matching
entry in `SHA256SUMS`. Extract the whole archive into a directory on `PATH`,
keeping the versioned `DirgoPredictor` directory beside `dgo.exe`, then run
`dgo setup`.

Release assets also carry GitHub artifact attestations. With the GitHub CLI installed, verify provenance with `gh attestation verify <archive> --repo RudySource/Dirgo`.

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

That builds the cross-platform CLI and the safe PowerShell `Ctrl+F` fallback.
To build the native PowerShell 7.4 predictor on Windows, also install .NET 8,
then run:

```powershell
dotnet build powershell/DirgoPredictor/DirgoPredictor.csproj --configuration Release --locked-mode
```

Keep the resulting DLL plus `DirgoPredictor.psd1` under
`DirgoPredictor/<version>` beside `dgo.exe`.

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

The first search builds a missing index automatically. Run `dgo refresh` whenever you want to pick up filesystem changes immediately.

## Interactive picker

The picker opens when a query is ambiguous or when you run `dgo` without a destination. Typing filters results immediately; preview work stays off the input path.

| Key | Action |
| --- | --- |
| `↑` / `↓` or `Ctrl-K` / `Ctrl-J` | Move the selection |
| `Home` / `End` or `PageUp` / `PageDown` | Move through long lists |
| `Enter` | Go to the selected directory |
| `Tab` | Toggle the directory preview |
| `Shift-↑` / `Shift-↓` | Scroll the directory contents without moving the selection |
| `Shift-Home` / `Shift-End`, `Shift-PageUp` / `Shift-PageDown` | Jump or page through the directory contents |
| `Ctrl-B` / `Ctrl-F` | Alternative page controls for the directory contents |
| `Ctrl-R` | Rebuild the index atomically |
| `Ctrl-O` / `Ctrl-Y` / `Ctrl-E` | Open, copy, or launch the configured editor |
| `Ctrl-U` | Clear the query |
| `Esc` / `Ctrl-C` | Close without navigating |

Use `NO_COLOR=1` or `--no-color` without losing visual hierarchy. Use `--no-unicode` for ASCII-only symbols. `TERM=dumb` activates the plain numbered selector.

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

Suggestions are local and opt-in. Enable them, then open a new shell or reload
the managed Dirgo integration:

```bash
dgo suggestions enable
```

Dirgo merges indexed directories, filesystem entries, commands found on
`PATH`, known subcommands/options, navigation history, and (when enabled)
filtered command history. The built-in command pack covers Git, Docker and
Compose, Cargo/Rustup, npm/pnpm/Yarn/Bun, kubectl, GitHub CLI, Homebrew, Go,
Helm, Terraform, Podman, major cloud CLIs, .NET, Python/PHP/JVM tooling,
deployment CLIs, curl/SSH, ripgrep/fd, and other common developer tools. For
example, `git ch`, `docker co`, and `docker compose u` offer `checkout`,
`compose`, and `up`. Source labels such as `DIR`, `PATH`, `SUB`, and `OPT` make a
larger list easy to scan.

- **Zsh:** after a 30 ms debounce, a three-row panel appears immediately below
  the editable command line and expands to six rows while navigating. Use
  `Up`/`Down`, `Page Up`/`Page Down`, `Tab` to insert, or `Esc` to dismiss.
  Results are fetched in pages and the header shows the visible range plus the
  exact total, so large command catalogs stay fast without being truncated to
  the first few matches. On wide terminals the selected command's description
  appears in a quiet preview column; narrower terminals place the same concise
  explanation below the active row without hiding the prompt.
- **PowerShell 7.4.x + PSReadLine 2.2.2+:** suggestions use native ListView;
  `F2` switches PSReadLine's prediction view.
- **Fish and Bash 4+:** Dirgo enriches each shell's native `Tab` completion
  menu without replacing the line editor or other commands' completions.

On every supported shell, `Ctrl+F` inserts the best result and `Shift+Tab`
opens the explicit source-labelled picker. Insertion never submits or executes
the command. Other PowerShell 7 versions retain navigation and `Ctrl+F`.

Command-history suggestions remain off until separately enabled with
`dgo suggestions history enable`. Likely credentials and unsafe terminal text
are rejected before storage. Disable collection without deleting existing data
with `history disable`, or erase only this store with `history clear`.

Installed tools that are not in the built-in pack still appear as `PATH`
commands and keep their shell's normal Tab completion. To teach Dirgo a private
or less common CLI, create a `.toml` file in a `completions` directory beside
the path printed by `dgo config path`:

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

These files are bounded, data-only metadata. Dirgo does not run a tool,
completion script, or `--help` command while you type.

Dirgo checks GitHub Releases in a detached background process at most once per
day. A later invocation displays a short notice when a newer stable version is
available, so directory navigation never waits for the network. `dgo --update`
uses the detected installation source (Homebrew, Cargo, Scoop, or the verified
release installer). The notification preference is persistent and does not
disable manual updates.

Action flags work on either side of the query, so both forms below are equivalent:

```bash
dgo --finder "project name"
dgo "project name" --finder
```

The same ordering applies to `--open`, `--code`, `--copy`, and `--print`. Only one action may be requested at a time.

### Resolution safety

Dirgo navigates automatically only when it has an existing explicit path, an exact bookmark, a unique exact basename, or a strongly dominant visited prefix. Fuzzy typos, duplicate names, close scores, stale paths, and forced `?` queries require selection.

This bias is deliberate: one picker is cheaper than silently changing to the wrong project.

## Configuration

Dirgo follows the XDG base-directory convention and reads:

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

`actions.editor` accepts one executable name or path, never a shell command line.
Suggestion and command-history enablement are separate privacy choices.
`max_results` is additionally constrained to 5–12 visible rows according to
terminal height. `debounce_ms` controls the Zsh live panel delay;
`native_timeout_ms` bounds the native predictor response budget.
`ui.height_percent` is capped to the useful picker content height, so closing the picker does not leave a large empty terminal block.

</details>

The disposable index lives below `XDG_CACHE_HOME`. Bookmarks, visits, and navigation sessions live separately below `XDG_STATE_HOME`, so rebuilding the index never discards user state.

State growth is bounded: history retains at most 50,000 rows, each shell session keeps its latest 256 transitions, and abandoned sessions are pruned in batches.

## Platform support

| Platform | Distribution | Navigation support |
| --- | --- | --- |
| macOS Apple Silicon | One-command installer, Homebrew, archive | Zsh, Bash, Fish |
| macOS Intel | One-command installer, archive | Zsh, Bash, Fish |
| Linux x86_64 GNU | One-command installer, archive; glibc 2.35+ | Zsh, Bash, Fish |
| Windows x86_64 MSVC | PowerShell installer, Scoop, archive | PowerShell 7+ navigation and insertion; native predictor on 7.4.x |

WSL uses its installed Zsh, Bash, or Fish adapter. Windows PowerShell 5.1 and
`cmd.exe` are outside the supported matrix.

## Performance

Dirgo keeps direct paths in the shell and streams large indexes into the matcher. On an Apple Silicon macOS host, an optimized release build produced the following three-sample PTY results:

| Indexed directories | First picker paint | First useful result |
| ---: | ---: | ---: |
| 100,000 | 52.628 ms | 35.154 ms |
| 500,000 | 53.529 ms | 35.209 ms |
| 1,000,000 | 55.180 ms | 35.236 ms |

¹ These are reproducible local measurements, not universal latency guarantees. The 1M-directory release budget is 100 ms for both metrics.

Run a lightweight benchmark against your own index:

```bash
dgo bench --query punk --samples 5
```

## Privacy and security

- No telemetry, analytics, account, cloud sync, or network call during normal use.
- Suggestions and command-history collection are disabled independently by default; suggestion data remains local and can be cleared without touching navigation history.
- Paths cross the shell boundary as data and are never interpolated into executable shell text.
- Human-facing output escapes terminal controls and bidirectional overrides from untrusted filenames.
- Index publication is atomic; corrupted local data is preserved under a timestamped backup name before recovery.
- Unknown persistent-state schemas are never overwritten.

> [!WARNING]
> The local index contains filesystem paths. Protect your XDG cache and state directories like other private application data, and remove personal paths from issue logs or screenshots.

Report vulnerabilities privately through the process in [SECURITY.md](SECURITY.md).

<details>
<summary><strong>Machine-readable contract</strong></summary>

```bash
dgo query punk --json
```

Resolved paths are written to stdout without decoration. Diagnostics and selector UI use stderr. Resolver exit codes are `0` for success, `3` for no match, and `4` for ambiguous or cancelled selection; Clap uses `2` for invalid arguments.

UTF-8 paths with spaces, quotes, Unicode, brackets, emoji, and leading dashes are supported. Newline-containing and non-UTF-8 Unix paths are rejected at the shell command-substitution boundary. Direct existing paths handled by the wrapper do not cross that boundary.

</details>

## Technology

The implementation is primarily **Rust**, with focused Shell, PowerShell, C#,
and Ruby packaging surfaces.

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

**Dirgo 0.4 is the current product release.** It adds private shell-native
suggestions and complete PowerShell 7 integration while preserving the
shell as the sole owner of editing and execution.

| Status | Release area | Outcome |
| --- | --- | --- |
| ✅ Shipped | Navigation core | Indexed discovery, fuzzy picker, bookmarks, recent history, project roots, and per-shell back/forward navigation |
| ✅ Shipped | Safety and privacy | Local-only operation, escaped terminal output, bounded state, non-destructive recovery, and repository hygiene gates |
| ✅ Shipped | Installation | One-command macOS/Linux and Windows installers, Homebrew, the official Scoop bucket, verified archives, and reversible Zsh/Bash/Fish/PowerShell setup |
| ✅ Shipped | Release quality | Rust formatting and lint gates, unit/integration/PTY tests, dependency policy, checksums, and build attestations |
| ✅ Shipped | Action workflow | Open, Finder, editor, clipboard, and print actions work before or after a query and pass safely through shell integration |
| ✅ Shipped | Picker ergonomics | Useful-content inline height, reliable cursor restoration, configurable preview visibility, and scrollable bounded directory contents |
| ✅ Shipped | Package automation | Release checksums render validated Homebrew and Scoop packages, with a safe manual fallback when cross-repository credentials are not configured |
| ✅ Shipped | Shell suggestions | Opt-in local providers, safe insertion, explicit Unix picker, native PowerShell predictor, privacy controls, and bounded storage |
| 🎯 Next | Project intelligence | Turn local project manifests into fast, contextual actions without executing project code |
| 🔭 Future | More platforms | Linux ARM64 and musl builds, followed by additional package managers where demand justifies maintenance |
| 🔭 Future | Distribution trust | Platform code signing/notarization and a stable-package publication flow for ecosystems beyond Homebrew |

> [!NOTE]
> Future items are update candidates, not missing release requirements. They
> should enter development only with a defined user need, supported platform
> matrix, regression tests, and a versioned release plan.

### Dirgo 0.5 · Project Intelligence

The next release moves Dirgo from knowing *where a project is* to understanding
*what can be done there*. Suggestions will stay local, data-only, and safe to
inspect: Dirgo may insert an action into the shell buffer, but the user remains
the only one who can execute it.

<p align="center"><strong>Detect the project → read trusted metadata → rank useful actions → insert, never execute</strong></p>

| Stage | Product surface | Release outcome |
| --- | --- | --- |
| **01 · Foundation** | Provider registry, typed suggestion metadata, source provenance, per-provider budgets | New sources plug in without growing one central engine; slow or malformed providers cannot delay input |
| **02 · Project actions** | `package.json`, Cargo configuration, Make, Just, Taskfile, mise, and Compose | Scripts, targets, services, and aliases appear only where they are relevant, with their source and concise description |
| **03 · Context** | Project-aware history with cwd, success, duration, and recency | Commands that worked in this project rank above generic history while collection remains separately opt-in |
| **04 · Freshness** | Cached manifest fingerprints and incremental project refresh | Edited tasks appear quickly without a full filesystem rebuild or a required always-on daemon |
| **05 · Experience** | Quiet live rows, richer selected-item detail, native fallbacks, narrow-terminal layouts | The extra context stays readable in Zsh, Bash, Fish, PowerShell, ASCII, and `NO_COLOR` modes |
| **06 · Trust** | Schema migrations, privacy controls, adversarial fixtures, PTY and native Windows gates | Every accepted suggestion still inserts only; project files are parsed as bounded data and never sourced or executed |

**0.5 ships only when:** the 0.4 latency budgets do not regress; all parsers are
bounded and cacheable; disabling a provider takes effect immediately; corrupted
metadata degrades to no suggestions; and the complete macOS, Linux, Windows,
Zsh, Bash, Fish, and PowerShell release matrix is green.

Out of scope for 0.5: cloud sync, telemetry, generative AI, automatic command
execution, arbitrary completion-script evaluation, and a mandatory background
service. Those would weaken the product's speed or trust model instead of
strengthening its core workflow.

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
