use std::fs;

use assert_cmd::Command;
use dirgo::{
    paths::AppPaths,
    update::{
        CacheFreshness, RefreshDisposition, StableVersion, UpdateView, VersionRelation,
        local_view_at, render_version_status,
    },
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
fn local_view_keeps_version_knowledge_separate_from_freshness() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = paths(&temp);
    fs::create_dir_all(&paths.cache_dir).expect("cache dir");
    fs::create_dir_all(&paths.state_dir).expect("state dir");

    let missing = local_view_at(&paths, 1_000);
    assert_eq!(missing.relation, VersionRelation::Unknown);
    assert_eq!(missing.freshness, CacheFreshness::Missing);
    fs::write(
        &paths.update_cache_file,
        r#"{"checked_at":1000,"latest_version":"9.9.9"}"#,
    )
    .expect("available cache");
    let available = local_view_at(&paths, 1_000);
    assert_eq!(
        available.relation,
        VersionRelation::UpdateAvailable {
            latest: StableVersion::new(9, 9, 9)
        }
    );
    assert_eq!(available.freshness, CacheFreshness::Fresh);

    fs::write(
        &paths.update_cache_file,
        format!(
            r#"{{"checked_at":{},"latest_version":"{}"}}"#,
            1_000,
            env!("CARGO_PKG_VERSION")
        ),
    )
    .expect("current cache");
    let current = local_view_at(&paths, 1_000);
    assert!(matches!(current.relation, VersionRelation::Current { .. }));
    assert_eq!(current.freshness, CacheFreshness::Fresh);

    let clock_rollback = local_view_at(&paths, 999);
    assert_eq!(clock_rollback.freshness, CacheFreshness::Stale);

    fs::write(
        &paths.update_cache_file,
        r#"{"checked_at":1000,"latest_version":"0.0.1"}"#,
    )
    .expect("older stable cache");
    assert!(matches!(
        local_view_at(&paths, 1_000).relation,
        VersionRelation::AheadOfLatest { .. }
    ));

    fs::write(
        &paths.update_cache_file,
        r#"{"checked_at":1,"latest_version":"9.9.9"}"#,
    )
    .expect("stale cache");
    let stale_available = local_view_at(&paths, 24 * 60 * 60 + 2);
    assert_eq!(
        stale_available.relation,
        VersionRelation::UpdateAvailable {
            latest: StableVersion::new(9, 9, 9)
        }
    );
    assert_eq!(stale_available.freshness, CacheFreshness::Stale);

    fs::write(
        &paths.update_cache_file,
        format!(
            r#"{{"checked_at":1,"latest_version":"{}"}}"#,
            env!("CARGO_PKG_VERSION")
        ),
    )
    .expect("stale current cache");
    let stale_current = local_view_at(&paths, 24 * 60 * 60 + 2);
    assert!(matches!(
        stale_current.relation,
        VersionRelation::Current { .. }
    ));
    assert_eq!(stale_current.freshness, CacheFreshness::Stale);

    fs::write(&paths.update_notice_disabled_file, "disabled\n").expect("disabled marker");
    assert_eq!(
        local_view_at(&paths, 24 * 60 * 60 + 2).refresh,
        RefreshDisposition::Disabled
    );

    fs::write(&paths.update_notice_disabled_file, "unexpected\n").expect("malformed marker");
    assert_eq!(
        local_view_at(&paths, 24 * 60 * 60 + 2).refresh,
        RefreshDisposition::StartFailed
    );
}

#[test]
fn interactive_status_copy_is_compact_stylish_and_actionable() {
    let available = render_version_status(
        &UpdateView {
            relation: VersionRelation::UpdateAvailable {
                latest: StableVersion::new(0, 8, 0),
            },
            freshness: CacheFreshness::Stale,
            last_success_at: Some(1),
            refresh: RefreshDisposition::Started,
        },
        false,
        true,
    );
    let disabled = render_version_status(
        &UpdateView {
            relation: VersionRelation::Unknown,
            freshness: CacheFreshness::Missing,
            last_success_at: None,
            refresh: RefreshDisposition::Disabled,
        },
        false,
        false,
    );

    assert!(available.contains("●  Update 0.8.0 available"));
    assert!(available.contains("Cached result · checking again"));
    assert!(available.contains("dgo --update"));
    assert!(disabled.contains("*  Update checks are off"));
    assert!(disabled.contains("dgo update-notifications on"));
    assert!(!available.contains("\u{1b}"));
}

#[test]
fn rendering_is_truthful_for_refresh_and_degraded_states() {
    let view = |relation, freshness, refresh| UpdateView {
        relation,
        freshness,
        last_success_at: Some(1),
        refresh,
    };
    let current = StableVersion::new(0, 7, 1);

    for refresh in [
        RefreshDisposition::Started,
        RefreshDisposition::AlreadyRunning,
    ] {
        let rendered = render_version_status(
            &view(
                VersionRelation::Current { latest: current },
                CacheFreshness::Stale,
                refresh,
            ),
            false,
            true,
        );
        assert!(rendered.contains("Checking for updates"));
        assert!(rendered.contains("Last known stable: 0.7.1"));
    }

    for refresh in [
        RefreshDisposition::NotDue,
        RefreshDisposition::BackingOff { retry_at: 99 },
        RefreshDisposition::StartFailed,
    ] {
        let rendered = render_version_status(
            &view(VersionRelation::Unknown, CacheFreshness::Missing, refresh),
            false,
            false,
        );
        assert!(rendered.contains("Update status unavailable"));
        assert!(!rendered.contains("Checking for updates"));
        assert!(!rendered.contains("checking again"));
    }

    let ahead = render_version_status(
        &view(
            VersionRelation::AheadOfLatest {
                latest: StableVersion::new(0, 7, 0),
            },
            CacheFreshness::Fresh,
            RefreshDisposition::NotDue,
        ),
        false,
        false,
    );
    assert!(ahead.contains("Running ahead of latest stable"));
    assert!(!ahead.contains("up to date"));
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
        assert_eq!(local_view_at(&paths, 10).relation, VersionRelation::Unknown);
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

    assert_eq!(
        local_view_at(&paths, 1).refresh,
        RefreshDisposition::StartFailed
    );
    assert!(set_notifications(&paths, false).is_err());
    assert_eq!(
        fs::read_to_string(&victim).expect("victim bytes"),
        "keep me"
    );
}
