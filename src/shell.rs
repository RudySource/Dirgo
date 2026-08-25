use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
    #[value(name = "powershell", alias = "pwsh")]
    PowerShell,
}

impl Shell {
    pub fn name(self) -> &'static str {
        match self {
            Self::Zsh => "zsh",
            Self::Bash => "bash",
            Self::Fish => "fish",
            Self::PowerShell => "powershell",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Zsh => "Zsh",
            Self::Bash => "Bash",
            Self::Fish => "Fish",
            Self::PowerShell => "PowerShell",
        }
    }

    pub fn suggestion_kind(self) -> crate::suggestions::ShellKind {
        match self {
            Self::Zsh => crate::suggestions::ShellKind::Zsh,
            Self::Bash => crate::suggestions::ShellKind::Bash,
            Self::Fish => crate::suggestions::ShellKind::Fish,
            Self::PowerShell => crate::suggestions::ShellKind::PowerShell,
        }
    }
}

pub fn integration(shell: Shell) -> &'static str {
    match shell {
        Shell::Zsh => ZSH,
        Shell::Bash => BASH,
        Shell::Fish => FISH,
        Shell::PowerShell => POWERSHELL,
    }
}

pub fn completions(shell: Shell) -> String {
    match shell {
        Shell::Zsh => ZSH_COMPLETIONS.into(),
        Shell::Bash => BASH_COMPLETIONS.into(),
        Shell::Fish => FISH_COMPLETIONS.into(),
        Shell::PowerShell => POWERSHELL_COMPLETIONS.into(),
    }
}

pub fn validate_output_path(path: &std::path::Path) -> Result<()> {
    crate::paths::validate_shell_path(path)
}

const ZSH: &str = r#"# Dirgo shell integration for zsh
if [[ -z ${DGO_SESSION_ID:-} ]]; then
  export DGO_SESSION_ID="zsh-$$-$RANDOM-$RANDOM"
fi

function dgo() {
    case "${1:-}" in
    setup|init|completions|refresh|query|explain|bench|bookmarks|bookmark|import|doctor|stats|config|support|suggestions|update-notifications|--update|--open|--finder|--code|--copy|--print|--refresh|-r|--doctor|--bookmarks|--forget|--help|-h|--version|-V)
      command dgo "$@"
      return $?
      ;;
  esac

  local argument
  for argument in "$@"; do
    case "$argument" in
      --update|--open|--finder|--code|--copy|--print)
        command dgo "$@"
        return $?
        ;;
    esac
  done

  if (( $# == 1 )); then
    if [[ "$1" == "-" ]]; then
      builtin cd -
      return $?
    elif [[ -d "$1" ]]; then
      builtin cd -- "$1"
      return $?
    fi
  fi

  local destination
  destination="$(command dgo __resolve --cwd "$PWD" -- "$@")"
  local resolve_status=$?
  if (( resolve_status == 10 )); then
    return 0
  elif (( resolve_status == 0 )); then
    builtin cd -- "$destination"
    return $?
  fi
  return $resolve_status
}

if command dgo __suggest-enabled >/dev/null 2>&1 && [[ -o interactive ]]; then
  autoload -Uz add-zle-hook-widget compinit 2>/dev/null
  (( $+functions[compdef] )) || compinit -C
  if (( ! $+functions[_dgo] )); then
    source <(command dgo completions zsh)
  fi

  typeset -g _DGO_LIVE_GENERATION=0
  typeset -g _DGO_LIVE_FD=-1
  typeset -g _DGO_LIVE_SNAPSHOT=''
  typeset -g _DGO_LIVE_LAST_BUFFER=''
  typeset -g _DGO_LIVE_LAST_LINES=0
  typeset -g _DGO_LIVE_LAST_COLUMNS=0
  typeset -g _DGO_LIVE_GUARD=0
  typeset -g _DGO_LIVE_SELECTED=1
  typeset -g _DGO_LIVE_BASE_KEYMAP=''
  typeset -g _DGO_LIVE_TAB_WIDGET=''
  typeset -g _DGO_LIVE_UP_WIDGET=''
  typeset -g _DGO_LIVE_DOWN_WIDGET=''
  typeset -g _DGO_LIVE_ESC_WIDGET=''
  typeset -ga _DGO_LIVE_VALUES=()
  typeset -ga _DGO_LIVE_DISPLAYS=()
  typeset -ga _DGO_LIVE_LABELS=()
  typeset -ga _DGO_LIVE_REPLACEMENTS=()

  function _dgo_live_cancel_timer() {
    if (( _DGO_LIVE_FD >= 0 )); then
      zle -F "$_DGO_LIVE_FD" 2>/dev/null
      exec {_DGO_LIVE_FD}<&-
      _DGO_LIVE_FD=-1
    fi
  }

  function _dgo_live_ready() {
    local fd="$1" generation='' value display label replacement
    zle -F "$fd" 2>/dev/null
    IFS= read -r -d $'\0' generation <&$fd
    _DGO_LIVE_VALUES=()
    _DGO_LIVE_DISPLAYS=()
    _DGO_LIVE_LABELS=()
    _DGO_LIVE_REPLACEMENTS=()
    while IFS= read -r -d $'\0' value <&$fd &&
          IFS= read -r -d $'\0' display <&$fd &&
          IFS= read -r -d $'\0' label <&$fd &&
          IFS= read -r -d $'\0' replacement <&$fd; do
      _DGO_LIVE_VALUES+=("$value")
      _DGO_LIVE_DISPLAYS+=("$display")
      _DGO_LIVE_LABELS+=("$label")
      _DGO_LIVE_REPLACEMENTS+=("$replacement")
    done
    exec {fd}<&-
    [[ "$fd" == "$_DGO_LIVE_FD" ]] && _DGO_LIVE_FD=-1
    (( generation == _DGO_LIVE_GENERATION )) || return
    [[ "$BUFFER" == "$_DGO_LIVE_SNAPSHOT" ]] || return
    [[ -n "${BUFFER//[[:space:]]/}" ]] || return
    if (( ${#_DGO_LIVE_VALUES} == 0 )); then
      _dgo_live_dismiss
      return
    fi
    _DGO_LIVE_LAST_BUFFER="$BUFFER"
    _DGO_LIVE_LAST_LINES=$LINES
    _DGO_LIVE_LAST_COLUMNS=$COLUMNS
    _DGO_LIVE_SELECTED=1
    _DGO_LIVE_GUARD=1
    if [[ -z "$_DGO_LIVE_BASE_KEYMAP" ]]; then
      _DGO_LIVE_BASE_KEYMAP="$KEYMAP"
      _dgo_live_install_keys
    fi
    _dgo_live_render
    _DGO_LIVE_GUARD=0
  }

  function _dgo_live_render() {
    local panel='Dirgo suggestions' index marker
    for (( index = 1; index <= ${#_DGO_LIVE_VALUES}; index++ )); do
      marker=' '
      (( index == _DGO_LIVE_SELECTED )) && marker='›'
      panel+=$'\n'"$marker  ${_DGO_LIVE_LABELS[index]}  ${_DGO_LIVE_DISPLAYS[index]}"
    done
    panel+=$'\n'"↑↓ select   Tab insert   Esc dismiss"
    zle -M "$panel"
  }

  function _dgo_live_install_keys() {
    local -a binding
    binding=(${(z)"$(bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^I' 2>/dev/null)"})
    _DGO_LIVE_TAB_WIDGET="${binding[-1]:-}"
    binding=(${(z)"$(bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^[[A' 2>/dev/null)"})
    _DGO_LIVE_UP_WIDGET="${binding[-1]:-}"
    binding=(${(z)"$(bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^[[B' 2>/dev/null)"})
    _DGO_LIVE_DOWN_WIDGET="${binding[-1]:-}"
    binding=(${(z)"$(bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^[' 2>/dev/null)"})
    _DGO_LIVE_ESC_WIDGET="${binding[-1]:-}"
    [[ -n "$_DGO_LIVE_TAB_WIDGET" ]] && bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^I' _dgo_live_accept
    [[ -n "$_DGO_LIVE_UP_WIDGET" ]] && bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^[[A' _dgo_live_up
    [[ -n "$_DGO_LIVE_DOWN_WIDGET" ]] && bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^[[B' _dgo_live_down
    [[ -n "$_DGO_LIVE_ESC_WIDGET" ]] && bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^[' _dgo_live_dismiss
  }

  function _dgo_live_restore_keys() {
    [[ -z "$_DGO_LIVE_BASE_KEYMAP" ]] && return
    [[ -n "$_DGO_LIVE_TAB_WIDGET" ]] && bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^I' "$_DGO_LIVE_TAB_WIDGET"
    [[ -n "$_DGO_LIVE_UP_WIDGET" ]] && bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^[[A' "$_DGO_LIVE_UP_WIDGET"
    [[ -n "$_DGO_LIVE_DOWN_WIDGET" ]] && bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^[[B' "$_DGO_LIVE_DOWN_WIDGET"
    [[ -n "$_DGO_LIVE_ESC_WIDGET" ]] && bindkey -M "$_DGO_LIVE_BASE_KEYMAP" '^[' "$_DGO_LIVE_ESC_WIDGET"
    _DGO_LIVE_TAB_WIDGET=''
    _DGO_LIVE_UP_WIDGET=''
    _DGO_LIVE_DOWN_WIDGET=''
    _DGO_LIVE_ESC_WIDGET=''
  }

  function _dgo_live_pre_redraw() {
    (( _DGO_LIVE_GUARD )) && return
    if [[ "$KEYMAP" == isearch || "$KEYMAP" == vicmd ]]; then
      _dgo_live_dismiss
      return
    fi
    if [[ -z "${BUFFER//[[:space:]]/}" ]]; then
      _dgo_live_cancel_timer
      _DGO_LIVE_LAST_BUFFER=''
      zle -R -c
      return
    fi
    if [[ "$BUFFER" == "$_DGO_LIVE_LAST_BUFFER" &&
          $LINES == $_DGO_LIVE_LAST_LINES &&
          $COLUMNS == $_DGO_LIVE_LAST_COLUMNS ]]; then
      return
    fi
    if (( _DGO_LIVE_FD >= 0 )) && [[ "$BUFFER" == "$_DGO_LIVE_SNAPSHOT" ]]; then
      return
    fi
    _dgo_live_cancel_timer
    (( _DGO_LIVE_GENERATION++ ))
    _DGO_LIVE_SNAPSHOT="$BUFFER"
    local generation=$_DGO_LIVE_GENERATION
    local snapshot="$_DGO_LIVE_SNAPSHOT" after="$RBUFFER" cwd="$PWD" rows=$LINES columns=$COLUMNS
    exec {_DGO_LIVE_FD}< <(
      sleep 0.03
      print -rn -- "$generation"$'\0'
      printf '%s\0%s\0' "$snapshot" "$after" |
        command dgo __suggest-complete --shell zsh --cwd "$cwd" \
          --terminal-rows "$rows" --terminal-columns "$columns" 2>/dev/null
    )
    zle -F -w "$_DGO_LIVE_FD" _dgo_live_ready
  }

  function _dgo_live_accept() {
    local replacement="${_DGO_LIVE_REPLACEMENTS[_DGO_LIVE_SELECTED]:-}"
    _dgo_live_cancel_timer
    if [[ -n "$replacement" ]]; then
      LBUFFER="$replacement"
    fi
    _dgo_live_dismiss
  }

  function _dgo_live_dismiss() {
    _dgo_live_cancel_timer
    _DGO_LIVE_LAST_BUFFER="$BUFFER"
    _DGO_LIVE_LAST_LINES=$LINES
    _DGO_LIVE_LAST_COLUMNS=$COLUMNS
    _dgo_live_restore_keys
    _DGO_LIVE_BASE_KEYMAP=''
    _DGO_LIVE_VALUES=()
    _DGO_LIVE_DISPLAYS=()
    _DGO_LIVE_LABELS=()
    _DGO_LIVE_REPLACEMENTS=()
    zle -M ''
    zle -R -c
  }

  function _dgo_live_up() {
    (( _DGO_LIVE_SELECTED > 1 )) && (( _DGO_LIVE_SELECTED-- ))
    _dgo_live_render
  }

  function _dgo_live_down() {
    (( _DGO_LIVE_SELECTED < ${#_DGO_LIVE_VALUES} )) && (( _DGO_LIVE_SELECTED++ ))
    _dgo_live_render
  }

  function _dgo_live_line_finish() {
    _dgo_live_cancel_timer
    _dgo_live_restore_keys
    _DGO_LIVE_LAST_BUFFER=''
    _DGO_LIVE_SNAPSHOT=''
    _DGO_LIVE_BASE_KEYMAP=''
    zle -M ''
    zle -R -c
  }

  function _dgo_accept_suggestion() {
    local before="$LBUFFER" after="$RBUFFER" suggestion
    suggestion="$(printf '%s\0%s\0' "$before" "$after" | command dgo __suggest-shell --shell zsh --cwd "$PWD" 2>/dev/null)"
    if [[ -n "$suggestion" ]]; then
      BUFFER="${suggestion}${after}"
      CURSOR=${#suggestion}
    fi
  }
  function _dgo_pick_suggestion() {
    local before="$LBUFFER" after="$RBUFFER" suggestion result_file
    result_file="${TMPDIR:-/tmp}/dirgo-suggestion.$$.$RANDOM.$RANDOM"
    if command dgo __suggest-pick --shell zsh --cwd "$PWD" --request-path /dev/fd/3 --output-path "$result_file" 3< <(printf '%s\0%s\0' "$before" "$after"); then
      [[ -f "$result_file" ]] && suggestion="$(<"$result_file")"
    fi
    command rm -f -- "$result_file"
    if [[ -n "$suggestion" ]]; then
      BUFFER="${suggestion}${after}"
      CURSOR=${#suggestion}
    fi
  }

  zle -N _dgo_accept_suggestion
  zle -N _dgo_pick_suggestion
  zle -N _dgo_live_ready
  zle -N _dgo_live_accept
  zle -N _dgo_live_dismiss
  zle -N _dgo_live_up
  zle -N _dgo_live_down
  bindkey '^F' _dgo_accept_suggestion
  bindkey '^[[Z' _dgo_pick_suggestion
  if command dgo __suggest-live-enabled >/dev/null 2>&1; then
    add-zle-hook-widget line-pre-redraw _dgo_live_pre_redraw
    add-zle-hook-widget line-finish _dgo_live_line_finish
  fi
fi

if command dgo __suggest-history-enabled >/dev/null 2>&1 && [[ -o interactive ]]; then
  autoload -Uz add-zsh-hook 2>/dev/null
  function _dgo_record_suggestion_history() {
    setopt localoptions nobgnice
    printf '%s\n' "$1" | command dgo __suggest-record >/dev/null 2>&1 &!
  }
  add-zsh-hook preexec _dgo_record_suggestion_history
fi
"#;

const BASH: &str = r#"# Dirgo shell integration for bash
if [[ -z ${DGO_SESSION_ID:-} ]]; then
  export DGO_SESSION_ID="bash-$$-$RANDOM-$RANDOM"
fi

dgo() {
  case "${1:-}" in
    setup|init|completions|refresh|query|explain|bench|bookmarks|bookmark|import|doctor|stats|config|support|suggestions|update-notifications|--update|--open|--finder|--code|--copy|--print|--refresh|-r|--doctor|--bookmarks|--forget|--help|-h|--version|-V)
      command dgo "$@"
      return $?
      ;;
  esac

  local argument
  for argument in "$@"; do
    case "$argument" in
      --update|--open|--finder|--code|--copy|--print)
        command dgo "$@"
        return $?
        ;;
    esac
  done

  if [[ $# -eq 1 ]]; then
    if [[ "$1" == "-" ]]; then
      builtin cd -
      return $?
    elif [[ -d "$1" ]]; then
      builtin cd -- "$1"
      return $?
    fi
  fi

  local destination status
  destination="$(command dgo __resolve --cwd "$PWD" -- "$@")"
  status=$?
  if [[ $status -eq 10 ]]; then
    return 0
  elif [[ $status -eq 0 ]]; then
    builtin cd -- "$destination"
    return $?
  fi
  return $status
}

if command dgo __suggest-enabled >/dev/null 2>&1 && (( BASH_VERSINFO[0] >= 4 )); then
  source <(command dgo completions bash)
  _dgo_accept_suggestion() {
    local before after suggestion
    before="${READLINE_LINE:0:READLINE_POINT}"
    after="${READLINE_LINE:READLINE_POINT}"
    suggestion="$(printf '%s\0%s\0' "$before" "$after" | command dgo __suggest-shell --shell bash --cwd "$PWD" 2>/dev/null)"
    if [[ -n "$suggestion" ]]; then
      READLINE_LINE="${suggestion}${after}"
      READLINE_POINT=${#suggestion}
    fi
  }
  _dgo_pick_suggestion() {
    local before after suggestion result_file
    before="${READLINE_LINE:0:READLINE_POINT}"
    after="${READLINE_LINE:READLINE_POINT}"
    result_file="${TMPDIR:-/tmp}/dirgo-suggestion.$$.$RANDOM.$RANDOM"
    if command dgo __suggest-pick --shell bash --cwd "$PWD" --request-path /dev/fd/3 --output-path "$result_file" 3< <(printf '%s\0%s\0' "$before" "$after"); then
      [[ -f "$result_file" ]] && suggestion="$(<"$result_file")"
    fi
    command rm -f -- "$result_file"
    if [[ -n "$suggestion" ]]; then
      READLINE_LINE="${suggestion}${after}"
      READLINE_POINT=${#suggestion}
    fi
  }
  bind -x '"\C-f":_dgo_accept_suggestion'
  bind -x '"\e[Z":_dgo_pick_suggestion'
fi

if command dgo __suggest-history-enabled >/dev/null 2>&1; then
  _DGO_HISTORY_LAST=""
  _dgo_record_suggestion_history() {
    local previous_status=$? entry
    entry="$(HISTTIMEFORMAT= builtin history 1)"
    while [[ "$entry" == [[:space:]]* ]]; do entry="${entry#?}"; done
    while [[ "$entry" == [0-9]* ]]; do entry="${entry#?}"; done
    while [[ "$entry" == [[:space:]]* ]]; do entry="${entry#?}"; done
    if [[ -n "$entry" && "$entry" != "$_DGO_HISTORY_LAST" ]]; then
      _DGO_HISTORY_LAST="$entry"
      printf '%s\n' "$entry" | command dgo __suggest-record >/dev/null 2>&1 &
    fi
    return "$previous_status"
  }
  if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then
    PROMPT_COMMAND=(_dgo_record_suggestion_history "${PROMPT_COMMAND[@]}")
  elif [[ -n ${PROMPT_COMMAND:-} ]]; then
    PROMPT_COMMAND="_dgo_record_suggestion_history;${PROMPT_COMMAND}"
  else
    PROMPT_COMMAND="_dgo_record_suggestion_history"
  fi
fi
"#;

const FISH: &str = r#"# Dirgo shell integration for fish
if not set -q DGO_SESSION_ID
    set -gx DGO_SESSION_ID "fish-$fish_pid-"(random)"-"(random)
end

function dgo --description 'Go anywhere. Instantly.'
    if test (count $argv) -gt 0
        switch "$argv[1]"
            case setup init completions refresh query explain bench bookmarks bookmark import doctor stats config support suggestions update-notifications --update --open --finder --code --copy --print --refresh -r --doctor --bookmarks --forget --help -h --version -V
                command dgo $argv
                return $status
        end
    end

    for argument in $argv
        switch "$argument"
            case --update --open --finder --code --copy --print
                command dgo $argv
                return $status
        end
    end

    if test (count $argv) -eq 1
        if test "$argv[1]" = "-"
            builtin cd -
            return $status
        else if test -d "$argv[1]"
            builtin cd -- "$argv[1]"
            return $status
        end
    end

    set -l destination (command dgo __resolve --cwd "$PWD" -- $argv)
    set -l resolve_status $status
    if test $resolve_status -eq 10
        return 0
    else if test $resolve_status -eq 0
        builtin cd -- "$destination"
        return $status
    end
    return $resolve_status
end

if command dgo __suggest-enabled >/dev/null 2>&1
    command dgo completions fish | source
    function __dgo_accept_suggestion --description 'Insert a Dirgo suggestion'
        set -l buffer (commandline -b)
        set -l cursor (commandline -C)
        if test $cursor -ne (string length -- "$buffer")
            return
        end
        set -l suggestion (printf '%s\0\0' "$buffer" | command dgo __suggest-shell --shell fish --cwd "$PWD" 2>/dev/null)
        if test -n "$suggestion"
            commandline -r -- "$suggestion"
            commandline -C (string length -- "$suggestion")
        end
    end
    function __dgo_pick_suggestion --description 'Choose and insert a Dirgo suggestion'
        set -l buffer (commandline -b)
        set -l cursor (commandline -C)
        if test $cursor -ne (string length -- "$buffer")
            return
        end
        set -l temp_root /tmp
        if set -q TMPDIR
            set temp_root $TMPDIR
        end
        set -l result_file "$temp_root/dirgo-suggestion.$fish_pid."(random)"."(random)
        set -l suggestion
        if command dgo __suggest-pick --shell fish --cwd "$PWD" --request-path (printf '%s\0\0' "$buffer" | psub) --output-path "$result_file"; and test -f "$result_file"
            set suggestion (string collect <$result_file)
        end
        command rm -f -- "$result_file"
        if test -n "$suggestion"
            commandline -r -- "$suggestion"
            commandline -C (string length -- "$suggestion")
        end
    end
    bind \cf __dgo_accept_suggestion
    bind \e\[Z __dgo_pick_suggestion
end

if command dgo __suggest-history-enabled >/dev/null 2>&1
    function __dgo_record_suggestion_history --on-event fish_preexec
        printf '%s\n' "$argv[1]" | command dgo __suggest-record >/dev/null 2>&1 &
        disown
    end
end
"#;

const POWERSHELL: &str = r#"# Dirgo shell integration for PowerShell 7+
$global:DirgoExecutablePath = (Get-Command dgo -CommandType Application -ErrorAction Stop |
    Select-Object -First 1).Source
if (-not $env:DGO_SESSION_ID) {
    $env:DGO_SESSION_ID = "powershell-$PID-$([guid]::NewGuid().ToString('N'))"
}

function global:dgo {
    [CmdletBinding(PositionalBinding = $false)]
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$DirgoArguments)

    $management = @('setup', 'init', 'completions', 'refresh', 'query', 'explain', 'bench',
        'bookmarks', 'bookmark', 'import', 'doctor', 'stats', 'config', 'support',
        'suggestions', 'update-notifications', '--update', '--open', '--finder', '--code',
        '--copy', '--print', '--refresh', '-r', '--doctor', '--bookmarks', '--forget',
        '--help', '-h', '--version', '-V')
    if ($DirgoArguments.Count -gt 0 -and $management -contains $DirgoArguments[0]) {
        & $global:DirgoExecutablePath @DirgoArguments
        return
    }
    if ($DirgoArguments | Where-Object { $_ -in @('--update', '--open', '--finder', '--code', '--copy', '--print') }) {
        & $global:DirgoExecutablePath @DirgoArguments
        return
    }
    if ($DirgoArguments.Count -eq 1) {
        if ($DirgoArguments[0] -eq '-') {
            Set-Location -Path '-'
            $env:DGO_PREDICTOR_CWD = $ExecutionContext.SessionState.Path.CurrentFileSystemLocation.ProviderPath
            return
        }
        if (Test-Path -LiteralPath $DirgoArguments[0] -PathType Container) {
            Set-Location -LiteralPath $DirgoArguments[0]
            $env:DGO_PREDICTOR_CWD = $ExecutionContext.SessionState.Path.CurrentFileSystemLocation.ProviderPath
            return
        }
    }

    $destination = & $global:DirgoExecutablePath __resolve --cwd (Get-Location).Path -- @DirgoArguments
    $resolveStatus = $LASTEXITCODE
    if ($resolveStatus -eq 10) { return }
    if ($resolveStatus -eq 0 -and $null -ne $destination) {
        Set-Location -LiteralPath ([string]$destination)
        $env:DGO_PREDICTOR_CWD = $ExecutionContext.SessionState.Path.CurrentFileSystemLocation.ProviderPath
        return
    }
    $global:LASTEXITCODE = $resolveStatus
}

function global:Invoke-DirgoSuggestion {
    param([string]$BeforeCursor, [string]$AfterCursor)
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $global:DirgoExecutablePath
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.ArgumentList.Add('__suggest-shell')
    $start.ArgumentList.Add('--shell')
    $start.ArgumentList.Add('powershell')
    $start.ArgumentList.Add('--cwd')
    $start.ArgumentList.Add((Get-Location).Path)
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    if (-not $process.Start()) { return $null }
    $process.StandardInput.Write($BeforeCursor)
    $process.StandardInput.Write([char]0)
    $process.StandardInput.Write($AfterCursor)
    $process.StandardInput.Write([char]0)
    $process.StandardInput.Close()
    $replacement = $process.StandardOutput.ReadToEnd().TrimEnd("`r", "`n")
    $process.WaitForExit()
    if ($process.ExitCode -eq 0 -and $replacement -and $replacement.IndexOfAny(@([char]0, [char]10, [char]13)) -lt 0) {
        return $replacement
    }
    return $null
}

function global:Send-DirgoSuggestionHistory {
    param([string]$CommandLine)
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $global:DirgoExecutablePath
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardInput = $true
    $start.ArgumentList.Add('__suggest-record')
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    if ($process.Start()) {
        $process.StandardInput.WriteLine($CommandLine)
        $process.StandardInput.Close()
        $process.Dispose()
    }
}

$null = & $global:DirgoExecutablePath __suggest-enabled 2>$null
$suggestionsEnabled = $LASTEXITCODE -eq 0
if ($suggestionsEnabled -and (Get-Module -ListAvailable PSReadLine)) {
    $dirgoPredictorRegistered = $false
    $env:DGO_PREDICTOR_EXECUTABLE = $global:DirgoExecutablePath
    $env:DGO_PREDICTOR_CWD = $ExecutionContext.SessionState.Path.CurrentFileSystemLocation.ProviderPath
    if (-not (Get-Variable -Name DirgoPredictorCwdSubscription -Scope Global -ErrorAction SilentlyContinue)) {
        $global:DirgoPredictorCwdSubscription = Register-EngineEvent -SourceIdentifier PowerShell.OnIdle -SupportEvent -Action {
            $env:DGO_PREDICTOR_CWD = $ExecutionContext.SessionState.Path.CurrentFileSystemLocation.ProviderPath
        }
    }

    $dirgoVersion = (& $global:DirgoExecutablePath --version) -replace '^dgo\s+', ''
    $predictorManifest = Join-Path (Split-Path -Parent $global:DirgoExecutablePath) "DirgoPredictor/$dirgoVersion/DirgoPredictor.psd1"
    if (-not (Test-Path -LiteralPath $predictorManifest)) {
        $predictorManifest = Join-Path (Split-Path -Parent $global:DirgoExecutablePath) 'DirgoPredictor/DirgoPredictor.psd1'
    }
    $psReadLineVersion = (Get-Module -ListAvailable PSReadLine | Sort-Object Version -Descending | Select-Object -First 1).Version
    if ($PSVersionTable.PSVersion.Major -eq 7 -and
        $PSVersionTable.PSVersion.Minor -eq 4 -and
        $psReadLineVersion -ge [version]'2.2.2' -and
        (Test-Path -LiteralPath $predictorManifest)) {
        Import-Module $predictorManifest -ErrorAction SilentlyContinue
        $predictorSubsystem = [System.Management.Automation.Subsystem.SubsystemManager]::GetSubsystemInfo(
            [System.Management.Automation.Subsystem.SubsystemKind]::CommandPredictor)
        if ($predictorSubsystem.Implementations.Name -contains 'Dirgo' -and
            $Host.UI.SupportsVirtualTerminal) {
            $dirgoPredictorRegistered = $true
            Set-PSReadLineOption -PredictionSource HistoryAndPlugin
            Set-PSReadLineOption -PredictionViewStyle ListView
        }
    }

    Set-PSReadLineKeyHandler -Chord Ctrl+f -BriefDescription 'DirgoSuggestion' -LongDescription 'Insert a Dirgo suggestion without executing it' -ScriptBlock {
        param($key, $arg)
        $line = $null
        $cursor = 0
        [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)
        $replacement = Invoke-DirgoSuggestion -BeforeCursor $line.Substring(0, $cursor) -AfterCursor $line.Substring($cursor)
        if ($replacement) {
            [Microsoft.PowerShell.PSConsoleReadLine]::Replace(0, $cursor, $replacement)
        } elseif ($cursor -lt $line.Length) {
            [Microsoft.PowerShell.PSConsoleReadLine]::ForwardChar($key, $arg)
        }
    }

    $null = & $global:DirgoExecutablePath __suggest-history-enabled 2>$null
    if ($LASTEXITCODE -eq 0 -and -not $dirgoPredictorRegistered -and
        -not (Get-Variable -Name DirgoHistoryHandlerInstalled -Scope Global -ErrorAction SilentlyContinue)) {
        $global:DirgoPreviousHistoryHandler = (Get-PSReadLineOption).AddToHistoryHandler
        Set-PSReadLineOption -AddToHistoryHandler {
            param([string]$line)
            Send-DirgoSuggestionHistory -CommandLine $line
            if ($global:DirgoPreviousHistoryHandler) {
                return $global:DirgoPreviousHistoryHandler.Invoke($line)
            }
            return $true
        }
        $global:DirgoHistoryHandlerInstalled = $true
    }
}
"#;

const ZSH_COMPLETIONS: &str = r#"#compdef dgo
_dgo_bookmark_names() {
  local -a names
  names=(${(f)"$(command dgo bookmarks 2>/dev/null | sed -n 's/^@\([^ ]*\).*/\1/p')"})
  _describe 'bookmark' names
}
_dgo_live_engine_matches() {
  local value display label replacement
  local -a values labels
  while IFS= read -r -d $'\0' value &&
        IFS= read -r -d $'\0' display &&
        IFS= read -r -d $'\0' label &&
        IFS= read -r -d $'\0' replacement; do
    values+=("$value")
    labels+=("$label  $display")
  done < <(printf '%s\0%s\0' "$LBUFFER" "$RBUFFER" |
    command dgo __suggest-complete --shell zsh --cwd "$PWD" \
      --terminal-rows "$LINES" --terminal-columns "$COLUMNS" 2>/dev/null)
  (( ${#values} )) && compadd -Q -d labels -a values
}
_dgo() {
  local -a commands global_options
  commands=(
    'setup:connect Dirgo to this shell safely'
    'init:print shell integration' 'completions:print shell completion script'
    'refresh:rebuild the directory index' 'query:resolve a directory query'
    'explain:show ranked candidates as JSON' 'bench:measure local work'
    'root:go to the current project root' 'repo:find a repository'
    'recent:find recently visited directories' 'back:go back in session history'
    'forward:go forward in session history' 'import:import navigation history'
    'bookmarks:list bookmarks' 'bookmark:manage bookmarks' 'doctor:inspect configuration'
    'stats:show local statistics' 'config:inspect configuration' 'support:show support guidance'
    'suggestions:manage shell-native suggestions'
    'update-notifications:enable or disable update notices'
  )
  global_options=(
    '--open[open with the OS]' '--finder[open in file browser]' '--code[open in configured editor]'
    '--copy[copy path]' '--print[print path]' '--no-color[disable color]' '--no-unicode[use ASCII]'
    '--verbose[show diagnostics]' '--update[install the latest Dirgo release]'
    '--refresh[compatibility alias]' '--doctor[compatibility alias]'
    '--bookmarks[compatibility alias]' '--forget=[remove bookmark]:bookmark:_dgo_bookmark_names'
  )
  _arguments -C $global_options '1:command:->command' '*:query:->query'
  case $state in
    command) _describe 'command' commands; _dgo_live_engine_matches ;;
    query)
      case $words[2] in
        bookmark) _arguments '1:operation:(add remove rename)' '2:bookmark:_dgo_bookmark_names' ;;
        config) _arguments '1:operation:(path show)' ;;
        update-notifications) _arguments '1:mode:(on off)' ;;
        import) _arguments '1:source:(zoxide)' ;;
        init|completions) _arguments '1:shell:(zsh bash fish powershell)' ;;
        suggestions) _arguments '1:operation:(enable disable status doctor history)' ;;
      esac ;;
  esac
  [[ "$state" == query ]] && _dgo_live_engine_matches
}
compdef _dgo dgo
"#;

const BASH_COMPLETIONS: &str = r#"if [[ -z ${_DGO_PREVIOUS_COMPLETE_FUNCTION+x} ]]; then
  _DGO_PREVIOUS_COMPLETE_FUNCTION=''
  if [[ "$(complete -p dgo 2>/dev/null)" =~ -F[[:space:]]+([^[:space:]]+) ]]; then
    _DGO_PREVIOUS_COMPLETE_FUNCTION="${BASH_REMATCH[1]}"
  fi
fi

_dgo_bookmarks() {
  command dgo bookmarks 2>/dev/null | sed -n 's/^@\([^ ]*\).*/\1/p'
}
_dgo_complete() {
  local cur prev commands options candidate description
  local -a previous_replies=()
  if [[ -n "$_DGO_PREVIOUS_COMPLETE_FUNCTION" &&
        "$_DGO_PREVIOUS_COMPLETE_FUNCTION" != _dgo_complete &&
        $(type -t "$_DGO_PREVIOUS_COMPLETE_FUNCTION") == function ]]; then
    "$_DGO_PREVIOUS_COMPLETE_FUNCTION"
    previous_replies=("${COMPREPLY[@]}")
  fi
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"
  commands='setup init completions refresh query explain bench root repo recent back forward import bookmarks bookmark doctor stats config support suggestions update-notifications'
  options='--update --open --finder --code --copy --print --no-color --no-unicode --verbose --refresh --doctor --bookmarks --forget --help --version'
  case "$prev" in
    init|completions) COMPREPLY=( $(compgen -W 'zsh bash fish powershell' -- "$cur") ) ;;
    suggestions) COMPREPLY=( $(compgen -W 'enable disable status doctor history' -- "$cur") ) ;;
    import) COMPREPLY=( $(compgen -W 'zoxide' -- "$cur") ) ;;
    config) COMPREPLY=( $(compgen -W 'path show' -- "$cur") ) ;;
    update-notifications) COMPREPLY=( $(compgen -W 'on off' -- "$cur") ) ;;
    bookmark) COMPREPLY=( $(compgen -W 'add remove rename' -- "$cur") ) ;;
    remove|rename|--forget) COMPREPLY=( $(compgen -W "$(_dgo_bookmarks)" -- "$cur") ) ;;
    *) COMPREPLY=( $(compgen -W "$commands $options" -- "$cur") ) ;;
  esac
  COMPREPLY=("${previous_replies[@]}" "${COMPREPLY[@]}")
  while IFS=$'\t' read -r candidate description; do
    [[ -n "$candidate" ]] && COMPREPLY+=("$candidate")
  done < <(printf '%s\0%s\0' "${COMP_LINE:0:COMP_POINT}" "${COMP_LINE:COMP_POINT}" |
    command dgo __suggest-complete --shell bash --cwd "$PWD" --format lines \
      --terminal-rows "${LINES:-24}" --terminal-columns "${COLUMNS:-80}" 2>/dev/null)

  local -A seen=()
  local -a unique=()
  for candidate in "${COMPREPLY[@]}"; do
    [[ -n "$candidate" && -z ${seen["$candidate"]+x} ]] || continue
    seen["$candidate"]=1
    unique+=("$candidate")
  done
  COMPREPLY=("${unique[@]}")
}
complete -F _dgo_complete dgo
"#;

const FISH_COMPLETIONS: &str = r#"function __dgo_bookmarks
    command dgo bookmarks 2>/dev/null | string replace -r '^@([^ ]+).*' '$1'
end
function __dgo_live_candidates
    set -l buffer (commandline -cp)
    printf '%s\0\0' "$buffer" | command dgo __suggest-complete --shell fish --cwd "$PWD" \
        --format lines --terminal-rows "$LINES" --terminal-columns "$COLUMNS" 2>/dev/null
end
complete -c dgo -a '(__dgo_live_candidates)'
complete -c dgo -n '__fish_use_subcommand' -a 'setup init completions refresh query explain bench root repo recent back forward import bookmarks bookmark doctor stats config support suggestions update-notifications'
complete -c dgo -l update -d 'Install the latest Dirgo release'
complete -c dgo -l open -d 'Open with the OS'
complete -c dgo -l finder -d 'Open in file browser'
complete -c dgo -l code -d 'Open in configured editor'
complete -c dgo -l copy -d 'Copy path'
complete -c dgo -l print -d 'Print path'
complete -c dgo -l no-color -d 'Disable color'
complete -c dgo -l no-unicode -d 'Use ASCII'
complete -c dgo -l verbose -d 'Show diagnostics'
complete -c dgo -l forget -a '(__dgo_bookmarks)' -d 'Remove bookmark'
complete -c dgo -n '__fish_seen_subcommand_from init completions' -a 'zsh bash fish powershell'
complete -c dgo -n '__fish_seen_subcommand_from import' -a zoxide
complete -c dgo -n '__fish_seen_subcommand_from config' -a 'path show'
complete -c dgo -n '__fish_seen_subcommand_from update-notifications' -a 'on off'
complete -c dgo -n '__fish_seen_subcommand_from bookmark' -a 'add remove rename'
complete -c dgo -n '__fish_seen_subcommand_from suggestions' -a 'enable disable status doctor history'
complete -c dgo -n '__fish_seen_subcommand_from remove rename' -a '(__dgo_bookmarks)'
"#;

const POWERSHELL_COMPLETIONS: &str = r#"Register-ArgumentCompleter -Native -CommandName dgo -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $commands = @('setup', 'init', 'completions', 'refresh', 'query', 'explain', 'bench',
        'root', 'repo', 'recent', 'back', 'forward', 'import', 'bookmarks', 'bookmark',
        'doctor', 'stats', 'config', 'support', 'suggestions', 'update-notifications')
    foreach ($command in $commands) {
        if ($command.StartsWith($wordToComplete, [System.StringComparison]::OrdinalIgnoreCase)) {
            [System.Management.Automation.CompletionResult]::new($command, $command, 'ParameterValue', $command)
        }
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrappers_use_builtin_for_directories_and_command_for_binary() {
        for shell in [Shell::Zsh, Shell::Bash, Shell::Fish, Shell::PowerShell] {
            let script = integration(shell);
            assert!(script.contains("builtin cd") || script.contains("Set-Location -LiteralPath"));
            assert!(script.contains("command dgo __resolve") || script.contains("__resolve --cwd"));
            assert!(
                script.contains("--update|--open|--finder|--code|--copy|--print")
                    || script.contains("case --update --open --finder --code --copy --print")
                    || script.contains(
                        "'--update', '--open', '--finder', '--code', '--copy', '--print'"
                    )
            );
            assert!(!script.contains("eval $destination"));
        }
    }

    #[test]
    fn powershell_loads_the_binary_predictor_with_a_safe_key_handler_fallback() {
        let script = integration(Shell::PowerShell);
        assert!(script.contains("DirgoPredictor/$dirgoVersion/DirgoPredictor.psd1"));
        assert!(script.contains("Get-Command dgo -CommandType Application"));
        assert!(script.contains("Select-Object -First 1"));
        assert!(script.contains("SubsystemManager]::GetSubsystemInfo"));
        assert!(script.contains("Set-PSReadLineOption -PredictionSource HistoryAndPlugin"));
        assert!(script.contains("Set-PSReadLineOption -PredictionViewStyle ListView"));
        assert!(script.contains("Set-PSReadLineKeyHandler -Chord Ctrl+f"));
        assert!(!script.contains("AcceptLine"));
    }

    #[test]
    fn unix_picker_reads_a_result_only_after_successful_private_creation() {
        for shell in [Shell::Zsh, Shell::Bash] {
            let script = integration(shell);
            assert!(script.contains("if command dgo __suggest-pick"));
            assert!(script.contains("then\n      [[ -f \"$result_file\" ]]"));
        }
        let fish = integration(Shell::Fish);
        assert!(fish.contains("if command dgo __suggest-pick --shell fish"));
        assert!(fish.contains("; and test -f \"$result_file\""));
    }

    #[test]
    fn zsh_live_panel_uses_redraw_hooks_and_a_stale_safe_native_timer() {
        let script = integration(Shell::Zsh);
        assert!(script.contains("add-zle-hook-widget line-pre-redraw _dgo_live_pre_redraw"));
        assert!(script.contains("add-zle-hook-widget line-finish _dgo_live_line_finish"));
        assert!(script.contains("zle -F"));
        assert!(script.contains("sleep 0.03"));
        assert!(script.contains("_DGO_LIVE_GENERATION"));
        assert!(script.contains("_DGO_LIVE_SNAPSHOT"));
        assert!(script.contains("zle -M \"$panel\""));
        assert!(script.contains("function _dgo_live_accept()"));
        assert!(script.contains("function _dgo_live_dismiss()"));
        assert!(!script.contains("bindkey 'a'"));
        assert!(!script.contains("bindkey ' '"));
    }

    #[test]
    fn fish_and_bash_enrich_native_completion_without_replacing_editor_behavior() {
        let fish = completions(Shell::Fish);
        assert!(fish.contains("function __dgo_live_candidates"));
        assert!(fish.contains("__suggest-complete --shell fish"));
        assert!(!fish.contains("set fish_autosuggestion_enabled"));
        assert!(!fish.contains("complete -c dgo -f"));

        let bash = completions(Shell::Bash);
        assert!(bash.contains("_DGO_PREVIOUS_COMPLETE_FUNCTION"));
        assert!(bash.contains("__suggest-complete --shell bash"));
        assert!(bash.contains("complete -F _dgo_complete dgo"));
        assert!(!bash.contains("bind -x 'a"));
        assert!(!bash.contains("bind -x ' "));

        assert!(integration(Shell::Fish).contains("command dgo completions fish | source"));
        assert!(integration(Shell::Bash).contains("source <(command dgo completions bash)"));
    }
}
