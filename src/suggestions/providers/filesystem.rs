use std::path::Path;

use super::{
    super::{Suggestion, SuggestionRequest, SuggestionSource, TextEdit},
    escape::escape_token,
};

pub fn filesystem_suggestions(request: &SuggestionRequest) -> Vec<Suggestion> {
    let (prefix, token) = split_current_token(&request.before_cursor);
    if prefix.is_empty() || token.is_empty() || token.contains(['\'', '"']) {
        return Vec::new();
    }

    let token_path = Path::new(token);
    let (directory, visible_parent, query) = match (token_path.parent(), token_path.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => (
            request.cwd.join(parent),
            format!("{}{sep}", parent.display(), sep = std::path::MAIN_SEPARATOR),
            name.to_string_lossy().into_owned(),
        ),
        (_, Some(name)) => (
            request.cwd.clone(),
            String::new(),
            name.to_string_lossy().into_owned(),
        ),
        _ => return Vec::new(),
    };

    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut suggestions = Vec::new();
    for entry in entries.take(256).flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !smart_starts_with(&name, &query) {
            continue;
        }
        let is_directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
        let mut completed = format!("{visible_parent}{name}");
        if is_directory {
            completed.push(std::path::MAIN_SEPARATOR);
        }
        let escaped = escape_token(request.shell, &completed);
        suggestions.push(Suggestion {
            id: filesystem_id(&entry.path()),
            edit: TextEdit {
                expected_before: request.before_cursor.clone(),
                replacement: format!("{prefix}{escaped}"),
            },
            display: name,
            description: Some(if is_directory { "DIR" } else { "FILE" }.into()),
            source: SuggestionSource::Filesystem,
            score: 8_000.0 - completed.len() as f64,
        });
    }
    suggestions
}

fn split_current_token(input: &str) -> (&str, &str) {
    let start = input
        .char_indices()
        .rev()
        .find_map(|(index, character)| {
            character
                .is_whitespace()
                .then_some(index + character.len_utf8())
        })
        .unwrap_or(0);
    input.split_at(start)
}

fn smart_starts_with(value: &str, query: &str) -> bool {
    if query.chars().any(char::is_uppercase) {
        value.starts_with(query)
    } else {
        value.to_lowercase().starts_with(&query.to_lowercase())
    }
}

fn filesystem_id(path: &Path) -> String {
    format!("filesystem:{}", path.display())
}
