use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaletteSource {
    All,
    Files,
    Tasks,
    Workflows,
    Git,
    Compose,
    Places,
}

impl PaletteSource {
    pub const FILTERS: [Self; 7] = [
        Self::All,
        Self::Files,
        Self::Tasks,
        Self::Workflows,
        Self::Git,
        Self::Compose,
        Self::Places,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Files => "files",
            Self::Tasks => "tasks",
            Self::Workflows => "workflows",
            Self::Git => "git",
            Self::Compose => "compose",
            Self::Places => "places",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::FILTERS
            .iter()
            .position(|source| *source == self)
            .unwrap_or(0);
        Self::FILTERS[(index + 1) % Self::FILTERS.len()]
    }

    pub fn previous(self) -> Self {
        let index = Self::FILTERS
            .iter()
            .position(|source| *source == self)
            .unwrap_or(0);
        Self::FILTERS[(index + Self::FILTERS.len() - 1) % Self::FILTERS.len()]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PaletteAction {
    Navigate { path: PathBuf },
    Insert { text: String },
    InsertCommand { program: String, args: Vec<String> },
    Open { path: PathBuf },
    CopyPath { path: PathBuf },
    OpenEditor { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaletteItem {
    pub id: String,
    pub source: PaletteSource,
    pub title: String,
    pub subtitle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_preview: Option<WorkflowPreview>,
    pub action: PaletteAction,
    pub score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowPreview {
    pub steps: Vec<String>,
    pub next_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderState {
    Ready,
    TimedOut,
    Failed,
}
