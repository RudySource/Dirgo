use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};

use crate::{
    DirgoError, Result,
    model::{Bookmark, Candidate, DirectoryRecord, PathHistory, QueryResponse},
    paths::absolute_directory,
};

pub struct SearchContext<'a> {
    pub records: &'a [DirectoryRecord],
    pub bookmarks: &'a HashMap<String, Bookmark>,
    pub history: &'a HashMap<PathBuf, PathHistory>,
    pub cwd: &'a Path,
}

pub fn picker_candidates(
    records: Vec<DirectoryRecord>,
    bookmarks: &HashMap<String, Bookmark>,
    history: &HashMap<PathBuf, PathHistory>,
    cwd: &Path,
) -> Vec<Candidate> {
    let bookmarks_by_path: HashMap<&Path, &str> = bookmarks
        .values()
        .map(|bookmark| (bookmark.path.as_path(), bookmark.name.as_str()))
        .collect();
    let mut candidates: Vec<_> = records
        .into_iter()
        .map(|record| {
            let bookmark = bookmarks_by_path
                .get(record.path.as_path())
                .map(|name| (*name).to_owned());
            let history = history.get(&record.path);
            let bookmark_bonus = if bookmark.is_some() { 2_000.0 } else { 0.0 };
            let project_bonus = if record.is_project_root { 500.0 } else { 0.0 };
            let frequency_bonus = history
                .map(|row| (row.visit_count as f64).ln_1p() * 250.0)
                .unwrap_or(0.0);
            let proximity_bonus = common_ancestor_depth(&record.path, cwd) as f64 * 25.0;
            Candidate {
                path: record.path,
                display_path: record.display_path,
                basename: record.basename,
                score: bookmark_bonus + project_bonus + frequency_bonus + proximity_bonus
                    - record.depth as f64 * 2.0,
                source: "browse",
                is_project_root: record.is_project_root,
                bookmark,
            }
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
    });
    candidates
}

pub fn resolve(
    query_parts: &[String],
    context: &SearchContext<'_>,
    force_picker: bool,
) -> Result<QueryResponse> {
    let query = query_parts.join(" ");
    if !force_picker && query_parts.len() == 1 {
        if let Some(path) = absolute_directory(&query, context.cwd)? {
            return Ok(resolved(query, path, 1.0, "existing_path"));
        }
        if let Some(name) = query.strip_prefix('@') {
            let bookmark = context
                .bookmarks
                .get(name)
                .ok_or_else(|| DirgoError::BookmarkMissing(name.into()))?;
            if bookmark.path.is_dir() {
                return Ok(resolved(query, bookmark.path.clone(), 1.0, "bookmark"));
            }
            return Err(DirgoError::User(format!(
                "bookmark @{name} points to a missing directory: {}",
                bookmark.path.display()
            )));
        }
    }

    let local_scope = query_parts.first().is_some_and(|part| part == ".");
    let effective_query = if local_scope {
        query_parts[1..].join(" ")
    } else {
        query.clone()
    };
    let records: Vec<&DirectoryRecord> = context
        .records
        .iter()
        .filter(|record| !local_scope || record.path.starts_with(context.cwd))
        .collect();

    if !force_picker && !effective_query.is_empty() {
        let exact: Vec<_> = records
            .iter()
            .filter(|record| smart_equal(&record.basename, &effective_query))
            .collect();
        if exact.len() == 1 {
            return Ok(resolved(
                query,
                exact[0].path.clone(),
                1.0,
                "exact_basename",
            ));
        }
    }

    let mut candidates = fuzzy_candidates(&effective_query, &records, context);
    candidates.truncate(50);
    Ok(unresolved(query, candidates))
}

fn fuzzy_candidates(
    query: &str,
    records: &[&DirectoryRecord],
    context: &SearchContext<'_>,
) -> Vec<Candidate> {
    let normalized_query = query.trim();
    let mut matcher = Matcher::new(NucleoConfig::DEFAULT.match_paths());
    let mut candidates = Vec::new();
    for record in records {
        let fuzzy = if normalized_query.is_empty() {
            0
        } else {
            let match_path = normalized_query
                .chars()
                .any(|character| character.is_whitespace() || character == '/');
            let path_haystack;
            let path_needle;
            let (haystack, needle) = if match_path {
                path_haystack = fuzzy_path_text(&record.display_path);
                path_needle = fuzzy_path_text(normalized_query);
                (path_haystack.as_str(), path_needle.as_str())
            } else {
                (record.basename.as_str(), normalized_query)
            };
            let mut haystack_buffer = Vec::new();
            let mut needle_buffer = Vec::new();
            matcher
                .fuzzy_match(
                    Utf32Str::new(haystack, &mut haystack_buffer),
                    Utf32Str::new(needle, &mut needle_buffer),
                )
                .unwrap_or(0)
        };
        if !normalized_query.is_empty() && fuzzy == 0 {
            continue;
        }
        let bookmark = context
            .bookmarks
            .values()
            .find(|bookmark| bookmark.path == record.path)
            .map(|bookmark| bookmark.name.clone());
        let history = context.history.get(&record.path);
        let exact_bonus = if smart_equal(&record.basename, normalized_query) {
            20_000.0
        } else {
            0.0
        };
        let prefix_bonus = if smart_starts_with(&record.basename, normalized_query) {
            3_000.0
        } else {
            0.0
        };
        let bookmark_bonus = if bookmark.is_some() { 2_000.0 } else { 0.0 };
        let project_bonus = if record.is_project_root { 500.0 } else { 0.0 };
        let frequency_bonus = history
            .map(|row| (row.visit_count as f64).ln_1p() * 250.0)
            .unwrap_or(0.0);
        let proximity_bonus = common_ancestor_depth(&record.path, context.cwd) as f64 * 25.0;
        let depth_penalty = record.depth as f64 * 2.0;
        candidates.push(Candidate {
            path: record.path.clone(),
            display_path: record.display_path.clone(),
            basename: record.basename.clone(),
            score: fuzzy as f64
                + exact_bonus
                + prefix_bonus
                + bookmark_bonus
                + project_bonus
                + frequency_bonus
                + proximity_bonus
                - depth_penalty,
            source: if normalized_query.is_empty() {
                "browse"
            } else {
                "fuzzy"
            },
            is_project_root: record.is_project_root,
            bookmark,
        });
    }
    candidates.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
    });
    candidates
}

fn resolved(query: String, path: PathBuf, confidence: f64, source: &'static str) -> QueryResponse {
    QueryResponse {
        query,
        resolved: true,
        path: Some(path),
        confidence: Some(confidence),
        source: Some(source),
        candidates: Vec::new(),
    }
}

fn unresolved(query: String, candidates: Vec<Candidate>) -> QueryResponse {
    QueryResponse {
        query,
        resolved: false,
        path: None,
        confidence: None,
        source: None,
        candidates,
    }
}

fn smart_equal(candidate: &str, query: &str) -> bool {
    if query.chars().any(char::is_uppercase) {
        candidate == query
    } else {
        candidate.to_lowercase() == query.to_lowercase()
    }
}

fn smart_starts_with(candidate: &str, query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    if query.chars().any(char::is_uppercase) {
        candidate.starts_with(query)
    } else {
        candidate.to_lowercase().starts_with(&query.to_lowercase())
    }
}

fn common_ancestor_depth(left: &Path, right: &Path) -> usize {
    left.components()
        .zip(right.components())
        .take_while(|(a, b)| a == b)
        .count()
}

fn fuzzy_path_text(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if matches!(character, '/' | '\\') {
                ' '
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::unix_now;

    fn record(path: &str) -> DirectoryRecord {
        let path = PathBuf::from(path);
        DirectoryRecord {
            basename: path
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned(),
            display_path: path.display().to_string(),
            parent: path.parent().unwrap_or(&path).to_path_buf(),
            depth: path.components().count(),
            path,
            is_project_root: false,
            project_kind: None,
            last_seen: unix_now(),
        }
    }

    #[test]
    fn unique_exact_basename_resolves() {
        let records = vec![record("/work/Punk"), record("/work/frontend")];
        let bookmarks = HashMap::new();
        let history = HashMap::new();
        let response = resolve(
            &["punk".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: Path::new("/tmp"),
            },
            false,
        )
        .expect("resolve");
        assert_eq!(response.path, Some(PathBuf::from("/work/Punk")));
    }

    #[test]
    fn duplicate_basename_stays_ambiguous() {
        let records = vec![record("/a/api"), record("/b/api")];
        let bookmarks = HashMap::new();
        let history = HashMap::new();
        let response = resolve(
            &["api".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: Path::new("/tmp"),
            },
            false,
        )
        .expect("resolve");
        assert!(!response.resolved);
        assert_eq!(response.candidates.len(), 2);
    }

    #[test]
    fn single_word_fuzzy_does_not_match_an_unrelated_parent_path() {
        let records = vec![
            record("/tmp/api-fixture/Punk"),
            record("/tmp/api-fixture/api"),
        ];
        let bookmarks = HashMap::new();
        let history = HashMap::new();
        let response = resolve(
            &["api".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: Path::new("/tmp"),
            },
            true,
        )
        .expect("resolve");
        assert_eq!(response.candidates.len(), 1);
        assert_eq!(response.candidates[0].basename, "api");
    }

    #[test]
    fn multi_word_query_matches_separate_path_segments() {
        let records = vec![
            record("/work/punk/apps/frontend"),
            record("/work/other/apps/frontend"),
        ];
        let bookmarks = HashMap::new();
        let history = HashMap::new();
        let response = resolve(
            &["punk".into(), "frontend".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: Path::new("/tmp"),
            },
            true,
        )
        .expect("resolve");
        assert_eq!(response.candidates.len(), 1);
        assert_eq!(
            response.candidates[0].path,
            PathBuf::from("/work/punk/apps/frontend")
        );
    }
}
