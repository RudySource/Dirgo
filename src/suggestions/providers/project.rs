use super::super::{
    ProjectCommandSnapshot, Suggestion, SuggestionRequest, SuggestionSource, TextEdit,
};

const PROJECT_COMMAND_SCORE: f64 = 40_000.0;

pub fn project_command_suggestions(
    request: &SuggestionRequest,
    snapshot: Option<&ProjectCommandSnapshot>,
) -> Vec<Suggestion> {
    let Some(snapshot) = snapshot.filter(|snapshot| snapshot.contains(&request.cwd)) else {
        return Vec::new();
    };
    let query = request.before_cursor.trim_start();
    if query.is_empty() {
        return Vec::new();
    }
    let leading_whitespace = &request.before_cursor[..request.before_cursor.len() - query.len()];
    snapshot
        .commands()
        .iter()
        .filter(|command| prefix_matches(&command.replacement, query))
        .map(|command| Suggestion {
            id: format!("project:{}", command.stable_id),
            edit: TextEdit {
                expected_before: request.before_cursor.clone(),
                replacement: format!("{leading_whitespace}{}", command.replacement),
            },
            display: command.display.clone(),
            description: Some(command.description.clone()),
            source: SuggestionSource::ProjectCommand,
            score: PROJECT_COMMAND_SCORE
                + (query.len() as f64 / command.replacement.len().max(1) as f64),
        })
        .collect()
}

fn prefix_matches(command: &str, query: &str) -> bool {
    command
        .get(..query.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(query))
}
