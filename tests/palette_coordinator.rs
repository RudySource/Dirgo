use std::{collections::HashMap, path::PathBuf, time::Duration};

use dirgo::palette::{
    PaletteAction, PaletteCoordinator, PaletteItem, PaletteSource, ProviderBatch, ProviderBudget,
    ProviderState,
};

fn item(source: PaletteSource, id: &str) -> PaletteItem {
    PaletteItem {
        id: format!("{}:{id}", source.as_str()),
        source,
        title: id.into(),
        subtitle: format!("{id} detail"),
        insert_text: Some(format!("insert-{id}")),
        preview_key: Some(format!("preview-{id}")),
        action: PaletteAction::Insert {
            text: format!("insert-{id}"),
        },
        score: 100,
    }
}

#[test]
fn provider_budgets_cap_results_and_all_source_merges_fairly() {
    let budgets = HashMap::from([
        (
            PaletteSource::Files,
            ProviderBudget::new(2, Duration::from_millis(20)),
        ),
        (
            PaletteSource::Tasks,
            ProviderBudget::new(1, Duration::from_millis(20)),
        ),
        (
            PaletteSource::Git,
            ProviderBudget::new(1, Duration::from_millis(20)),
        ),
    ]);
    let batches = vec![
        ProviderBatch::ready(
            PaletteSource::Files,
            vec![
                item(PaletteSource::Files, "file-a"),
                item(PaletteSource::Files, "file-b"),
                item(PaletteSource::Files, "file-c"),
            ],
            Duration::from_millis(2),
        ),
        ProviderBatch::ready(
            PaletteSource::Tasks,
            vec![
                item(PaletteSource::Tasks, "task-a"),
                item(PaletteSource::Tasks, "task-b"),
            ],
            Duration::from_millis(3),
        ),
        ProviderBatch::ready(
            PaletteSource::Git,
            vec![item(PaletteSource::Git, "branch-a")],
            Duration::from_millis(4),
        ),
    ];

    let snapshot = PaletteCoordinator::new(budgets).merge(batches);

    assert_eq!(snapshot.items(PaletteSource::Files).len(), 2);
    assert_eq!(snapshot.items(PaletteSource::Tasks).len(), 1);
    let all = snapshot.items(PaletteSource::All);
    assert_eq!(all.len(), 4);
    assert_eq!(
        all.iter().map(|item| item.source).collect::<Vec<_>>(),
        vec![
            PaletteSource::Files,
            PaletteSource::Tasks,
            PaletteSource::Git,
            PaletteSource::Files,
        ]
    );
}

#[test]
fn duplicate_ids_are_deterministic_and_slow_or_failed_providers_do_not_block_others() {
    let budgets = HashMap::from([
        (
            PaletteSource::Files,
            ProviderBudget::new(4, Duration::from_millis(5)),
        ),
        (
            PaletteSource::Tasks,
            ProviderBudget::new(4, Duration::from_millis(5)),
        ),
        (
            PaletteSource::Places,
            ProviderBudget::new(4, Duration::from_millis(5)),
        ),
    ]);
    let duplicate = item(PaletteSource::Files, "same");
    let batches = vec![
        ProviderBatch::ready(
            PaletteSource::Files,
            vec![duplicate.clone(), duplicate],
            Duration::from_millis(12),
        ),
        ProviderBatch::failed(PaletteSource::Tasks, "manifest unavailable"),
        ProviderBatch::ready(
            PaletteSource::Places,
            vec![item(PaletteSource::Places, "bookmark")],
            Duration::from_millis(1),
        ),
    ];

    let snapshot = PaletteCoordinator::new(budgets).merge(batches);

    assert_eq!(snapshot.items(PaletteSource::Files).len(), 1);
    assert_eq!(
        snapshot.state(PaletteSource::Files),
        ProviderState::TimedOut
    );
    assert_eq!(snapshot.state(PaletteSource::Tasks), ProviderState::Failed);
    assert_eq!(snapshot.items(PaletteSource::Tasks).len(), 0);
    assert_eq!(snapshot.state(PaletteSource::Places), ProviderState::Ready);
    assert_eq!(snapshot.items(PaletteSource::All).len(), 2);
}

#[test]
fn source_switching_order_is_stable_and_actions_are_explicit_data() {
    assert_eq!(PaletteSource::All.next(), PaletteSource::Files);
    assert_eq!(PaletteSource::Files.previous(), PaletteSource::All);
    assert_eq!(PaletteSource::Places.next(), PaletteSource::All);

    let navigate = PaletteAction::Navigate {
        path: PathBuf::from("/work/project"),
    };
    let insert = PaletteAction::Insert {
        text: "cargo test".into(),
    };
    assert!(matches!(navigate, PaletteAction::Navigate { .. }));
    assert!(matches!(insert, PaletteAction::Insert { .. }));
}
