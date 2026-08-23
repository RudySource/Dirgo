use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{DirgoError, Result, paths::AppPaths};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub roots: Vec<PathBuf>,
    pub ignore: Vec<String>,
    pub respect_gitignore: bool,
    pub follow_symlinks: bool,
    pub ranking: RankingConfig,
    pub ui: UiConfig,
    pub actions: ActionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RankingConfig {
    pub frequency: f64,
    pub recency: f64,
    pub proximity: f64,
    pub bookmarks: f64,
    pub projects: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    pub preview: bool,
    pub accent: String,
    pub icons: String,
    pub height_percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActionConfig {
    pub editor: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            roots: vec![dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))],
            ignore: default_ignores().into_iter().map(str::to_owned).collect(),
            respect_gitignore: true,
            follow_symlinks: false,
            ranking: RankingConfig::default(),
            ui: UiConfig::default(),
            actions: ActionConfig::default(),
        }
    }
}

impl Default for RankingConfig {
    fn default() -> Self {
        Self {
            frequency: 1.0,
            recency: 0.85,
            proximity: 0.55,
            bookmarks: 1.25,
            projects: 0.30,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            preview: true,
            accent: "cyan".into(),
            icons: "auto".into(),
            height_percent: 70,
        }
    }
}

impl Default for ActionConfig {
    fn default() -> Self {
        Self {
            editor: "auto".into(),
        }
    }
}

impl Config {
    pub fn load(paths: &AppPaths) -> Result<Self> {
        if !paths.config_file.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&paths.config_file)
            .map_err(|error| DirgoError::io(&paths.config_file, error))?;
        let config: Self =
            toml::from_str(&raw).map_err(|error| DirgoError::Config(error.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err(DirgoError::Config(format!(
                "unsupported schema_version {}; expected 1",
                self.schema_version
            )));
        }
        if self.roots.is_empty() {
            return Err(DirgoError::Config(
                "roots must contain at least one directory".into(),
            ));
        }
        if !(30..=100).contains(&self.ui.height_percent) {
            return Err(DirgoError::Config(
                "ui.height_percent must be between 30 and 100".into(),
            ));
        }
        Ok(())
    }
}

pub fn default_ignores() -> [&'static str; 20] {
    [
        ".git",
        ".svn",
        "node_modules",
        "Library",
        ".cache",
        ".Trash",
        ".npm",
        ".pnpm-store",
        ".yarn",
        ".docker",
        ".gradle",
        ".next",
        ".nuxt",
        ".venv",
        "venv",
        "vendor",
        "coverage",
        "target",
        "build",
        "dist",
    ]
}
