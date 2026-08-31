use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Instant,
};

use crate::{
    model::{Bookmark, DirectoryRecord, ProjectKind},
    palette::{PaletteAction, PaletteItem, PaletteSource, ProviderBatch, ProviderBudget},
};

pub fn places(
    records: &[DirectoryRecord],
    bookmarks: &HashMap<String, Bookmark>,
    budget: ProviderBudget,
) -> ProviderBatch {
    let started = Instant::now();
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut bookmarks = bookmarks.values().collect::<Vec<_>>();
    bookmarks.sort_by(|left, right| left.name.cmp(&right.name));
    for bookmark in bookmarks {
        if items.len() >= budget.max_items || started.elapsed() >= budget.deadline {
            break;
        }
        let key = path_key(&bookmark.path);
        seen.insert(key);
        let project = records.iter().find(|record| {
            record.is_project_root && path_key(&record.path) == path_key(&bookmark.path)
        });
        let mut details = vec!["Bookmark".to_owned()];
        if let Some(project) = project.and_then(|record| record.project_kind) {
            details.push(project_label(project).to_owned());
        }
        if !bookmark.path.is_dir() {
            details.push("missing directory".into());
        }
        items.push(PaletteItem {
            id: format!("places:bookmark:{}", bookmark.name),
            source: PaletteSource::Places,
            title: format!("@{}", bookmark.name),
            subtitle: details.join(" · "),
            insert_text: None,
            preview_key: Some(format!("place:{}", bookmark.path.display())),
            action: PaletteAction::Navigate {
                path: bookmark.path.clone(),
            },
            score: 30_000,
        });
    }
    let mut projects = records
        .iter()
        .filter(|record| record.is_project_root)
        .collect::<Vec<_>>();
    projects.sort_by(|left, right| left.path.cmp(&right.path));
    for project in projects {
        if items.len() >= budget.max_items || started.elapsed() >= budget.deadline {
            break;
        }
        if !seen.insert(path_key(&project.path)) {
            continue;
        }
        items.push(PaletteItem {
            id: format!("places:project:{}", project.path.display()),
            source: PaletteSource::Places,
            title: project.basename.clone(),
            subtitle: project
                .project_kind
                .map(project_label)
                .unwrap_or("Project")
                .into(),
            insert_text: None,
            preview_key: Some(format!("place:{}", project.path.display())),
            action: PaletteAction::Navigate {
                path: project.path.clone(),
            },
            score: 20_000_i64.saturating_sub(project.depth as i64),
        });
    }
    ProviderBatch::ready(PaletteSource::Places, items, started.elapsed())
}

fn path_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn project_label(kind: ProjectKind) -> &'static str {
    match kind {
        ProjectKind::Git => "Git project",
        ProjectKind::Rust => "Rust project",
        ProjectKind::Node => "Node project",
        ProjectKind::Go => "Go project",
        ProjectKind::Python => "Python project",
        ProjectKind::Java => "Java project",
        ProjectKind::Ruby => "Ruby project",
        ProjectKind::Php => "PHP project",
        ProjectKind::Generic => "Project",
    }
}
