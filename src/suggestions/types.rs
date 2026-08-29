use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 2;
pub const HISTORY_RECORD_PROTOCOL_VERSION: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellKind {
    Zsh,
    Bash,
    Fish,
    PowerShell,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandHistoryRecordFrame {
    pub protocol_version: u16,
    pub command: String,
    pub cwd: PathBuf,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub session_id: Option<String>,
    pub shell: ShellKind,
    pub started_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodedHistoryRecord {
    V2(CommandHistoryRecordFrame),
    LegacyCommand(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    Success,
    Failure,
    Unknown,
}

impl CommandOutcome {
    pub fn from_exit_code(exit_code: Option<i32>) -> Self {
        match exit_code {
            Some(0) => Self::Success,
            Some(_) => Self::Failure,
            None => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandHistoryEventV2 {
    pub id: u64,
    pub command: String,
    pub started_at: u64,
    pub duration_ms: Option<u64>,
    pub cwd: PathBuf,
    pub project_root: Option<PathBuf>,
    pub exit_code: Option<i32>,
    pub outcome: CommandOutcome,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandHistoryAggregateV2 {
    pub scope_key: String,
    pub command: String,
    pub use_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub unknown_count: u64,
    pub last_used: u64,
    pub last_success: Option<u64>,
    pub last_failure: Option<u64>,
    pub total_duration_ms: u64,
    pub measured_duration_count: u64,
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
    ProjectCommand,
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
