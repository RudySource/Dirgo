use std::time::Duration;

use dirgo::{
    palette::{
        PaletteAction, PaletteSource, ProviderBudget,
        providers::{compose, tasks},
    },
    suggestions::{ProjectCommand, ProjectCommandSnapshot},
};

fn snapshot() -> ProjectCommandSnapshot {
    ProjectCommandSnapshot::new(
        "/work/project".into(),
        vec![
            ProjectCommand::new(
                "npm run dev",
                "dev",
                "package.json script",
                "package-json:npm:dev",
            ),
            ProjectCommand::new("cargo test", "test", "Cargo task", "cargo:test"),
            ProjectCommand::new(
                "docker compose up api",
                "api",
                "Compose service",
                "compose-service:api",
            ),
        ],
    )
}

#[test]
fn tasks_and_compose_are_separate_bounded_sources_that_only_insert_text() {
    let snapshot = snapshot();
    let budget = ProviderBudget::new(8, Duration::from_secs(1));

    let tasks = tasks(&snapshot, budget);
    let compose = compose(&snapshot, budget);

    assert_eq!(tasks.source, PaletteSource::Tasks);
    assert_eq!(tasks.items.len(), 2);
    assert!(
        tasks
            .items
            .iter()
            .all(|item| item.source == PaletteSource::Tasks)
    );
    assert!(tasks.items.iter().all(|item| {
        matches!(
            &item.action,
            PaletteAction::Insert { text } if item.insert_text.as_deref() == Some(text)
        )
    }));
    assert_eq!(compose.source, PaletteSource::Compose);
    assert_eq!(compose.items.len(), 1);
    assert_eq!(compose.items[0].title, "api");
    assert_eq!(
        compose.items[0].action,
        PaletteAction::Insert {
            text: "docker compose up api".into()
        }
    );
}

#[test]
fn task_and_compose_budgets_are_hard_caps() {
    let snapshot = snapshot();
    let budget = ProviderBudget::new(1, Duration::from_secs(1));

    assert_eq!(tasks(&snapshot, budget).items.len(), 1);
    assert_eq!(compose(&snapshot, budget).items.len(), 1);
}
