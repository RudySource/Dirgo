use std::fs;

use dirgo::{
    config::Config,
    config_edit::{ConfigMutation, mutate_config},
    paths::AppPaths,
};

fn app_paths(temp: &tempfile::TempDir) -> AppPaths {
    let cache_dir = temp.path().join("cache");
    let state_dir = temp.path().join("state");
    AppPaths {
        config_file: temp.path().join("config.toml"),
        index_file: cache_dir.join("index.redb"),
        state_file: state_dir.join("state.redb"),
        suggestions_state_file: state_dir.join("suggestions.redb"),
        update_cache_file: cache_dir.join("update.json"),
        update_check_file: cache_dir.join("update-check"),
        update_notice_disabled_file: state_dir.join("update-notifications-disabled"),
        cache_dir,
        state_dir,
    }
}

#[test]
fn root_mutation_preserves_comments_formatting_and_unrelated_sections() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = app_paths(&temp);
    let existing_root = temp.path().join("existing");
    let added_root = temp.path().join("тема 'quoted'");
    fs::create_dir_all(&existing_root).expect("existing root");
    fs::create_dir_all(&added_root).expect("added root");

    let original = format!(
        "# owner note\nschema_version = 1\nroots = [\"{}\"] # keep root note\nignore = [\"target\"]\nrespect_gitignore = true\nfollow_symlinks = false\n\n[ui]\npreview = true\naccent = \"violet\" # keep accent note\nicons = \"never\"\nheight_percent = 61\n\n[actions]\neditor = \"code\"\n",
        existing_root.display()
    );
    fs::write(&paths.config_file, &original).expect("write config");

    let outcome = mutate_config(
        &paths.config_file,
        ConfigMutation::AddRoot(added_root.canonicalize().expect("canonical added root")),
    )
    .expect("add focused root");

    assert!(outcome.changed);
    let updated = fs::read_to_string(&paths.config_file).expect("updated config");
    assert!(updated.starts_with("# owner note\n"));
    assert!(updated.contains("# keep root note"));
    assert!(updated.contains("accent = \"violet\" # keep accent note"));
    assert!(updated.contains("[actions]\neditor = \"code\""));

    let loaded = Config::load(&paths).expect("mutated config remains valid");
    assert_eq!(
        loaded.roots,
        vec![
            existing_root,
            added_root.canonicalize().expect("canonical added root"),
        ]
    );
}

#[test]
fn removing_a_root_changes_only_the_roots_array() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = app_paths(&temp);
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir_all(&first).expect("first root");
    fs::create_dir_all(&second).expect("second root");
    let original = format!(
        "# preserved\nschema_version = 1\nroots = [\n  \"{}\",\n  \"{}\",\n]\nignore = [\"Library\"]\nrespect_gitignore = true\nfollow_symlinks = false\n",
        first.display(),
        second.display()
    );
    fs::write(&paths.config_file, &original).expect("write config");

    let outcome = mutate_config(
        &paths.config_file,
        ConfigMutation::RemoveRoot(first.canonicalize().expect("canonical first root")),
    )
    .expect("remove root");

    assert!(outcome.changed);
    let updated = fs::read_to_string(&paths.config_file).expect("updated config");
    assert!(updated.starts_with("# preserved\n"));
    assert!(updated.contains("ignore = [\"Library\"]"));
    assert_eq!(
        Config::load(&paths).expect("valid config").roots,
        vec![second]
    );
}

#[test]
fn missing_root_is_rejected_without_changing_the_config() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = app_paths(&temp);
    let existing = temp.path().join("existing");
    fs::create_dir_all(&existing).expect("existing root");
    let original = format!("schema_version = 1\nroots = [\"{}\"]\n", existing.display());
    fs::write(&paths.config_file, &original).expect("write config");

    let error = mutate_config(
        &paths.config_file,
        ConfigMutation::AddRoot(temp.path().join("missing")),
    )
    .expect_err("missing root must fail");

    assert!(error.to_string().contains("existing directory"));
    assert_eq!(
        fs::read_to_string(&paths.config_file).expect("config"),
        original
    );
}

#[cfg(unix)]
#[test]
fn config_symlink_is_rejected_without_touching_its_target() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let target = temp.path().join("target.toml");
    let link = temp.path().join("config.toml");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir_all(&first).expect("first root");
    fs::create_dir_all(&second).expect("second root");
    let original = format!("schema_version = 1\nroots = [\"{}\"]\n", first.display());
    fs::write(&target, &original).expect("target config");
    symlink(&target, &link).expect("config symlink");

    let error = mutate_config(
        &link,
        ConfigMutation::AddRoot(second.canonicalize().expect("canonical second")),
    )
    .expect_err("symlink config must fail closed");

    assert!(error.to_string().contains("symlink"));
    assert_eq!(fs::read_to_string(&target).expect("target"), original);
    assert!(
        fs::symlink_metadata(&link)
            .expect("link metadata")
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn first_config_is_private_and_contains_the_default_and_added_roots() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = app_paths(&temp);
    let added = temp.path().join("focused");
    fs::create_dir_all(&added).expect("focused root");

    let outcome = mutate_config(&paths.config_file, ConfigMutation::AddRoot(added.clone()))
        .expect("create config");

    assert!(outcome.changed);
    let mode = fs::metadata(&paths.config_file)
        .expect("config metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    let config = Config::load(&paths).expect("valid new config");
    assert!(
        config
            .roots
            .contains(&added.canonicalize().expect("canonical added"))
    );
    assert!(config.roots.len() >= 2);
}

#[cfg(unix)]
#[test]
fn existing_config_permissions_survive_mutation() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = app_paths(&temp);
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir_all(&first).expect("first root");
    fs::create_dir_all(&second).expect("second root");
    fs::write(
        &paths.config_file,
        format!("schema_version = 1\nroots = [\"{}\"]\n", first.display()),
    )
    .expect("config");
    fs::set_permissions(&paths.config_file, fs::Permissions::from_mode(0o640)).expect("set mode");

    mutate_config(&paths.config_file, ConfigMutation::AddRoot(second)).expect("mutate config");

    assert_eq!(
        fs::metadata(&paths.config_file)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
}

#[cfg(unix)]
#[test]
fn symlink_alias_is_an_idempotent_duplicate() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = app_paths(&temp);
    let root = temp.path().join("root");
    let alias = temp.path().join("alias");
    fs::create_dir_all(&root).expect("root");
    symlink(&root, &alias).expect("alias");
    let original = format!("schema_version = 1\nroots = [\"{}\"]\n", root.display());
    fs::write(&paths.config_file, &original).expect("config");

    let outcome =
        mutate_config(&paths.config_file, ConfigMutation::AddRoot(alias)).expect("duplicate alias");

    assert!(!outcome.changed);
    assert_eq!(
        fs::read_to_string(&paths.config_file).expect("config"),
        original
    );
}

#[test]
fn final_root_removal_is_rejected_without_changing_the_config() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = app_paths(&temp);
    let root = temp.path().join("only");
    fs::create_dir_all(&root).expect("root");
    let original = format!("schema_version = 1\nroots = [\"{}\"]\n", root.display());
    fs::write(&paths.config_file, &original).expect("config");

    let error = mutate_config(&paths.config_file, ConfigMutation::RemoveRoot(root))
        .expect_err("final root must remain");

    assert!(error.to_string().contains("at least one"));
    assert_eq!(
        fs::read_to_string(&paths.config_file).expect("config"),
        original
    );
}

#[test]
fn file_and_newline_roots_are_rejected_without_changing_the_config() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = app_paths(&temp);
    let root = temp.path().join("root");
    let file = temp.path().join("file");
    fs::create_dir_all(&root).expect("root");
    fs::write(&file, b"not a directory").expect("file");
    let original = format!("schema_version = 1\nroots = [\"{}\"]\n", root.display());
    fs::write(&paths.config_file, &original).expect("config");

    assert!(mutate_config(&paths.config_file, ConfigMutation::AddRoot(file)).is_err());
    assert!(
        mutate_config(
            &paths.config_file,
            ConfigMutation::AddRoot(temp.path().join("line\nbreak")),
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(&paths.config_file).expect("config"),
        original
    );
}

#[test]
fn concurrent_root_mutations_preserve_every_successful_addition() {
    use std::sync::{Arc, Barrier};

    const WRITERS: usize = 16;
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = app_paths(&temp);
    let initial = temp.path().join("initial");
    fs::create_dir_all(&initial).expect("initial root");
    fs::write(
        &paths.config_file,
        format!("schema_version = 1\nroots = [\"{}\"]\n", initial.display()),
    )
    .expect("config");
    let roots = (0..WRITERS)
        .map(|index| temp.path().join(format!("root-{index:02}")))
        .collect::<Vec<_>>();
    for root in &roots {
        fs::create_dir_all(root).expect("concurrent root");
    }
    let barrier = Arc::new(Barrier::new(WRITERS));
    let handles = roots
        .iter()
        .cloned()
        .map(|root| {
            let config_file = paths.config_file.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                mutate_config(&config_file, ConfigMutation::AddRoot(root))
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle
            .join()
            .expect("writer thread")
            .expect("successful mutation");
    }

    let config = Config::load(&paths).expect("valid concurrent config");
    assert_eq!(config.roots.len(), WRITERS + 1);
    for root in roots {
        assert!(
            config
                .roots
                .contains(&root.canonicalize().expect("canonical concurrent root"))
        );
    }
}
