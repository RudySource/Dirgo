use super::super::{CommandCatalog, Suggestion, SuggestionRequest, SuggestionSource, TextEdit};

pub fn executable_suggestions(
    request: &SuggestionRequest,
    catalog: &CommandCatalog,
) -> Vec<Suggestion> {
    let input = request.before_cursor.trim_start();
    if input.contains(char::is_whitespace) {
        return Vec::new();
    }
    catalog
        .executable_names()
        .filter(|executable| smart_prefix(executable, input))
        .map(|executable| Suggestion {
            id: format!("executable:{executable}"),
            edit: TextEdit {
                expected_before: request.before_cursor.clone(),
                replacement: executable.to_owned(),
            },
            display: executable.to_owned(),
            description: Some("PATH".into()),
            source: SuggestionSource::Command,
            score: 10_000.0 - executable.len() as f64,
        })
        .collect()
}

fn smart_prefix(candidate: &str, input: &str) -> bool {
    if input.chars().any(char::is_uppercase) {
        candidate.starts_with(input)
    } else {
        candidate
            .get(..input.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(input))
    }
}
