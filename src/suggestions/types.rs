use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellKind {
    Zsh,
    Bash,
    Fish,
    PowerShell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionPresentation {
    Inline,
    List,
    Explicit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestionRequest {
    pub protocol_version: u16,
    pub request_id: u64,
    pub shell: ShellKind,
    pub cwd: PathBuf,
    pub before_cursor: String,
    pub after_cursor: String,
    pub max_results: usize,
    pub terminal_rows: Option<u16>,
    pub terminal_columns: Option<u16>,
    pub presentation: SuggestionPresentation,
}

pub fn visible_result_limit(terminal_rows: Option<u16>, configured: usize) -> usize {
    let configured = configured.clamp(5, 12);
    terminal_rows.map_or(configured, |rows| {
        configured.min(usize::from(rows / 3).clamp(5, 12))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextEdit {
    pub expected_before: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionSource {
    Directory,
    NavigationHistory,
    CommandHistory,
    Executable,
    Command,
    Subcommand,
    Option,
    Builtin,
    Alias,
    Filesystem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Suggestion {
    pub id: String,
    pub edit: TextEdit,
    pub display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source: SuggestionSource,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestionResponse {
    pub protocol_version: u16,
    pub request_id: u64,
    pub suggestions: Vec<Suggestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SuggestionResponse {
    pub fn success(request_id: u64, suggestions: Vec<Suggestion>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            suggestions,
            error: None,
        }
    }

    pub fn error(request_id: u64, message: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            suggestions: Vec::new(),
            error: Some(message.into()),
        }
    }
}

pub fn apply_text_edit(before_cursor: &str, after_cursor: &str, edit: &TextEdit) -> Option<String> {
    let prefix = before_cursor.strip_suffix(&edit.expected_before)?;
    let mut output =
        String::with_capacity(prefix.len() + edit.replacement.len() + after_cursor.len());
    output.push_str(prefix);
    output.push_str(&edit.replacement);
    output.push_str(after_cursor);
    Some(output)
}
