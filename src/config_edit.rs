use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use toml_edit::DocumentMut;

use crate::{DirgoError, Result, config::Config};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigMutation {
    AddRoot(PathBuf),
    RemoveRoot(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationOutcome {
    pub changed: bool,
}

pub fn mutate_config(path: &Path, mutation: ConfigMutation) -> Result<MutationOutcome> {
    let mutation = match mutation {
        ConfigMutation::AddRoot(root) => ConfigMutation::AddRoot(validate_new_root(&root)?),
        ConfigMutation::RemoveRoot(root) => ConfigMutation::RemoveRoot(root),
    };
    let _lock = lock_config(path)?;
    reject_unsafe_target(path)?;
    let original = if path.exists() {
        fs::read_to_string(path).map_err(|error| DirgoError::io(path, error))?
    } else {
        toml::to_string_pretty(&Config::default())
            .map_err(|error| DirgoError::Config(error.to_string()))?
    };
    let mut document = original
        .parse::<DocumentMut>()
        .map_err(|error| DirgoError::Config(error.to_string()))?;
    let roots = document
        .get_mut("roots")
        .and_then(toml_edit::Item::as_array_mut)
        .ok_or_else(|| DirgoError::Config("roots must be a top-level array".into()))?;

    let changed = match mutation {
        ConfigMutation::AddRoot(root) => {
            let duplicate = roots
                .iter()
                .filter_map(toml_edit::Value::as_str)
                .any(|value| equivalent_path(Path::new(value), &root));
            if duplicate {
                false
            } else {
                roots.push(path_text(&root)?);
                true
            }
        }
        ConfigMutation::RemoveRoot(root) => {
            let before = roots.len();
            roots.retain(|value| {
                value
                    .as_str()
                    .is_none_or(|value| !equivalent_path(Path::new(value), &root))
            });
            if roots.is_empty() {
                return Err(DirgoError::Config(
                    "roots must contain at least one directory".into(),
                ));
            }
            roots.len() != before
        }
    };

    if !changed {
        return Ok(MutationOutcome { changed: false });
    }
    let updated = document.to_string();
    let config: Config =
        toml::from_str(&updated).map_err(|error| DirgoError::Config(error.to_string()))?;
    config.validate()?;
    atomic_write(path, updated.as_bytes())?;
    Ok(MutationOutcome { changed: true })
}

fn lock_config(path: &Path) -> Result<fs::File> {
    let parent = path.parent().ok_or_else(|| {
        DirgoError::User(format!(
            "configuration path has no parent: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| DirgoError::io(parent, error))?;
    let lock_path = parent.join(".dirgo-config.lock");
    if fs::symlink_metadata(&lock_path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(DirgoError::User(format!(
            "refusing to use symlink configuration lock: {}",
            lock_path.display()
        )));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options
        .open(&lock_path)
        .map_err(|error| DirgoError::io(&lock_path, error))?;
    fs2::FileExt::lock_exclusive(&file).map_err(|error| DirgoError::io(&lock_path, error))?;
    Ok(file)
}

fn reject_unsafe_target(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        return Err(DirgoError::User(format!(
            "refusing to update symlink configuration: {}",
            path.display()
        )));
    }
    if !metadata.file_type().is_file() {
        return Err(DirgoError::User(format!(
            "configuration is not a regular file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_new_root(path: &Path) -> Result<PathBuf> {
    path_text(path)?;
    let canonical = path.canonicalize().map_err(|_| {
        DirgoError::User(format!(
            "search root must be an existing directory: {}",
            path.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(DirgoError::User(format!(
            "search root must be an existing directory: {}",
            path.display()
        )));
    }
    path_text(&canonical)?;
    Ok(canonical)
}

fn path_text(path: &Path) -> Result<String> {
    let value = path
        .to_str()
        .ok_or_else(|| DirgoError::User("root path is not valid UTF-8".into()))?;
    if value.contains(['\n', '\r']) {
        return Err(DirgoError::NewlinePath);
    }
    Ok(value.to_owned())
}

fn equivalent_path(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        DirgoError::User(format!(
            "configuration path has no parent: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| DirgoError::io(parent, error))?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| DirgoError::io(parent, error))?;
    temporary
        .write_all(contents)
        .and_then(|_| temporary.as_file_mut().sync_all())
        .map_err(|error| DirgoError::io(temporary.path(), error))?;
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(temporary.path(), metadata.permissions())
            .map_err(|error| DirgoError::io(temporary.path(), error))?;
    } else {
        set_private_permissions(temporary.path())?;
    }
    let temporary = temporary.into_temp_path();
    replace_file(&temporary, path)
}

#[cfg(unix)]
pub(crate) fn replace_file(temporary: &Path, path: &Path) -> Result<()> {
    fs::rename(temporary, path).map_err(|error| {
        let _ = fs::remove_file(temporary);
        DirgoError::io(path, error)
    })
}

#[cfg(windows)]
pub(crate) fn replace_file(temporary: &Path, path: &Path) -> Result<()> {
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
