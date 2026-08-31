use std::{fs, path::Path};

use assert_cmd::Command;
use predicates::prelude::*;

struct Fixture {
    temp: tempfile::TempDir,
    primary: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let primary = temp.path().join("home");
        fs::create_dir_all(&primary).expect("primary root");
        let config_dir = temp.path().join("config/dirgo");
        fs::create_dir_all(&config_dir).expect("config dir");
        fs::write(
            config_dir.join("config.toml"),
            format!(
                "# keep\nschema_version = 1\nroots = [{}]\n",
                toml_string(&primary)
            ),
        )
        .expect("config");
        Self { temp, primary }
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
fn roots_help_exposes_list_add_remove_and_batching_contract() {
    Command::cargo_bin("dgo")
        .expect("binary")
        .args(["roots", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("list")
                .and(predicate::str::contains("add"))
                .and(predicate::str::contains("remove")),
        );
    Command::cargo_bin("dgo")
        .expect("binary")
        .args(["roots", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--no-refresh"));
}

#[test]
fn roots_add_is_idempotent_and_json_marks_a_nested_focused_root() {
    let fixture = Fixture::new();
    let focused = fixture
        .primary
        .join("Library/Application Support/Adobe/CEP");
    fs::create_dir_all(&focused).expect("focused root");

    fixture
        .command()
        .args([
            "roots",
            "add",
            focused.to_str().expect("utf8"),
            "--no-refresh",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added search root"));
    fixture
        .command()
        .args([
            "roots",
            "add",
            focused.to_str().expect("utf8"),
            "--no-refresh",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("already configured"));

    let output = fixture
        .command()
        .args(["roots", "list", "--json"])
        .output()
        .expect("roots json");
    assert!(output.status.success());
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let rows = rows.as_array().expect("JSON array");
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0]["path"],
        fixture
            .primary
            .canonicalize()
            .expect("primary")
            .display()
            .to_string()
    );
    assert_eq!(rows[0]["accessible"], true);
    assert_eq!(rows[0]["focused"], false);
    assert_eq!(
        rows[1]["path"],
        focused
            .canonicalize()
            .expect("focused")
            .display()
            .to_string()
    );
    assert_eq!(rows[1]["accessible"], true);
    assert_eq!(rows[1]["focused"], true);
}

#[test]
fn roots_remove_never_deletes_data_and_refuses_the_final_root() {
    let fixture = Fixture::new();
    let second = fixture.temp.path().join("second");
    fs::create_dir_all(&second).expect("second root");
    fixture
        .command()
        .args([
            "roots",
            "add",
            second.to_str().expect("utf8"),
            "--no-refresh",
        ])
        .assert()
        .success();

    fixture
        .command()
        .args([
            "roots",
            "remove",
            second.to_str().expect("utf8"),
            "--no-refresh",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Removed search root").and(predicate::str::contains(
                "Bookmarks and navigation history were not changed",
            )),
        );
    assert!(second.is_dir());

    fixture
        .command()
        .args([
            "roots",
            "remove",
            fixture.primary.to_str().expect("utf8"),
            "--no-refresh",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("final search root"));
    assert!(fixture.primary.is_dir());
}

#[test]
fn roots_add_accepts_relative_tilde_and_leading_dash_paths() {
    let fixture = Fixture::new();
    let relative = fixture.primary.join("relative space");
    let dash = fixture.primary.join("-dash");
    let home = fixture.temp.path().join("synthetic-home");
    let tilde = home.join("focused");
    for path in [&relative, &dash, &tilde] {
        fs::create_dir_all(path).expect("root path");
    }

    fixture
        .command()
        .current_dir(&fixture.primary)
        .args(["roots", "add", "relative space", "--no-refresh"])
        .assert()
        .success();
    fixture
        .command()
        .current_dir(&fixture.primary)
        .args(["roots", "add", "--no-refresh", "--", "-dash"])
        .assert()
        .success();
    fixture
        .command()
        .env("HOME", &home)
        .args(["roots", "add", "~/focused", "--no-refresh"])
        .assert()
        .success();

    let output = fixture
        .command()
        .args(["roots", "list", "--json"])
        .output()
        .expect("roots json");
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let paths = rows
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|row| row["path"].as_str())
        .collect::<Vec<_>>();
    assert!(
        paths.contains(
            &relative
                .canonicalize()
                .expect("relative")
                .to_str()
                .expect("utf8")
        )
    );
    assert!(paths.contains(&dash.canonicalize().expect("dash").to_str().expect("utf8")));
    assert!(paths.contains(&tilde.canonicalize().expect("tilde").to_str().expect("utf8")));
}

#[test]
fn roots_list_reports_missing_and_file_valued_entries_without_failing() {
    let fixture = Fixture::new();
    let missing = fixture.temp.path().join("missing");
    let file = fixture.temp.path().join("not-a-directory");
    fs::write(&file, b"file").expect("file root");
    fs::write(
        fixture.temp.path().join("config/dirgo/config.toml"),
        format!(
            "schema_version = 1\nroots = [{}, {}, {}]\n",
            toml_string(&fixture.primary),
            toml_string(&missing),
            toml_string(&file)
        ),
    )
    .expect("config");

    fixture
        .command()
        .args(["roots", "list"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("directory no longer exists")
                .and(predicate::str::contains("path is a file")),
        );
}

#[test]
fn failed_refresh_keeps_the_saved_root_and_reports_last_good_index_semantics() {
    let fixture = Fixture::new();
    let focused = fixture.temp.path().join("focused");
    fs::create_dir_all(&focused).expect("focused root");
    let blocked_cache = fixture.temp.path().join("blocked-cache");
    fs::write(&blocked_cache, b"not a directory").expect("blocked cache");

    fixture
        .command()
        .env("XDG_CACHE_HOME", &blocked_cache)
        .args(["roots", "add", focused.to_str().expect("utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added search root"))
        .stderr(
            predicate::str::contains("Root was saved, but the index refresh failed")
                .and(predicate::str::contains("last good index remains active")),
        );

    fixture
        .command()
        .args(["roots", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            focused
                .canonicalize()
                .expect("focused")
                .display()
                .to_string(),
        ));
}

#[test]
fn generated_completions_expose_roots_without_accessing_storage() {
    for shell in ["zsh", "bash", "fish", "powershell"] {
        Command::cargo_bin("dgo")
            .expect("binary")
            .args(["completions", shell])
            .assert()
            .success()
            .stdout(
                predicate::str::contains("roots")
                    .and(predicate::str::contains("add"))
                    .and(predicate::str::contains("remove")),
            );
    }
}

#[test]
fn focused_root_is_searchable_below_an_ignored_parent_without_indexing_its_siblings() {
    let fixture = Fixture::new();
    let focused = fixture
        .primary
        .join("Library/Application Support/Adobe/CEP");
    let extension = focused.join("extensions/sample-extension");
    let ignored_sibling = fixture.primary.join("Library/Unrelated/Noise");
    fs::create_dir_all(&extension).expect("focused tree");
    fs::create_dir_all(&ignored_sibling).expect("ignored sibling");

    fixture
        .command()
        .args(["roots", "add", focused.to_str().expect("utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed"));

    let output = fixture
        .command()
        .args(["explain", "library/adobe/cep"])
        .output()
        .expect("focused query");
    assert!(output.status.success());
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("focused query JSON");
    let candidates = response["candidates"].as_array().expect("candidates");
    assert!(candidates.iter().any(|candidate| {
        candidate["path"].as_str()
            == Some(
                focused
                    .canonicalize()
                    .expect("focused")
                    .to_str()
                    .expect("utf8"),
            )
    }));

    let output = fixture
        .command()
        .args(["explain", "library/unrelated/noise"])
        .output()
        .expect("ignored sibling query");
    assert!(output.status.success());
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ignored query JSON");
    assert!(response.get("candidates").is_none());
}

#[test]
fn ignored_path_miss_explains_focused_roots_while_normal_miss_stays_short() {
    let fixture = Fixture::new();

    fixture
        .command()
        .args(["query", "library/adobe/cep"])
        .assert()
        .code(3)
        .stderr(
            predicate::str::contains("No indexed directory matches")
                .and(predicate::str::contains(
                    "\"Library\" is excluded from the default index",
                ))
                .and(predicate::str::contains("dgo roots add <PATH>")),
        );

    fixture
        .command()
        .args(["query", "ordinary-missing-name"])
        .assert()
        .code(3)
        .stderr(
            predicate::str::contains("Try a shorter query or run `dgo refresh`")
                .and(predicate::str::contains("dgo roots add <PATH>").not()),
        );
}
