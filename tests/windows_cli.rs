#![cfg(windows)]

use std::{fs, path::Path};

use assert_cmd::Command;
use predicates::prelude::*;

struct Fixture {
    temp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("filesystem");
        fs::create_dir_all(root.join("Projects/Punk/src")).expect("directory tree");
        let config_dir = temp.path().join("config/dirgo");
        fs::create_dir_all(&config_dir).expect("config directory");
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
fn native_binary_reports_the_package_version() {
    Command::cargo_bin("dgo")
        .expect("binary")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(concat!(
            "dgo ",
            env!("CARGO_PKG_VERSION")
        )));
}

#[test]
fn refresh_and_exact_query_return_the_native_windows_path() {
    let fixture = Fixture::new();
    fixture
        .command()
        .arg("refresh")
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed"));

    let expected = fixture
        .temp
        .path()
        .join("filesystem/Projects/Punk")
        .canonicalize()
        .expect("canonical path")
        .display()
        .to_string();
    fixture
        .command()
        .args(["query", "punk"])
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
}

#[test]
fn doctor_runs_against_isolated_windows_storage() {
    Fixture::new()
        .command()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Dirgo Doctor"));
}
