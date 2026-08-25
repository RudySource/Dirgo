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
    pub suggestions: SuggestionsConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SuggestionsConfig {
    pub enabled: bool,
    pub command_history: bool,
    pub max_results: usize,
    pub retention_entries: usize,
    pub retention_days: u64,
    pub deny_patterns: Vec<String>,
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
            suggestions: SuggestionsConfig::default(),
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

impl Default for SuggestionsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command_history: false,
            max_results: 8,
            retention_entries: 10_000,
            retention_days: 180,
            deny_patterns: Vec::new(),
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
        if !matches!(self.ui.icons.as_str(), "auto" | "always" | "never") {
            return Err(DirgoError::Config(
                "ui.icons must be one of: auto, always, never".into(),
            ));
        }
        if !(1..=20).contains(&self.suggestions.max_results) {
            return Err(DirgoError::Config(
                "suggestions.max_results must be between 1 and 20".into(),
            ));
        }
        if !(1..=50_000).contains(&self.suggestions.retention_entries) {
            return Err(DirgoError::Config(
                "suggestions.retention_entries must be between 1 and 50000".into(),
            ));
        }
        if !(1..=3_650).contains(&self.suggestions.retention_days) {
            return Err(DirgoError::Config(
                "suggestions.retention_days must be between 1 and 3650".into(),
            ));
        }
        if self.suggestions.deny_patterns.iter().any(|pattern| {
            pattern.is_empty() || pattern.len() > 256 || pattern.chars().any(char::is_control)
        }) {
            return Err(DirgoError::Config(
                "suggestions.deny_patterns entries must contain 1 to 256 printable bytes".into(),
            ));
        }
        for (name, weight) in [
            ("frequency", self.ranking.frequency),
            ("recency", self.ranking.recency),
            ("proximity", self.ranking.proximity),
            ("bookmarks", self.ranking.bookmarks),
            ("projects", self.ranking.projects),
        ] {
            if !weight.is_finite() || weight < 0.0 {
                return Err(DirgoError::Config(format!(
                    "ranking.{name} must be a finite non-negative number"
                )));
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_negative_or_non_finite_ranking_weights() {
        let mut config = Config::default();
        config.ranking.recency = -0.1;
        assert!(config.validate().is_err());

        config.ranking.recency = f64::NAN;
        assert!(config.validate().is_err());
    }

    #[test]
    fn permits_disabling_a_ranking_signal_with_zero() {
        let mut config = Config::default();
        config.ranking.frequency = 0.0;
        config.ranking.recency = 0.0;
        config.ranking.proximity = 0.0;
        config.ranking.bookmarks = 0.0;
        config.ranking.projects = 0.0;
        config.validate().expect("zero weights are valid");
    }

    #[test]
    fn suggestions_are_opt_in_and_old_configs_receive_safe_defaults() {
        let config = Config::default();
        assert!(!config.suggestions.enabled);
        assert!(!config.suggestions.command_history);
        assert_eq!(config.suggestions.max_results, 8);

        let parsed: Config = toml::from_str("schema_version = 1\nroots = ['/tmp']\n")
            .expect("legacy config remains readable");
        assert!(!parsed.suggestions.enabled);
        parsed.validate().expect("legacy defaults are valid");
    }

    #[test]
    fn suggestion_limits_are_bounded() {
        let mut config = Config::default();
        config.suggestions.max_results = 21;
        assert!(config.validate().is_err());

        config.suggestions.max_results = 8;
        config.suggestions.retention_entries = 0;
        assert!(config.validate().is_err());
    }
}
