use std::{collections::HashMap, path::PathBuf};

use dirgo::{
    config::RankingConfig,
    model::{DirectoryRecord, PathHistory},
    suggestions::{
        CommandCatalog, CommandHistoryEntry, CompletionContext, PROTOCOL_VERSION, ShellKind,
        Suggestion, SuggestionData, SuggestionEngine, SuggestionRequest, SuggestionSource,
        TextEdit, TopSuggestions,
    },
};

fn request(shell: ShellKind, cwd: &str, before_cursor: &str) -> SuggestionRequest {
    SuggestionRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: 1,
        shell,
        cwd: cwd.into(),
        before_cursor: before_cursor.into(),
        after_cursor: String::new(),
        max_results: 8,
    }
}

fn record(path: &str) -> DirectoryRecord {
    let path = PathBuf::from(path);
    DirectoryRecord {
        display_path: path.display().to_string(),
        basename: path
            .file_name()
            .expect("basename")
            .to_string_lossy()
            .into_owned(),
        parent: path.parent().expect("parent").to_path_buf(),
        depth: path.components().count(),
        path,
        is_project_root: true,
        project_kind: None,
        last_seen: 1,
    }
}

#[test]
fn directory_provider_reuses_dirgo_ranking_and_shell_escapes_the_edit() {
    let project = record("/work/Client Projects/Dirgo");
    let mut navigation_history = HashMap::new();
    navigation_history.insert(
        project.path.clone(),
        PathHistory {
            path: project.path.clone(),
            visit_count: 12,
            first_visit: 1,
            last_visit: u64::MAX,
        },
    );
    let data = SuggestionData {
        records: vec![project],
        navigation_history,
        ranking: RankingConfig::default(),
        ..SuggestionData::default()
    };

    let suggestions =
        SuggestionEngine::new(data).suggest(&request(ShellKind::Zsh, "/work", "cd dir"));

    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].source, SuggestionSource::NavigationHistory);
    assert_eq!(suggestions[0].display, "Dirgo");
    assert_eq!(suggestions[0].edit.expected_before, "cd dir");
    assert_eq!(
        suggestions[0].edit.replacement,
        "cd '/work/Client Projects/Dirgo'"
    );
}

#[test]
fn powershell_directory_edits_use_literal_single_quoted_paths() {
    let data = SuggestionData {
        records: vec![record("C:/Work/Rudy's Project")],
        ..SuggestionData::default()
    };

    let suggestions = SuggestionEngine::new(data).suggest(&request(
        ShellKind::PowerShell,
        "C:/Work",
        "Set-Location rud",
    ));

    assert_eq!(
        suggestions[0].edit.replacement,
        "Set-Location 'C:/Work/Rudy''s Project'"
    );
}

#[test]
fn command_history_is_prefix_ranked_deduplicated_and_sanitized() {
    let data = SuggestionData {
        catalog: CommandCatalog::from_executable_names(["git".into(), "gitu".into()]),
        command_history: vec![
            CommandHistoryEntry::new("git status", 5, 100),
            CommandHistoryEntry::new("git status", 2, 90),
            CommandHistoryEntry::new("git push\nrm -rf /", 99, 200),
            CommandHistoryEntry::new("export API_TOKEN=super-secret", 99, 201),
        ],
        ..SuggestionData::default()
    };

    let suggestions =
        SuggestionEngine::new(data).suggest(&request(ShellKind::Bash, "/work", "git"));

    assert_eq!(suggestions[0].edit.replacement, "git status");
    assert_eq!(suggestions[0].source, SuggestionSource::CommandHistory);
    assert_eq!(
        suggestions
            .iter()
            .filter(|item| item.edit.replacement == "git status")
            .count(),
        1
    );
    assert!(
        suggestions
            .iter()
            .all(|item| !item.edit.replacement.contains('\n'))
    );
    assert!(
        suggestions
            .iter()
            .all(|item| !item.edit.replacement.contains("API_TOKEN"))
    );
}

#[test]
fn command_history_drops_probable_credentials_before_ranking() {
    let data = SuggestionData {
        command_history: vec![
            CommandHistoryEntry::new("export API_TOKEN=super-secret", 99, 201),
            CommandHistoryEntry::new("export APP_MODE=production", 1, 200),
        ],
        ..SuggestionData::default()
    };

    let suggestions =
        SuggestionEngine::new(data).suggest(&request(ShellKind::Zsh, "/work", "export A"));

    assert_eq!(suggestions.len(), 1);
    assert_eq!(
        suggestions[0].edit.replacement,
        "export APP_MODE=production"
    );
}

#[test]
fn filesystem_provider_completes_relative_arguments_without_executing_them() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(temp.path().join("main file.rs"), "fn main() {}").expect("fixture");
    std::fs::write(temp.path().join("manual.md"), "docs").expect("fixture");

    let suggestions = SuggestionEngine::new(SuggestionData::default()).suggest(&request(
        ShellKind::Bash,
        temp.path().to_str().expect("utf8 temp path"),
        "cat mai",
    ));

    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].source, SuggestionSource::Filesystem);
    assert_eq!(suggestions[0].display, "main file.rs");
    assert_eq!(suggestions[0].edit.replacement, "cat 'main file.rs'");
}

#[cfg(unix)]
#[test]
fn executable_discovery_is_bounded_unique_and_requires_execute_permission() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::create_dir_all(&first).expect("first path entry");
    std::fs::create_dir_all(&second).expect("second path entry");
    for path in [
        first.join("dgo-tool"),
        second.join("dgo-tool"),
        second.join("dgo-other"),
    ] {
        std::fs::write(&path, "#!/bin/sh\n").expect("executable fixture");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
            .expect("executable permissions");
    }
    std::fs::write(first.join("dgo-private"), "not executable").expect("plain fixture");
    let path = std::env::join_paths([&first, &second]).expect("PATH");

    assert_eq!(
        CommandCatalog::discover(Some(path.as_os_str()))
            .executable_names()
            .collect::<Vec<_>>(),
        vec!["dgo-other", "dgo-tool"]
    );
}

#[test]
fn completion_context_finds_command_and_partial_subcommand_without_evaluating_shell_text() {
    let context = CompletionContext::parse(ShellKind::Zsh, "dgo sl");

    assert_eq!(context.command(), Some("dgo"));
    assert_eq!(context.current_token(), "sl");
    assert_eq!(context.replacement_start(), 4);
    assert!(!context.is_option());
}

#[test]
fn completion_context_recognizes_partial_options_and_preserves_quoted_tokens() {
    let option = CompletionContext::parse(ShellKind::Bash, "dgo --upd");
    assert_eq!(option.command(), Some("dgo"));
    assert_eq!(option.current_token(), "--upd");
    assert!(option.is_option());

    let quoted = CompletionContext::parse(ShellKind::Zsh, "git commit -m 'release can");
    assert_eq!(quoted.command(), Some("git"));
    assert_eq!(quoted.current_token(), "release can");
    assert_eq!(quoted.replacement_start(), 15);
}

#[test]
fn bounded_top_k_keeps_only_best_unique_replacements_in_deterministic_order() {
    let suggestion = |id: &str, replacement: &str, score: f64| Suggestion {
        id: id.into(),
        edit: TextEdit {
            expected_before: "d".into(),
            replacement: replacement.into(),
        },
        display: replacement.into(),
        description: None,
        source: SuggestionSource::Command,
        score,
    };
    let mut top = TopSuggestions::new(3);
    top.push(suggestion("docker-low", "docker", 10.0));
    top.push(suggestion("dirgo", "dgo", 40.0));
    top.push(suggestion("delta", "delta", 30.0));
    top.push(suggestion("docker-high", "docker", 50.0));
    top.push(suggestion("dust", "dust", 20.0));

    let results = top.finish();
    assert_eq!(
        results
            .iter()
            .map(|item| item.edit.replacement.as_str())
            .collect::<Vec<_>>(),
        vec!["docker", "dgo", "delta"]
    );
    assert_eq!(results[0].id, "docker-high");
}

#[test]
fn dirgo_catalog_completes_public_subcommands_and_options() {
    let engine = SuggestionEngine::new(SuggestionData::default());

    let subcommands = engine.suggest(&request(ShellKind::Zsh, "/work", "dgo sug"));
    assert_eq!(subcommands[0].edit.replacement, "dgo suggestions");
    assert_eq!(subcommands[0].source, SuggestionSource::Subcommand);
    assert!(
        subcommands[0]
            .description
            .as_deref()
            .is_some_and(|value| value.contains("shell-native"))
    );

    let options = engine.suggest(&request(ShellKind::PowerShell, "/work", "dgo --upd"));
    assert_eq!(options[0].edit.replacement, "dgo --update");
    assert_eq!(options[0].source, SuggestionSource::Option);
}

#[test]
fn dgo_partial_query_still_prioritizes_matching_directories() {
    let data = SuggestionData {
        records: vec![record("/work/Slash")],
        ..SuggestionData::default()
    };

    let suggestions =
        SuggestionEngine::new(data).suggest(&request(ShellKind::Zsh, "/work", "dgo sl"));
    assert_eq!(suggestions[0].edit.replacement, "dgo /work/Slash");
    assert_eq!(suggestions[0].source, SuggestionSource::Directory);
}

#[test]
fn dirgo_catalog_follows_nested_subcommands_and_never_exposes_internal_commands() {
    let engine = SuggestionEngine::new(SuggestionData::default());

    let nested = engine.suggest(&request(ShellKind::Fish, "/work", "dgo suggestions hi"));
    assert_eq!(nested[0].edit.replacement, "dgo suggestions history");
    assert_eq!(nested[0].source, SuggestionSource::Subcommand);

    let hidden = engine.suggest(&request(ShellKind::Bash, "/work", "dgo __s"));
    assert!(hidden.iter().all(|item| !item.display.starts_with("__")));
}

#[test]
fn path_and_history_candidates_are_merged_instead_of_suppressing_each_other() {
    let data = SuggestionData {
        catalog: CommandCatalog::from_executable_names(["git".into(), "gitsome".into()]),
        command_history: vec![CommandHistoryEntry::new("git status", 4, 10)],
        ..SuggestionData::default()
    };

    let suggestions = SuggestionEngine::new(data).suggest(&request(ShellKind::Zsh, "/work", "git"));
    assert!(
        suggestions
            .iter()
            .any(|item| item.source == SuggestionSource::Command)
    );
    assert!(
        suggestions
            .iter()
            .any(|item| item.source == SuggestionSource::CommandHistory)
    );
}
