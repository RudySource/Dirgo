use std::{collections::HashMap, path::PathBuf};

use dirgo::{
    config::RankingConfig,
    model::{DirectoryRecord, PathHistory},
    suggestions::{
        CommandCatalog, CommandHistoryEntry, CompletionContext, PROTOCOL_VERSION, ProjectCommand,
        ProjectCommandSnapshot, ShellKind, Suggestion, SuggestionData, SuggestionEngine,
        SuggestionRequest, SuggestionSource, TextEdit, TopSuggestions,
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
        terminal_rows: None,
        terminal_columns: None,
        presentation: dirgo::suggestions::SuggestionPresentation::Explicit,
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
fn directory_provider_exposes_ordered_focused_root_paths_without_crawling() {
    let focused = record("/home/Library/Application Support/Adobe/CEP/extensions");
    let unrelated = record("/home/Library/Unrelated/Noise");
    let engine = SuggestionEngine::new_indexed(SuggestionData {
        records: vec![unrelated, focused],
        ..SuggestionData::default()
    });

    let suggestions = engine.suggest(&request(
        ShellKind::Zsh,
        "/home",
        "dgo library/adobe/cep/ext",
    ));

    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].source, SuggestionSource::Directory);
    assert_eq!(suggestions[0].display, "extensions");
    assert_eq!(
        suggestions[0].edit.replacement,
        "dgo '/home/Library/Application Support/Adobe/CEP/extensions'"
    );
}

#[test]
fn directory_suggestion_description_identifies_the_contextual_path() {
    let engine = SuggestionEngine::new(SuggestionData {
        records: vec![record("/work/one/gif"), record("/work/two/gif")],
        ..SuggestionData::default()
    });

    let suggestions = engine.suggest(&request(ShellKind::Zsh, "/work", "dgo gif"));

    assert_eq!(suggestions.len(), 2);
    assert_eq!(suggestions[0].description.as_deref(), Some("one/gif"));
    assert_eq!(suggestions[1].description.as_deref(), Some("two/gif"));
}

#[cfg(unix)]
#[test]
fn directory_description_recognizes_a_symlinked_cwd_alias() {
    use std::{fs, os::unix::fs::symlink};

    let temp = tempfile::tempdir().expect("tempdir");
    let real_root = temp.path().join("real");
    let directory = real_root.join("Projects/Slash");
    fs::create_dir_all(&directory).expect("fixture directory");
    let alias = temp.path().join("alias");
    symlink(&real_root, &alias).expect("cwd alias");
    let canonical_directory = fs::canonicalize(&directory).expect("canonical fixture directory");
    let engine = SuggestionEngine::new(SuggestionData {
        records: vec![record(canonical_directory.to_str().expect("UTF-8 fixture"))],
        ..SuggestionData::default()
    });

    let suggestions = engine.suggest(&request(
        ShellKind::Zsh,
        alias.to_str().expect("UTF-8 fixture"),
        "dgo sl",
    ));

    assert_eq!(suggestions.len(), 1);
    assert_eq!(
        suggestions[0].description.as_deref(),
        Some("Projects/Slash")
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
fn command_history_prefers_current_project_cwd_and_reliable_success() {
    let temp = tempfile::tempdir().expect("tempdir");
    let current = temp.path().join("current");
    let other = temp.path().join("other");
    std::fs::create_dir_all(current.join("src")).expect("current");
    std::fs::create_dir_all(&other).expect("other");
    std::fs::write(current.join("Cargo.toml"), "[workspace]").expect("marker");
    std::fs::write(other.join("Cargo.toml"), "[workspace]").expect("marker");

    let mut local = CommandHistoryEntry::new("cargo test --workspace", 4, 200);
    local.scope_key = format!("project:{}", current.display());
    local.success_count = 4;
    local.recent_cwds.push(current.join("src"));
    let mut unrelated = CommandHistoryEntry::new("cargo test --all", 50, 300);
    unrelated.scope_key = format!("project:{}", other.display());
    unrelated.failure_count = 50;
    let mut global = CommandHistoryEntry::new("cargo test --global", 100, 400);
    global.scope_key = "global".into();
    global.unknown_count = 100;

    let suggestions = SuggestionEngine::new(SuggestionData {
        command_history: vec![global, unrelated, local],
        ..SuggestionData::default()
    })
    .suggest(&request(
        ShellKind::Zsh,
        current.join("src").to_str().expect("cwd"),
        "cargo test --",
    ));

    assert_eq!(suggestions[0].edit.replacement, "cargo test --workspace");
    assert_eq!(suggestions[0].source, SuggestionSource::CommandHistory);
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

    let catalog = CommandCatalog::discover(Some(path.as_os_str()));
    assert_eq!(
        catalog.executable_names().collect::<Vec<_>>(),
        vec!["dgo-other", "dgo-tool"]
    );
    let suggestions = SuggestionEngine::new(SuggestionData {
        catalog,
        ..SuggestionData::default()
    })
    .suggest(&request(
        ShellKind::Zsh,
        temp.path().to_str().expect("utf8 temp path"),
        "dgo-",
    ));
    assert!(
        suggestions
            .iter()
            .all(|suggestion| suggestion.source == SuggestionSource::Executable)
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
fn catalog_pages_cover_every_ranked_match_without_overlap() {
    let catalog =
        CommandCatalog::from_executable_names((0..205).map(|index| format!("slx-{index:03}")));
    let engine = SuggestionEngine::new(SuggestionData {
        catalog,
        ..SuggestionData::default()
    });
    let request = request(ShellKind::Zsh, "/work", "slx-");

    let first = engine.suggest_page(&request, 0, 96);
    let second = engine.suggest_page(&request, 96, 96);
    let last = engine.suggest_page(&request, 192, 96);

    assert_eq!(first.total, 205);
    assert_eq!(first.suggestions.len(), 96);
    assert_eq!(second.total, 205);
    assert_eq!(second.suggestions.len(), 96);
    assert_eq!(last.total, 205);
    assert_eq!(last.suggestions.len(), 13);
    assert_eq!(first.suggestions[0].display, "slx-000");
    assert_eq!(second.suggestions[0].display, "slx-096");
    assert_eq!(last.suggestions[12].display, "slx-204");

    let mut replacements = first
        .suggestions
        .iter()
        .chain(&second.suggestions)
        .chain(&last.suggestions)
        .map(|suggestion| suggestion.edit.replacement.as_str())
        .collect::<Vec<_>>();
    replacements.sort_unstable();
    replacements.dedup();
    assert_eq!(replacements.len(), 205);
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

    let version = engine.suggest(&request(ShellKind::Zsh, "/work", "dgo --ver"));
    assert!(version.iter().any(|suggestion| {
        suggestion.edit.replacement == "dgo --version"
            && suggestion.source == SuggestionSource::Option
    }));
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
fn external_command_catalog_completes_git_and_docker_without_running_them() {
    let engine = SuggestionEngine::new(SuggestionData::default());

    for shell in [
        ShellKind::Zsh,
        ShellKind::Bash,
        ShellKind::Fish,
        ShellKind::PowerShell,
    ] {
        let git = engine.suggest(&request(shell, "/work", "git ch"));
        assert_eq!(git[0].edit.replacement, "git checkout");
        assert_eq!(git[0].source, SuggestionSource::Subcommand);
        assert_eq!(
            git[0].description.as_deref(),
            Some("Switch branches or restore files")
        );

        let docker = engine.suggest(&request(shell, "/work", "docker co"));
        assert!(docker.iter().any(|item| {
            item.edit.replacement == "docker compose" && item.source == SuggestionSource::Subcommand
        }));

        let compose = engine.suggest(&request(shell, "/work", "docker compose u"));
        assert_eq!(compose[0].edit.replacement, "docker compose up");
        assert_eq!(
            compose[0].description.as_deref(),
            Some("Create and start services")
        );
    }
}

#[test]
fn external_command_catalog_completes_nested_options_and_package_tools() {
    let engine = SuggestionEngine::new(SuggestionData::default());

    let cargo = engine.suggest(&request(ShellKind::Zsh, "/work", "cargo b"));
    assert!(
        cargo
            .iter()
            .any(|item| item.edit.replacement == "cargo build")
    );

    let git_option = engine.suggest(&request(ShellKind::PowerShell, "/work", "git commit --am"));
    assert_eq!(git_option[0].edit.replacement, "git commit --amend");
    assert_eq!(git_option[0].source, SuggestionSource::Option);

    let npm = engine.suggest(&request(ShellKind::Fish, "/work", "npm r"));
    assert!(npm.iter().any(|item| item.edit.replacement == "npm run"));

    let aws = engine.suggest(&request(ShellKind::Zsh, "/work", "aws s3 s"));
    assert!(
        aws.iter()
            .any(|item| item.edit.replacement == "aws s3 sync")
    );

    let dotnet = engine.suggest(&request(ShellKind::PowerShell, "/work", "dotnet b"));
    assert!(
        dotnet
            .iter()
            .any(|item| item.edit.replacement == "dotnet build")
    );

    let curl = engine.suggest(&request(ShellKind::Bash, "/work", "curl --hea"));
    assert!(
        curl.iter()
            .any(|item| item.edit.replacement == "curl --header")
    );
}

#[test]
fn completed_leaf_command_offers_options_before_a_dash_is_typed() {
    let suggestions = SuggestionEngine::new(SuggestionData::default()).suggest(&request(
        ShellKind::Zsh,
        "/work",
        "git commit ",
    ));

    assert!(suggestions.iter().any(|item| {
        item.display == "-m"
            && item.edit.replacement == "git commit -m"
            && item.source == SuggestionSource::Option
    }));
}

#[test]
fn exact_short_option_remains_visible_and_advances_to_its_argument() {
    let suggestions = SuggestionEngine::new(SuggestionData::default()).suggest(&request(
        ShellKind::Zsh,
        "/work",
        "git commit -m",
    ));

    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].display, "-m");
    assert_eq!(suggestions[0].edit.replacement, "git commit -m ");
    assert_eq!(suggestions[0].source, SuggestionSource::Option);
    assert_eq!(
        suggestions[0].description.as_deref(),
        Some("Use the given commit message")
    );
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
            .any(|item| item.source == SuggestionSource::Executable)
    );
    assert!(
        suggestions
            .iter()
            .any(|item| item.source == SuggestionSource::CommandHistory)
    );
}

#[test]
fn project_commands_are_scoped_to_the_snapshot_root_and_only_insert_text() {
    let snapshot = ProjectCommandSnapshot::new(
        PathBuf::from("/work/app"),
        vec![
            ProjectCommand::new(
                "npm run build",
                "build",
                "package.json script",
                "package-json:build",
            ),
            ProjectCommand::new(
                "cargo run --bin api",
                "api",
                "Cargo binary",
                "cargo-bin:api",
            ),
        ],
    );
    let engine = SuggestionEngine::new(SuggestionData::default());

    for shell in [
        ShellKind::Zsh,
        ShellKind::Bash,
        ShellKind::Fish,
        ShellKind::PowerShell,
    ] {
        let suggestions = engine.suggest_with_project(
            &request(shell, "/work/app/crates/ui", "npm run bu"),
            Some(&snapshot),
        );
        let build = suggestions
            .iter()
            .find(|item| item.edit.replacement == "npm run build")
            .expect("project command");
        assert_eq!(build.source, SuggestionSource::ProjectCommand);
        assert_eq!(build.edit.expected_before, "npm run bu");
        assert_eq!(build.description.as_deref(), Some("package.json script"));
    }

    let outside = engine.suggest_with_project(
        &request(ShellKind::Zsh, "/work/another", "npm run bu"),
        Some(&snapshot),
    );
    assert!(
        outside
            .iter()
            .all(|item| item.source != SuggestionSource::ProjectCommand)
    );
}

#[test]
fn project_command_acceptance_preserves_leading_shell_whitespace() {
    let snapshot = ProjectCommandSnapshot::new(
        PathBuf::from("/work/app"),
        vec![ProjectCommand::new(
            "npm run build",
            "build",
            "package.json script",
            "package-json:build",
        )],
    );
    let engine = SuggestionEngine::new(SuggestionData::default());

    let suggestions = engine.suggest_with_project(
        &request(ShellKind::Zsh, "/work/app", "  npm run bu"),
        Some(&snapshot),
    );
    let build = suggestions
        .iter()
        .find(|item| item.source == SuggestionSource::ProjectCommand)
        .expect("project command");

    assert_eq!(build.edit.expected_before, "  npm run bu");
    assert_eq!(build.edit.replacement, "  npm run build");
}

#[test]
fn current_project_command_outranks_the_same_global_history_entry() {
    let snapshot = ProjectCommandSnapshot::new(
        PathBuf::from("/work/app"),
        vec![ProjectCommand::new(
            "npm run build",
            "build",
            "package.json script",
            "package-json:build",
        )],
    );
    let engine = SuggestionEngine::new(SuggestionData {
        command_history: vec![CommandHistoryEntry::new(
            "npm run build",
            500,
            2_000_000_000,
        )],
        ..SuggestionData::default()
    });

    let suggestions = engine.suggest_with_project(
        &request(ShellKind::Zsh, "/work/app", "npm run bu"),
        Some(&snapshot),
    );

    assert_eq!(suggestions[0].source, SuggestionSource::ProjectCommand);
    assert_eq!(suggestions[0].display, "build");
}
