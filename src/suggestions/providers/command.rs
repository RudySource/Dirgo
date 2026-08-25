use super::super::{
    CommandCatalog, CommandOption, CommandSpec, CompletionContext, Suggestion, SuggestionRequest,
    SuggestionSource, TextEdit,
};

pub fn command_suggestions(
    request: &SuggestionRequest,
    context: &CompletionContext,
    catalog: &CommandCatalog,
) -> Vec<Suggestion> {
    let Some(mut command) = context.command().and_then(|name| catalog.command(name)) else {
        return Vec::new();
    };
    for token in context.completed_tokens().iter().skip(1) {
        if token.starts_with('-') {
            continue;
        }
        if let Some(next) = find_subcommand(command, token) {
            command = next;
        }
    }
    if context.is_option() {
        option_suggestions(request, context, command)
    } else {
        subcommand_suggestions(request, context, command)
    }
}

fn find_subcommand<'a>(command: &'a CommandSpec, token: &str) -> Option<&'a CommandSpec> {
    command.subcommands.iter().find(|candidate| {
        candidate.name == token || candidate.aliases.iter().any(|alias| alias == token)
    })
}

fn subcommand_suggestions(
    request: &SuggestionRequest,
    context: &CompletionContext,
    command: &CommandSpec,
) -> Vec<Suggestion> {
    command
        .subcommands
        .iter()
        .filter(|candidate| smart_prefix(&candidate.name, context.current_token()))
        .map(|candidate| {
            suggestion(
                request,
                context,
                &candidate.name,
                candidate.description.clone(),
                SuggestionSource::Subcommand,
                30_000.0,
            )
        })
        .collect()
}

fn option_suggestions(
    request: &SuggestionRequest,
    context: &CompletionContext,
    command: &CommandSpec,
) -> Vec<Suggestion> {
    command
        .options
        .iter()
        .filter_map(|option| matching_option(option, context.current_token()))
        .map(|(option, value)| {
            suggestion(
                request,
                context,
                value,
                option.description.clone(),
                SuggestionSource::Option,
                29_000.0,
            )
        })
        .collect()
}

fn matching_option<'a>(
    option: &'a CommandOption,
    input: &str,
) -> Option<(&'a CommandOption, &'a str)> {
    std::iter::once(option.name.as_str())
        .chain(option.aliases.iter().map(String::as_str))
        .find(|candidate| smart_prefix(candidate, input))
        .map(|value| (option, value))
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

fn suggestion(
    request: &SuggestionRequest,
    context: &CompletionContext,
    value: &str,
    description: Option<String>,
    source: SuggestionSource,
    score: f64,
) -> Suggestion {
    let mut replacement = request.before_cursor[..context.replacement_start()].to_owned();
    replacement.push_str(value);
    let kind = if source == SuggestionSource::Option {
        "option"
    } else {
        "subcommand"
    };
    Suggestion {
        id: format!("{kind}:{value}"),
        edit: TextEdit {
            expected_before: request.before_cursor.clone(),
            replacement,
        },
        display: value.to_owned(),
        description,
        source,
        score: score - value.len() as f64,
    }
}
