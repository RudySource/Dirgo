use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};

use super::{PaletteItem, PaletteSource, ProviderState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderBudget {
    pub max_items: usize,
    pub deadline: Duration,
}

impl ProviderBudget {
    pub fn new(max_items: usize, deadline: Duration) -> Self {
        Self {
            max_items,
            deadline,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderBatch {
    pub source: PaletteSource,
    pub items: Vec<PaletteItem>,
    pub elapsed: Duration,
    pub error: Option<String>,
}

impl ProviderBatch {
    pub fn ready(source: PaletteSource, items: Vec<PaletteItem>, elapsed: Duration) -> Self {
        Self {
            source,
            items,
            elapsed,
            error: None,
        }
    }

    pub fn failed(source: PaletteSource, error: impl Into<String>) -> Self {
        Self {
            source,
            items: Vec::new(),
            elapsed: Duration::ZERO,
            error: Some(error.into()),
        }
    }
}

pub struct PaletteCoordinator {
    budgets: HashMap<PaletteSource, ProviderBudget>,
}

impl PaletteCoordinator {
    pub fn new(budgets: HashMap<PaletteSource, ProviderBudget>) -> Self {
        Self { budgets }
    }

    pub fn merge(&self, batches: Vec<ProviderBatch>) -> PaletteSnapshot {
        let mut by_source = HashMap::new();
        let mut states = HashMap::new();
        for batch in batches {
            let budget = self
                .budgets
                .get(&batch.source)
                .copied()
                .unwrap_or_else(|| ProviderBudget::new(64, Duration::from_millis(50)));
            let state = if batch.error.is_some() {
                ProviderState::Failed
            } else if batch.elapsed > budget.deadline {
                ProviderState::TimedOut
            } else {
                ProviderState::Ready
            };
            let mut seen = HashSet::new();
            let mut items = if state == ProviderState::Failed {
                Vec::new()
            } else {
                batch
                    .items
                    .into_iter()
                    .filter(|item| seen.insert(item.id.clone()))
                    .take(budget.max_items)
                    .collect::<Vec<_>>()
            };
            items.sort_by(|left, right| {
                right
                    .score
                    .cmp(&left.score)
                    .then_with(|| left.id.cmp(&right.id))
            });
            by_source.insert(batch.source, items);
            states.insert(batch.source, state);
        }
        let all = fair_merge(&by_source);
        PaletteSnapshot {
            all,
            by_source,
            states,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PaletteSnapshot {
    all: Vec<PaletteItem>,
    by_source: HashMap<PaletteSource, Vec<PaletteItem>>,
    states: HashMap<PaletteSource, ProviderState>,
}

impl PaletteSnapshot {
    pub fn items(&self, source: PaletteSource) -> &[PaletteItem] {
        if source == PaletteSource::All {
            &self.all
        } else {
            self.by_source.get(&source).map_or(&[], Vec::as_slice)
        }
    }

    pub fn state(&self, source: PaletteSource) -> ProviderState {
        self.states
            .get(&source)
            .copied()
            .unwrap_or(ProviderState::Failed)
    }
}

fn fair_merge(by_source: &HashMap<PaletteSource, Vec<PaletteItem>>) -> Vec<PaletteItem> {
    let mut queues = PaletteSource::FILTERS
        .into_iter()
        .filter(|source| *source != PaletteSource::All)
        .map(|source| {
            (
                source,
                by_source
                    .get(&source)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .collect::<VecDeque<_>>(),
            )
        })
        .collect::<Vec<_>>();
    let mut merged = Vec::new();
    loop {
        let mut changed = false;
        for (_, queue) in &mut queues {
            if let Some(item) = queue.pop_front() {
                merged.push(item);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    merged
}
