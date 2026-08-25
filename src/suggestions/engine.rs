use std::collections::HashMap;

use crate::{
    config::RankingConfig,
    model::{Bookmark, DirectoryRecord, PathHistory},
};

use super::{
    CommandCatalog, CompletionContext, Suggestion, SuggestionRequest, TopSuggestions,
    providers::{
        CommandHistoryEntry, command_suggestions, directory_query, directory_suggestions,
        executable_suggestions, history_suggestions,
    },
    sanitize_suggestion,
};

#[derive(Debug, Clone, Default)]
pub struct SuggestionData {
    pub records: Vec<DirectoryRecord>,
    pub bookmarks: HashMap<String, Bookmark>,
    pub navigation_history: HashMap<std::path::PathBuf, PathHistory>,
    pub ranking: RankingConfig,
    pub catalog: CommandCatalog,
    pub command_history: Vec<CommandHistoryEntry>,
}

pub struct SuggestionEngine {
    data: SuggestionData,
    directory_order: Option<Vec<(String, usize)>>,
}

impl SuggestionEngine {
    pub fn new(data: SuggestionData) -> Self {
        Self {
            data,
            directory_order: None,
        }
    }

    pub fn new_indexed(data: SuggestionData) -> Self {
        let mut directory_order: Vec<_> = data
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| (record.basename.to_lowercase(), index))
            .collect();
        directory_order.sort_unstable_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| data.records[left.1].path.cmp(&data.records[right.1].path))
        });
        Self {
            data,
            directory_order: Some(directory_order),
        }
    }

    pub fn suggest(&self, request: &SuggestionRequest) -> Vec<Suggestion> {
        let records = self.prefix_records(request);
        let context = CompletionContext::parse(request.shell, &request.before_cursor);
        let mut candidates = directory_suggestions(request, &self.data, &records);
        candidates.extend(command_suggestions(request, &context, &self.data.catalog));
        candidates.extend(history_suggestions(request, &self.data.command_history));
        candidates.extend(executable_suggestions(request, &self.data.catalog));
        candidates.extend(super::providers::filesystem_suggestions(request));

        let mut top = TopSuggestions::new(super::visible_result_limit(
            request.terminal_rows,
            request.max_results,
        ));
        for suggestion in candidates.into_iter().filter_map(sanitize_suggestion) {
            if suggestion.edit.replacement == request.before_cursor {
                continue;
            }
            top.push(suggestion);
        }
        top.finish()
    }

    fn prefix_records(&self, request: &SuggestionRequest) -> Vec<crate::model::DirectoryRecord> {
        const MAX_PREFIX_CANDIDATES: usize = 512;
        let Some(query) = directory_query(&request.before_cursor) else {
            return Vec::new();
        };
        let query = query.trim_start();
        if query.is_empty() {
            return Vec::new();
        }
        let folded = query.to_lowercase();
        let Some(directory_order) = &self.directory_order else {
            return self
                .data
                .records
                .iter()
                .filter(|record| starts_with_folded(&record.basename, query, &folded))
                .take(MAX_PREFIX_CANDIDATES)
                .cloned()
                .collect();
        };
        let start = directory_order.partition_point(|(basename, _)| basename < &folded);
        directory_order[start..]
            .iter()
            .take_while(|(basename, _)| basename.starts_with(&folded))
            .map(|(_, index)| &self.data.records[*index])
            .filter(|record| {
                !query.chars().any(char::is_uppercase) || record.basename.starts_with(query)
            })
            .take(MAX_PREFIX_CANDIDATES)
            .cloned()
            .collect()
    }
}

fn starts_with_folded(value: &str, query: &str, folded_query: &str) -> bool {
    if query.chars().any(char::is_uppercase) {
        return value.starts_with(query);
    }
    if value.is_ascii() && query.is_ascii() {
        return value
            .get(..query.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(query));
    }
    value.to_lowercase().starts_with(folded_query)
}
