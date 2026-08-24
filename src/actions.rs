use std::{
    env,
    ffi::{OsStr, OsString},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{DirgoError, Result, config::ActionConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Go,
    Print,
    Open,
    Copy,
    Editor,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Availability {
    pub open: bool,
    pub copy: bool,
    pub editor: bool,
}

pub fn availability(config: &ActionConfig) -> Availability {
    Availability {
        open: open_command().is_some(),
        copy: copy_command().is_some(),
        editor: editor_command(config).is_ok(),
    }
}

pub fn execute(action: Action, path: &Path, config: &ActionConfig) -> Result<()> {
    match action {
        Action::Open => run_path_command(
            open_command().ok_or_else(|| unavailable("open", open_install_hint()))?,
            path,
            "open the directory",
        ),
        Action::Editor => run_path_command(editor_command(config)?, path, "open the editor"),
        Action::Copy => copy_path(path),
        Action::Go | Action::Print => Ok(()),
    }
}

fn run_path_command(command: CommandSpec, path: &Path, description: &str) -> Result<()> {
    let status = Command::new(&command.program)
        .args(&command.args)
        .arg(path)
        .status()
        .map_err(|error| DirgoError::io(PathBuf::from(&command.program), error))?;
    if status.success() {
        Ok(())
    } else {
        Err(DirgoError::User(format!(
            "Dirgo could not {description}: {} exited with {status}",
            command.program.to_string_lossy()
        )))
    }
}

fn copy_path(path: &Path) -> Result<()> {
    let command = copy_command().ok_or_else(|| unavailable("copy", copy_install_hint()))?;
    let mut child = Command::new(&command.program)
        .args(&command.args)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| DirgoError::io(PathBuf::from(&command.program), error))?;
    child
        .stdin
        .take()
        .ok_or_else(|| DirgoError::User("Dirgo could not open the clipboard input".into()))?
        .write_all(path_for_clipboard(path).as_ref())
        .map_err(|error| DirgoError::io("clipboard", error))?;
    let status = child
        .wait()
        .map_err(|error| DirgoError::io(PathBuf::from(&command.program), error))?;
    if status.success() {
        Ok(())
    } else {
        Err(DirgoError::User(format!(
            "Dirgo could not copy the path: {} exited with {status}",
            command.program.to_string_lossy()
        )))
    }
}

#[derive(Debug)]
struct CommandSpec {
    program: OsString,
    args: Vec<OsString>,
}

fn open_command() -> Option<CommandSpec> {
    #[cfg(target_os = "macos")]
    let candidates: [(&str, &[&str]); 1] = [("open", &[])];
    #[cfg(target_os = "windows")]
    let candidates: [(&str, &[&str]); 1] = [("explorer.exe", &[])];
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let candidates: [(&str, &[&str]); 1] = [("xdg-open", &[])];
    candidates
        .into_iter()
        .find(|(program, _)| executable_exists(program))
        .map(|(program, args)| CommandSpec {
            program: program.into(),
            args: args.iter().map(OsString::from).collect(),
        })
}

fn copy_command() -> Option<CommandSpec> {
    #[cfg(target_os = "macos")]
    let candidates: [(&str, &[&str]); 1] = [("pbcopy", &[])];
    #[cfg(target_os = "windows")]
    let candidates: [(&str, &[&str]); 1] = [(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::InputEncoding=[System.Text.Encoding]::UTF8; Set-Clipboard -Value ([Console]::In.ReadToEnd())",
        ],
    )];
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let candidates: [(&str, &[&str]); 2] =
        [("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])];
    candidates
        .into_iter()
        .find(|(program, _)| executable_exists(program))
        .map(|(program, args)| CommandSpec {
            program: program.into(),
            args: args.iter().map(OsString::from).collect(),
        })
}

#[cfg(not(target_os = "windows"))]
fn path_for_clipboard(path: &Path) -> std::borrow::Cow<'_, [u8]> {
    std::borrow::Cow::Borrowed(path.as_os_str().as_encoded_bytes())
}

#[cfg(target_os = "windows")]
fn path_for_clipboard(path: &Path) -> std::borrow::Cow<'_, [u8]> {
    std::borrow::Cow::Owned(path.to_string_lossy().as_bytes().to_vec())
}

fn editor_command(config: &ActionConfig) -> Result<CommandSpec> {
    let configured = config.editor.trim();
    if configured != "auto" {
        return validated_editor(configured);
    }
    for variable in ["VISUAL", "EDITOR"] {
        if let Some(value) = env::var_os(variable) {
            let value = value.to_string_lossy();
            if let Ok(command) = validated_editor(value.trim()) {
                return Ok(command);
            }
        }
    }
    for candidate in ["code", "cursor", "zed"] {
        if executable_exists(candidate) {
            return Ok(CommandSpec {
                program: candidate.into(),
                args: Vec::new(),
            });
        }
    }
    Err(unavailable(
        "open an editor",
        "set actions.editor to an installed executable",
    ))
}

fn validated_editor(value: &str) -> Result<CommandSpec> {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return Err(DirgoError::Config(
            "actions.editor must be one executable path without arguments".into(),
        ));
    }
    if !executable_exists(value) {
        return Err(unavailable(
            "open an editor",
            &format!("install {value} or change actions.editor"),
        ));
    }
    Ok(CommandSpec {
        program: value.into(),
        args: Vec::new(),
    })
}

fn executable_exists(program: impl AsRef<OsStr>) -> bool {
    let program = Path::new(program.as_ref());
    if program.components().count() > 1 {
        return is_executable(program);
    }
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|directory| is_executable(&directory.join(program)))
    })
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn unavailable(action: &str, hint: &str) -> DirgoError {
    DirgoError::User(format!("Dirgo cannot {action} on this system. {hint}."))
}

#[cfg(target_os = "macos")]
fn open_install_hint() -> &'static str {
    "the macOS `open` utility is missing"
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
fn open_install_hint() -> &'static str {
    "install xdg-utils"
}

#[cfg(target_os = "windows")]
fn open_install_hint() -> &'static str {
    "the Windows `explorer.exe` utility is missing"
}

#[cfg(target_os = "macos")]
fn copy_install_hint() -> &'static str {
    "the macOS `pbcopy` utility is missing"
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
fn copy_install_hint() -> &'static str {
    "install wl-clipboard or xclip"
}

#[cfg(target_os = "windows")]
fn copy_install_hint() -> &'static str {
    "Windows PowerShell with Set-Clipboard is required"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_config_never_accepts_a_command_line() {
        let config = ActionConfig {
            editor: "code --reuse-window".into(),
        };
        let error = editor_command(&config).expect_err("arguments must be rejected");
        assert!(error.to_string().contains("without arguments"));
    }
}
