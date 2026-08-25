use serde::{Deserialize, Serialize};

use super::super::{Suggestion, SuggestionRequest, SuggestionSource, TextEdit};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandHistoryEntry {
    pub command: String,
    pub use_count: u64,
    pub last_used: u64,
}

impl CommandHistoryEntry {
    pub fn new(command: impl Into<String>, use_count: u64, last_used: u64) -> Self {
        Self {
            command: command.into(),
            use_count,
            last_used,
        }
    }
}

pub fn history_suggestions(
    request: &SuggestionRequest,
    history: &[CommandHistoryEntry],
) -> Vec<Suggestion> {
    if request.before_cursor.is_empty() {
        return Vec::new();
    }
    history
        .iter()
        .filter(|entry| {
            entry.command.starts_with(&request.before_cursor)
                && !super::super::privacy::is_sensitive_command(&entry.command, &[])
        })
        .map(|entry| Suggestion {
            id: format!("history:{:016x}", stable_hash(&entry.command)),
            edit: TextEdit {
                expected_before: request.before_cursor.clone(),
                replacement: entry.command.clone(),
            },
            display: entry.command.clone(),
            description: Some("HIST".into()),
            source: SuggestionSource::CommandHistory,
            score: 20_000.0
                + (entry.use_count as f64).ln_1p() * 500.0
                + entry.last_used as f64 * 1e-9,
        })
        .collect()
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
