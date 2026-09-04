use std::{collections::BTreeSet, time::Duration};

use dirgo::{
    palette::{PaletteAction, PaletteSource, ProviderBudget, providers},
    suggestions::{CommandHistoryEventV2, CommandOutcome, WorkflowSnapshot},
    workflows::{SavedWorkflowV1, WorkflowScope, WorkflowStep},
};

#[test]
fn workflows_follow_tasks_and_insert_exactly_one_next_step() {
    assert_eq!(PaletteSource::Tasks.next(), PaletteSource::Workflows);
    assert_eq!(PaletteSource::Workflows.next(), PaletteSource::Git);

    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='0.1.0'\n",
    )
    .expect("marker");
    let project = project.canonicalize().expect("canonical");
    let steps = vec![
        WorkflowStep {
            command: "cargo fmt".into(),
        },
        WorkflowStep {
            command: "cargo test".into(),
        },
        WorkflowStep {
            command: "cargo clippy".into(),
        },
    ];
    let snapshot = WorkflowSnapshot::new(
        Vec::new(),
        vec![SavedWorkflowV1 {
            id: 9,
            name: "Quality".into(),
            scope_key: format!("project:{}", project.display()),
            steps: steps.clone(),
            created_at: 1,
            updated_at: 1,
        }],
        vec![CommandHistoryEventV2 {
            id: 1,
            command: "cargo fmt".into(),
            started_at: 1,
            duration_ms: None,
            cwd: project.clone(),
            project_root: Some(project.clone()),
            exit_code: Some(0),
            outcome: CommandOutcome::Success,
            session_id: Some("active".into()),
        }],
        "active",
    );
    let batch = providers::workflows(
        &snapshot,
        WorkflowScope::Project(project),
        &BTreeSet::new(),
        ProviderBudget::new(64, Duration::from_millis(20)),
    );
    assert_eq!(batch.source, PaletteSource::Workflows);
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].title, "Quality");
    assert!(matches!(
        &batch.items[0].action,
        PaletteAction::Insert { text } if text == "cargo test"
    ));
    let preview = batch.items[0].workflow_preview.as_ref().expect("preview");
    assert_eq!(preview.next_index, 1);
    assert_eq!(
        preview.steps,
        steps
            .iter()
            .map(|step| step.command.clone())
            .collect::<Vec<_>>()
    );
}
