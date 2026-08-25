use std::{collections::HashMap, path::PathBuf};

#[cfg(unix)]
use dirgo::suggestions::discover_executables;
use dirgo::{
    config::RankingConfig,
    model::{DirectoryRecord, PathHistory},
    suggestions::{
        CommandHistoryEntry, PROTOCOL_VERSION, ShellKind, SuggestionData, SuggestionEngine,
        SuggestionRequest, SuggestionSource,
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
        executables: vec!["git".into(), "gitu".into()],
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
        discover_executables(Some(path.as_os_str())),
        vec!["dgo-other".to_string(), "dgo-tool".to_string()]
    );
}
