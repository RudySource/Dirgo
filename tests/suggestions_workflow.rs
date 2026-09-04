use std::collections::HashMap;

use dirgo::{
    config::RankingConfig,
    suggestions::{
        CommandCatalog, CommandHistoryEventV2, CommandOutcome, ShellKind, SuggestionData,
        SuggestionEngine, SuggestionPresentation, SuggestionRequest, SuggestionSource,
        WorkflowSnapshot, apply_text_edit,
    },
    workflows::{SavedWorkflowV1, WorkflowStep},
};

#[test]
fn next_is_prefix_gated_inserts_only_text_and_preserves_the_right_buffer() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='0.1.0'\n",
    )
    .expect("marker");
    let project = project.canonicalize().expect("canonical project");
    let workflow = SavedWorkflowV1 {
        id: 7,
        name: "Quality gate".into(),
        scope_key: format!("project:{}", project.display()),
        steps: vec![
            WorkflowStep {
                command: "cargo fmt".into(),
            },
            WorkflowStep {
                command: "cargo test".into(),
            },
        ],
        created_at: 1,
        updated_at: 1,
    };
    let event = CommandHistoryEventV2 {
        id: 1,
        command: "cargo fmt".into(),
        started_at: 1,
        duration_ms: Some(1),
        cwd: project.clone(),
        project_root: Some(project.clone()),
        exit_code: Some(0),
        outcome: CommandOutcome::Success,
        session_id: Some("active-shell".into()),
    };
    let engine = SuggestionEngine::new(SuggestionData {
        records: Vec::new(),
        bookmarks: HashMap::new(),
        navigation_history: HashMap::new(),
        ranking: RankingConfig::default(),
        catalog: CommandCatalog::default(),
        command_history: Vec::new(),
        workflow: Some(WorkflowSnapshot::new(
            Vec::new(),
            vec![workflow],
            vec![event],
            "active-shell",
        )),
    });
    let request = |before: &str| SuggestionRequest {
        protocol_version: 2,
        request_id: 1,
        shell: ShellKind::Zsh,
        cwd: project.clone(),
        before_cursor: before.into(),
        after_cursor: " && echo done".into(),
        max_results: 8,
        terminal_rows: Some(24),
        terminal_columns: Some(100),
        presentation: SuggestionPresentation::List,
    };

    assert!(engine.suggest(&request("")).is_empty());
    let suggestions = engine.suggest(&request("cargo t"));
    let next = suggestions
        .iter()
        .find(|item| item.source == SuggestionSource::Workflow)
        .expect("NEXT candidate");
    assert_eq!(next.edit.replacement, "cargo test");
    assert!(
        next.description
            .as_deref()
            .unwrap()
            .contains("never executed")
    );
    assert_eq!(
        apply_text_edit("cargo t", " && echo done", &next.edit).as_deref(),
        Some("cargo test && echo done")
    );
}
