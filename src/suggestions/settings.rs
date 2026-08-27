use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{DirgoError, Result, config::Config, model::unix_now};

pub fn write_suggestions_config(path: &Path, config: &Config) -> Result<()> {
    config.validate()?;
    let existing = if path.exists() {
        fs::read_to_string(path).map_err(|error| DirgoError::io(path, error))?
    } else {
        toml::to_string_pretty(config).map_err(|error| DirgoError::Config(error.to_string()))?
    };
    let section = render_section(config)?;
    let updated = replace_section(&existing, "suggestions", &section);
    atomic_write(path, updated.as_bytes())
}

fn render_section(config: &Config) -> Result<String> {
    let body = toml::to_string_pretty(&config.suggestions)
        .map_err(|error| DirgoError::Config(error.to_string()))?;
    Ok(format!("[suggestions]\n{body}"))
}

fn replace_section(input: &str, name: &str, replacement: &str) -> String {
    let header = format!("[{name}]");
    let lines: Vec<&str> = input.lines().collect();
    let start = lines.iter().position(|line| line.trim() == header);
    let mut output = String::new();
    if let Some(start) = start {
        let end = lines
            .iter()
            .enumerate()
            .skip(start + 1)
            .find_map(|(index, line)| {
                let line = line.trim();
                (line.starts_with('[') && line.ends_with(']')).then_some(index)
            })
            .unwrap_or(lines.len());
        for line in &lines[..start] {
            output.push_str(line);
            output.push('\n');
        }
        output.push_str(replacement.trim_end());
        output.push('\n');
        for line in &lines[end..] {
            output.push_str(line);
            output.push('\n');
        }
    } else {
        output.push_str(input.trim_end());
        output.push_str("\n\n");
        output.push_str(replacement.trim_end());
        output.push('\n');
    }
    output
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        DirgoError::User(format!(
            "configuration path has no parent: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| DirgoError::io(parent, error))?;
    let (temporary, mut file) = create_temporary(parent)?;
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
    replace_file(&temporary, path)
}

fn temporary_path(parent: &Path) -> PathBuf {
    parent.join(format!(
        ".dirgo-config-{}-{}.tmp",
        std::process::id(),
        unix_now()
    ))
}

fn create_temporary(parent: &Path) -> Result<(PathBuf, fs::File)> {
    let base = temporary_path(parent);
    for attempt in 0..1_000_u16 {
        let candidate = if attempt == 0 {
            base.clone()
        } else {
            parent.join(format!(
                "{}.{}",
                base.file_name().unwrap_or_default().to_string_lossy(),
                attempt
            ))
        };
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(DirgoError::io(&candidate, error)),
        }
    }
    Err(DirgoError::User(
        "could not allocate a temporary configuration file".into(),
    ))
}

#[cfg(unix)]
pub(super) fn replace_file(temporary: &Path, path: &Path) -> Result<()> {
    fs::rename(temporary, path).map_err(|error| {
        let _ = fs::remove_file(temporary);
        DirgoError::io(path, error)
    })
}

#[cfg(windows)]
pub(super) fn replace_file(temporary: &Path, path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source: Vec<u16> = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let destination: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: both pointers reference NUL-terminated UTF-16 buffers that live
    // for the duration of the call. The flags request an atomic replacement
    // on the same volume and flush the published directory entry.
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        let error = std::io::Error::last_os_error();
        let _ = fs::remove_file(temporary);
        return Err(DirgoError::io(path, error));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_config_files_are_collision_safe() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (first_path, first_file) = create_temporary(temp.path()).expect("first temporary");
        let (second_path, second_file) = create_temporary(temp.path()).expect("second temporary");

        assert_ne!(first_path, second_path);
        drop(first_file);
        drop(second_file);
    }
}
