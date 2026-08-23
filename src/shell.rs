use std::fmt::Write;

use crate::{DirgoError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

pub fn integration(shell: Shell) -> &'static str {
    match shell {
        Shell::Zsh => ZSH,
        Shell::Bash => BASH,
        Shell::Fish => FISH,
    }
}

pub fn completions(shell: Shell) -> String {
    let mut output = String::new();
    match shell {
        Shell::Zsh => {
            writeln!(output, "#compdef dgo\n_dgo() {{\n  local -a commands\n  commands=(refresh root repo recent back forward bookmarks bookmark doctor stats query config support)\n  _describe 'command' commands\n}}\ncompdef _dgo dgo").ok();
        }
        Shell::Bash => {
            writeln!(output, "_dgo_complete() {{\n  COMPREPLY=( $(compgen -W 'refresh root repo recent back forward bookmarks bookmark doctor stats query config support' -- \"${{COMP_WORDS[COMP_CWORD]}}\") )\n}}\ncomplete -F _dgo_complete dgo").ok();
        }
        Shell::Fish => {
            writeln!(output, "complete -c dgo -f -a 'refresh root repo recent back forward bookmarks bookmark doctor stats query config support'").ok();
        }
    }
    output
}

pub fn validate_output_path(path: &std::path::Path) -> Result<()> {
    if path.to_string_lossy().contains('\n') {
        Err(DirgoError::NewlinePath)
    } else {
        Ok(())
    }
}

const ZSH: &str = r#"# Dirgo shell integration for zsh
if [[ -z ${DGO_SESSION_ID:-} ]]; then
  export DGO_SESSION_ID="zsh-$$-$RANDOM-$RANDOM"
fi

function dgo() {
  case "${1:-}" in
    init|completions|refresh|query|bookmarks|bookmark|doctor|stats|config|support|--open|--finder|--code|--copy|--print|--refresh|-r|--doctor|--bookmarks|--forget|--help|-h|--version|-V)
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
    init|completions|refresh|query|bookmarks|bookmark|doctor|stats|config|support|--open|--finder|--code|--copy|--print|--refresh|-r|--doctor|--bookmarks|--forget|--help|-h|--version|-V)
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
            case init completions refresh query bookmarks bookmark doctor stats config support --open --finder --code --copy --print --refresh -r --doctor --bookmarks --forget --help -h --version -V
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
