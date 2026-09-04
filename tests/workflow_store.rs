use std::path::PathBuf;

use dirgo::{
    config::SuggestionsConfig,
    suggestions::{
        CommandHistoryAggregateV2, CommandHistoryEventV2, CommandHistoryStore, CommandOutcome,
    },
    workflows::{
        NextAction, SavedWorkflowV1, WORKFLOW_SCHEMA_VERSION, WorkflowScope, WorkflowSource,
        WorkflowStep, WorkflowStore, WorkflowTransitionV1, rebuild_transitions,
    },
};
use redb::{Database, ReadableDatabase, ReadableTableMetadata, TableDefinition};

const HISTORY_META: TableDefinition<&str, u64> = TableDefinition::new("history_meta");
const EVENTS: TableDefinition<u64, &[u8]> = TableDefinition::new("command_events_v2");
const AGGREGATES: TableDefinition<&str, &[u8]> = TableDefinition::new("command_aggregates_v2");
const TRANSITIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("workflow_transitions_v1");
const SAVED_WORKFLOWS: TableDefinition<u64, &[u8]> = TableDefinition::new("saved_workflows_v1");

#[test]
fn workflow_domain_types_round_trip_without_losing_scope_or_evidence() {
    let transition = WorkflowTransitionV1 {
        scope_key: "project:/fixture/alpha".into(),
        predecessors: vec!["cargo fmt".into(), "cargo clippy".into()],
        predecessor_outcome: CommandOutcome::Success,
        next_command: "cargo test".into(),
        observations: 6,
        evidence_sessions: vec!["session-a".into(), "session-b".into()],
        next_successes: 5,
        next_failures: 1,
        next_unknown: 0,
        first_seen: 100,
        last_seen: 200,
    };
    let encoded = serde_json::to_vec(&transition).expect("encode transition");
    assert_eq!(
        serde_json::from_slice::<WorkflowTransitionV1>(&encoded).expect("decode transition"),
        transition
    );

    let saved = SavedWorkflowV1 {
        id: 7,
        name: "Verify before push".into(),
        scope_key: "project:/fixture/alpha".into(),
        steps: vec![
            WorkflowStep {
                command: "cargo fmt".into(),
            },
            WorkflowStep {
                command: "cargo test".into(),
            },
        ],
        created_at: 300,
        updated_at: 300,
    };
    let encoded = serde_json::to_vec(&saved).expect("encode saved workflow");
    assert_eq!(
        serde_json::from_slice::<SavedWorkflowV1>(&encoded).expect("decode saved workflow"),
        saved
    );

    let action = NextAction {
        command: "cargo test".into(),
        source: WorkflowSource::Learned,
        workflow_id: None,
        confidence: 840,
        reason: "Next in this project · 6 times · 83% successful".into(),
    };
    assert_eq!(action.confidence, 840);
    assert_eq!(
        WorkflowScope::Project(PathBuf::from("/fixture/alpha")),
        WorkflowScope::Project(PathBuf::from("/fixture/alpha"))
    );
    assert_ne!(
        WorkflowScope::Global,
        WorkflowScope::Project(PathBuf::new())
    );
}

#[test]
fn transition_rebuild_is_deterministic_and_uses_one_and_two_command_contexts() {
    let project = PathBuf::from("/fixture/alpha");
    let events = [
        event(1, "cargo fmt", 100, &project, "session-a", Some(0)),
        event(2, "cargo clippy", 110, &project, "session-a", Some(0)),
        event(3, "cargo test", 120, &project, "session-a", Some(0)),
        event(4, "cargo fmt", 200, &project, "session-b", Some(0)),
        event(5, "cargo clippy", 210, &project, "session-b", Some(0)),
        event(6, "cargo test", 220, &project, "session-b", Some(1)),
    ];

    let forward = rebuild_transitions(events.iter().cloned()).expect("forward rebuild");
    let reverse = rebuild_transitions(events.iter().rev().cloned()).expect("reverse rebuild");
    assert_eq!(forward, reverse);
    assert_eq!(forward.len(), 3);

    let two_step = forward
        .iter()
        .find(|transition| transition.predecessors.len() == 2)
        .expect("two-command predecessor");
    assert_eq!(two_step.predecessors, ["cargo fmt", "cargo clippy"]);
    assert_eq!(two_step.next_command, "cargo test");
    assert_eq!(two_step.observations, 2);
    assert_eq!(two_step.evidence_sessions, ["session-a", "session-b"]);
    assert_eq!(two_step.next_successes, 1);
    assert_eq!(two_step.next_failures, 1);
    assert_eq!(two_step.predecessor_outcome, CommandOutcome::Success);
}

#[test]
fn transition_rebuild_breaks_at_session_scope_time_and_retention_gaps() {
    let alpha = PathBuf::from("/fixture/alpha");
    let beta = PathBuf::from("/fixture/beta");
    let events = vec![
        event(1, "one", 100, &alpha, "session-a", Some(0)),
        event(2, "other session", 110, &alpha, "session-b", Some(0)),
        event(3, "other project", 120, &beta, "session-b", Some(0)),
        event(4, "long gap", 2_000, &beta, "session-b", Some(0)),
        event(6, "retention gap", 2_010, &beta, "session-b", Some(0)),
        CommandHistoryEventV2 {
            session_id: None,
            ..event(7, "missing session", 2_020, &beta, "unused", Some(0))
        },
    ];

    assert!(
        rebuild_transitions(events)
            .expect("boundary rebuild")
            .is_empty()
    );
}

#[test]
fn schema_two_migrates_atomically_without_rewriting_existing_rows() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let event = CommandHistoryEventV2 {
        id: 7,
        command: "cargo test".into(),
        started_at: 100,
        duration_ms: Some(20),
        cwd: PathBuf::from("/fixture/alpha"),
        project_root: Some(PathBuf::from("/fixture/alpha")),
        exit_code: Some(0),
        outcome: CommandOutcome::Success,
        session_id: Some("session-a".into()),
    };
    let aggregate = CommandHistoryAggregateV2 {
        scope_key: "project:/fixture/alpha".into(),
        command: event.command.clone(),
        use_count: 1,
        success_count: 1,
        failure_count: 0,
        unknown_count: 0,
        last_used: 100,
        last_success: Some(100),
        last_failure: None,
        total_duration_ms: 20,
        measured_duration_count: 1,
    };
    let event_bytes = serde_json::to_vec(&event).expect("event json");
    let aggregate_bytes = serde_json::to_vec(&aggregate).expect("aggregate json");
    write_schema_two(&path, &event_bytes, &aggregate_bytes);

    let store = WorkflowStore::open(&path).expect("migrate schema 2");
    assert_eq!(
        store.schema_version().expect("schema"),
        WORKFLOW_SCHEMA_VERSION
    );
    assert!(store.transitions().expect("transitions").is_empty());
    assert!(store.saved_workflows().expect("saved workflows").is_empty());
    drop(store);

    let db = redb::ReadOnlyDatabase::open(&path).expect("read migrated database");
    let read = db.begin_read().expect("read transaction");
    let meta = read.open_table(HISTORY_META).expect("meta");
    assert_eq!(
        meta.get("next_saved_workflow_id")
            .expect("saved id read")
            .expect("saved id")
            .value(),
        1
    );
    assert_eq!(
        meta.get("last_workflow_rebuild")
            .expect("rebuild read")
            .expect("rebuild marker")
            .value(),
        0
    );
    drop(meta);
    let events = read.open_table(EVENTS).expect("events");
    assert_eq!(
        events.get(7).expect("event read").expect("event").value(),
        event_bytes
    );
    let aggregates = read.open_table(AGGREGATES).expect("aggregates");
    assert_eq!(
        aggregates
            .get("fixture")
            .expect("aggregate read")
            .expect("aggregate")
            .value(),
        aggregate_bytes
    );
    drop(aggregates);
    drop(events);
    drop(read);
    drop(db);

    let reopened = WorkflowStore::open(&path).expect("idempotent schema 3 reopen");
    assert_eq!(
        reopened.schema_version().expect("reopened schema"),
        WORKFLOW_SCHEMA_VERSION
    );
}

#[test]
fn clearing_command_history_also_clears_derived_transitions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let project = temp.path().join("alpha");
    let config = SuggestionsConfig {
        command_history: true,
        workflow_suggestions: true,
        ..SuggestionsConfig::default()
    };
    let history = CommandHistoryStore::open(&path).expect("history");
    history
        .record_event(
            event(1, "cargo fmt", 100, &project, "session-a", Some(0)),
            &config,
        )
        .expect("first event");
    history
        .record_event(
            event(2, "cargo test", 110, &project, "session-a", Some(0)),
            &config,
        )
        .expect("second event");
    drop(history);
    assert_eq!(
        WorkflowStore::open(&path)
            .expect("workflow store")
            .transitions()
            .expect("transitions")
            .len(),
        1
    );

    let history = CommandHistoryStore::open(&path).expect("history reopen");
    history.clear().expect("clear history");
    drop(history);
    assert!(
        WorkflowStore::open(&path)
            .expect("workflow store")
            .transitions()
            .expect("cleared transitions")
            .is_empty()
    );
}

#[test]
fn interrupted_schema_three_transaction_leaves_schema_two_readable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    write_schema_two(&path, br#"{"id":7}"#, br#"{"command":"fixture"}"#);

    let db = Database::create(&path).expect("open schema 2");
    let write = db.begin_write().expect("begin interrupted migration");
    let _transitions = write.open_table(TRANSITIONS).expect("transition table");
    let _saved = write.open_table(SAVED_WORKFLOWS).expect("saved table");
    drop(_saved);
    drop(_transitions);
    drop(write);
    drop(db);

    let db = redb::ReadOnlyDatabase::open(&path).expect("schema 2 remains readable");
    let read = db.begin_read().expect("read schema");
    assert_eq!(
        read.open_table(HISTORY_META)
            .expect("meta")
            .get("schema_version")
            .expect("schema read")
            .expect("schema")
            .value(),
        2
    );
    drop(read);
    drop(db);

    assert_eq!(
        WorkflowStore::open(&path)
            .expect("retry migration")
            .schema_version()
            .expect("schema"),
        WORKFLOW_SCHEMA_VERSION
    );
}

#[test]
fn future_schema_is_preserved_and_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let db = Database::create(&path).expect("future database");
    let write = db.begin_write().expect("future write");
    {
        let mut meta = write.open_table(HISTORY_META).expect("meta");
        meta.insert("schema_version", WORKFLOW_SCHEMA_VERSION + 1)
            .expect("future schema");
    }
    write.commit().expect("commit future schema");
    drop(db);
    let before = std::fs::read(&path).expect("future bytes");

    let error = WorkflowStore::open(&path)
        .err()
        .expect("future schema rejected")
        .to_string();
    assert!(error.contains("schema version 4 is unsupported"), "{error}");
    assert_eq!(std::fs::read(&path).expect("preserved bytes"), before);
}

#[test]
fn malformed_transition_is_reported_without_mutating_the_database() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let store = WorkflowStore::open(&path).expect("schema 3 store");
    drop(store);
    let db = Database::create(&path).expect("open schema 3");
    let write = db.begin_write().expect("write malformed row");
    {
        let mut transitions = write.open_table(TRANSITIONS).expect("transitions");
        transitions
            .insert("broken", b"{not-json".as_slice())
            .expect("malformed row");
    }
    write.commit().expect("commit malformed row");
    drop(db);
    let store = WorkflowStore::open(&path).expect("open malformed store");
    let error = store
        .transitions()
        .expect_err("malformed transition rejected")
        .to_string();
    assert!(error.contains("invalid"), "{error}");
    drop(store);
    let db = redb::ReadOnlyDatabase::open(&path).expect("read preserved row");
    let read = db.begin_read().expect("read transaction");
    let transitions = read.open_table(TRANSITIONS).expect("transitions");
    assert_eq!(
        transitions
            .get("broken")
            .expect("row read")
            .expect("malformed row retained")
            .value(),
        b"{not-json"
    );
}

#[cfg(unix)]
#[test]
fn workflow_store_is_private_and_refuses_symlink_database_paths() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let temp = tempfile::tempdir().expect("tempdir");
    let target = temp.path().join("target.redb");
    let link = temp.path().join("link.redb");
    let store = WorkflowStore::open(&target).expect("target store");
    drop(store);
    assert_eq!(
        std::fs::metadata(&target)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    symlink(&target, &link).expect("symlink");
    let error = WorkflowStore::open(&link)
        .err()
        .expect("symlink rejected")
        .to_string();
    assert!(error.contains("symlink"), "{error}");
}

#[test]
fn store_rebuild_publishes_learned_transitions_without_changing_saved_workflows() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let project = temp.path().join("alpha");
    let config = SuggestionsConfig {
        command_history: true,
        ..SuggestionsConfig::default()
    };
    let history = CommandHistoryStore::open(&path).expect("history store");
    for row in [
        event(1, "cargo fmt", 100, &project, "session-a", Some(0)),
        event(2, "cargo clippy", 110, &project, "session-a", Some(0)),
        event(3, "cargo test", 120, &project, "session-a", Some(0)),
    ] {
        history.record_event(row, &config).expect("record event");
    }
    drop(history);

    let db = Database::create(&path).expect("open database");
    let write = db.begin_write().expect("saved write");
    let saved = SavedWorkflowV1 {
        id: 9,
        name: "Local verification".into(),
        scope_key: format!("project:{}", project.display()),
        steps: vec![
            WorkflowStep {
                command: "cargo fmt".into(),
            },
            WorkflowStep {
                command: "cargo test".into(),
            },
        ],
        created_at: 90,
        updated_at: 90,
    };
    {
        let mut table = write.open_table(SAVED_WORKFLOWS).expect("saved table");
        let encoded = serde_json::to_vec(&saved).expect("saved json");
        table
            .insert(saved.id, encoded.as_slice())
            .expect("saved row");
    }
    write.commit().expect("commit saved row");
    drop(db);

    let store = WorkflowStore::open(&path).expect("workflow store");
    assert_eq!(store.rebuild_transitions(120).expect("rebuild"), 3);
    assert_eq!(store.transitions().expect("transitions").len(), 3);
    assert_eq!(store.saved_workflows().expect("saved workflows"), [saved]);
}

#[test]
fn rebuild_caps_transitions_and_evidence_sessions() {
    let project = PathBuf::from("/fixture/cap");
    let mut repeated = Vec::new();
    for session in 0..9_u64 {
        repeated.push(event(
            session * 2 + 1,
            "cargo fmt",
            session * 100,
            &project,
            &format!("session-{session}"),
            Some(0),
        ));
        repeated.push(event(
            session * 2 + 2,
            "cargo test",
            session * 100 + 1,
            &project,
            &format!("session-{session}"),
            Some(0),
        ));
    }
    let rows = rebuild_transitions(repeated).expect("evidence rebuild");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].observations, 9);
    assert_eq!(rows[0].evidence_sessions.len(), 8);

    let unique = (0..10_100_u64)
        .map(|index| {
            event(
                index + 1,
                &format!("command-{index}"),
                index,
                &project,
                "one-session",
                Some(0),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rebuild_transitions(unique).expect("bounded rebuild").len(),
        10_000
    );
}

#[test]
fn structurally_invalid_transition_rows_fail_closed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let store = WorkflowStore::open(&path).expect("workflow store");
    drop(store);
    let invalid = WorkflowTransitionV1 {
        scope_key: "global".into(),
        predecessors: Vec::new(),
        predecessor_outcome: CommandOutcome::Success,
        next_command: "cargo test".into(),
        observations: 1,
        evidence_sessions: vec!["session-a".into()],
        next_successes: 1,
        next_failures: 0,
        next_unknown: 0,
        first_seen: 100,
        last_seen: 100,
    };
    let db = Database::create(&path).expect("database");
    let write = db.begin_write().expect("write invalid row");
    {
        let mut table = write.open_table(TRANSITIONS).expect("transitions");
        let encoded = serde_json::to_vec(&invalid).expect("invalid json");
        table
            .insert("invalid", encoded.as_slice())
            .expect("invalid row");
    }
    write.commit().expect("commit invalid row");
    drop(db);

    let error = WorkflowStore::open(&path)
        .expect("store")
        .transitions()
        .expect_err("invalid row rejected")
        .to_string();
    assert!(error.contains("workflow transition"), "{error}");
}

#[test]
fn recording_derives_transitions_only_after_separate_workflow_opt_in() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let project = temp.path().join("alpha");
    let mut config = SuggestionsConfig {
        command_history: true,
        ..SuggestionsConfig::default()
    };
    assert!(!config.workflow_suggestions);

    let history = CommandHistoryStore::open(&path).expect("history");
    history
        .record_event(
            event(1, "cargo fmt", 100, &project, "session-a", Some(0)),
            &config,
        )
        .expect("first event");
    history
        .record_event(
            event(2, "cargo test", 110, &project, "session-a", Some(0)),
            &config,
        )
        .expect("second event");
    drop(history);
    assert!(
        WorkflowStore::open(&path)
            .expect("workflow store")
            .transitions()
            .expect("disabled transitions")
            .is_empty()
    );

    config.workflow_suggestions = true;
    let history = CommandHistoryStore::open(&path).expect("history reopen");
    history
        .record_event(
            event(3, "cargo fmt", 200, &project, "session-b", Some(0)),
            &config,
        )
        .expect("third event");
    history
        .record_event(
            event(4, "cargo test", 210, &project, "session-b", Some(0)),
            &config,
        )
        .expect("fourth event");
    drop(history);
    let transitions = WorkflowStore::open(&path)
        .expect("workflow store")
        .transitions()
        .expect("enabled transitions");
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0].observations, 2);
    assert_eq!(transitions[0].evidence_sessions.len(), 2);
}

#[test]
fn failed_transition_publication_rolls_back_the_completed_event() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("suggestions.redb");
    let config = SuggestionsConfig {
        command_history: true,
        workflow_suggestions: true,
        ..SuggestionsConfig::default()
    };
    let history = CommandHistoryStore::open(&path).expect("history");
    drop(history);
    let db = Database::create(&path).expect("database");
    let write = db.begin_write().expect("malformed write");
    {
        let mut events = write.open_table(EVENTS).expect("events");
        events
            .insert(1, b"{not-json".as_slice())
            .expect("malformed retained event");
        let mut meta = write.open_table(HISTORY_META).expect("meta");
        meta.insert("next_event_id", 2).expect("next event id");
    }
    write.commit().expect("commit malformed event");
    drop(db);

    let history = CommandHistoryStore::open(&path).expect("history reopen");
    let result = history.record_event(
        CommandHistoryEventV2 {
            id: 0,
            command: "cargo test".into(),
            started_at: 100,
            duration_ms: None,
            cwd: temp.path().to_path_buf(),
            project_root: Some(temp.path().to_path_buf()),
            exit_code: Some(0),
            outcome: CommandOutcome::Success,
            session_id: Some("session-a".into()),
        },
        &config,
    );
    assert!(result.is_err());
    drop(history);
    let db = redb::ReadOnlyDatabase::open(&path).expect("read after rollback");
    let read = db.begin_read().expect("read transaction");
    let events = read.open_table(EVENTS).expect("events");
    assert_eq!(events.len().expect("event count"), 1);
    assert!(events.get(2).expect("event read").is_none());
}

fn write_schema_two(path: &std::path::Path, event: &[u8], aggregate: &[u8]) {
    let db = Database::create(path).expect("schema 2 database");
    let write = db.begin_write().expect("schema 2 write");
    {
        let mut meta = write.open_table(HISTORY_META).expect("meta");
        meta.insert("schema_version", 2).expect("schema version");
        meta.insert("next_event_id", 8).expect("next event");
        meta.insert("last_successful_migration", 90)
            .expect("migration marker");
    }
    {
        let mut events = write.open_table(EVENTS).expect("events");
        events.insert(7, event).expect("event");
    }
    {
        let mut aggregates = write.open_table(AGGREGATES).expect("aggregates");
        aggregates.insert("fixture", aggregate).expect("aggregate");
    }
    write.commit().expect("commit schema 2");
}

fn event(
    id: u64,
    command: &str,
    started_at: u64,
    project: &std::path::Path,
    session: &str,
    exit_code: Option<i32>,
) -> CommandHistoryEventV2 {
    CommandHistoryEventV2 {
        id,
        command: command.into(),
        started_at,
        duration_ms: None,
        cwd: project.to_path_buf(),
        project_root: Some(project.to_path_buf()),
        exit_code,
        outcome: CommandOutcome::from_exit_code(exit_code),
        session_id: Some(session.into()),
    }
}
