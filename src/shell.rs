use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

impl Shell {
    pub fn name(self) -> &'static str {
        match self {
            Self::Zsh => "zsh",
            Self::Bash => "bash",
            Self::Fish => "fish",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Zsh => "Zsh",
            Self::Bash => "Bash",
            Self::Fish => "Fish",
        }
    }
}

pub fn integration(shell: Shell) -> &'static str {
    match shell {
        Shell::Zsh => ZSH,
        Shell::Bash => BASH,
        Shell::Fish => FISH,
    }
}

pub fn completions(shell: Shell) -> String {
    match shell {
        Shell::Zsh => ZSH_COMPLETIONS.into(),
        Shell::Bash => BASH_COMPLETIONS.into(),
        Shell::Fish => FISH_COMPLETIONS.into(),
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
    setup|init|completions|refresh|query|explain|bench|bookmarks|bookmark|import|doctor|stats|config|support|--open|--finder|--code|--copy|--print|--refresh|-r|--doctor|--bookmarks|--forget|--help|-h|--version|-V)
      command dgo "$@"
      return $?
      ;;
  esac

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
"#;

const BASH: &str = r#"# Dirgo shell integration for bash
if [[ -z ${DGO_SESSION_ID:-} ]]; then
  export DGO_SESSION_ID="bash-$$-$RANDOM-$RANDOM"
fi

dgo() {
  case "${1:-}" in
    setup|init|completions|refresh|query|explain|bench|bookmarks|bookmark|import|doctor|stats|config|support|--open|--finder|--code|--copy|--print|--refresh|-r|--doctor|--bookmarks|--forget|--help|-h|--version|-V)
      command dgo "$@"
      return $?
      ;;
  esac

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
"#;

const FISH: &str = r#"# Dirgo shell integration for fish
if not set -q DGO_SESSION_ID
    set -gx DGO_SESSION_ID "fish-$fish_pid-"(random)"-"(random)
end

function dgo --description 'Go anywhere. Instantly.'
    if test (count $argv) -gt 0
        switch "$argv[1]"
            case setup init completions refresh query explain bench bookmarks bookmark import doctor stats config support --open --finder --code --copy --print --refresh -r --doctor --bookmarks --forget --help -h --version -V
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
"#;

const ZSH_COMPLETIONS: &str = r#"#compdef dgo
_dgo_bookmark_names() {
  local -a names
  names=(${(f)"$(command dgo bookmarks 2>/dev/null | sed -n 's/^@\([^ ]*\).*/\1/p')"})
  _describe 'bookmark' names
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
  )
  global_options=(
    '--open[open with the OS]' '--finder[open in file browser]' '--code[open in configured editor]'
    '--copy[copy path]' '--print[print path]' '--no-color[disable color]' '--no-unicode[use ASCII]'
    '--verbose[show diagnostics]' '--refresh[compatibility alias]' '--doctor[compatibility alias]'
    '--bookmarks[compatibility alias]' '--forget=[remove bookmark]:bookmark:_dgo_bookmark_names'
  )
  _arguments -C $global_options '1:command:->command' '*:query:->query'
  case $state in
    command) _describe 'command' commands ;;
    query)
      case $words[2] in
        bookmark) _arguments '1:operation:(add remove rename)' '2:bookmark:_dgo_bookmark_names' ;;
        config) _arguments '1:operation:(path show)' ;;
        import) _arguments '1:source:(zoxide)' ;;
        init|completions) _arguments '1:shell:(zsh bash fish)' ;;
      esac ;;
  esac
}
compdef _dgo dgo
"#;

const BASH_COMPLETIONS: &str = r#"_dgo_bookmarks() {
  command dgo bookmarks 2>/dev/null | sed -n 's/^@\([^ ]*\).*/\1/p'
}
_dgo_complete() {
  local cur prev commands options
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"
  commands='setup init completions refresh query explain bench root repo recent back forward import bookmarks bookmark doctor stats config support'
  options='--open --finder --code --copy --print --no-color --no-unicode --verbose --refresh --doctor --bookmarks --forget --help --version'
  case "$prev" in
    init|completions) COMPREPLY=( $(compgen -W 'zsh bash fish' -- "$cur") ); return ;;
    import) COMPREPLY=( $(compgen -W 'zoxide' -- "$cur") ); return ;;
    config) COMPREPLY=( $(compgen -W 'path show' -- "$cur") ); return ;;
    bookmark) COMPREPLY=( $(compgen -W 'add remove rename' -- "$cur") ); return ;;
    remove|rename|--forget) COMPREPLY=( $(compgen -W "$(_dgo_bookmarks)" -- "$cur") ); return ;;
  esac
  COMPREPLY=( $(compgen -W "$commands $options" -- "$cur") )
}
complete -F _dgo_complete dgo
"#;

const FISH_COMPLETIONS: &str = r#"function __dgo_bookmarks
    command dgo bookmarks 2>/dev/null | string replace -r '^@([^ ]+).*' '$1'
end
complete -c dgo -f
complete -c dgo -n '__fish_use_subcommand' -a 'setup init completions refresh query explain bench root repo recent back forward import bookmarks bookmark doctor stats config support'
complete -c dgo -l open -d 'Open with the OS'
complete -c dgo -l finder -d 'Open in file browser'
complete -c dgo -l code -d 'Open in configured editor'
complete -c dgo -l copy -d 'Copy path'
complete -c dgo -l print -d 'Print path'
complete -c dgo -l no-color -d 'Disable color'
complete -c dgo -l no-unicode -d 'Use ASCII'
complete -c dgo -l verbose -d 'Show diagnostics'
complete -c dgo -l forget -a '(__dgo_bookmarks)' -d 'Remove bookmark'
complete -c dgo -n '__fish_seen_subcommand_from init completions' -a 'zsh bash fish'
complete -c dgo -n '__fish_seen_subcommand_from import' -a zoxide
complete -c dgo -n '__fish_seen_subcommand_from config' -a 'path show'
complete -c dgo -n '__fish_seen_subcommand_from bookmark' -a 'add remove rename'
complete -c dgo -n '__fish_seen_subcommand_from remove rename' -a '(__dgo_bookmarks)'
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrappers_use_builtin_for_directories_and_command_for_binary() {
        for shell in [Shell::Zsh, Shell::Bash, Shell::Fish] {
            let script = integration(shell);
            assert!(script.contains("builtin cd"));
            assert!(script.contains("command dgo __resolve"));
            assert!(!script.contains("eval $destination"));
        }
    }
}
