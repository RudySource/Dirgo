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
            .env("XDG_STATE_HOME", self.temp.path().join("state"));
        command
    }
}

fn toml_string(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
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
                .and(predicate::str::contains("bookmark|doctor"))
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
            "eval \"$(command dgo init zsh)\"; builtin cd \"$DGO_TEST_ROOT\"; dgo Punk; dgo ..; print -r -- \"$PWD\"",
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
    let expected = fixture
        .temp
        .path()
        .join("filesystem/Projects")
        .canonicalize()
        .expect("canonical expected path");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        expected.display().to_string()
    );
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
