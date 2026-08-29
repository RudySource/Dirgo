use std::fs;

use dirgo::{
    config::SuggestionsConfig,
    suggestions::{
        CommandHistoryEntry, CommandHistoryEventV2, CommandHistoryStore, CommandOutcome,
        HISTORY_SCHEMA_VERSION, read_command_history,
    },
};
use redb::{Database, ReadableDatabase, TableDefinition};

const V1_HISTORY: TableDefinition<&str, &[u8]> = TableDefinition::new("command_history");
const HISTORY_META: TableDefinition<&str, u64> = TableDefinition::new("history_meta");

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

#[test]
fn schema_v2_records_full_context_and_rebuilds_aggregates_after_pruning() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let project = temp.path().join("project");
    let nested = project.join("src");
    std::fs::create_dir_all(&nested).expect("project cwd");
    let config = SuggestionsConfig {
        command_history: true,
        retention_entries: 2,
        retention_days: 180,
        ..SuggestionsConfig::default()
    };
    let store = CommandHistoryStore::open(&path).expect("store");

    for (started_at, exit_code, duration_ms, session_id) in [
        (100, Some(0), Some(20), Some("session-a")),
        (110, Some(1), Some(30), Some("session-a")),
        (120, None, None, Some("session-b")),
    ] {
        store
            .record_event(
                CommandHistoryEventV2 {
                    id: 0,
                    command: "cargo test".into(),
                    started_at,
                    duration_ms,
                    cwd: nested.clone(),
                    project_root: Some(project.clone()),
                    exit_code,
                    outcome: CommandOutcome::from_exit_code(exit_code),
                    session_id: session_id.map(str::to_owned),
                },
                &config,
            )
            .expect("record event");
    }

    let events = store.all_events().expect("events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].id, 2);
    assert_eq!(events[0].exit_code, Some(1));
    assert_eq!(events[1].session_id.as_deref(), Some("session-b"));

    let aggregates = store.all_aggregates().expect("aggregates");
    assert_eq!(aggregates.len(), 1);
    let aggregate = &aggregates[0];
    assert_eq!(
        aggregate.scope_key,
        format!("project:{}", project.display())
    );
    assert_eq!(aggregate.use_count, 2);
    assert_eq!(aggregate.success_count, 0);
    assert_eq!(aggregate.failure_count, 1);
    assert_eq!(aggregate.unknown_count, 1);
    assert_eq!(aggregate.last_failure, Some(110));
    assert_eq!(aggregate.total_duration_ms, 30);
    assert_eq!(aggregate.measured_duration_count, 1);
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

#[cfg(unix)]
#[test]
fn command_history_refuses_a_symlink_database_path() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().expect("tempdir");
    let target = temp.path().join("target.redb");
    let link = temp.path().join("suggestions.redb");
    let _target_store = CommandHistoryStore::open(&target).expect("target");
    symlink(&target, &link).expect("symlink");
    let error = CommandHistoryStore::open(&link)
        .err()
        .expect("reject symlink")
        .to_string();
    assert!(error.contains("symlink"), "{error}");
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

#[test]
fn populated_v1_migrates_once_without_inventing_context_or_events() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    write_v1_history(
        &path,
        &[
            CommandHistoryEntry::new("cargo test", 4, 120),
            CommandHistoryEntry::new("git status", 2, 110),
        ],
    );

    let store = CommandHistoryStore::open(&path).expect("migrate v1");
    assert_eq!(
        store.schema_version().expect("schema"),
        HISTORY_SCHEMA_VERSION
    );
    let aggregates = store.all_aggregates().expect("aggregates");
    assert_eq!(aggregates.len(), 2);
    assert_eq!(aggregates[0].scope_key, "legacy_global");
    assert_eq!(aggregates[0].command, "cargo test");
    assert_eq!(aggregates[0].use_count, 4);
    assert_eq!(aggregates[0].unknown_count, 4);
    assert_eq!(aggregates[0].success_count, 0);
    assert_eq!(aggregates[0].failure_count, 0);
    assert!(store.all_events().expect("events").is_empty());
    drop(store);

    let reopened = CommandHistoryStore::open(&path).expect("schema 2 reopen");
    assert_eq!(
        reopened.all_aggregates().expect("reopened rows"),
        aggregates
    );
    drop(reopened);

    let db = redb::ReadOnlyDatabase::open(&path).expect("read migrated db");
    let read = db.begin_read().expect("read transaction");
    assert!(matches!(
        read.open_table(V1_HISTORY),
        Err(redb::TableError::TableDoesNotExist(_))
    ));
}

#[test]
fn future_history_schema_is_rejected_without_overwriting_the_database() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let db = Database::create(&path).expect("future db");
    let write = db.begin_write().expect("write");
    {
        let mut meta = write.open_table(HISTORY_META).expect("meta");
        meta.insert("schema_version", HISTORY_SCHEMA_VERSION + 1)
            .expect("future version");
    }
    write.commit().expect("commit future db");
    drop(db);
    let before = std::fs::read(&path).expect("future bytes");

    let error = CommandHistoryStore::open(&path)
        .err()
        .expect("future schema must fail")
        .to_string();
    assert!(error.contains("history schema version 3 is unsupported"));
    assert_eq!(std::fs::read(&path).expect("unchanged bytes"), before);
    assert_eq!(
        std::fs::read_dir(temp.path())
            .expect("directory")
            .filter_map(Result::ok)
            .count(),
        1
    );
}

#[test]
fn malformed_v1_row_fails_closed_and_preserves_a_private_recovery_copy() {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let db = Database::create(&path).expect("v1 db");
    let write = db.begin_write().expect("write");
    {
        let mut table = write.open_table(V1_HISTORY).expect("v1 table");
        table
            .insert("broken", b"{not-json".as_slice())
            .expect("malformed row");
    }
    write.commit().expect("commit");
    drop(db);
    let original = fs::read(&path).expect("original bytes");

    let error = CommandHistoryStore::open(&path)
        .err()
        .expect("migration failure")
        .to_string();
    assert!(error.contains("invalid"), "{error}");
    let recovery = fs::read_dir(temp.path())
        .expect("dir")
        .filter_map(Result::ok)
        .find(|entry| entry.file_name().to_string_lossy().contains("recovery"))
        .expect("recovery copy")
        .path();
    assert_eq!(fs::read(&recovery).expect("recovery bytes"), original);
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(recovery)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

fn write_v1_history(path: &std::path::Path, entries: &[CommandHistoryEntry]) {
    let db = Database::create(path).expect("v1 db");
    let write = db.begin_write().expect("write v1");
    {
        let mut table = write.open_table(V1_HISTORY).expect("v1 table");
        for entry in entries {
            let encoded = serde_json::to_vec(entry).expect("v1 entry json");
            table
                .insert(entry.command.as_str(), encoded.as_slice())
                .expect("insert v1 entry");
        }
    }
    write.commit().expect("commit v1");
}
