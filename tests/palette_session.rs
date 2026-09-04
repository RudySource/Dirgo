use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use dirgo::palette::{
    PaletteAction, PaletteCoordinator, PaletteItem, PaletteSession, PaletteSource, PreviewResponse,
    ProviderBatch, ProviderBudget,
};

fn item(source: PaletteSource, title: &str) -> PaletteItem {
    PaletteItem {
        id: format!("{}:{title}", source.as_str()),
        source,
        title: title.into(),
        subtitle: format!("{title} detail"),
        insert_text: Some(title.into()),
        preview_key: Some(format!("preview:{title}")),
        workflow_preview: None,
        action: PaletteAction::Insert { text: title.into() },
        score: 100,
    }
}

fn snapshot() -> dirgo::palette::PaletteSnapshot {
    let budgets = HashMap::from([
        (
            PaletteSource::Files,
            ProviderBudget::new(8, Duration::from_secs(1)),
        ),
        (
            PaletteSource::Tasks,
            ProviderBudget::new(8, Duration::from_secs(1)),
        ),
    ]);
    PaletteCoordinator::new(budgets).merge(vec![
        ProviderBatch::ready(
            PaletteSource::Files,
            vec![item(PaletteSource::Files, "src/main.rs")],
            Duration::ZERO,
        ),
        ProviderBatch::ready(
            PaletteSource::Tasks,
            vec![item(PaletteSource::Tasks, "cargo test")],
            Duration::ZERO,
        ),
    ])
}

#[test]
fn switching_sources_preserves_query_and_uses_the_existing_snapshot() {
    let now = Instant::now();
    let mut session = PaletteSession::new(snapshot(), "cargo".into(), now);

    assert_eq!(session.source(), PaletteSource::All);
    assert_eq!(session.visible().len(), 1);
    assert_eq!(session.visible()[0].title, "cargo test");
    session.switch_next(now + Duration::from_millis(10));
    assert_eq!(session.source(), PaletteSource::Files);
    assert_eq!(session.query(), "cargo");
    assert!(session.visible().is_empty());
    session.switch_previous(now + Duration::from_millis(20));
    assert_eq!(session.source(), PaletteSource::All);
    assert_eq!(session.visible().len(), 1);
}

#[test]
fn lazy_preview_waits_for_debounce_and_rejects_a_stale_generation() {
    let now = Instant::now();
    let mut session = PaletteSession::new(snapshot(), String::new(), now);

    assert!(
        session
            .preview_request(now + Duration::from_millis(89))
            .is_none()
    );
    let request = session
        .preview_request(now + Duration::from_millis(90))
        .expect("debounced preview request");
    assert_eq!(request.key, "preview:src/main.rs");

    session.switch_next(now + Duration::from_millis(91));
    assert!(!session.accept_preview(PreviewResponse {
        generation: request.generation,
        key: request.key,
        lines: vec!["stale".into()],
    }));
    assert!(session.preview().is_none());
}
