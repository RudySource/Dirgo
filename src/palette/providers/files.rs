use std::{collections::VecDeque, fs, path::Path, time::Instant};

use crate::palette::{PaletteAction, PaletteItem, PaletteSource, ProviderBatch, ProviderBudget};

const MAX_DEPTH: usize = 8;

pub fn files(root: &Path, budget: ProviderBudget) -> ProviderBatch {
    let started = Instant::now();
    let root = match root.canonicalize() {
        Ok(root) if root.is_dir() => root,
        Ok(_) => {
            return ProviderBatch::failed(PaletteSource::Files, "file scope is not a directory");
        }
        Err(error) => return ProviderBatch::failed(PaletteSource::Files, error.to_string()),
    };
    let scan_limit = budget.max_items.saturating_mul(32).clamp(256, 8_192);
    let mut scanned = 0_usize;
    let mut queue = VecDeque::from([(root.clone(), 0_usize)]);
    let mut items = Vec::new();
    while let Some((directory, depth)) = queue.pop_front() {
        if items.len() >= budget.max_items
            || scanned >= scan_limit
            || started.elapsed() >= budget.deadline
        {
            break;
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            if items.len() >= budget.max_items
                || scanned >= scan_limit
                || started.elapsed() >= budget.deadline
            {
                break;
            }
            scanned += 1;
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
                continue;
            }
            let path = entry.path();
            if !path.starts_with(&root) {
                continue;
            }
            let relative = path.strip_prefix(&root).unwrap_or(&path);
            let title = relative.display().to_string();
            let is_directory = kind.is_dir();
            items.push(PaletteItem {
                id: format!("files:{}", path.display()),
                source: PaletteSource::Files,
                title,
                subtitle: if is_directory { "Directory" } else { "File" }.into(),
                insert_text: None,
                preview_key: Some(format!("file:{}", path.display())),
                action: if is_directory {
                    PaletteAction::Navigate { path: path.clone() }
                } else {
                    PaletteAction::OpenEditor { path: path.clone() }
                },
                score: 20_000_i64.saturating_sub(depth as i64),
            });
            if is_directory && depth < MAX_DEPTH {
                queue.push_back((path, depth + 1));
            }
        }
    }
    ProviderBatch::ready(PaletteSource::Files, items, started.elapsed())
}
