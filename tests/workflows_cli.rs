use std::{fs, path::Path};

use assert_cmd::Command;
use dirgo::suggestions::{CommandHistoryRecordFrame, HISTORY_RECORD_PROTOCOL_VERSION, ShellKind};
use predicates::prelude::*;

struct Fixture {
    temp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("filesystem/Project");
        fs::create_dir_all(&root).expect("fixture tree");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\n",
        )
        .expect("project marker");
        let config_dir = temp.path().join("config/dirgo");
        fs::create_dir_all(&config_dir).expect("config directory");
        fs::write(
            config_dir.join("config.toml"),
            format!("schema_version = 1\nroots = [{}]\n", toml_string(&root)),
        )
        .expect("config");
        Self { temp }
    }

    fn project(&self) -> std::path::PathBuf {
        self.temp.path().join("filesystem/Project")
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("dgo").expect("binary");
        command
            .current_dir(self.project())
            .env("XDG_CONFIG_HOME", self.temp.path().join("config"))
            .env("XDG_CACHE_HOME", self.temp.path().join("cache"))
            .env("XDG_STATE_HOME", self.temp.path().join("state"))
            .env("DGO_DISABLE_UPDATE_CHECK", "1")
            .env("DGO_SESSION_ID", "workflow-session");
        command
    }

    fn record(&self, command: &str, at: u64) {
        let frame = CommandHistoryRecordFrame {
            protocol_version: HISTORY_RECORD_PROTOCOL_VERSION,
            command: command.into(),
            cwd: self.project(),
            exit_code: Some(0),
            duration_ms: Some(10),
            session_id: Some("workflow-session".into()),
            shell: ShellKind::Zsh,
            started_at: at,
        };
        self.command()
            .arg("__suggest-record")
            .write_stdin(serde_json::to_vec(&frame).expect("frame"))
            .assert()
            .success();
    }
}

fn toml_string(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

#[test]
fn workflow_enable_is_separate_and_requires_history() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["workflows", "enable"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("dgo suggestions history enable"));

    fixture
        .command()
        .args(["suggestions", "history", "enable"])
        .assert()
        .success();
    fixture
        .command()
        .args(["workflows", "enable"])
        .assert()
        .success();
    fixture
        .command()
        .args(["workflows", "status", "--json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"enabled\":true")
                .and(predicate::str::contains("\"schema_version\":3")),
        );
}

#[test]
fn saved_workflow_lifecycle_and_export_are_bounded_and_private() {
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
        .args(["workflows", "enable"])
        .assert()
        .success();
    fixture.record("cargo fmt", 1_800_000_001);
    fixture.record("cargo test", 1_800_000_002);

    fixture
        .command()
        .args(["workflows", "save", "Verify", "--last", "2", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Saved workflow 1"));
    let database = fixture.temp.path().join("state/dirgo/suggestions.redb");
    let before_reads = fs::read(&database).expect("database before reads");
    fixture
        .command()
        .args(["workflows", "list", "--json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"name\":\"Verify\"")
                .and(predicate::str::contains("cargo fmt")),
        );
    fixture
        .command()
        .args(["workflows", "show", "1", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cargo test"));
    fixture
        .command()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 saved"));
    fixture.command().arg("stats").assert().success().stdout(
        predicate::str::contains("History events        2")
            .and(predicate::str::contains("Saved workflows       1")),
    );
    assert_eq!(
        fs::read(&database).expect("database after reads"),
        before_reads,
        "status/list/show/doctor/stats must not mutate workflow storage"
    );
    fixture
        .command()
        .args(["workflows", "rename", "1", "Quality gate"])
        .assert()
        .success();

    let output = fixture.temp.path().join("exports/workflows.jsonl");
    fixture
        .command()
        .args([
            "workflows",
            "export",
            "--all",
            "--output",
            output.to_str().expect("path"),
        ])
        .assert()
        .success();
    let exported = fs::read_to_string(&output).expect("export");
    assert!(exported.contains("dirgo-workflows"));
    assert!(exported.contains("Quality gate"));
    assert!(!exported.contains(fixture.project().to_str().expect("path")));
    #[cfg(unix)]
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&fs::metadata(&output).unwrap().permissions())
            & 0o777,
        0o600
    );
    fixture
        .command()
        .args([
            "workflows",
            "export",
            "--all",
            "--output",
            output.to_str().expect("path"),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--force"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let target = fixture.temp.path().join("exports/untouched");
        fs::write(&target, "keep").expect("target");
        let link = fixture.temp.path().join("exports/link.jsonl");
        symlink(&target, &link).expect("symlink");
        fixture
            .command()
            .args([
                "workflows",
                "export",
                "--all",
                "--output",
                link.to_str().expect("path"),
                "--force",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains("symlink"));
        assert_eq!(fs::read_to_string(target).expect("target"), "keep");
    }

    fixture
        .command()
        .args(["workflows", "remove", "1"])
        .assert()
        .success();
    fixture
        .command()
        .args(["workflows", "show", "1"])
        .assert()
        .failure();
}

#[test]
fn save_rejects_missing_session_and_hostile_names() {
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
        .args(["workflows", "enable"])
        .assert()
        .success();
    fixture
        .command()
        .env_remove("DGO_SESSION_ID")
        .args(["workflows", "save", "Missing", "--last", "2", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("DGO_SESSION_ID"));
    fixture
        .command()
        .args([
            "workflows",
            "save",
            "bad\u{202e}name",
            "--last",
            "2",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("workflow name"));
}

#[test]
fn save_rechecks_configured_privacy_filters_before_printing_or_persisting_steps() {
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
        .args(["workflows", "enable"])
        .assert()
        .success();
    fixture.record("cargo fmt", 1_800_000_001);
    fixture.record("internal deploy", 1_800_000_002);
    let config_path = fixture.temp.path().join("config/dirgo/config.toml");
    let config = fs::read_to_string(&config_path).expect("config").replace(
        "deny_patterns = []",
        "deny_patterns = [\"internal deploy\"]",
    );
    fs::write(&config_path, config).expect("config");

    fixture
        .command()
        .args(["workflows", "save", "Private", "--last", "2", "--yes"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("internal deploy").not())
        .stderr(
            predicate::str::contains("blocked by privacy filters")
                .and(predicate::str::contains("internal deploy").not()),
        );
    fixture
        .command()
        .args(["workflows", "list", "--all", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"saved\":[]"));
}
