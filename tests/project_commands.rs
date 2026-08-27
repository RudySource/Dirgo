#[cfg(unix)]
use dirgo::suggestions::claim_project_command_refresh;
use dirgo::suggestions::{
    load_cached_project_command_snapshot, load_project_command_snapshot,
    refresh_project_command_cache,
};

#[test]
fn package_json_scripts_use_the_declared_manager_without_exposing_script_bodies() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        temp.path().join("package.json"),
        r#"{
          "name": "acme-web",
          "packageManager": "pnpm@10.4.0",
          "scripts": {
            "build": "vite build --token private-value",
            "dev": "vite --host",
            "bad\nname": "echo unsafe"
          }
        }"#,
    )
    .expect("package.json");

    let snapshot = load_project_command_snapshot(temp.path()).expect("snapshot");
    let commands = snapshot.commands();

    assert!(commands.iter().any(|command| {
        command.replacement == "pnpm run build"
            && command.display == "build"
            && command.description == "package.json script · acme-web"
    }));
    assert!(
        commands
            .iter()
            .any(|command| command.replacement == "pnpm run dev")
    );
    assert!(commands.iter().all(|command| {
        !command.replacement.contains('\n')
            && !command.description.contains("private-value")
            && !command.description.contains("vite")
    }));
}

#[test]
fn package_json_lockfile_selects_bun_when_package_manager_is_absent() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        temp.path().join("package.json"),
        r#"{"scripts":{"test":"vitest"}}"#,
    )
    .expect("package.json");
    std::fs::write(temp.path().join("bun.lock"), "lockfileVersion = 1").expect("bun lock");

    let snapshot = load_project_command_snapshot(temp.path()).expect("snapshot");

    assert!(
        snapshot
            .commands()
            .iter()
            .any(|command| command.replacement == "bun run test")
    );
}

#[test]
fn cargo_manifest_exposes_declared_targets_features_and_workspace_packages() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(temp.path().join("crates/api")).expect("member dir");
    std::fs::write(
        temp.path().join("Cargo.toml"),
        r#"
            [package]
            name = "acme-cli"
            version = "0.1.0"

            [features]
            fast = []
            internal = []

            [[bin]]
            name = "acme"
            path = "src/main.rs"

            [[example]]
            name = "tour"
            path = "examples/tour.rs"

            [workspace]
            members = ["crates/api"]
        "#,
    )
    .expect("root Cargo.toml");
    std::fs::write(
        temp.path().join("crates/api/Cargo.toml"),
        r#"
            [package]
            name = "acme-api"
            version = "0.1.0"

            [[bin]]
            name = "server"
            path = "src/main.rs"
        "#,
    )
    .expect("member Cargo.toml");

    let snapshot = load_project_command_snapshot(temp.path()).expect("snapshot");
    let replacements = snapshot
        .commands()
        .iter()
        .map(|command| command.replacement.as_str())
        .collect::<Vec<_>>();

    assert!(replacements.contains(&"cargo run --bin acme"));
    assert!(replacements.contains(&"cargo run --example tour"));
    assert!(replacements.contains(&"cargo build --features fast"));
    assert!(replacements.contains(&"cargo test -p acme-api"));
    assert!(replacements.contains(&"cargo run -p acme-api --bin server"));
    assert!(snapshot.commands().iter().all(|command| {
        command.description.len() <= 80 && !command.description.contains("src/main.rs")
    }));
}

#[cfg(unix)]
#[test]
fn cargo_workspace_members_cannot_escape_the_project_through_symlinks() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("project");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&root).expect("project root");
    std::fs::create_dir_all(&outside).expect("outside root");
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"linked\"]\n",
    )
    .expect("workspace Cargo.toml");
    std::fs::write(
        outside.join("Cargo.toml"),
        r#"
            [package]
            name = "outside-private"
            version = "0.1.0"

            [[bin]]
            name = "outside-server"
            path = "src/main.rs"
        "#,
    )
    .expect("outside Cargo.toml");
    symlink(&outside, root.join("linked")).expect("workspace member symlink");

    let snapshot = load_project_command_snapshot(&root).expect("snapshot");

    assert!(snapshot.commands().iter().all(|command| {
        !command.replacement.contains("outside-private")
            && !command.replacement.contains("outside-server")
    }));
}

#[test]
fn oversized_cargo_workspace_glob_does_not_return_a_partial_nondeterministic_catalog() {
    let temp = tempfile::tempdir().expect("tempdir");
    let crates = temp.path().join("crates");
    std::fs::create_dir(&crates).expect("crates dir");
    std::fs::write(
        temp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .expect("workspace Cargo.toml");
    for index in 0..65 {
        let member = crates.join(format!("member-{index:02}"));
        std::fs::create_dir(&member).expect("member dir");
        std::fs::write(
            member.join("Cargo.toml"),
            format!("[package]\nname = \"member-{index:02}\"\nversion = \"0.1.0\"\n"),
        )
        .expect("member Cargo.toml");
    }

    let snapshot = load_project_command_snapshot(temp.path()).expect("snapshot");

    assert!(snapshot.commands().is_empty());
}

#[cfg(unix)]
#[test]
fn root_manifests_cannot_escape_the_project_through_symlinks() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("project");
    let outside = temp.path().join("outside-package.json");
    std::fs::create_dir(&root).expect("project root");
    std::fs::create_dir(root.join(".git")).expect("project marker");
    std::fs::write(
        &outside,
        r#"{"name":"outside-private","scripts":{"publish-secrets":"echo no"}}"#,
    )
    .expect("outside package.json");
    symlink(&outside, root.join("package.json")).expect("package.json symlink");

    let snapshot = load_project_command_snapshot(&root).expect("snapshot");

    assert!(snapshot.commands().is_empty());
}

#[test]
fn disk_cache_refreshes_changed_manifests_and_serves_nested_working_directories() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("project");
    let nested = root.join("src/components");
    let cache = temp.path().join("cache");
    std::fs::create_dir_all(&nested).expect("nested cwd");
    std::fs::write(root.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#)
        .expect("package.json");

    assert!(load_cached_project_command_snapshot(&cache, &nested).is_none());
    refresh_project_command_cache(&cache, &nested).expect("first refresh");
    let first = load_cached_project_command_snapshot(&cache, &nested).expect("warm cache");
    assert!(
        first
            .commands()
            .iter()
            .any(|command| command.replacement == "npm run dev")
    );

    std::fs::write(
        root.join("package.json"),
        r#"{"scripts":{"build":"vite build"}}"#,
    )
    .expect("changed package.json");
    let hot_path = load_cached_project_command_snapshot(&cache, &nested).expect("cached snapshot");
    assert!(
        hot_path
            .commands()
            .iter()
            .any(|command| command.replacement == "npm run dev")
    );
    refresh_project_command_cache(&cache, &nested).expect("changed refresh");
    let changed = load_cached_project_command_snapshot(&cache, &nested).expect("changed cache");
    assert!(
        changed
            .commands()
            .iter()
            .any(|command| command.replacement == "npm run build")
    );
    assert!(
        changed
            .commands()
            .iter()
            .all(|command| command.replacement != "npm run dev")
    );
}

#[test]
fn simple_make_just_and_compose_entries_are_included_while_dynamic_forms_are_ignored() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(temp.path().join(".git")).expect("project marker");
    std::fs::write(
        temp.path().join("Makefile"),
        "build: deps\n\t@echo build\n.PHONY: build\npattern-%:\n$DYNAMIC:\n",
    )
    .expect("Makefile");
    std::fs::write(
        temp.path().join("justfile"),
        "serve port='3000':\n  cargo run\n_private:\n  echo hidden\n{{dynamic}}:\nimage := 'acme/api'\n",
    )
    .expect("justfile");
    std::fs::write(
        temp.path().join("compose.yaml"),
        "services:\n  api:\n    image: acme/api\n  web-app:\n    build: .\n  ${DYNAMIC}:\n    image: unsafe\nnetworks:\n  default:\n",
    )
    .expect("compose.yaml");

    let snapshot = load_project_command_snapshot(temp.path()).expect("snapshot");
    let replacements = snapshot
        .commands()
        .iter()
        .map(|command| command.replacement.as_str())
        .collect::<Vec<_>>();

    assert!(replacements.contains(&"make build"));
    assert!(replacements.contains(&"just serve"));
    assert!(replacements.contains(&"docker compose up api"));
    assert!(replacements.contains(&"docker compose up web-app"));
    assert!(replacements.iter().all(|command| {
        !command.contains("pattern-%")
            && !command.contains("DYNAMIC")
            && !command.contains("_private")
            && !command.contains("image")
    }));
}

#[test]
fn malformed_optional_manifest_does_not_suppress_other_project_commands() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(temp.path().join("package.json"), "{ definitely invalid").expect("invalid json");
    std::fs::write(temp.path().join("Makefile"), "build:\n\t@echo build\n").expect("Makefile");

    let snapshot = load_project_command_snapshot(temp.path()).expect("resilient snapshot");

    assert!(
        snapshot
            .commands()
            .iter()
            .any(|command| command.replacement == "make build")
    );
}

#[test]
fn project_cache_prunes_old_roots_to_a_bounded_set() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cache = temp.path().join("cache");
    for index in 0..70 {
        let root = temp.path().join(format!("project-{index:02}"));
        std::fs::create_dir(&root).expect("project root");
        std::fs::write(root.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#)
            .expect("package.json");
        refresh_project_command_cache(&cache, &root).expect("refresh");
    }

    let cache_files = std::fs::read_dir(cache.join("project-commands"))
        .expect("cache directory")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "json")
        })
        .count();
    assert!(cache_files <= 64, "found {cache_files} project snapshots");
}

#[cfg(unix)]
#[test]
fn project_cache_snapshots_are_private_to_the_current_user() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let cache = temp.path().join("cache");
    std::fs::write(
        temp.path().join("package.json"),
        r#"{"scripts":{"dev":"vite"}}"#,
    )
    .expect("package.json");
    refresh_project_command_cache(&cache, temp.path()).expect("refresh");
    let snapshot = std::fs::read_dir(cache.join("project-commands"))
        .expect("cache directory")
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "json")
        })
        .expect("snapshot");

    let mode = snapshot.metadata().expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[cfg(unix)]
#[test]
fn refresh_marker_does_not_follow_a_cache_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let cache = temp.path().join("cache");
    let outside = temp.path().join("outside.txt");
    std::fs::write(
        temp.path().join("package.json"),
        r#"{"scripts":{"dev":"vite"}}"#,
    )
    .expect("package.json");
    refresh_project_command_cache(&cache, temp.path()).expect("refresh");
    assert!(claim_project_command_refresh(&cache, temp.path()));
    let marker = std::fs::read_dir(cache.join("project-commands"))
        .expect("cache directory")
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "checked")
        })
        .expect("refresh marker")
        .path();
    std::fs::remove_file(&marker).expect("remove marker");
    std::fs::write(&outside, "keep me").expect("outside file");
    symlink(&outside, &marker).expect("marker symlink");

    assert!(claim_project_command_refresh(&cache, temp.path()));
    assert_eq!(
        std::fs::read_to_string(&outside).expect("outside text"),
        "keep me"
    );
    assert!(
        std::fs::symlink_metadata(marker)
            .expect("new marker")
            .file_type()
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn cached_snapshot_loader_rejects_symlinks() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let cache = temp.path().join("cache");
    std::fs::write(
        temp.path().join("package.json"),
        r#"{"scripts":{"dev":"vite"}}"#,
    )
    .expect("package.json");
    refresh_project_command_cache(&cache, temp.path()).expect("refresh");
    let snapshot_path = std::fs::read_dir(cache.join("project-commands"))
        .expect("cache directory")
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "json")
        })
        .expect("snapshot")
        .path();
    let outside = temp.path().join("outside-cache.json");
    std::fs::rename(&snapshot_path, &outside).expect("move snapshot outside cache");
    symlink(&outside, &snapshot_path).expect("cache symlink");

    assert!(load_cached_project_command_snapshot(&cache, temp.path()).is_none());
}
