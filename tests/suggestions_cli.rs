use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::Stdio,
};

use assert_cmd::Command;
use dirgo::suggestions::{
    PROTOCOL_VERSION, ShellKind, SuggestionRequest, SuggestionResponse, SuggestionSource,
};
use predicates::prelude::*;

struct Fixture {
    temp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("filesystem");
        fs::create_dir_all(root.join("Projects/Punk")).expect("fixture tree");
        fs::create_dir_all(root.join("Projects/Slash")).expect("fixture tree");
        let config_dir = temp.path().join("config/dirgo");
        fs::create_dir_all(&config_dir).expect("config directory");
        fs::write(
            config_dir.join("config.toml"),
            format!(
                "# keep this comment\nschema_version = 1\nroots = [{}]\n",
                toml_string(&root)
            ),
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

fn toml_string(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

fn assert_path_suffix(value: &str, suffix: &str) {
    let normalized = value.replace('\\', "/");
    let normalized = normalized.trim().trim_matches(['\'', '"']);
    assert!(
        normalized.ends_with(suffix),
        "expected {normalized:?} to end with {suffix:?}"
    );
}

#[test]
fn suggestions_are_explicitly_enabled_disabled_and_preserve_existing_config() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "status"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Suggestions      disabled")
                .and(predicate::str::contains("Command history  disabled")),
        );

    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success()
        .stdout(predicate::str::contains("enabled"));
    let config = fs::read_to_string(fixture.temp.path().join("config/dirgo/config.toml"))
        .expect("updated config");
    assert!(config.contains("# keep this comment"));
    assert!(config.contains("[suggestions]"));
    assert!(config.contains("enabled = true"));

    fixture
        .command()
        .args(["suggestions", "history", "enable"])
        .assert()
        .success();
    fixture
        .command()
        .args(["suggestions", "disable"])
        .assert()
        .success();
    fixture
        .command()
        .args(["suggestions", "status"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Suggestions      disabled")
                .and(predicate::str::contains("Command history  enabled")),
        );
}

#[test]
fn hidden_suggestion_command_reads_the_buffer_from_stdin_and_emits_one_frame() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    fixture.command().arg("refresh").assert().success();
    let cwd = fixture.temp.path().join("filesystem");
    let request = SuggestionRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: 77,
        shell: ShellKind::Zsh,
        cwd,
        before_cursor: "cd pun".into(),
        after_cursor: String::new(),
        max_results: 8,
        terminal_rows: Some(24),
        terminal_columns: Some(120),
        presentation: dirgo::suggestions::SuggestionPresentation::List,
    };
    let input = format!(
        "{}\n",
        serde_json::to_string(&request).expect("request json")
    );
    let assert = fixture
        .command()
        .arg("__suggest")
        .write_stdin(input)
        .assert()
        .success()
        .stderr(predicate::str::is_empty());

    let output = std::str::from_utf8(&assert.get_output().stdout).expect("utf8 response");
    assert_eq!(output.matches('\n').count(), 1);
    let response: SuggestionResponse = serde_json::from_str(output).expect("response json");
    assert_eq!(response.request_id, 77);
    assert_eq!(response.suggestions.len(), 1);
    assert_path_suffix(&response.suggestions[0].edit.replacement, "/Projects/Punk");
}

#[test]
fn suggestion_worker_loads_custom_command_specs_beside_config() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    let completions = fixture.temp.path().join("config/dirgo/completions");
    fs::create_dir_all(&completions).expect("completions directory");
    fs::write(
        completions.join("acme.toml"),
        r#"
name = "acme"

[[subcommands]]
name = "deploy"
description = "Deploy the current service"
"#,
    )
    .expect("custom command spec");
    let request = SuggestionRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: 81,
        shell: ShellKind::PowerShell,
        cwd: fixture.temp.path().join("filesystem"),
        before_cursor: "acme de".into(),
        after_cursor: String::new(),
        max_results: 8,
        terminal_rows: Some(24),
        terminal_columns: Some(120),
        presentation: dirgo::suggestions::SuggestionPresentation::List,
    };

    let output = fixture
        .command()
        .arg("__suggest")
        .write_stdin(format!(
            "{}\n",
            serde_json::to_string(&request).expect("request json")
        ))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let response: SuggestionResponse = serde_json::from_slice(&output).expect("response");
    assert_eq!(response.suggestions[0].edit.replacement, "acme deploy");
    assert_eq!(
        response.suggestions[0].description.as_deref(),
        Some("Deploy the current service")
    );
}

#[test]
fn zsh_completion_stream_returns_nul_delimited_tokens_without_executing_them() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    fixture.command().arg("refresh").assert().success();
    let cwd = fixture.temp.path().join("filesystem");
    let output = fixture
        .command()
        .args([
            "__suggest-complete",
            "--shell",
            "zsh",
            "--cwd",
            cwd.to_str().expect("utf8 cwd"),
            "--terminal-rows",
            "24",
            "--terminal-columns",
            "100",
        ])
        .write_stdin(b"dgo sl\0\0".as_slice())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let fields = output
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .map(|field| std::str::from_utf8(field).expect("utf8 completion field"))
        .collect::<Vec<_>>();
    assert_eq!(fields.len(), 4);
    assert_path_suffix(fields[0], "/Projects/Slash");
    assert_eq!(fields[1], "Slash");
    assert_eq!(fields[2], "DIR");
    assert_path_suffix(fields[3], "/Projects/Slash");
}

#[test]
fn zsh_catalog_page_prefixes_the_stream_with_exact_total() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    fixture.command().arg("refresh").assert().success();
    let cwd = fixture.temp.path().join("filesystem");
    let output = fixture
        .command()
        .args([
            "__suggest-complete",
            "--shell",
            "zsh",
            "--cwd",
            cwd.to_str().expect("utf8 cwd"),
            "--terminal-rows",
            "24",
            "--terminal-columns",
            "100",
            "--page-offset",
            "0",
            "--page-size",
            "96",
            "--include-total",
            "--frame-generation",
            "42",
        ])
        .write_stdin(b"dgo sl\0\0".as_slice())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let fields = output
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .map(|field| std::str::from_utf8(field).expect("utf8 completion field"))
        .collect::<Vec<_>>();

    assert_eq!(fields[0], "42");
    assert_eq!(fields[1], "1");
    assert_eq!(fields.len(), 6);
    assert_path_suffix(fields[2], "/Projects/Slash");
    assert_eq!(fields[3], "Slash");
    assert_eq!(fields[4], "DIR");
    assert_path_suffix(fields[5], "/Projects/Slash");
}

#[test]
fn zsh_catalog_page_can_include_command_descriptions_for_preview() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    let cwd = fixture.temp.path().join("filesystem");
    let output = fixture
        .command()
        .args([
            "__suggest-complete",
            "--shell",
            "zsh",
            "--cwd",
            cwd.to_str().expect("utf8 cwd"),
            "--terminal-rows",
            "24",
            "--terminal-columns",
            "100",
            "--page-offset",
            "0",
            "--page-size",
            "96",
            "--include-total",
            "--include-descriptions",
            "--frame-generation",
            "43",
        ])
        .write_stdin(b"git co\0\0".as_slice())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let mut fields = output
        .split(|byte| *byte == 0)
        .map(|field| std::str::from_utf8(field).expect("utf8 completion field"))
        .collect::<Vec<_>>();
    assert_eq!(fields.pop(), Some(""), "NUL frame must have a terminator");

    assert_eq!(fields[0], "43");
    let commit = fields[2..]
        .chunks_exact(5)
        .find(|candidate| candidate[1].trim_end() == "commit")
        .expect("git commit candidate");
    assert_eq!(commit[2], "SUB");
    assert_eq!(commit[3], "Record changes to the repository");
    assert_eq!(commit[4], "git commit");
}

#[test]
fn description_frame_preserves_an_empty_field_without_shifting_the_tuple() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    let cwd = fixture.temp.path().join("filesystem");
    let output = fixture
        .command()
        .args([
            "__suggest-complete",
            "--shell",
            "zsh",
            "--cwd",
            cwd.to_str().expect("utf8 cwd"),
            "--terminal-rows",
            "24",
            "--terminal-columns",
            "100",
            "--page-size",
            "96",
            "--include-total",
            "--include-descriptions",
        ])
        .write_stdin(b"cargo ad\0\0".as_slice())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let mut fields = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    assert_eq!(
        fields.pop(),
        Some(&[][..]),
        "NUL frame must have a terminator"
    );
    let add = fields[1..]
        .chunks_exact(5)
        .find(|candidate| {
            std::str::from_utf8(candidate[1])
                .expect("utf8 display")
                .trim_end()
                == "add"
        })
        .expect("cargo add candidate");

    assert_eq!(add[2], b"SUB");
    assert_eq!(add[3], b"");
    assert_eq!(add[4], b"cargo add");
}

#[test]
fn later_catalog_page_keeps_non_empty_descriptions_aligned() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    let completions = fixture.temp.path().join("config/dirgo/completions");
    fs::create_dir_all(&completions).expect("completions directory");
    let children = (0..110)
        .map(|index| {
            format!(
                "[[subcommands]]\nname = \"item-{index:03}\"\ndescription = \"Describe item-{index:03}\"\n"
            )
        })
        .collect::<String>();
    fs::write(
        completions.join("bulk.toml"),
        format!("name = \"bulk\"\n{children}"),
    )
    .expect("bulk command spec");
    let cwd = fixture.temp.path().join("filesystem");
    let output = fixture
        .command()
        .args([
            "__suggest-complete",
            "--shell",
            "zsh",
            "--cwd",
            cwd.to_str().expect("utf8 cwd"),
            "--terminal-rows",
            "24",
            "--terminal-columns",
            "100",
            "--page-offset",
            "96",
            "--page-size",
            "14",
            "--include-total",
            "--include-descriptions",
        ])
        .write_stdin(b"bulk item-\0\0".as_slice())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let mut fields = output
        .split(|byte| *byte == 0)
        .map(|field| std::str::from_utf8(field).expect("utf8 completion field"))
        .collect::<Vec<_>>();
    assert_eq!(fields.pop(), Some(""), "NUL frame must have a terminator");
    assert_eq!(fields[0], "110");
    let page = &fields[1..];
    assert_eq!(page.len(), 14 * 5);
    assert_eq!(page[0], "item-096");
    assert_eq!(page[1].trim_end(), "item-096");
    assert_eq!(page[2], "SUB");
    assert_eq!(page[3], "Describe item-096");
    assert_eq!(page[4], "bulk item-096");
}

#[test]
fn fish_completion_stream_is_line_delimited_and_labeled_for_native_pager() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    fixture.command().arg("refresh").assert().success();
    let cwd = fixture.temp.path().join("filesystem");
    let assert = fixture
        .command()
        .args([
            "__suggest-complete",
            "--shell",
            "fish",
            "--cwd",
            cwd.to_str().expect("utf8 cwd"),
            "--format",
            "lines",
        ])
        .write_stdin(b"dgo sl\0\0".as_slice())
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    let output = std::str::from_utf8(&assert.get_output().stdout).expect("utf8 completion stream");
    let normalized = output.replace('\\', "/");
    assert!(normalized.contains("Projects/Slash"));
    assert!(normalized.contains("\tDIR  Slash\n"));
}

#[test]
fn hidden_shell_settings_reflect_the_validated_suggestion_config() {
    let fixture = Fixture::new();
    let config_path = fixture.temp.path().join("config/dirgo/config.toml");
    let mut config = fs::read_to_string(&config_path).expect("config");
    config.push_str(
        "[suggestions]\nenabled = true\nnative_completions = false\ndebounce_ms = 47\nnative_timeout_ms = 91\n",
    );
    fs::write(config_path, config).expect("custom suggestion config");

    fixture
        .command()
        .arg("__suggest-native-enabled")
        .assert()
        .code(1);
    fixture
        .command()
        .arg("__suggest-debounce")
        .assert()
        .success()
        .stdout("0.047\n");
    fixture
        .command()
        .arg("__suggest-native-timeout")
        .assert()
        .success()
        .stdout("91\n");
}

#[test]
fn worker_serves_multiple_frames_and_recording_requires_history_opt_in() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    fixture.command().arg("refresh").assert().success();
    let cwd = fixture.temp.path().join("filesystem");
    let frame = |request_id, before_cursor: &str| {
        serde_json::to_string(&SuggestionRequest {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            shell: ShellKind::Zsh,
            cwd: cwd.clone(),
            before_cursor: before_cursor.into(),
            after_cursor: String::new(),
            max_results: 8,
            terminal_rows: Some(24),
            terminal_columns: Some(120),
            presentation: dirgo::suggestions::SuggestionPresentation::List,
        })
        .expect("request json")
    };
    let input = format!(
        "{}\n{}\n{}\n",
        frame(1, "cd pun"),
        frame(2, "dgo pun"),
        frame(1, "dgo stale")
    );
    let assert = fixture
        .command()
        .arg("__suggest-worker")
        .write_stdin(input)
        .assert()
        .success();
    let lines: Vec<_> = std::str::from_utf8(&assert.get_output().stdout)
        .expect("utf8 responses")
        .lines()
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(
        serde_json::from_str::<SuggestionResponse>(lines[0])
            .expect("first response")
            .request_id,
        1
    );
    assert_eq!(
        serde_json::from_str::<SuggestionResponse>(lines[1])
            .expect("second response")
            .request_id,
        2
    );
    let stale = serde_json::from_str::<SuggestionResponse>(lines[2]).expect("stale response");
    assert_eq!(stale.request_id, 1);
    assert!(stale.suggestions.is_empty());
    assert_eq!(stale.error.as_deref(), Some("stale request id"));

    fixture
        .command()
        .arg("__suggest-record")
        .write_stdin("cargo test\n")
        .assert()
        .success();
    assert!(
        !fixture
            .temp
            .path()
            .join("state/dirgo/suggestions.redb")
            .exists()
    );

    fixture
        .command()
        .args(["suggestions", "history", "enable"])
        .assert()
        .success();
    fixture
        .command()
        .arg("__suggest-record")
        .write_stdin("cargo test\n")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    assert!(
        fixture
            .temp
            .path()
            .join("state/dirgo/suggestions.redb")
            .is_file()
    );

    let output = fixture
        .command()
        .arg("__suggest")
        .write_stdin(format!("{}\n", frame(3, "cargo t")))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let response: SuggestionResponse = serde_json::from_slice(&output).expect("history response");
    assert_eq!(response.suggestions[0].edit.replacement, "cargo test");
}

#[test]
fn disabling_suggestions_stops_an_already_loaded_history_hook_from_recording() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    fixture
        .command()
        .args(["suggestions", "history", "enable"])
        .assert()
        .success();
    fixture
        .command()
        .arg("__suggest-record")
        .write_stdin("cargo test\n")
        .assert()
        .success();

    fixture
        .command()
        .args(["suggestions", "disable"])
        .assert()
        .success();
    fixture
        .command()
        .arg("__suggest-record")
        .write_stdin("cargo publish\n")
        .assert()
        .success();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();

    let request = SuggestionRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: 55,
        shell: ShellKind::Zsh,
        cwd: fixture.temp.path().join("filesystem"),
        before_cursor: "cargo p".into(),
        after_cursor: String::new(),
        max_results: 8,
        terminal_rows: Some(24),
        terminal_columns: Some(120),
        presentation: dirgo::suggestions::SuggestionPresentation::List,
    };
    let output = fixture
        .command()
        .arg("__suggest")
        .write_stdin(format!(
            "{}\n",
            serde_json::to_string(&request).expect("request json")
        ))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let response: SuggestionResponse = serde_json::from_slice(&output).expect("response json");
    assert!(
        response
            .suggestions
            .iter()
            .all(|suggestion| suggestion.edit.replacement != "cargo publish"
                || suggestion.source != SuggestionSource::CommandHistory)
    );
}

#[test]
fn worker_readiness_handshake_precedes_protocol_frames() {
    Fixture::new()
        .command()
        .args(["__suggest-worker", "--ready"])
        .write_stdin("")
        .assert()
        .success()
        .stdout("READY 2\n");
}

#[test]
fn warmed_project_cache_is_served_by_protocol_and_shell_completion_paths() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    let project = fixture.temp.path().join("filesystem/Projects/App");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        project.join("package.json"),
        r#"{"name":"app","scripts":{"build":"vite build"}}"#,
    )
    .expect("package.json");
    fixture
        .command()
        .arg("__suggest-project-refresh")
        .arg("--cwd")
        .arg(&project)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let request = SuggestionRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: 501,
        shell: ShellKind::PowerShell,
        cwd: project.clone(),
        before_cursor: "npm run bu".into(),
        after_cursor: String::new(),
        max_results: 8,
        terminal_rows: Some(24),
        terminal_columns: Some(120),
        presentation: dirgo::suggestions::SuggestionPresentation::List,
    };
    let output = fixture
        .command()
        .arg("__suggest")
        .write_stdin(format!(
            "{}\n",
            serde_json::to_string(&request).expect("request json")
        ))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let response: SuggestionResponse = serde_json::from_slice(&output).expect("response");
    assert!(response.suggestions.iter().any(|suggestion| {
        suggestion.source == SuggestionSource::ProjectCommand
            && suggestion.edit.replacement == "npm run build"
    }));

    fixture
        .command()
        .args(["__suggest-complete", "--shell", "zsh", "--cwd"])
        .arg(&project)
        .args(["--include-descriptions"])
        .write_stdin("npm run bu\0\0")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("PROJ").and(predicate::str::contains("package.json script")),
        );
}

#[test]
fn long_lived_worker_reloads_command_history_after_it_changes() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    fixture
        .command()
        .args(["suggestions", "history", "enable"])
        .assert()
        .success();

    let mut worker = std::process::Command::new(assert_cmd::cargo::cargo_bin!("dgo"))
        .env("XDG_CONFIG_HOME", fixture.temp.path().join("config"))
        .env("XDG_CACHE_HOME", fixture.temp.path().join("cache"))
        .env("XDG_STATE_HOME", fixture.temp.path().join("state"))
        .env("DGO_DISABLE_UPDATE_CHECK", "1")
        .arg("__suggest-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn worker");
    let mut input = worker.stdin.take().expect("worker stdin");
    let mut output = BufReader::new(worker.stdout.take().expect("worker stdout"));

    fixture
        .command()
        .arg("__suggest-record")
        .write_stdin("cargo test\n")
        .assert()
        .success();
    let request = SuggestionRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: 91,
        shell: ShellKind::PowerShell,
        cwd: fixture.temp.path().join("filesystem"),
        before_cursor: "cargo t".into(),
        after_cursor: String::new(),
        max_results: 8,
        terminal_rows: Some(24),
        terminal_columns: Some(120),
        presentation: dirgo::suggestions::SuggestionPresentation::List,
    };
    writeln!(
        input,
        "{}",
        serde_json::to_string(&request).expect("request json")
    )
    .expect("write request");
    input.flush().expect("flush request");
    let mut line = String::new();
    output.read_line(&mut line).expect("read response");
    let response: SuggestionResponse = serde_json::from_str(&line).expect("response json");
    assert_eq!(response.suggestions[0].edit.replacement, "cargo test");

    drop(input);
    assert!(worker.wait().expect("worker exit").success());
}

#[test]
fn long_lived_worker_stops_serving_after_suggestions_are_disabled() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    fixture.command().arg("refresh").assert().success();

    let mut worker = std::process::Command::new(assert_cmd::cargo::cargo_bin!("dgo"))
        .env("XDG_CONFIG_HOME", fixture.temp.path().join("config"))
        .env("XDG_CACHE_HOME", fixture.temp.path().join("cache"))
        .env("XDG_STATE_HOME", fixture.temp.path().join("state"))
        .env("DGO_DISABLE_UPDATE_CHECK", "1")
        .arg("__suggest-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn worker");
    let mut input = worker.stdin.take().expect("worker stdin");
    let mut output = BufReader::new(worker.stdout.take().expect("worker stdout"));
    let request = SuggestionRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: 101,
        shell: ShellKind::PowerShell,
        cwd: fixture.temp.path().join("filesystem"),
        before_cursor: "cd pun".into(),
        after_cursor: String::new(),
        max_results: 8,
        terminal_rows: Some(24),
        terminal_columns: Some(120),
        presentation: dirgo::suggestions::SuggestionPresentation::List,
    };

    writeln!(
        input,
        "{}",
        serde_json::to_string(&request).expect("request json")
    )
    .expect("write enabled request");
    input.flush().expect("flush enabled request");
    let mut enabled_line = String::new();
    output
        .read_line(&mut enabled_line)
        .expect("read enabled response");
    let enabled: SuggestionResponse =
        serde_json::from_str(&enabled_line).expect("enabled response json");
    assert_eq!(enabled.suggestions.len(), 1);

    fixture
        .command()
        .args(["suggestions", "disable"])
        .assert()
        .success();
    writeln!(
        input,
        "{}",
        serde_json::to_string(&request).expect("request json")
    )
    .expect("write disabled request");
    input.flush().expect("flush disabled request");
    let mut disabled_line = String::new();
    output
        .read_line(&mut disabled_line)
        .expect("read disabled response");
    let disabled: SuggestionResponse =
        serde_json::from_str(&disabled_line).expect("disabled response json");
    assert!(disabled.suggestions.is_empty());

    drop(input);
    assert!(worker.wait().expect("worker exit").success());
}

#[test]
fn shell_adapter_protocol_is_stdin_only_and_enablement_is_explicit() {
    let fixture = Fixture::new();
    fixture.command().arg("refresh").assert().success();
    fixture
        .command()
        .arg("__suggest-enabled")
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty());

    fixture
        .command()
        .args(["suggestions", "enable"])
        .assert()
        .success();
    fixture
        .command()
        .arg("__suggest-enabled")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let cwd = fixture.temp.path().join("filesystem");
    let assert = fixture
        .command()
        .args([
            "__suggest-shell",
            "--shell",
            "zsh",
            "--cwd",
            cwd.to_str().expect("utf8 cwd"),
        ])
        .write_stdin(b"cd pun\0".as_slice())
        .assert()
        .success();
    let output = std::str::from_utf8(&assert.get_output().stdout).expect("utf8 shell response");
    assert_path_suffix(output, "/Projects/Punk");
}

#[test]
fn generated_shell_integrations_never_execute_suggestions() {
    for shell in ["zsh", "bash", "fish", "powershell"] {
        let assert = Command::cargo_bin("dgo")
            .expect("binary")
            .args(["init", shell])
            .assert()
            .success();
        let script = String::from_utf8_lossy(&assert.get_output().stdout);
        assert!(script.contains("__suggest-shell"));
        assert!(script.contains("__suggest-enabled"));
        assert!(script.contains("__suggest-record"));
        assert!(!script.contains("AcceptLine"));
        assert!(!script.contains("accept-line"));
        assert!(!script.contains("eval \"$suggestion\""));
        if shell == "powershell" {
            assert!(script.contains("Ctrl+f"));
            assert!(script.contains("AddToHistoryHandler"));
            assert!(script.contains("DirgoPredictor/$dirgoVersion/DirgoPredictor.psd1"));
        }
        assert!(assert.get_output().stderr.is_empty());
    }
}
