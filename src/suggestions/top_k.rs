use std::cmp::Ordering;

use super::Suggestion;

pub struct TopSuggestions {
    limit: usize,
    suggestions: Vec<Suggestion>,
}

impl TopSuggestions {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            suggestions: Vec::with_capacity(limit),
        }
    }

    pub fn push(&mut self, suggestion: Suggestion) {
        if self.limit == 0 {
            return;
        }
        if let Some(existing) = self
            .suggestions
            .iter_mut()
            .find(|existing| existing.edit.replacement == suggestion.edit.replacement)
        {
            if compare_best(&suggestion, existing).is_gt() {
                *existing = suggestion;
            }
            return;
        }
        if self.suggestions.len() < self.limit {
            self.suggestions.push(suggestion);
            return;
        }
        let worst = self
            .suggestions
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| compare_best(left, right))
            .map(|(index, _)| index)
            .expect("a full bounded collection has a worst item");
        if compare_best(&suggestion, &self.suggestions[worst]).is_gt() {
            self.suggestions[worst] = suggestion;
        }
    }

    pub fn finish(mut self) -> Vec<Suggestion> {
        self.suggestions
            .sort_by(|left, right| compare_best(right, left));
        self.suggestions
    }
}

fn compare_best(left: &Suggestion, right: &Suggestion) -> Ordering {
    left.score
        .total_cmp(&right.score)
        .then_with(|| right.display.cmp(&left.display))
        .then_with(|| right.id.cmp(&left.id))
}
