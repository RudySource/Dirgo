use crate::{DirgoError, Result, shell::Shell};

use super::{PickerAccept, Suggestion, SuggestionSource};

const FRAME_VERSION: &str = "DGS1";
const DIRECTORY_ID_PREFIX: &str = "directory:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionPickerResultKind {
    Navigate,
    Insert,
}

impl SuggestionPickerResultKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Navigate => "navigate",
            Self::Insert => "insert",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionPickerResultFrame {
    kind: SuggestionPickerResultKind,
    payload: String,
}

impl SuggestionPickerResultFrame {
    pub fn from_selection(
        suggestion: &Suggestion,
        accept: PickerAccept,
        _shell: Shell,
    ) -> Result<Self> {
        let (kind, payload) = if accept == PickerAccept::Enter
            && matches!(
                suggestion.source,
                SuggestionSource::Directory | SuggestionSource::NavigationHistory
            ) {
            let path = suggestion
                .id
                .strip_prefix(DIRECTORY_ID_PREFIX)
                .ok_or_else(|| {
                    DirgoError::User("Directory suggestion is missing its literal path".into())
                })?;
            (SuggestionPickerResultKind::Navigate, path.to_owned())
        } else {
            (
                SuggestionPickerResultKind::Insert,
                suggestion.edit.replacement.clone(),
            )
        };
        validate_payload(&payload)?;
        Ok(Self { kind, payload })
    }

    pub fn kind(&self) -> SuggestionPickerResultKind {
        self.kind
    }

    pub fn encode(&self) -> String {
        format!("{FRAME_VERSION} {}\n{}", self.kind.as_str(), self.payload)
    }
}

fn validate_payload(value: &str) -> Result<()> {
    if value.contains(['\r', '\n']) || value.chars().any(char::is_control) {
        return Err(DirgoError::User(
            "Suggestion actions cannot contain line breaks or terminal control characters".into(),
        ));
    }
    Ok(())
}
