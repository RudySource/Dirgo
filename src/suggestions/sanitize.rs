use super::Suggestion;

pub fn sanitize_suggestion(suggestion: Suggestion) -> Option<Suggestion> {
    let safe = !contains_unsafe_text(&suggestion.id)
        && !contains_unsafe_text(&suggestion.edit.expected_before)
        && !contains_unsafe_text(&suggestion.edit.replacement)
        && !contains_unsafe_text(&suggestion.display)
        && suggestion
            .description
            .as_deref()
            .is_none_or(|value| !contains_unsafe_text(value))
        && suggestion.score.is_finite();
    safe.then_some(suggestion)
}

fn contains_unsafe_text(value: &str) -> bool {
    value.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
    })
}
