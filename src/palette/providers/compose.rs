use std::time::Instant;

use crate::{
    palette::{PaletteAction, PaletteItem, PaletteSource, ProviderBatch, ProviderBudget},
    suggestions::ProjectCommandSnapshot,
};

pub fn compose(snapshot: &ProjectCommandSnapshot, budget: ProviderBudget) -> ProviderBatch {
    let started = Instant::now();
    let items = snapshot
        .commands()
        .iter()
        .filter(|command| command.stable_id.starts_with("compose-service:"))
        .take_while(|_| started.elapsed() < budget.deadline)
        .take(budget.max_items)
        .map(|command| PaletteItem {
            id: format!("compose:{}", command.stable_id),
            source: PaletteSource::Compose,
            title: command.display.clone(),
            subtitle: command.description.clone(),
            insert_text: Some(command.replacement.clone()),
            preview_key: Some(format!("compose:{}", command.stable_id)),
            workflow_preview: None,
            action: PaletteAction::Insert {
                text: command.replacement.clone(),
            },
            score: 24_000,
        })
        .collect();
    ProviderBatch::ready(PaletteSource::Compose, items, started.elapsed())
}
