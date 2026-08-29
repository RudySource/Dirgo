use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::super::{Suggestion, SuggestionRequest, SuggestionSource, TextEdit};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandHistoryEntry {
    pub command: String,
    pub use_count: u64,
    pub last_used: u64,
    #[serde(default)]
    pub scope_key: String,
    #[serde(default)]
    pub success_count: u64,
    #[serde(default)]
    pub failure_count: u64,
    #[serde(default)]
    pub unknown_count: u64,
    #[serde(default)]
    pub last_success: Option<u64>,
    #[serde(default)]
    pub last_failure: Option<u64>,
    #[serde(default)]
    pub recent_cwds: Vec<PathBuf>,
    #[serde(default)]
    pub recent_sessions: Vec<String>,
}

impl CommandHistoryEntry {
    pub fn new(command: impl Into<String>, use_count: u64, last_used: u64) -> Self {
        Self {
            command: command.into(),
            use_count,
            last_used,
            scope_key: String::new(),
            success_count: 0,
            failure_count: 0,
            unknown_count: use_count,
            last_success: None,
            last_failure: None,
            recent_cwds: Vec::new(),
            recent_sessions: Vec::new(),
        }
    }
}

pub fn history_suggestions(
    request: &SuggestionRequest,
    history: &[CommandHistoryEntry],
) -> Vec<Suggestion> {
    if request.before_cursor.is_empty() || history.is_empty() {
        return Vec::new();
    }
    let project_scope = current_project_scope(&request.cwd);
    let current_session = std::env::var("DGO_SESSION_ID").ok();
    history
        .iter()
        .filter(|entry| {
            entry.command.starts_with(&request.before_cursor)
                && !super::super::privacy::is_sensitive_command(&entry.command, &[])
        })
        .map(|entry| {
            let scope_boost = if project_scope.as_deref() == Some(entry.scope_key.as_str()) {
                6_000.0
            } else if matches!(entry.scope_key.as_str(), "global" | "legacy_global" | "") {
                500.0
            } else {
                0.0
            };
            let cwd_boost = if entry.recent_cwds.iter().any(|cwd| cwd == &request.cwd) {
                800.0
            } else if entry
                .recent_cwds
                .iter()
                .any(|cwd| request.cwd.starts_with(cwd))
            {
                400.0
            } else {
                0.0
            };
            let same_session = current_session.as_ref().is_some_and(|session| {
                entry
                    .recent_sessions
                    .iter()
                    .any(|candidate| candidate == session)
            });
            let session_boost = if same_session { 300.0 } else { 0.0 };
            let known = entry.success_count.saturating_add(entry.failure_count);
            let outcome_boost = if known >= 3 {
                ((entry.success_count + 1) as f64 / (known + 2) as f64) * 1_000.0
            } else {
                500.0
            };
            let recovery_boost = if entry.last_success > entry.last_failure {
                250.0
            } else {
                0.0
            };
            Suggestion {
                id: format!("history:{:016x}", stable_hash(&entry.command)),
                edit: TextEdit {
                    expected_before: request.before_cursor.clone(),
                    replacement: entry.command.clone(),
                },
                display: entry.command.clone(),
                description: Some("HIST".into()),
                source: SuggestionSource::CommandHistory,
                score: 20_000.0
                    + scope_boost
                    + cwd_boost
                    + session_boost
                    + outcome_boost
                    + recovery_boost
                    + (entry.use_count as f64).ln_1p() * 500.0
                    + entry.last_used as f64 * 1e-9,
            }
        })
        .collect()
}

fn current_project_scope(cwd: &std::path::Path) -> Option<String> {
    crate::index::find_project_root(cwd)
        .and_then(|(root, _)| root.to_str().map(|root| format!("project:{root}")))
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
