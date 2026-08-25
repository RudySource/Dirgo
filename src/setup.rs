use std::{
    env, fs,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{DirgoError, Result, cli::SetupArgs, paths::AppPaths, shell::Shell, terminal};

const START_MARKER: &str = "# >>> dirgo setup >>>";
const END_MARKER: &str = "# <<< dirgo setup <<<";

#[derive(Debug, Serialize, Deserialize)]
struct SetupReceipt {
    schema_version: u8,
    shell: String,
    rc_file: PathBuf,
    backup_file: Option<PathBuf>,
    updated_at: u64,
}

pub fn run(paths: &AppPaths, args: &SetupArgs, no_color: bool, no_unicode: bool) -> Result<i32> {
    let shell = args.shell.or_else(detect_shell).ok_or_else(|| {
        DirgoError::User(
            "Dirgo could not detect a supported shell; use `dgo setup --shell zsh|bash|fish|powershell`"
                .into(),
        )
    })?;
    let requested_rc = args.rc.clone().unwrap_or(default_rc_file(shell)?);
    let rc_file = resolve_rc_target(&requested_rc)?;
    let old = read_optional(&rc_file)?;
    let managed_block = integration_block(shell)?;
    let new = if args.remove {
        remove_managed_block(&old)?.unwrap_or_else(|| old.clone())
    } else {
        upsert_managed_block(&old, &managed_block)?
    };
    let changed = new != old;

    print_preview(
        shell,
        &rc_file,
        &managed_block,
        args.remove,
        changed,
        no_color,
    );
    if args.dry_run || !changed {
        if !changed {
            println!(
                "\n{}",
                if args.remove {
                    "Nothing to remove."
                } else {
                    "Ready. Dirgo is already connected."
                }
            );
        }
        return Ok(0);
    }

    if !args.yes && !confirm()? {
        println!("\nNo changes made.");
        return Ok(0);
    }

    if read_optional(&rc_file)? != old {
        return Err(DirgoError::User(format!(
            "{} changed while setup was waiting; review and rerun `dgo setup`",
            terminal::safe_path(&rc_file)
        )));
    }

    paths.ensure_dirs()?;
    let backup = if rc_file.exists() {
        Some(create_backup(&rc_file)?)
    } else {
        None
    };
    atomic_write(&rc_file, new.as_bytes())?;

    let receipt_path = paths.state_dir.join(format!("setup-{}.json", shell.name()));
    if args.remove {
        match fs::remove_file(&receipt_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(DirgoError::io(&receipt_path, error)),
        }
    } else {
        let receipt = SetupReceipt {
            schema_version: 1,
            shell: shell.name().into(),
            rc_file: rc_file.clone(),
            backup_file: backup.clone(),
            updated_at: unix_now()?,
        };
        let encoded = serde_json::to_vec_pretty(&receipt)?;
        atomic_write(&receipt_path, &encoded)?;
    }

    println!();
    println!(
        "{} {} detected",
        check(no_color, no_unicode),
        shell.display_name()
    );
    if let Some(path) = &backup {
        println!(
            "{} Backup saved to {}",
            check(no_color, no_unicode),
            display_path(path)
        );
    }
    println!(
        "{} Shell {}",
        check(no_color, no_unicode),
        if args.remove {
            "disconnected"
        } else {
            "connected"
        }
    );
    println!(
        "\n{}",
        if args.remove {
            "Done. Open a new terminal to finish removing Dirgo from this shell."
        } else {
            "Ready. Open a new terminal and run dgo."
        }
    );
    Ok(0)
}

fn detect_shell() -> Option<Shell> {
    if cfg!(windows) && env::var_os("PSModulePath").is_some() {
        return Some(Shell::PowerShell);
    }
    let executable = PathBuf::from(env::var_os("SHELL")?);
    match executable.file_name()?.to_string_lossy().as_ref() {
        "zsh" => Some(Shell::Zsh),
        "bash" => Some(Shell::Bash),
        "fish" => Some(Shell::Fish),
        "pwsh" | "powershell" | "powershell.exe" => Some(Shell::PowerShell),
        _ => None,
    }
}

fn default_rc_file(shell: Shell) -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| DirgoError::User("Dirgo could not determine your home directory".into()))?;
    Ok(match shell {
        Shell::Zsh => env::var_os("ZDOTDIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    home.join(path)
                }
            })
            .unwrap_or_else(|| home.clone())
            .join(".zshrc"),
        Shell::Bash => home.join(".bashrc"),
        Shell::Fish => env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("fish/config.fish"),
        Shell::PowerShell => dirs::document_dir()
            .unwrap_or_else(|| home.join("Documents"))
            .join("PowerShell/Microsoft.PowerShell_profile.ps1"),
    })
}

fn resolve_rc_target(path: &Path) -> Result<PathBuf> {
    crate::paths::validate_shell_path(path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => path
            .canonicalize()
            .map_err(|error| DirgoError::io(path, error))
            .and_then(|target| {
                let metadata =
                    fs::metadata(&target).map_err(|error| DirgoError::io(&target, error))?;
                crate::paths::validate_shell_path(&target)?;
                if metadata.is_file() {
                    Ok(target)
                } else {
                    Err(DirgoError::User(format!(
                        "shell startup symlink does not point to a regular file: {}",
                        terminal::safe_path(path)
                    )))
                }
            }),
        Ok(metadata) if metadata.is_file() => Ok(path.to_path_buf()),
        Ok(_) => Err(DirgoError::User(format!(
            "shell startup path is not a regular file: {}",
            terminal::safe_path(path)
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(DirgoError::io(path, error)),
    }
}

fn read_optional(path: &Path) -> Result<String> {
    match fs::File::open(path) {
        Ok(mut file) => {
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .map_err(|error| DirgoError::io(path, error))?;
            Ok(contents)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(DirgoError::io(path, error)),
    }
}

fn integration_block(shell: Shell) -> Result<String> {
    let mut lines = vec![START_MARKER.to_string()];
    let binary_dir = effective_binary_dir()?;
    let escaped = shell_escape::escape(binary_dir.to_string_lossy()).into_owned();
    let powershell_path = binary_dir.to_string_lossy().replace('\'', "''");
    lines.push(match shell {
        Shell::Zsh | Shell::Bash => format!("export PATH={escaped}:\"$PATH\""),
        Shell::Fish => format!("fish_add_path --path {escaped}"),
        Shell::PowerShell => format!("$env:PATH = '{powershell_path};' + $env:PATH"),
    });
    lines.push(match shell {
        Shell::Zsh => "eval \"$(command dgo init zsh)\"".into(),
        Shell::Bash => "eval \"$(command dgo init bash)\"".into(),
        Shell::Fish => "command dgo init fish | source".into(),
        Shell::PowerShell => {
            "& ([scriptblock]::Create((& dgo init powershell | Out-String)))".into()
        }
    });
    lines.push(END_MARKER.to_string());
    Ok(lines.join("\n"))
}

fn effective_binary_dir() -> Result<PathBuf> {
    let current = env::current_exe().map_err(|error| DirgoError::io("dgo", error))?;
    let current = current.canonicalize().unwrap_or(current);
    let executable = if cfg!(windows) { "dgo.exe" } else { "dgo" };
    for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        if directory.as_os_str().is_empty() || !directory.is_absolute() {
            continue;
        }
        let candidate = directory.join(executable);
        if candidate.is_file() {
            let candidate = candidate.canonicalize().unwrap_or(candidate);
            if candidate == current {
                return Ok(directory);
            }
        }
    }
    current.parent().map(Path::to_path_buf).ok_or_else(|| {
        DirgoError::User("Dirgo could not determine its installation directory".into())
    })
}

fn upsert_managed_block(old: &str, block: &str) -> Result<String> {
    let without = remove_managed_block(old)?.unwrap_or_else(|| old.to_string());
    let trimmed = without.trim_end_matches(['\r', '\n']);
    Ok(if trimmed.is_empty() {
        format!("{block}\n")
    } else {
        format!("{trimmed}\n\n{block}\n")
    })
}

fn remove_managed_block(contents: &str) -> Result<Option<String>> {
    let starts = exact_marker_lines(contents, START_MARKER);
    let ends = exact_marker_lines(contents, END_MARKER);
    if starts.len() > 1 || ends.len() > 1 {
        return Err(DirgoError::User(
            "multiple Dirgo setup blocks were found; remove duplicates manually before rerunning setup"
                .into(),
        ));
    }
    let Some(&(start, _)) = starts.first() else {
        if !ends.is_empty() {
            return Err(DirgoError::User(
                "the Dirgo setup block is incomplete; repair the marker before running setup"
                    .into(),
            ));
        }
        return Ok(None);
    };
    let Some(&(end_start, end)) = ends.first() else {
        return Err(DirgoError::User(
            "the Dirgo setup block is incomplete; repair the marker before running setup".into(),
        ));
    };
    if end_start < start {
        return Err(DirgoError::User(
            "the Dirgo setup markers are out of order; repair them before running setup".into(),
        ));
    }
    let before = contents[..start].trim_end_matches(['\r', '\n']);
    let after = contents[end..].trim_start_matches(['\r', '\n']);
    let result = match (before.is_empty(), after.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("{before}\n"),
        (true, false) => after.to_string(),
        (false, false) => format!("{before}\n\n{after}"),
    };
    Ok(Some(result))
}

fn exact_marker_lines(contents: &str, marker: &str) -> Vec<(usize, usize)> {
    let mut offset = 0;
    contents
        .split_inclusive('\n')
        .filter_map(|line| {
            let start = offset;
            offset += line.len();
            (line.trim_end_matches(['\r', '\n']) == marker).then_some((start, offset))
        })
        .collect()
}

fn confirm() -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Err(DirgoError::User(
            "setup needs confirmation; rerun interactively or pass `--yes` after reviewing `dgo setup --dry-run`"
                .into(),
        ));
    }
    print!("\nApply this change? [Y/n] ");
    io::stdout()
        .flush()
        .map_err(|error| DirgoError::io("stdout", error))?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| DirgoError::io("stdin", error))?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "" | "y" | "yes"
    ))
}

fn print_preview(
    shell: Shell,
    rc_file: &Path,
    block: &str,
    remove: bool,
    changed: bool,
    no_color: bool,
) {
    let color = color_enabled(no_color);
    if color {
        println!("\x1b[1;34mDIRGO\x1b[0m\nGo anywhere. Instantly.\n");
    } else {
        println!("DIRGO\nGo anywhere. Instantly.\n");
    }
    println!("Shell   {}", shell.display_name());
    println!("File    {}", display_path(rc_file));
    println!(
        "Action  {}",
        if !changed {
            "No change"
        } else if remove {
            "Remove the managed block"
        } else {
            "Add or repair the managed block"
        }
    );
    if changed && !remove {
        println!("\n{block}");
    }
}

fn create_backup(path: &Path) -> Result<PathBuf> {
    let timestamp = unix_now()?;
    let mut backup = PathBuf::from(format!("{}.dirgo-backup-{timestamp}", path.display()));
    let mut suffix = 1_u32;
    while backup.exists() {
        backup = PathBuf::from(format!(
            "{}.dirgo-backup-{timestamp}.{suffix}",
            path.display()
        ));
        suffix += 1;
    }
    fs::copy(path, &backup).map_err(|error| DirgoError::io(&backup, error))?;
    Ok(backup)
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        DirgoError::User(format!("path has no parent: {}", terminal::safe_path(path)))
    })?;
    fs::create_dir_all(parent).map_err(|error| DirgoError::io(parent, error))?;
    let timestamp = unix_now()?;
    let mut temporary = parent.join(format!(".dirgo-setup-{timestamp}.tmp"));
    let mut suffix = 1_u32;
    while temporary.exists() {
        temporary = parent.join(format!(".dirgo-setup-{timestamp}.{suffix}.tmp"));
        suffix += 1;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| DirgoError::io(&temporary, error))?;
    if let Err(error) = file.write_all(contents).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(DirgoError::io(&temporary, error));
    }
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(&temporary, metadata.permissions())
            .map_err(|error| DirgoError::io(&temporary, error))?;
    } else {
        set_private_permissions(&temporary)?;
    }
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        DirgoError::io(path, error)
    })
}

fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir()
        && let Ok(relative) = path.strip_prefix(&home)
    {
        return if relative.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~/{}", terminal::safe_path(relative))
        };
    }
    terminal::safe_path(path)
}

fn color_enabled(no_color: bool) -> bool {
    !no_color
        && env::var_os("NO_COLOR").is_none()
        && env::var("TERM").is_ok_and(|term| term != "dumb")
        && io::stdout().is_terminal()
}

fn check(no_color: bool, no_unicode: bool) -> &'static str {
    if no_unicode || env::var("TERM").is_ok_and(|term| term == "dumb") {
        "OK"
    } else if color_enabled(no_color) {
        "\x1b[32m✓\x1b[0m"
    } else {
        "✓"
    }
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| DirgoError::io(path, error))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn unix_now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| DirgoError::User("system clock is earlier than the Unix epoch".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_replaces_and_removes_one_managed_block() {
        let first = upsert_managed_block(
            "export EDITOR=vim\n",
            "# >>> dirgo setup >>>\nnew\n# <<< dirgo setup <<<",
        )
        .expect("insert");
        assert!(first.starts_with("export EDITOR=vim\n\n"));
        let replaced = upsert_managed_block(
            &first,
            "# >>> dirgo setup >>>\nupdated\n# <<< dirgo setup <<<",
        )
        .expect("replace");
        assert_eq!(replaced.matches(START_MARKER).count(), 1);
        assert!(replaced.contains("updated"));
        assert!(!replaced.contains("\nnew\n"));
        assert_eq!(
            remove_managed_block(&replaced).expect("remove"),
            Some("export EDITOR=vim\n".into())
        );
    }

    #[test]
    fn refuses_incomplete_managed_block() {
        assert!(remove_managed_block(START_MARKER).is_err());
        let duplicate = format!("{START_MARKER}\na\n{END_MARKER}\n{START_MARKER}\nb\n{END_MARKER}");
        assert!(remove_managed_block(&duplicate).is_err());
        assert_eq!(
            remove_managed_block("echo '# >>> dirgo setup >>>'\nkeep me\n")
                .expect("inline marker text"),
            None
        );
    }

    #[test]
    fn powershell_managed_block_loads_generated_code_without_expression_evaluation() {
        let block = integration_block(Shell::PowerShell).expect("PowerShell block");
        assert!(block.contains("scriptblock]::Create"));
        assert!(block.contains("dgo init powershell"));
        assert!(!block.contains("Invoke-Expression"));
        assert_eq!(block.matches(START_MARKER).count(), 1);
        assert_eq!(block.matches(END_MARKER).count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn follows_a_shell_file_symlink_without_replacing_it() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("dotfiles/zshrc");
        fs::create_dir_all(target.parent().expect("parent")).expect("dotfiles");
        fs::write(&target, "existing\n").expect("target");
        let link = temp.path().join(".zshrc");
        symlink(&target, &link).expect("symlink");
        assert_eq!(
            resolve_rc_target(&link).expect("resolve"),
            target.canonicalize().expect("canonical target")
        );
        assert!(
            fs::symlink_metadata(link)
                .expect("metadata")
                .file_type()
                .is_symlink()
        );
    }
}
