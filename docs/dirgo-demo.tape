Output docs/assets/dirgo-demo.gif

Set Shell zsh
Set FontSize 18
Set Width 1000
Set Height 620
Set Padding 18
Set Framerate 30
Set TypingSpeed 35ms
Set Theme "Catppuccin Mocha"

Hide
Type 'export PATH="$PWD/target/release:$PATH"' Enter
Type 'eval "$(scripts/demo-setup.sh)"' Enter
Type 'eval "$(dgo init zsh)"' Enter
Type 'export PS1="%F{cyan}❯%f "' Enter
Ctrl+L
Show

Type "dgo refresh" Enter
Sleep 1s
Type "dgo punk" Enter
Sleep 800ms
Type "pwd" Enter
Sleep 800ms
Type "dgo +work" Enter
Sleep 800ms

Type "dgo ../.." Enter
Sleep 800ms
Type "dgo '?' api" Enter
Sleep 1.5s
Down
Sleep 600ms
Enter
Sleep 1s
Type "pwd" Enter
Sleep 800ms
Type "dgo @work" Enter
Sleep 1s
Type "pwd" Enter
Sleep 2s
