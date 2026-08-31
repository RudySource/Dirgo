mod catalog;
mod context;
mod engine;
mod picker;
mod privacy;
mod project;
mod protocol;
mod providers;
mod result;
mod sanitize;
pub(crate) mod settings;
mod specs;
mod store;
mod top_k;
mod types;

pub use catalog::{CommandCatalog, CommandOption, CommandSpec, portable_command_name};
pub use context::CompletionContext;
pub use engine::{SuggestionData, SuggestionEngine, SuggestionPage};
pub use picker::{PickerAccept, PickerOptions, PickerSelection, pick_suggestion};
pub use privacy::is_sensitive_command;
pub use project::{
    ProjectCommand, ProjectCommandSnapshot, claim_project_command_refresh,
    load_cached_project_command_snapshot, load_project_command_snapshot,
    refresh_project_command_cache,
};
pub use protocol::{
    MAX_REQUEST_BYTES, ProtocolError, decode_history_record_frame, decode_request_line,
    encode_response_line, read_bounded_frame,
};
pub use providers::CommandHistoryEntry;
pub use result::{SuggestionPickerResultFrame, SuggestionPickerResultKind};
pub use sanitize::sanitize_suggestion;
pub use settings::write_suggestions_config;
pub use store::{
    CommandHistoryScope, CommandHistorySnapshot, CommandHistoryStatus, CommandHistoryStore,
    HISTORY_SCHEMA_VERSION, read_command_history, read_history_snapshot,
};
pub use top_k::TopSuggestions;
pub use types::{
    CommandHistoryAggregateV2, CommandHistoryEventV2, CommandHistoryRecordFrame, CommandOutcome,
    DecodedHistoryRecord, HISTORY_RECORD_PROTOCOL_VERSION, PROTOCOL_VERSION, ShellKind, Suggestion,
    SuggestionPresentation, SuggestionRequest, SuggestionResponse, SuggestionSource, TextEdit,
    apply_text_edit, visible_result_limit,
};
