Output docs/assets/dirgo-suggestions.gif

Set Shell zsh
Set FontSize 18
Set Width 1000
Set Height 560
Set Padding 18
Set Framerate 30
Set TypingSpeed 90ms
Set Theme "Catppuccin Mocha"

Hide
Type 'export PATH="$PWD/target/release:$PATH"' Enter
Type 'eval "$(scripts/demo-setup.sh)"' Enter
Type 'dgo suggestions enable >/dev/null' Enter
Type 'dgo refresh >/dev/null' Enter
Type 'eval "$(dgo init zsh)"' Enter
Type 'export PS1="%F{cyan}❯%f "' Enter
Ctrl+L
Show

Type "git co"
Sleep 2s
Tab
Sleep 1.5s
Ctrl+U
Type "dgo pu"
Sleep 2s
Tab
Sleep 2s
