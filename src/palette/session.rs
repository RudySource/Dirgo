use std::time::{Duration, Instant};

use super::{PaletteItem, PaletteSnapshot, PaletteSource, ProviderState};

const PREVIEW_DEBOUNCE: Duration = Duration::from_millis(90);

#[derive(Debug, Clone)]
pub struct PreviewRequest {
    pub generation: u64,
    pub key: String,
    pub item: PaletteItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewResponse {
    pub generation: u64,
    pub key: String,
    pub lines: Vec<String>,
}

pub struct PaletteSession {
    snapshot: PaletteSnapshot,
    source: PaletteSource,
    query: String,
    visible: Vec<PaletteItem>,
    selected: usize,
    generation: u64,
    selection_changed_at: Instant,
    requested: Option<(u64, String)>,
    preview: Option<PreviewResponse>,
}

impl PaletteSession {
    pub fn new(snapshot: PaletteSnapshot, query: String, now: Instant) -> Self {
        let mut session = Self {
            snapshot,
            source: PaletteSource::All,
            query,
            visible: Vec::new(),
            selected: 0,
            generation: 0,
            selection_changed_at: now,
            requested: None,
            preview: None,
        };
        session.refresh_visible(now);
        session
    }

    pub fn source(&self) -> PaletteSource {
        self.source
    }

    pub fn provider_state(&self, source: PaletteSource) -> ProviderState {
        self.snapshot.state(source)
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn visible(&self) -> &[PaletteItem] {
        &self.visible
    }

    pub fn selected(&self) -> Option<&PaletteItem> {
        self.visible.get(self.selected)
    }

    pub fn selected_index(&self) -> Option<usize> {
        (!self.visible.is_empty()).then_some(self.selected)
    }

    pub fn preview(&self) -> Option<&PreviewResponse> {
        self.preview.as_ref()
    }

    pub fn switch_next(&mut self, now: Instant) {
        self.source = self.source.next();
        self.refresh_visible(now);
    }

    pub fn switch_previous(&mut self, now: Instant) {
        self.source = self.source.previous();
        self.refresh_visible(now);
    }

    pub fn set_query(&mut self, query: String, now: Instant) {
        self.query = query;
        self.refresh_visible(now);
    }

    pub fn move_selection(&mut self, amount: isize, now: Instant) {
        if self.visible.is_empty() {
            return;
        }
        let last = self.visible.len().saturating_sub(1) as isize;
        let next = (self.selected as isize + amount).clamp(0, last) as usize;
        if next != self.selected {
            self.selected = next;
            self.invalidate_preview(now);
        }
    }

    pub fn preview_request(&mut self, now: Instant) -> Option<PreviewRequest> {
        if now.saturating_duration_since(self.selection_changed_at) < PREVIEW_DEBOUNCE {
            return None;
        }
        let item = self.selected()?.clone();
        let key = item.preview_key.clone()?;
        if self.requested.as_ref() == Some(&(self.generation, key.clone())) {
            return None;
        }
        self.requested = Some((self.generation, key.clone()));
        Some(PreviewRequest {
            generation: self.generation,
            key,
            item,
        })
    }

    pub fn accept_preview(&mut self, response: PreviewResponse) -> bool {
        let selected_key = self.selected().and_then(|item| item.preview_key.as_deref());
        if response.generation != self.generation || selected_key != Some(&response.key) {
            return false;
        }
        self.preview = Some(response);
        true
    }

    fn refresh_visible(&mut self, now: Instant) {
        let query = self.query.trim();
        self.visible = self
            .snapshot
            .items(self.source)
            .iter()
            .filter(|item| matches_query(item, query))
            .cloned()
            .collect();
        self.selected = 0;
        self.invalidate_preview(now);
    }

    fn invalidate_preview(&mut self, now: Instant) {
        self.generation = self.generation.wrapping_add(1);
        self.selection_changed_at = now;
        self.requested = None;
        self.preview = None;
    }
}

fn matches_query(item: &PaletteItem, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    let haystack = format!("{} {}", item.title, item.subtitle).to_lowercase();
    haystack.contains(&query) || subsequence(&haystack, &query)
}

fn subsequence(haystack: &str, needle: &str) -> bool {
    let mut needle = needle.chars();
    let mut expected = needle.next();
    for character in haystack.chars() {
        if expected == Some(character) {
            expected = needle.next();
            if expected.is_none() {
                return true;
            }
        }
    }
    false
}
