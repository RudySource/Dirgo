mod context;
mod engine;
mod picker;
mod privacy;
mod protocol;
mod providers;
mod sanitize;
mod settings;
mod store;
mod top_k;
mod types;

pub use context::CompletionContext;
pub use engine::{SuggestionData, SuggestionEngine};
pub use picker::{PickerOptions, pick_suggestion};
pub use protocol::{
    MAX_REQUEST_BYTES, ProtocolError, decode_request_line, encode_response_line, read_bounded_frame,
};
pub use providers::{CommandHistoryEntry, discover_executables};
pub use sanitize::sanitize_suggestion;
pub use settings::write_suggestions_config;
pub use store::{CommandHistoryStore, read_command_history};
pub use top_k::TopSuggestions;
pub use types::{
    PROTOCOL_VERSION, ShellKind, Suggestion, SuggestionRequest, SuggestionResponse,
    SuggestionSource, TextEdit, apply_text_edit,
};
