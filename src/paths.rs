use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::{DirgoError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config_file: PathBuf,
    pub cache_dir: PathBuf,
    pub state_dir: PathBuf,
    pub index_file: PathBuf,
    pub state_file: PathBuf,
    pub suggestions_state_file: PathBuf,
    pub update_cache_file: PathBuf,
    pub update_check_file: PathBuf,
    pub update_notice_disabled_file: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| {
            DirgoError::User("Dirgo could not determine your home directory".into())
        })?;
        let config_home = env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let cache_home = env_path("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"));
        let state_home =
            env_path("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local").join("state"));
        let config_dir = config_home.join("dirgo");
        let cache_dir = cache_home.join("dirgo");
        let state_dir = state_home.join("dirgo");
        Ok(Self {
            config_file: config_dir.join("config.toml"),
            index_file: cache_dir.join("index.redb"),
            state_file: state_dir.join("state.redb"),
            suggestions_state_file: state_dir.join("suggestions.redb"),
            update_cache_file: cache_dir.join("update.json"),
            update_check_file: cache_dir.join("update-check"),
            update_notice_disabled_file: state_dir.join("update-notifications-disabled"),
            cache_dir,
            state_dir,
        })
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for path in [&self.cache_dir, &self.state_dir] {
            std::fs::create_dir_all(path).map_err(|error| DirgoError::io(path, error))?;
        }
        Ok(())
    }
}

pub fn timestamped_recovery_path(path: &Path, label: &str, timestamp: u64) -> PathBuf {
    let base = format!("{}.{}.{}", path.display(), label, timestamp);
    let mut candidate = PathBuf::from(&base);
    let mut suffix = 1_u32;
    while candidate.exists() {
        candidate = PathBuf::from(format!("{base}.{suffix}"));
        suffix += 1;
    }
    candidate
}

pub fn preserve_for_recovery(path: &Path, label: &str, timestamp: u64) -> Result<PathBuf> {
    let destination = timestamped_recovery_path(path, label, timestamp);
    fs::rename(path, &destination).map_err(|error| DirgoError::io(path, error))?;
    Ok(destination)
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn expand_path(input: &str) -> Result<PathBuf> {
    if input.contains('\n') {
        return Err(DirgoError::NewlinePath);
    }
    if input == "~" || input.starts_with("~/") {
        let home = env_path("HOME")
            .filter(|path| path.is_absolute())
            .or_else(dirs::home_dir)
            .ok_or_else(|| {
                DirgoError::User("Dirgo could not determine your home directory".into())
            })?;
        return Ok(if input == "~" {
            home
        } else {
            home.join(&input[2..])
        });
    }
    Ok(PathBuf::from(input))
}

pub fn absolute_directory(input: &str, cwd: &std::path::Path) -> Result<Option<PathBuf>> {
    let expanded = expand_path(input)?;
    let candidate = if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(expanded)
    };
    if !candidate.is_dir() {
        return Ok(None);
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|error| DirgoError::io(&candidate, error))?;
    validate_shell_path(&canonical)?;
    Ok(Some(canonical))
}

pub fn validate_shell_path(path: &std::path::Path) -> Result<()> {
    let path = path.to_str().ok_or(DirgoError::NonUtf8Path)?;
    if path.contains('\n') {
        Err(DirgoError::NewlinePath)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_names_are_timestamped_and_collision_safe() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("state.redb");
        std::fs::write(&path, "state").expect("state");
        let first = preserve_for_recovery(&path, "corrupt", 42).expect("preserve");
        assert!(first.ends_with("state.redb.corrupt.42"));
        std::fs::write(&path, "state again").expect("state");
        let second = preserve_for_recovery(&path, "corrupt", 42).expect("preserve");
        assert!(second.ends_with("state.redb.corrupt.42.1"));
        assert!(first.is_file());
        assert!(second.is_file());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_utf8_paths_at_the_shell_boundary() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let path = PathBuf::from(OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xff]));
        assert!(matches!(
            validate_shell_path(&path),
            Err(DirgoError::NonUtf8Path)
        ));
    }
}
