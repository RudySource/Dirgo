use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectKind {
    Git,
    Rust,
    Node,
    Go,
    Python,
    Java,
    Ruby,
    Php,
    Generic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRecord {
    pub path: PathBuf,
    pub display_path: String,
    pub basename: String,
    pub parent: PathBuf,
    pub depth: usize,
    pub is_project_root: bool,
    pub project_kind: Option<ProjectKind>,
    pub last_seen: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub name: String,
    pub path: PathBuf,
    pub created_at: u64,
    pub last_used: Option<u64>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathHistory {
    pub path: PathBuf,
    pub visit_count: u64,
    pub first_visit: u64,
    pub last_visit: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScoreBreakdown {
    pub fuzzy: f64,
    pub exact: f64,
    pub prefix: f64,
    pub path_segment: f64,
    pub bookmark: f64,
    pub frequency: f64,
    pub recency: f64,
    pub proximity: f64,
    pub project: f64,
    pub depth_penalty: f64,
    pub total: f64,
}

impl ScoreBreakdown {
    pub fn from_total(total: f64) -> Self {
        Self {
            fuzzy: 0.0,
            exact: 0.0,
            prefix: 0.0,
            path_segment: 0.0,
            bookmark: 0.0,
            frequency: 0.0,
            recency: 0.0,
            proximity: 0.0,
            project: 0.0,
            depth_penalty: 0.0,
            total,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub path: PathBuf,
    pub display_path: String,
    pub basename: String,
    pub score: f64,
    pub score_breakdown: ScoreBreakdown,
    pub source: &'static str,
    pub is_project_root: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bookmark: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueryResponse {
    pub query: String,
    pub resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Candidate>,
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
