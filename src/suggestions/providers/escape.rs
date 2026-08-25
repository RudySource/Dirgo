use std::borrow::Cow;

use super::super::ShellKind;

pub fn escape_token(shell: ShellKind, value: &str) -> Cow<'_, str> {
    match shell {
        ShellKind::PowerShell => Cow::Owned(format!("'{}'", value.replace('\'', "''"))),
        ShellKind::Zsh | ShellKind::Bash | ShellKind::Fish => {
            shell_escape::unix::escape(value.into())
        }
    }
}
