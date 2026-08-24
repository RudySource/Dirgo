use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};

use crate::{
    DirgoError, Result,
    config::RankingConfig,
    model::{
        Bookmark, Candidate, DirectoryRecord, PathHistory, QueryResponse, ScoreBreakdown, unix_now,
    },
    paths::absolute_directory,
};

const MIN_RANKED_PREFIX_QUERY_CHARS: usize = 3;
const MIN_RANKED_PREFIX_VISITS: u64 = 5;
const MIN_RANKED_PREFIX_ABSOLUTE_MARGIN: f64 = 1_000.0;
const MIN_RANKED_PREFIX_RELATIVE_MARGIN: f64 = 0.30;
const MAX_FUZZY_CANDIDATES: usize = 50;

pub struct SearchContext<'a> {
    pub records: &'a [DirectoryRecord],
    pub bookmarks: &'a HashMap<String, Bookmark>,
    pub history: &'a HashMap<PathBuf, PathHistory>,
    pub cwd: &'a Path,
    pub ranking: &'a RankingConfig,
}

pub struct PickerCandidateStream {
    records: std::vec::IntoIter<DirectoryRecord>,
    bookmarks_by_path: HashMap<PathBuf, String>,
    history: HashMap<PathBuf, PathHistory>,
    cwd: PathBuf,
    ranking: RankingConfig,
    now: u64,
    total: usize,
}

impl PickerCandidateStream {
    pub fn len(&self) -> usize {
        self.total
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }
}

impl Iterator for PickerCandidateStream {
    type Item = Candidate;

    fn next(&mut self) -> Option<Self::Item> {
        self.records.next().map(|record| {
            let bookmark = self.bookmarks_by_path.get(&record.path).cloned();
            let score_breakdown = score_candidate(
                &record,
                "",
                0.0,
                bookmark.is_some(),
                self.history.get(&record.path),
                &self.cwd,
                &self.ranking,
                self.now,
            );
            Candidate {
                path: record.path,
                display_path: record.display_path,
                basename: record.basename,
                score: score_breakdown.total,
                score_breakdown,
                source: "browse",
                is_project_root: record.is_project_root,
                bookmark,
            }
        })
    }
}

pub fn picker_candidate_stream(
    records: Vec<DirectoryRecord>,
    bookmarks: HashMap<String, Bookmark>,
    history: HashMap<PathBuf, PathHistory>,
    cwd: PathBuf,
    ranking: RankingConfig,
) -> PickerCandidateStream {
    let total = records.len();
    let bookmarks_by_path = bookmarks
        .into_values()
        .map(|bookmark| (bookmark.path, bookmark.name))
        .collect();
    PickerCandidateStream {
        records: records.into_iter(),
        bookmarks_by_path,
        history,
        cwd,
        ranking,
        now: unix_now(),
        total,
    }
}

pub fn picker_candidate(
    record: DirectoryRecord,
    bookmarks: &HashMap<PathBuf, String>,
    history: &HashMap<PathBuf, PathHistory>,
    cwd: &Path,
    ranking: &RankingConfig,
    now: u64,
) -> Candidate {
    let bookmark = bookmarks.get(&record.path).cloned();
    let score_breakdown = score_candidate(
        &record,
        "",
        0.0,
        bookmark.is_some(),
        history.get(&record.path),
        cwd,
        ranking,
        now,
    );
    Candidate {
        path: record.path,
        display_path: record.display_path,
        basename: record.basename,
        score: score_breakdown.total,
        score_breakdown,
        source: "browse",
        is_project_root: record.is_project_root,
        bookmark,
    }
}

pub fn picker_candidates(
    records: Vec<DirectoryRecord>,
    bookmarks: &HashMap<String, Bookmark>,
    history: &HashMap<PathBuf, PathHistory>,
    cwd: &Path,
    ranking: &RankingConfig,
) -> Vec<Candidate> {
    let mut candidates: Vec<_> = picker_candidate_stream(
        records,
        bookmarks.clone(),
        history.clone(),
        cwd.to_path_buf(),
        ranking.clone(),
    )
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
    include_fuzzy_candidates: bool,
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
                "bookmark @{name} points to a missing directory: {}. Repair it with `dgo bookmark add {name} --path <directory>` or run `dgo bookmark remove {name}`",
                crate::terminal::safe_path(&bookmark.path)
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

    let mut duplicate_exact = false;
    if !force_picker && !effective_query.is_empty() {
        let exact: Vec<_> = records
            .iter()
            .filter(|record| {
                smart_equal(&record.basename, &effective_query) && record.path.is_dir()
            })
            .collect();
        if exact.len() == 1 {
            return Ok(resolved(
                query,
                exact[0].path.clone(),
                1.0,
                "exact_basename",
            ));
        }
        duplicate_exact = exact.len() > 1;
    }

    if !force_picker
        && !duplicate_exact
        && let Some((path, confidence)) =
            ranked_prefix_resolution(&effective_query, &records, context, unix_now())
    {
        return Ok(resolved(query, path, confidence, "ranked_prefix"));
    }

    if !include_fuzzy_candidates {
        return Ok(unresolved(query, Vec::new()));
    }
    Ok(unresolved(
        query,
        fuzzy_candidates(&effective_query, &records, context),
    ))
}

fn ranked_prefix_resolution(
    query: &str,
    records: &[&DirectoryRecord],
    context: &SearchContext<'_>,
    now: u64,
) -> Option<(PathBuf, f64)> {
    if query.chars().count() < MIN_RANKED_PREFIX_QUERY_CHARS {
        return None;
    }
    let mut ranked = records
        .iter()
        .filter(|record| smart_starts_with(&record.basename, query))
        .map(|record| {
            let history = context.history.get(&record.path);
            let bookmarked = context
                .bookmarks
                .values()
                .any(|bookmark| bookmark.path == record.path);
            let score = score_candidate(
                record,
                query,
                0.0,
                bookmarked,
                history,
                context.cwd,
                context.ranking,
                now,
            )
            .total;
            (
                record.path.clone(),
                history.map_or(0, |row| row.visit_count),
                score,
            )
        })
        .collect::<Vec<_>>();
    if ranked.len() < 2 {
        return None;
    }
    ranked.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    let (top_path, top_visits, top_score) = &ranked[0];
    let runner_up_score = ranked[1].2;
    if *top_visits < MIN_RANKED_PREFIX_VISITS || *top_score <= 0.0 || !top_path.is_dir() {
        return None;
    }
    let absolute_margin = top_score - runner_up_score;
    let relative_margin = absolute_margin / top_score;
    if absolute_margin < MIN_RANKED_PREFIX_ABSOLUTE_MARGIN
        || relative_margin < MIN_RANKED_PREFIX_RELATIVE_MARGIN
    {
        return None;
    }
    let confidence = 0.8 + relative_margin.min(1.0) * 0.2;
    Some((top_path.clone(), confidence))
}

fn fuzzy_candidates(
    query: &str,
    records: &[&DirectoryRecord],
    context: &SearchContext<'_>,
) -> Vec<Candidate> {
    let normalized_query = query.trim();
    let now = unix_now();
    let mut matcher_config = NucleoConfig::DEFAULT.match_paths();
    matcher_config.ignore_case = !normalized_query.chars().any(char::is_uppercase);
    let mut matcher = Matcher::new(matcher_config);
    let bookmarks_by_path: HashMap<&Path, &str> = context
        .bookmarks
        .values()
        .map(|bookmark| (bookmark.path.as_path(), bookmark.name.as_str()))
        .collect();
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
        let bookmark = bookmarks_by_path
            .get(record.path.as_path())
            .map(|name| (*name).to_owned());
        let score_breakdown = score_candidate(
            record,
            normalized_query,
            fuzzy as f64,
            bookmark.is_some(),
            context.history.get(&record.path),
            context.cwd,
            context.ranking,
            now,
        );
        candidates.push(Candidate {
            path: record.path.clone(),
            display_path: record.display_path.clone(),
            basename: record.basename.clone(),
            score: score_breakdown.total,
            score_breakdown,
            source: if normalized_query.is_empty() {
                "browse"
            } else {
                "fuzzy"
            },
            is_project_root: record.is_project_root,
            bookmark,
        });
    }
    let compare = |a: &Candidate, b: &Candidate| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
    };
    if candidates.len() > MAX_FUZZY_CANDIDATES {
        candidates.select_nth_unstable_by(MAX_FUZZY_CANDIDATES, compare);
        candidates.truncate(MAX_FUZZY_CANDIDATES);
    }
    candidates.sort_unstable_by(compare);
    candidates
}

#[allow(clippy::too_many_arguments)]
fn score_candidate(
    record: &DirectoryRecord,
    query: &str,
    fuzzy: f64,
    bookmarked: bool,
    history: Option<&PathHistory>,
    cwd: &Path,
    ranking: &RankingConfig,
    now: u64,
) -> ScoreBreakdown {
    let exact = if !query.is_empty() && smart_equal(&record.basename, query) {
        20_000.0
    } else {
        0.0
    };
    let prefix = if smart_starts_with(&record.basename, query) {
        3_000.0
    } else {
        0.0
    };
    let path_segment = if !query.is_empty()
        && record
            .parent
            .components()
            .filter_map(|part| part.as_os_str().to_str())
            .any(|part| smart_equal(part, query))
    {
        1_500.0
    } else {
        0.0
    };
    let bookmark = if bookmarked {
        2_000.0 * ranking.bookmarks
    } else {
        0.0
    };
    let frequency = history
        .map(|row| (row.visit_count as f64).ln_1p() * 250.0 * ranking.frequency)
        .unwrap_or(0.0);
    let recency = history
        .map(|row| {
            let age_days = now.saturating_sub(row.last_visit) as f64 / 86_400.0;
            1_000.0 / (1.0 + age_days / 7.0) * ranking.recency
        })
        .unwrap_or(0.0);
    let proximity = common_ancestor_depth(&record.path, cwd) as f64 * 25.0 * ranking.proximity;
    let project = if record.is_project_root {
        500.0 * ranking.projects
    } else {
        0.0
    };
    let depth_penalty = record.depth as f64 * 2.0;
    let total = fuzzy
        + exact
        + prefix
        + path_segment
        + bookmark
        + frequency
        + recency
        + proximity
        + project
        - depth_penalty;
    ScoreBreakdown {
        fuzzy,
        exact,
        prefix,
        path_segment,
        bookmark,
        frequency,
        recency,
        proximity,
        project,
        depth_penalty,
        total,
    }
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
    use crate::{config::RankingConfig, model::unix_now};

    fn record(path: &str) -> DirectoryRecord {
        record_path(Path::new(path))
    }

    fn record_path(path: &Path) -> DirectoryRecord {
        let path = path.to_path_buf();
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

    fn history(path: &str, visits: u64, age_seconds: u64) -> PathHistory {
        let now = unix_now();
        PathHistory {
            path: PathBuf::from(path),
            visit_count: visits,
            first_visit: now.saturating_sub(age_seconds + 1_000),
            last_visit: now.saturating_sub(age_seconds),
        }
    }

    #[test]
    fn unique_exact_basename_resolves() {
        let temp = tempfile::tempdir().expect("tempdir");
        let punk = temp.path().join("Punk");
        let frontend = temp.path().join("frontend");
        std::fs::create_dir(&punk).expect("punk");
        std::fs::create_dir(&frontend).expect("frontend");
        let records = vec![record_path(&punk), record_path(&frontend)];
        let bookmarks = HashMap::new();
        let history = HashMap::new();
        let response = resolve(
            &["punk".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: Path::new("/tmp"),
                ranking: &RankingConfig::default(),
            },
            false,
            true,
        )
        .expect("resolve");
        assert_eq!(response.path, Some(punk));
    }

    #[test]
    fn duplicate_basename_stays_ambiguous() {
        let records = vec![record("/a/api"), record("/b/api")];
        let bookmarks = HashMap::new();
        let history = HashMap::from([
            (PathBuf::from("/a/api"), history("/a/api", 1_000, 0)),
            (PathBuf::from("/b/api"), history("/b/api", 1, 86_400 * 365)),
        ]);
        let response = resolve(
            &["api".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: Path::new("/tmp"),
                ranking: &RankingConfig::default(),
            },
            false,
            true,
        )
        .expect("resolve");
        assert!(!response.resolved);
        assert_eq!(response.candidates.len(), 2);
    }

    #[test]
    fn dominant_visited_prefix_can_resolve_with_measured_margin() {
        let temp = tempfile::tempdir().expect("tempdir");
        let frontend = temp.path().join("frontend");
        let frontier = temp.path().join("frontier");
        std::fs::create_dir(&frontend).expect("frontend");
        std::fs::create_dir(&frontier).expect("frontier");
        let records = vec![record_path(&frontend), record_path(&frontier)];
        let bookmarks = HashMap::new();
        let history = HashMap::from([
            (
                frontend.clone(),
                history(frontend.to_str().unwrap(), 100, 0),
            ),
            (
                frontier.clone(),
                history(frontier.to_str().unwrap(), 1, 86_400 * 365),
            ),
        ]);
        let response = resolve(
            &["fro".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: Path::new("/tmp"),
                ranking: &RankingConfig::default(),
            },
            false,
            false,
        )
        .expect("resolve");

        assert_eq!(response.path, Some(frontend));
        assert_eq!(response.source, Some("ranked_prefix"));
        assert!(response.confidence.is_some_and(|value| value >= 0.86));
    }

    #[test]
    fn close_prefix_scores_stay_ambiguous() {
        let records = vec![record("/work/frontend"), record("/work/frontier")];
        let bookmarks = HashMap::new();
        let history = HashMap::from([
            (
                PathBuf::from("/work/frontend"),
                history("/work/frontend", 10, 0),
            ),
            (
                PathBuf::from("/work/frontier"),
                history("/work/frontier", 9, 0),
            ),
        ]);
        let response = resolve(
            &["fro".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: Path::new("/tmp"),
                ranking: &RankingConfig::default(),
            },
            false,
            true,
        )
        .expect("resolve");

        assert!(!response.resolved);
        assert_eq!(response.candidates.len(), 2);
    }

    #[test]
    fn fuzzy_typo_never_uses_ranked_prefix_auto_resolution() {
        let records = vec![record("/work/frontend"), record("/work/friend")];
        let bookmarks = HashMap::new();
        let history = HashMap::from([(
            PathBuf::from("/work/frontend"),
            history("/work/frontend", 10_000, 0),
        )]);
        let response = resolve(
            &["frntnd".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: Path::new("/tmp"),
                ranking: &RankingConfig::default(),
            },
            false,
            true,
        )
        .expect("resolve");

        assert!(!response.resolved);
        assert_ne!(response.source, Some("ranked_prefix"));
    }

    #[test]
    fn fuzzy_matching_uses_smart_case() {
        let records = vec![record("/work/Punk"), record("/work/punk")];
        let bookmarks = HashMap::new();
        let history = HashMap::new();
        let context = SearchContext {
            records: &records,
            bookmarks: &bookmarks,
            history: &history,
            cwd: Path::new("/tmp"),
            ranking: &RankingConfig::default(),
        };

        let sensitive = resolve(&["Pn".into()], &context, true, true).expect("smart case");
        assert_eq!(sensitive.candidates.len(), 1);
        assert_eq!(sensitive.candidates[0].basename, "Punk");

        let insensitive = resolve(&["pn".into()], &context, true, true).expect("lower case");
        assert_eq!(insensitive.candidates.len(), 2);
    }

    #[test]
    fn fuzzy_candidates_keep_the_best_fifty_in_deterministic_order() {
        let records: Vec<_> = (0..75)
            .rev()
            .map(|index| record(&format!("/fixture/node-{index:03}")))
            .collect();
        let bookmarks = HashMap::new();
        let history = HashMap::new();
        let context = SearchContext {
            records: &records,
            bookmarks: &bookmarks,
            history: &history,
            cwd: Path::new("/fixture"),
            ranking: &RankingConfig::default(),
        };

        let candidates = fuzzy_candidates("node", &records.iter().collect::<Vec<_>>(), &context);

        assert_eq!(candidates.len(), MAX_FUZZY_CANDIDATES);
        let paths: Vec<_> = candidates
            .into_iter()
            .map(|candidate| candidate.path)
            .collect();
        let expected: Vec<_> = (0..MAX_FUZZY_CANDIDATES)
            .map(|index| PathBuf::from(format!("/fixture/node-{index:03}")))
            .collect();
        assert_eq!(paths, expected);
    }

    #[test]
    fn picker_stream_preserves_candidate_data_without_prebuilding_a_vector() {
        let records = vec![record("/fixture/first"), record("/fixture/second")];
        let bookmarks = HashMap::from([(
            "work".to_owned(),
            Bookmark {
                name: "work".to_owned(),
                path: PathBuf::from("/fixture/second"),
                created_at: 1,
                last_used: None,
                tags: Vec::new(),
            },
        )]);
        let stream = picker_candidate_stream(
            records,
            bookmarks,
            HashMap::new(),
            PathBuf::from("/fixture"),
            RankingConfig::default(),
        );

        assert_eq!(stream.len(), 2);
        let candidates: Vec<_> = stream.collect();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].basename, "first");
        assert_eq!(candidates[1].bookmark.as_deref(), Some("work"));
    }

    #[test]
    fn force_picker_overrides_a_dominant_prefix() {
        let records = vec![record("/work/frontend"), record("/work/frontier")];
        let bookmarks = HashMap::new();
        let history = HashMap::from([
            (
                PathBuf::from("/work/frontend"),
                history("/work/frontend", 1_000, 0),
            ),
            (
                PathBuf::from("/work/frontier"),
                history("/work/frontier", 1, 86_400 * 365),
            ),
        ]);
        let response = resolve(
            &["fro".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: Path::new("/tmp"),
                ranking: &RankingConfig::default(),
            },
            true,
            true,
        )
        .expect("resolve");

        assert!(!response.resolved);
    }

    #[test]
    fn stale_exact_basename_never_auto_resolves() {
        let temp = tempfile::tempdir().expect("tempdir");
        let stale = temp.path().join("vanished");
        std::fs::create_dir(&stale).expect("stale fixture");
        let records = vec![record_path(&stale)];
        std::fs::remove_dir(&stale).expect("remove fixture directory");
        let bookmarks = HashMap::new();
        let history = HashMap::new();
        let response = resolve(
            &["vanished".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: temp.path(),
                ranking: &RankingConfig::default(),
            },
            false,
            true,
        )
        .expect("resolve");

        assert!(!response.resolved);
    }

    #[test]
    fn stale_dominant_prefix_never_auto_resolves() {
        let temp = tempfile::tempdir().expect("tempdir");
        let stale = temp.path().join("frontend");
        let runner = temp.path().join("frontier");
        std::fs::create_dir(&stale).expect("stale fixture");
        std::fs::create_dir(&runner).expect("runner fixture");
        let records = vec![record_path(&stale), record_path(&runner)];
        std::fs::remove_dir(&stale).expect("remove fixture directory");
        let bookmarks = HashMap::new();
        let history = HashMap::from([
            (stale.clone(), history(stale.to_str().unwrap(), 10_000, 0)),
            (
                runner.clone(),
                history(runner.to_str().unwrap(), 1, 86_400 * 365),
            ),
        ]);
        let response = resolve(
            &["fro".into()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: temp.path(),
                ranking: &RankingConfig::default(),
            },
            false,
            false,
        )
        .expect("resolve");

        assert!(!response.resolved);
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
                ranking: &RankingConfig::default(),
            },
            true,
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
                ranking: &RankingConfig::default(),
            },
            true,
            true,
        )
        .expect("resolve");
        assert_eq!(response.candidates.len(), 1);
        assert_eq!(
            response.candidates[0].path,
            PathBuf::from("/work/punk/apps/frontend")
        );
    }

    #[test]
    fn score_components_honor_frequency_recency_and_proximity() {
        let ranking = RankingConfig::default();
        let now = 2_000_000;
        let row = PathHistory {
            path: PathBuf::from("/work/current/api"),
            visit_count: 9,
            first_visit: now - 100_000,
            last_visit: now - 60,
        };
        let score = score_candidate(
            &record("/work/current/api"),
            "ap",
            42.0,
            false,
            Some(&row),
            Path::new("/work/current"),
            &ranking,
            now,
        );

        assert_eq!(score.fuzzy, 42.0);
        assert_eq!(score.prefix, 3_000.0);
        assert!(score.frequency > 0.0);
        assert!(score.recency > 800.0);
        assert!(score.proximity > 0.0);
        assert_eq!(
            score.total,
            score.fuzzy
                + score.exact
                + score.prefix
                + score.path_segment
                + score.bookmark
                + score.frequency
                + score.recency
                + score.proximity
                + score.project
                - score.depth_penalty
        );
    }

    #[test]
    fn disabled_ranking_weights_remove_their_components() {
        let ranking = RankingConfig {
            frequency: 0.0,
            recency: 0.0,
            proximity: 0.0,
            bookmarks: 0.0,
            projects: 0.0,
        };
        let now = 2_000_000;
        let row = PathHistory {
            path: PathBuf::from("/work/api"),
            visit_count: 100,
            first_visit: now - 100_000,
            last_visit: now,
        };
        let mut project = record("/work/api");
        project.is_project_root = true;
        let score = score_candidate(
            &project,
            "",
            0.0,
            true,
            Some(&row),
            Path::new("/work"),
            &ranking,
            now,
        );

        assert_eq!(score.bookmark, 0.0);
        assert_eq!(score.frequency, 0.0);
        assert_eq!(score.recency, 0.0);
        assert_eq!(score.proximity, 0.0);
        assert_eq!(score.project, 0.0);
    }
}
