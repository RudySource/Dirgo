use crate::search::{self, SearchContext};
use std::fs;

use super::super::{Suggestion, SuggestionData, SuggestionRequest, SuggestionSource, TextEdit};
use super::escape::escape_token;

pub fn directory_suggestions(
    request: &SuggestionRequest,
    data: &SuggestionData,
    records: &[crate::model::DirectoryRecord],
) -> Vec<Suggestion> {
    let Some((command, query)) = directory_command(&request.before_cursor) else {
        return Vec::new();
    };
    if query.is_empty() {
        return Vec::new();
    }

    let context = SearchContext {
        records,
        bookmarks: &data.bookmarks,
        history: &data.navigation_history,
        cwd: &request.cwd,
        ranking: &data.ranking,
    };
    let query_parts = vec![query.to_owned()];
    let Ok(response) = search::resolve(&query_parts, &context, true, true) else {
        return Vec::new();
    };
    let canonical_cwd = fs::canonicalize(&request.cwd).ok();

    response
        .candidates
        .into_iter()
        .map(|candidate| {
            let source = if data.navigation_history.contains_key(&candidate.path) {
                SuggestionSource::NavigationHistory
            } else {
                SuggestionSource::Directory
            };
            let path = candidate.path.to_string_lossy();
            let escaped = escape_token(request.shell, &path);
            let description = candidate
                .path
                .strip_prefix(&request.cwd)
                .ok()
                .or_else(|| {
                    canonical_cwd
                        .as_deref()
                        .and_then(|cwd| candidate.path.strip_prefix(cwd).ok())
                })
                .filter(|relative| !relative.as_os_str().is_empty())
                .map(|relative| relative.display().to_string())
                .unwrap_or_else(|| candidate.display_path.clone());
            Suggestion {
                id: format!("directory:{}", candidate.path.display()),
                edit: TextEdit {
                    expected_before: request.before_cursor.clone(),
                    replacement: format!("{command} {escaped}"),
                },
                display: candidate.basename,
                description: Some(description),
                source,
                score: candidate.score,
            }
        })
        .collect()
}

pub fn directory_query(input: &str) -> Option<&str> {
    directory_command(input).map(|(_, query)| query)
}

fn directory_command(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim_start();
    for command in ["Set-Location", "dgo", "cd", "sl"] {
        if let Some(query) = trimmed
            .strip_prefix(command)
            .and_then(|rest| rest.strip_prefix(' '))
        {
            return Some((command, query.trim_start()));
        }
    }
    None
}
