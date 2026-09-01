use std::fs;

use assert_cmd::Command;
use dirgo::{
    paths::AppPaths,
    update::{UpdateStatus, local_status, render_version_status},
};

#[cfg(unix)]
use dirgo::update::set_notifications;

fn paths(temp: &tempfile::TempDir) -> AppPaths {
    let cache_dir = temp.path().join("cache/dirgo");
    let state_dir = temp.path().join("state/dirgo");
    AppPaths {
        config_file: temp.path().join("config/dirgo/config.toml"),
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
fn piped_version_is_exactly_one_line_and_never_mutates_update_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cache = temp.path().join("cache");
    let state = temp.path().join("state");

    Command::cargo_bin("dgo")
        .expect("binary")
        .env("XDG_CONFIG_HOME", temp.path().join("config"))
        .env("XDG_CACHE_HOME", &cache)
        .env("XDG_STATE_HOME", &state)
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("dgo {}\n", env!("CARGO_PKG_VERSION")))
        .stderr("");

    assert!(!cache.exists());
    assert!(!state.exists());
}

#[test]
fn local_status_distinguishes_available_current_stale_unknown_and_disabled() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = paths(&temp);
    fs::create_dir_all(&paths.cache_dir).expect("cache dir");
    fs::create_dir_all(&paths.state_dir).expect("state dir");

    assert_eq!(local_status(&paths), UpdateStatus::Unknown);
    fs::write(
        &paths.update_cache_file,
        format!(r#"{{"checked_at":{},"latest_version":"9.9.9"}}"#, u64::MAX),
    )
    .expect("available cache");
    assert_eq!(
        local_status(&paths),
        UpdateStatus::Available {
            latest: "9.9.9".into()
        }
    );

    fs::write(
        &paths.update_cache_file,
        format!(
            r#"{{"checked_at":{},"latest_version":"{}"}}"#,
            u64::MAX,
            env!("CARGO_PKG_VERSION")
        ),
    )
    .expect("current cache");
    assert_eq!(local_status(&paths), UpdateStatus::UpToDate);

    fs::write(
        &paths.update_cache_file,
        r#"{"checked_at":1,"latest_version":"9.9.9"}"#,
    )
    .expect("stale cache");
    assert!(matches!(local_status(&paths), UpdateStatus::Stale { .. }));

    fs::write(&paths.update_notice_disabled_file, "disabled\n").expect("disabled marker");
    assert_eq!(local_status(&paths), UpdateStatus::Disabled);
}

#[test]
fn interactive_status_copy_is_compact_stylish_and_actionable() {
    let available = render_version_status(
        &UpdateStatus::Available {
            latest: "0.8.0".into(),
        },
        false,
        true,
    );
    let disabled = render_version_status(&UpdateStatus::Disabled, false, false);

    assert!(available.contains("●  Update available"));
    assert!(available.contains(&format!("{}  →  0.8.0", env!("CARGO_PKG_VERSION"))));
    assert!(available.contains("dgo --update"));
    assert!(disabled.contains("*  Update checks are off"));
    assert!(disabled.contains("dgo update-notifications on"));
    assert!(!available.contains("\u{1b}"));
}

#[test]
fn corrupt_or_hostile_cache_is_unknown_and_never_rendered() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = paths(&temp);
    fs::create_dir_all(&paths.cache_dir).expect("cache dir");
    for bytes in [
        b"not json".as_slice(),
        br#"{"checked_at":9,"latest_version":"9.9.9\nowned"}"#.as_slice(),
        br#"{"checked_at":9,"latest_version":"nope"}"#.as_slice(),
    ] {
        fs::write(&paths.update_cache_file, bytes).expect("cache");
        assert_eq!(local_status(&paths), UpdateStatus::Unknown);
    }
}

#[cfg(unix)]
#[test]
fn notification_setting_is_private_and_refuses_symlink_targets() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = paths(&temp);
    set_notifications(&paths, false).expect("disable notifications");
    let mode = fs::metadata(&paths.update_notice_disabled_file)
        .expect("marker metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);

    fs::remove_file(&paths.update_notice_disabled_file).expect("remove fixture marker");
    let victim = temp.path().join("victim");
    fs::write(&victim, "keep me").expect("victim");
    symlink(&victim, &paths.update_notice_disabled_file).expect("marker symlink");

    assert!(set_notifications(&paths, false).is_err());
    assert_eq!(
        fs::read_to_string(&victim).expect("victim bytes"),
        "keep me"
    );
}
