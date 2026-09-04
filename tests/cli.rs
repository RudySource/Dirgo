#![cfg(unix)]

use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command as ProcessCommand, Stdio},
};

use assert_cmd::Command;
use predicates::prelude::*;

struct Fixture {
    temp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("filesystem");
        fs::create_dir_all(root.join("Projects/Punk/src")).expect("punk tree");
        fs::create_dir_all(root.join("Projects/quo'te space/子")).expect("quoted unicode tree");
        fs::create_dir_all(root.join("Services/api")).expect("api one");
        fs::create_dir_all(root.join("Archive/api")).expect("api two");
        fs::create_dir_all(root.join("-dash")).expect("leading dash tree");
        fs::write(
            root.join("Projects/Punk/Cargo.toml"),
            "[package]\nname='punk'\nversion='0.1.0'\n",
        )
        .expect("marker");
        let config_dir = temp.path().join("config/dirgo");
        fs::create_dir_all(&config_dir).expect("config dir");
        fs::write(
            config_dir.join("config.toml"),
            format!("schema_version = 1\nroots = [{}]\n", toml_string(&root)),
        )
        .expect("config");
        Self { temp }
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("dgo").expect("binary");
        command
            .env("XDG_CONFIG_HOME", self.temp.path().join("config"))
            .env("XDG_CACHE_HOME", self.temp.path().join("cache"))
            .env("XDG_STATE_HOME", self.temp.path().join("state"))
            .env("DGO_DISABLE_UPDATE_CHECK", "1");
        command
    }
}

#[test]
fn update_notifications_are_visible_and_can_be_disabled_persistently() {
    let fixture = Fixture::new();
    let cache_dir = fixture.temp.path().join("cache/dirgo");
    fs::create_dir_all(&cache_dir).expect("update cache dir");
    fs::write(
        cache_dir.join("update.json"),
        format!(
            r#"{{"checked_at":{},"latest_version":"9.9.9"}}"#,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_secs()
        ),
    )
    .expect("update cache");

    fixture
        .command()
        .env_remove("DGO_DISABLE_UPDATE_CHECK")
        .args(["query", "punk"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Dirgo 9.9.9 is ready"));

    fixture
        .command()
        .args(["update-notifications", "off"])
        .assert()
        .success()
        .stdout(predicate::str::contains("disabled"));
    fixture
        .command()
        .env_remove("DGO_DISABLE_UPDATE_CHECK")
        .args(["query", "punk"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());

    fixture
        .command()
        .args(["update-notifications", "on"])
        .assert()
        .success()
        .stdout(predicate::str::contains("enabled"));
}

fn toml_string(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

#[test]
fn help_explains_public_commands_and_nested_workflows() {
    Command::cargo_bin("dgo")
        .expect("binary")
        .arg("--help")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Print parent-shell integration")
                .and(predicate::str::contains(
                    "Rebuild the disposable filesystem index",
                ))
                .and(predicate::str::contains("Diagnose configuration, storage")),
        );
    Command::cargo_bin("dgo")
        .expect("binary")
        .args(["bookmark", "add", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Create a bookmark or repair its destination")
                .and(predicate::str::contains("Destination directory")),
        );
}

#[test]
fn refresh_then_exact_query_returns_only_the_path() {
    let fixture = Fixture::new();
    fixture
        .command()
        .arg("refresh")
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed"));
    fixture
        .command()
        .args(["query", "punk"])
        .assert()
        .success()
        .stdout(predicate::str::ends_with("/Projects/Punk\n"));
}

#[test]
fn completions_do_not_require_xdg_storage_and_cover_public_commands() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state_file = temp.path().join("not-a-directory");
    fs::write(&state_file, "blocked").expect("blocked state path");
    let mut command = Command::cargo_bin("dgo").expect("binary");
    command
        .env("XDG_CONFIG_HOME", &state_file)
        .env("XDG_CACHE_HOME", &state_file)
        .env("XDG_STATE_HOME", &state_file)
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("setup init completions refresh query explain bench root roots palette repo recent back forward import bookmarks bookmark doctor stats config support suggestions workflows update-notifications")
                .and(predicate::str::contains("_dgo_bookmarks"))
                .and(predicate::str::contains("clear-learned")),
        );
}

#[test]
fn every_generated_completion_exposes_workflow_management() {
    let fixture = Fixture::new();
    for shell in ["zsh", "bash", "fish", "powershell"] {
        fixture
            .command()
            .args(["completions", shell])
            .assert()
            .success()
            .stdout(
                predicate::str::contains("workflows")
                    .and(predicate::str::contains("clear-learned"))
                    .and(predicate::str::contains("export")),
            );
    }
}

#[test]
fn setup_is_previewable_idempotent_reversible_and_receipted() {
    let fixture = Fixture::new();
    let rc = fixture.temp.path().join("shell/zshrc");
    fs::create_dir_all(rc.parent().expect("rc parent")).expect("rc parent");
    fs::write(&rc, "export EDITOR=vim\n").expect("initial rc");
    fs::set_permissions(&rc, fs::Permissions::from_mode(0o640)).expect("rc permissions");

    fixture
        .command()
        .args([
            "setup",
            "--shell",
            "zsh",
            "--rc",
            rc.to_str().expect("utf8 rc"),
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("DIRGO")
                .and(predicate::str::contains("Add or repair the managed block"))
                .and(predicate::str::contains("eval \"$(command dgo init zsh)\"")),
        );
    assert_eq!(
        fs::read_to_string(&rc).expect("dry-run rc"),
        "export EDITOR=vim\n"
    );

    for _ in 0..2 {
        fixture
            .command()
            .args([
                "setup",
                "--shell",
                "zsh",
                "--rc",
                rc.to_str().expect("utf8 rc"),
                "--yes",
            ])
            .assert()
            .success();
    }
    let configured = fs::read_to_string(&rc).expect("configured rc");
    assert!(configured.starts_with("export EDITOR=vim\n\n"));
    assert_eq!(configured.matches("# >>> dirgo setup >>>").count(), 1);
    assert_eq!(
        fs::metadata(&rc).expect("rc metadata").permissions().mode() & 0o777,
        0o640
    );
    assert!(
        fs::read_dir(rc.parent().expect("rc parent"))
            .expect("rc directory")
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().contains("dirgo-backup"))
    );
    assert!(
        fixture
            .temp
            .path()
            .join("state/dirgo/setup-zsh.json")
            .is_file()
    );

    fixture
        .command()
        .args([
            "setup",
            "--shell",
            "zsh",
            "--rc",
            rc.to_str().expect("utf8 rc"),
            "--remove",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Shell disconnected"));
    assert!(
        !fs::read_to_string(&rc)
            .expect("removed rc")
            .contains("dirgo setup")
    );
    assert!(
        !fixture
            .temp
            .path()
            .join("state/dirgo/setup-zsh.json")
            .exists()
    );
}

#[test]
fn setup_refuses_noninteractive_mutation_without_explicit_consent() {
    let fixture = Fixture::new();
    let rc = fixture.temp.path().join("zshrc");
    fixture
        .command()
        .args([
            "setup",
            "--shell",
            "zsh",
            "--rc",
            rc.to_str().expect("utf8 rc"),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("setup needs confirmation"));
    assert!(!rc.exists());
}

#[test]
fn local_query_crawls_directories_created_after_global_refresh() {
    let fixture = Fixture::new();
    fixture.command().arg("refresh").assert().success();
    let local_root = fixture.temp.path().join("filesystem/Projects/Punk");
    let late = local_root.join("LateLocal");
    fs::create_dir(&late).expect("late local directory");

    fixture
        .command()
        .current_dir(&local_root)
        .args(["query", ".", "LateLocal"])
        .assert()
        .success()
        .stdout(predicate::str::ends_with("/Projects/Punk/LateLocal\n"));
}

#[test]
fn ambiguous_query_is_nonzero_and_lists_candidates_on_stderr() {
    let fixture = Fixture::new();
    fixture.command().arg("refresh").assert().success();
    fixture
        .command()
        .args(["query", "api"])
        .assert()
        .code(4)
        .stderr(
            predicate::str::contains("/Archive/api").and(predicate::str::contains("/Services/api")),
        );
}

#[test]
fn ambiguous_diagnostics_escape_terminal_control_characters_in_paths() {
    let fixture = Fixture::new();
    let root = fixture.temp.path().join("filesystem");
    fs::create_dir(root.join("danger\u{1b}[31m")).expect("control-name directory");
    fs::create_dir(root.join("danger-other")).expect("second directory");
    fixture.command().arg("refresh").assert().success();

    fixture.command().arg("danger").assert().code(4).stderr(
        predicate::str::contains("danger\\x1b[31m").and(predicate::str::contains("\u{1b}").not()),
    );
}

#[test]
fn ambiguous_json_exposes_explainable_score_components() {
    let fixture = Fixture::new();
    fixture.command().arg("refresh").assert().success();
    let assert = fixture
        .command()
        .args(["query", "api", "--json"])
        .assert()
        .code(4);
    let response: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("valid JSON response");
    let candidates = response["candidates"].as_array().expect("candidate array");

    assert_eq!(candidates.len(), 2);
    for candidate in candidates {
        let breakdown = &candidate["score_breakdown"];
        assert_eq!(candidate["score"], breakdown["total"]);
        assert!(breakdown["exact"].as_f64().expect("exact component") > 0.0);
        assert!(breakdown["proximity"].is_number());
        assert!(breakdown["recency"].is_number());
    }
}

#[test]
fn explain_forces_candidates_and_never_navigates() {
    let fixture = Fixture::new();
    fixture.command().arg("refresh").assert().success();
    let assert = fixture
        .command()
        .args(["explain", "punk"])
        .assert()
        .success();
    let response: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("valid explain JSON");
    assert_eq!(response["resolved"], false);
    assert_eq!(response["candidates"].as_array().map(Vec::len), Some(1));
    assert_eq!(response["candidates"][0]["basename"], "Punk");
    assert!(
        response["candidates"][0]["score_breakdown"]["exact"]
            .as_f64()
            .is_some_and(|score| score > 0.0)
    );
}

#[test]
fn doctor_reports_operational_checks_without_building_an_index() {
    let fixture = Fixture::new();
    fixture
        .command()
        .env("SHELL", "/bin/zsh")
        .arg("doctor")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("config         valid")
                .and(predicate::str::contains("storage        cache="))
                .and(predicate::str::contains("index          missing"))
                .and(predicate::str::contains("state          healthy"))
                .and(predicate::str::contains("roots          1 configured"))
                .and(predicate::str::contains("update         "))
                .and(predicate::str::contains(
                    "palette        files/tasks/git/compose/places",
                ))
                .and(predicate::str::contains("actions        open="))
                .and(predicate::str::contains("shell startup")),
        );
    assert!(!fixture.temp.path().join("cache/dirgo/index.redb").exists());
}

#[test]
fn doctor_and_config_path_work_when_configuration_is_broken() {
    let fixture = Fixture::new();
    let config = fixture.temp.path().join("config/dirgo/config.toml");
    fs::write(&config, "schema_version = nope\n").expect("broken config");

    fixture
        .command()
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains(config.display().to_string()));
    fixture.command().arg("doctor").assert().code(1).stdout(
        predicate::str::contains("config         invalid")
            .and(predicate::str::contains("Repair or move that file"))
            .and(predicate::str::contains("Doctor completed")),
    );
}

#[test]
fn support_does_not_require_valid_configuration_or_storage() {
    let temp = tempfile::tempdir().expect("tempdir");
    let blocked = temp.path().join("blocked");
    fs::write(&blocked, "not a directory").expect("blocked path");
    Command::cargo_bin("dgo")
        .expect("binary")
        .env("XDG_CONFIG_HOME", &blocked)
        .env("XDG_CACHE_HOME", &blocked)
        .env("XDG_STATE_HOME", &blocked)
        .arg("support")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "https://github.com/RudySource/Dirgo/issues",
        ));
}

#[test]
fn local_bench_reports_measurements_without_recording_navigation() {
    let fixture = Fixture::new();
    fixture.command().arg("refresh").assert().success();
    fixture
        .command()
        .args(["bench", "--query", "punk", "--samples", "3"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Dataset directories")
                .and(predicate::str::contains("Samples              3"))
                .and(predicate::str::contains("Fallback candidate build"))
                .and(predicate::str::contains("Fuzzy resolution")),
        );
    fixture.command().arg("stats").assert().success().stdout(
        predicate::str::contains("Dirgo navigations     0")
            .and(predicate::str::contains("Search roots          1"))
            .and(predicate::str::contains("Accessible roots      1")),
    );
}

#[test]
fn repeated_visits_enable_only_a_high_margin_ranked_prefix() {
    let fixture = Fixture::new();
    let root = fixture.temp.path().join("filesystem");
    fs::create_dir(root.join("frontend")).expect("frontend");
    fs::create_dir(root.join("frontier")).expect("frontier");
    fixture.command().arg("refresh").assert().success();

    for _ in 0..8 {
        fixture
            .command()
            .args(["__resolve", "--cwd"])
            .arg(&root)
            .args(["--", "frontend"])
            .assert()
            .success();
    }

    let assert = fixture
        .command()
        .args(["query", "fro", "--json"])
        .assert()
        .success();
    let response: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("valid JSON response");
    assert_eq!(response["resolved"], true);
    assert_eq!(response["source"], "ranked_prefix");
    assert!(
        response["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("/frontend"))
    );
    assert!(
        response["confidence"]
            .as_f64()
            .is_some_and(|confidence| confidence >= 0.86)
    );
}

#[test]
fn bookmark_is_persistent_without_an_existing_index() {
    let fixture = Fixture::new();
    let target = fixture.temp.path().join("filesystem/Projects/Punk");
    fixture
        .command()
        .args(["bookmark", "add", "work", "--path"])
        .arg(&target)
        .assert()
        .success();
    fixture
        .command()
        .args(["bookmarks"])
        .assert()
        .success()
        .stdout(predicate::str::contains("@work").and(predicate::str::contains("Projects/Punk")));
    fixture
        .command()
        .args(["query", "@work"])
        .assert()
        .success()
        .stdout(predicate::str::ends_with("/Projects/Punk\n"));
    assert!(!fixture.temp.path().join("cache/dirgo/index.redb").exists());
}

#[test]
fn stale_bookmark_reports_and_supports_in_place_repair() {
    let fixture = Fixture::new();
    let stale = fixture.temp.path().join("filesystem/stale-bookmark");
    let repaired = fixture.temp.path().join("filesystem/repaired-bookmark");
    fs::create_dir(&stale).expect("stale directory");
    fs::create_dir(&repaired).expect("repair directory");
    fixture
        .command()
        .args(["bookmark", "add", "work", "--path"])
        .arg(&stale)
        .assert()
        .success();
    fs::remove_dir(&stale).expect("remove stale directory");

    fixture
        .command()
        .args(["query", "@work"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("missing directory")
                .and(predicate::str::contains("dgo bookmark add work --path"))
                .and(predicate::str::contains("dgo bookmark remove work")),
        );

    fixture
        .command()
        .args(["bookmark", "add", "work", "--path"])
        .arg(&repaired)
        .assert()
        .success();
    fixture
        .command()
        .args(["query", "@work"])
        .assert()
        .success()
        .stdout(predicate::str::ends_with("/repaired-bookmark\n"));
}

#[test]
fn zoxide_import_is_explicit_safe_and_idempotent() {
    let fixture = Fixture::new();
    let imported = fixture.temp.path().join("filesystem/imported space");
    let stale = fixture.temp.path().join("filesystem/missing-zoxide-entry");
    let bin = fixture.temp.path().join("bin");
    let zoxide = bin.join("zoxide");
    let captured_args = fixture.temp.path().join("zoxide-args");
    fs::create_dir(&imported).expect("imported directory");
    fs::create_dir(&bin).expect("mock bin");
    fs::write(
        &zoxide,
        format!(
            "#!/bin/sh\nprintf '%s' \"$*\" > \"$DGO_ZOXIDE_ARGS\"\nprintf '7.2 %s\\n3.0 %s\\n' {} {}\n",
            shell_escape::unix::escape(imported.to_string_lossy()),
            shell_escape::unix::escape(stale.to_string_lossy()),
        ),
    )
    .expect("mock zoxide");
    fs::set_permissions(&zoxide, fs::Permissions::from_mode(0o755)).expect("mock permissions");
    let path = format!("{}:{}", bin.display(), env::var("PATH").unwrap_or_default());

    fixture
        .command()
        .env("PATH", &path)
        .env("DGO_ZOXIDE_ARGS", &captured_args)
        .args(["import", "zoxide"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Imported 1 zoxide entries (0 unchanged, 1 stale skipped).",
        ));
    assert_eq!(
        fs::read_to_string(&captured_args).expect("captured arguments"),
        "query --list --score"
    );
    fixture
        .command()
        .env("PATH", &path)
        .env("DGO_ZOXIDE_ARGS", &captured_args)
        .args(["import", "zoxide"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Imported 0 zoxide entries (1 unchanged, 1 stale skipped).",
        ));
    fixture
        .command()
        .arg("stats")
        .assert()
        .success()
        .stdout(predicate::str::contains("Dirgo navigations     8"));
}

#[test]
fn corrupt_index_is_quarantined_and_rebuilt_without_touching_state() {
    let fixture = Fixture::new();
    fixture.command().arg("refresh").assert().success();
    let index = fixture.temp.path().join("cache/dirgo/index.redb");
    fs::write(&index, "not a redb file").expect("corrupt index fixture");

    fixture
        .command()
        .args(["query", "punk"])
        .assert()
        .success()
        .stdout(predicate::str::ends_with("/Projects/Punk\n"))
        .stderr(predicate::str::contains("quarantined a corrupt index"));
    assert!(fixture.temp.path().join("cache/dirgo/index.redb").is_file());
    assert!(
        fs::read_dir(fixture.temp.path().join("cache/dirgo"))
            .expect("cache entries")
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .contains("index.redb.corrupt."))
    );
}

#[test]
fn corrupt_state_is_backed_up_and_recreated_empty() {
    let fixture = Fixture::new();
    let target = fixture.temp.path().join("filesystem/Projects/Punk");
    fixture
        .command()
        .args(["bookmark", "add", "work", "--path"])
        .arg(&target)
        .assert()
        .success();
    let state = fixture.temp.path().join("state/dirgo/state.redb");
    fs::write(&state, "not a redb file").expect("corrupt state fixture");

    fixture
        .command()
        .arg("bookmarks")
        .assert()
        .success()
        .stdout(predicate::str::contains("No bookmarks yet"))
        .stderr(predicate::str::contains("backed up corrupt state"));
    assert!(fixture.temp.path().join("state/dirgo/state.redb").is_file());
    assert!(
        fs::read_dir(fixture.temp.path().join("state/dirgo"))
            .expect("state entries")
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .contains("state.redb.corrupt."))
    );
}

#[test]
fn configured_editor_receives_a_literal_path_as_one_argument() {
    let fixture = Fixture::new();
    let editor = fixture.temp.path().join("capture-editor");
    let output = fixture.temp.path().join("editor-argument");
    fs::write(
        &editor,
        "#!/bin/sh\nprintf '%s' \"$1\" > \"$DGO_ACTION_OUTPUT\"\n",
    )
    .expect("editor fixture");
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755)).expect("editor permissions");
    fs::write(
        fixture.temp.path().join("config/dirgo/config.toml"),
        format!(
            "schema_version = 1\nroots = [{}]\n[actions]\neditor = {}\n",
            toml_string(&fixture.temp.path().join("filesystem")),
            toml_string(&editor),
        ),
    )
    .expect("action config");

    fixture
        .command()
        .env("DGO_ACTION_OUTPUT", &output)
        .args(["--code", "quo'te space"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    assert_eq!(
        fs::read_to_string(output).expect("captured editor argument"),
        fixture
            .temp
            .path()
            .join("filesystem/Projects/quo'te space")
            .canonicalize()
            .expect("canonical action path")
            .display()
            .to_string()
    );
}

#[test]
fn action_flags_work_after_the_query() {
    let fixture = Fixture::new();
    let editor = fixture.temp.path().join("capture-editor-after-query");
    let output = fixture.temp.path().join("editor-after-query-argument");
    fs::write(
        &editor,
        "#!/bin/sh\nprintf '%s' \"$1\" > \"$DGO_ACTION_OUTPUT\"\n",
    )
    .expect("editor fixture");
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755)).expect("editor permissions");
    fs::write(
        fixture.temp.path().join("config/dirgo/config.toml"),
        format!(
            "schema_version = 1\nroots = [{}]\n[actions]\neditor = {}\n",
            toml_string(&fixture.temp.path().join("filesystem")),
            toml_string(&editor),
        ),
    )
    .expect("action config");

    fixture
        .command()
        .env("DGO_ACTION_OUTPUT", &output)
        .args(["quo'te space", "--code"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    assert_eq!(
        fs::read_to_string(output).expect("captured editor argument"),
        fixture
            .temp
            .path()
            .join("filesystem/Projects/quo'te space")
            .canonicalize()
            .expect("canonical action path")
            .display()
            .to_string()
    );
}

#[test]
fn open_without_a_query_targets_the_current_directory_and_accepts_an_absolute_path() {
    let fixture = Fixture::new();
    let bin = fixture.temp.path().join("action-bin");
    fs::create_dir(&bin).expect("action bin");
    #[cfg(target_os = "macos")]
    let opener_name = "open";
    #[cfg(not(target_os = "macos"))]
    let opener_name = "xdg-open";
    let opener = bin.join(opener_name);
    let output = fixture.temp.path().join("opened-directory");
    fs::write(
        &opener,
        "#!/bin/sh\nprintf '%s' \"$1\" > \"$DGO_ACTION_OUTPUT\"\n",
    )
    .expect("opener fixture");
    fs::set_permissions(&opener, fs::Permissions::from_mode(0o755)).expect("opener permissions");
    let current = fixture.temp.path().join("filesystem/Projects/Punk");

    fixture
        .command()
        .current_dir(&current)
        .env("PATH", &bin)
        .env("DGO_ACTION_OUTPUT", &output)
        .arg("--open")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    assert_eq!(
        fs::read_to_string(&output).expect("current directory capture"),
        current
            .canonicalize()
            .expect("canonical current")
            .display()
            .to_string()
    );

    let absolute = fixture.temp.path().join("filesystem/Projects/quo'te space");
    fixture
        .command()
        .env("PATH", &bin)
        .env("DGO_ACTION_OUTPUT", &output)
        .arg("--open")
        .arg(&absolute)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    assert_eq!(
        fs::read_to_string(output).expect("absolute directory capture"),
        absolute
            .canonicalize()
            .expect("canonical absolute")
            .display()
            .to_string()
    );
}

#[test]
fn shell_resolver_recovers_trailing_action_flags() {
    let fixture = Fixture::new();
    let target = fixture.temp.path().join("filesystem/Projects/Punk");
    fixture
        .command()
        .args(["__resolve", "--cwd"])
        .arg(fixture.temp.path().join("filesystem"))
        .args(["--", "Punk", "--print"])
        .assert()
        .success()
        .stdout(predicate::str::ends_with(format!("{}\n", target.display())));
}

#[test]
fn shell_init_contains_fast_path_and_no_path_eval() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["init", "zsh"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("builtin cd")
                .and(predicate::str::contains("command dgo __resolve"))
                .and(predicate::str::contains("bookmark|import|doctor"))
                .and(predicate::str::contains("setup|init|completions"))
                .and(predicate::str::contains(
                    "--open|--finder|--code|--copy|--print",
                ))
                .and(predicate::str::contains("resolve_status == 10"))
                .and(predicate::str::contains("eval $destination").not()),
        );
}

#[test]
fn installed_zsh_wrapper_changes_directory_and_keeps_fast_path_working() {
    if ProcessCommand::new("zsh")
        .arg("--version")
        .stdout(Stdio::null())
        .status()
        .is_err()
    {
        return;
    }
    let fixture = Fixture::new();
    fixture.command().arg("refresh").assert().success();
    let binary = assert_cmd::cargo::cargo_bin!("dgo");
    let binary_dir = binary.parent().expect("binary dir");
    let path = format!(
        "{}:{}",
        binary_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = ProcessCommand::new("zsh")
        .args([
            "-f",
            "-c",
            "eval \"$(command dgo init zsh)\"; builtin cd \"$DGO_TEST_ROOT\"; dgo Punk; dgo back; print -r -- \"$PWD\"; dgo forward; print -r -- \"$PWD\"; dgo ..; print -r -- \"$PWD\"",
        ])
        .env("PATH", path)
        .env("DGO_TEST_ROOT", fixture.temp.path().join("filesystem"))
        .env("XDG_CONFIG_HOME", fixture.temp.path().join("config"))
        .env("XDG_CACHE_HOME", fixture.temp.path().join("cache"))
        .env("XDG_STATE_HOME", fixture.temp.path().join("state"))
        .output()
        .expect("run zsh");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root = fixture
        .temp
        .path()
        .join("filesystem")
        .canonicalize()
        .expect("canonical root");
    let punk = root.join("Projects/Punk");
    let projects = root.join("Projects");
    let actual = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(Path::new)
        .map(fs::canonicalize)
        .collect::<std::io::Result<Vec<_>>>()
        .expect("canonical output paths");
    assert_eq!(actual, vec![root, punk, projects]);
}

#[test]
fn installed_bash_wrapper_routes_management_and_handles_safe_paths() {
    if ProcessCommand::new("bash")
        .arg("--version")
        .stdout(Stdio::null())
        .status()
        .is_err()
    {
        return;
    }
    let fixture = Fixture::new();
    fixture.command().arg("refresh").assert().success();
    let binary = assert_cmd::cargo::cargo_bin!("dgo");
    let binary_dir = binary.parent().expect("binary dir");
    let path = format!(
        "{}:{}",
        binary_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let script = r#"
eval "$(command dgo init bash)"
builtin cd "$DGO_TEST_ROOT"
dgo bookmark add work --path "$DGO_TEST_ROOT/Projects/Punk" >/dev/null
dgo @work
printf '%s\n' "$PWD"
builtin cd "$DGO_TEST_ROOT"
dgo "Projects/quo'te space"
printf '%s\n' "$PWD"
builtin cd "$DGO_TEST_ROOT"
dgo ./-dash
printf '%s\n' "$PWD"
"#;
    let output = ProcessCommand::new("bash")
        .args(["--noprofile", "--norc", "-c", script])
        .env("PATH", path)
        .env("DGO_TEST_ROOT", fixture.temp.path().join("filesystem"))
        .env("XDG_CONFIG_HOME", fixture.temp.path().join("config"))
        .env("XDG_CACHE_HOME", fixture.temp.path().join("cache"))
        .env("XDG_STATE_HOME", fixture.temp.path().join("state"))
        .output()
        .expect("run bash");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root = fixture
        .temp
        .path()
        .join("filesystem")
        .canonicalize()
        .expect("canonical root");
    let expected = [
        root.join("Projects/Punk"),
        root.join("Projects/quo'te space"),
        root.join("-dash"),
    ];
    let stdout = String::from_utf8_lossy(&output.stdout);
    let actual: Vec<_> = stdout
        .lines()
        .map(|line| {
            Path::new(line)
                .canonicalize()
                .expect("canonical shell path")
        })
        .collect();
    assert_eq!(actual, expected);
}
