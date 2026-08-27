Output docs/assets/dirgo-project-commands.gif

Set Shell zsh
Set FontSize 18
Set Width 1100
Set Height 620
Set Padding 24
Set Framerate 24
Set TypingSpeed 55ms
Set Theme "iTerm2 Dark Background"
Env PROMPT "  › "

Hide
Type 'export DGO_REPO_ROOT="$PWD"' Enter
Type 'export PATH="$PWD/target/release:$PATH"' Enter
Type 'eval "$(scripts/demo-setup.sh)"' Enter
Type 'dgo suggestions enable >/dev/null' Enter
Type 'dgo refresh >/dev/null' Enter
Type 'cd "$DGO_DEMO_ROOT/filesystem/Projects/Punk"' Enter
Type 'dgo __suggest-project-refresh --cwd "$PWD"' Enter
Type 'eval "$(dgo init zsh)"' Enter
Type 'export PS1="  %F{#30D158}›%f "' Enter
Type 'source "$DGO_REPO_ROOT/scripts/demo-chapters.zsh"' Enter
Ctrl+L
Show

Ctrl+G
Sleep 1.8s
Type "pnpm run "
Sleep 2.2s
Tab
Sleep 1.4s
Ctrl+U

Ctrl+G
Sleep 1.4s
Type "git "
Sleep 2.2s
Down 3
Sleep 1.4s
Tab
Sleep 1.4s
Ctrl+U

Ctrl+G
Sleep 1.4s
Type "cargo "
Sleep 2.2s
Down
Sleep 1.2s
Tab
Sleep 1.6s
Ctrl+U

Ctrl+G
Sleep 2.8s
