use dirgo::{
    config::SuggestionsConfig,
    suggestions::{CommandHistoryStore, read_command_history},
};

#[test]
fn command_history_is_opt_in_filtered_bounded_and_clearable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("state/suggestions.redb");
    let store = CommandHistoryStore::open(&path).expect("open store");
    let mut config = SuggestionsConfig {
        command_history: true,
        retention_entries: 2,
        retention_days: 180,
        ..SuggestionsConfig::default()
    };

    assert!(
        store
            .record("git status", 100, &config)
            .expect("record command")
    );
    assert!(
        store
            .record("git status", 110, &config)
            .expect("record duplicate")
    );
    assert!(
        !store
            .record("export API_TOKEN=secret", 120, &config)
            .expect("filter secret")
    );
    assert!(
        store
            .record("cargo test", 130, &config)
            .expect("record command")
    );
    assert!(
        store
            .record("git diff", 140, &config)
            .expect("record command")
    );

    let entries = store.entries(140, &config).expect("load entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].command, "git diff");
    assert_eq!(entries[1].command, "cargo test");
    assert!(entries.iter().all(|entry| !entry.command.contains("TOKEN")));

    config.command_history = false;
    assert!(
        !store
            .record("cargo build", 150, &config)
            .expect("disabled history")
    );
    assert_eq!(store.clear().expect("clear history"), 2);
    assert!(
        store
            .entries(150, &config)
            .expect("empty entries")
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn command_history_file_is_user_only() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("state/suggestions.redb");
    let _store = CommandHistoryStore::open(&path).expect("open store");

    assert_eq!(
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn command_history_supports_multiple_suggestion_readers() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let mut config = SuggestionsConfig {
        command_history: true,
        ..SuggestionsConfig::default()
    };
    CommandHistoryStore::open(&path)
        .expect("store")
        .record("cargo test", 10, &config)
        .expect("record");

    let first = redb::ReadOnlyDatabase::open(&path).expect("first reader");
    let entries = read_command_history(&path, 10, &config).expect("second reader");
    assert_eq!(entries[0].command, "cargo test");
    drop(first);

    config.retention_days = 1;
    assert!(
        read_command_history(&path, 86_411, &config)
            .expect("expired read")
            .is_empty()
    );
}

#[test]
fn busy_command_history_never_blocks_other_suggestion_providers() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let config = SuggestionsConfig {
        command_history: true,
        ..SuggestionsConfig::default()
    };
    let writer = CommandHistoryStore::open(&path).expect("writer");

    assert!(
        read_command_history(&path, 10, &config)
            .expect("busy history is optional")
            .is_empty()
    );
    drop(writer);
}
