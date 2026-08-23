use std::{env, path::PathBuf};

use crate::{DirgoError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config_file: PathBuf,
    pub cache_dir: PathBuf,
    pub state_dir: PathBuf,
    pub index_file: PathBuf,
    pub state_file: PathBuf,
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
        let home = dirs::home_dir().ok_or_else(|| {
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
    reject_newline(&canonical)?;
    Ok(Some(canonical))
}

pub fn reject_newline(path: &std::path::Path) -> Result<()> {
    if path.to_string_lossy().contains('\n') {
        Err(DirgoError::NewlinePath)
    } else {
        Ok(())
    }
}
