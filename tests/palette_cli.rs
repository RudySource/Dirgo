#![cfg(unix)]

use std::{fs, path::Path};

use assert_cmd::Command;
use predicates::prelude::*;

struct Fixture {
    temp: tempfile::TempDir,
    project: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let project = temp.path().join("workspace/Dirgo Palette демо");
        fs::create_dir_all(project.join("src")).expect("project tree");
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname='palette-demo'\nversion='0.1.0'\nedition='2024'\n",
        )
        .expect("cargo manifest");
        fs::write(project.join("src/main.rs"), "fn main() {}\n").expect("source");
        fs::write(
            project.join("package.json"),
            r#"{"name":"palette-demo","scripts":{"dev":"vite"}}"#,
        )
        .expect("package manifest");
        fs::write(
            project.join("compose.yaml"),
            "services:\n  api:\n    image: example.invalid/api\n",
        )
        .expect("compose manifest");
        let config_dir = temp.path().join("config/dirgo");
        fs::create_dir_all(&config_dir).expect("config dir");
        fs::write(
            config_dir.join("config.toml"),
            format!("schema_version = 1\nroots = [{}]\n", toml_string(&project)),
        )
        .expect("config");
        Self { temp, project }
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

#[test]
fn palette_is_a_public_command_with_a_query() {
    Command::cargo_bin("dgo")
        .expect("binary")
        .args(["palette", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Open the Workspace Palette")
                .and(predicate::str::contains("QUERY")),
        );
}

#[test]
fn hidden_json_contract_exposes_one_bounded_snapshot_for_all_sources() {
    let fixture = Fixture::new();
    fixture.command().arg("refresh").assert().success();

    let output = fixture
        .command()
        .args([
            "__palette-json",
            "--cwd",
            fixture.project.to_str().expect("utf8 project"),
        ])
        .output()
        .expect("palette json");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    let items = value["items"].as_array().expect("items");

    assert!(items.iter().any(|item| item["source"] == "files"));
    assert!(items.iter().any(|item| item["source"] == "tasks"));
    assert!(items.iter().any(|item| item["source"] == "compose"));
    assert!(items.iter().any(|item| item["source"] == "places"));
    assert_eq!(value["states"]["files"], "ready");
    assert!(items.len() <= 768, "snapshot exceeded its global bound");
}
