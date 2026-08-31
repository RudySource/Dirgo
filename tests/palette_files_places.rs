use std::{collections::HashMap, fs, path::PathBuf, time::Duration};

use dirgo::{
    model::{Bookmark, DirectoryRecord, ProjectKind},
    palette::{
        PaletteAction, PaletteSource, ProviderBudget,
        providers::{files, places},
    },
};

#[cfg(unix)]
#[test]
fn files_snapshot_is_bounded_includes_dotfiles_and_never_follows_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    let outside = temp.path().join("outside");
    fs::create_dir_all(project.join("src/nested")).expect("project tree");
    fs::create_dir_all(&outside).expect("outside");
    fs::write(project.join(".env.example"), "safe").expect("dotfile");
    fs::write(project.join("src/main.rs"), "fn main() {}").expect("source");
    fs::write(project.join("src/nested/lib.rs"), "pub fn lib() {}").expect("nested");
    fs::write(outside.join("secret.txt"), "secret").expect("outside file");
    symlink(&outside, project.join("escape")).expect("escape symlink");

    let batch = files(&project, ProviderBudget::new(4, Duration::from_secs(1)));

    assert_eq!(batch.source, PaletteSource::Files);
    assert!(batch.error.is_none());
    assert!(batch.items.len() <= 4);
    assert!(batch.items.iter().any(|item| item.title == ".env.example"));
    assert!(batch.items.iter().any(|item| item.title == "src/main.rs"));
    assert!(
        batch
            .items
            .iter()
            .all(|item| !item.title.contains("secret.txt"))
    );
    assert!(batch.items.iter().all(|item| {
        matches!(
            item.action,
            PaletteAction::Navigate { .. } | PaletteAction::OpenEditor { .. }
        )
    }));
}

fn project_record(path: PathBuf, kind: ProjectKind) -> DirectoryRecord {
    DirectoryRecord {
        display_path: path.display().to_string(),
        basename: path
            .file_name()
            .expect("basename")
            .to_string_lossy()
            .into_owned(),
        parent: path.parent().expect("parent").to_path_buf(),
        depth: path.components().count(),
        path,
        is_project_root: true,
        project_kind: Some(kind),
        last_seen: 1,
    }
}

#[test]
fn places_merge_bookmarks_and_projects_without_duplicate_destinations() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rust = temp.path().join("rust-project");
    let node = temp.path().join("node-project");
    let missing = temp.path().join("missing-bookmark");
    fs::create_dir_all(&rust).expect("rust project");
    fs::create_dir_all(&node).expect("node project");
    let records = vec![
        project_record(rust.clone(), ProjectKind::Rust),
        project_record(node.clone(), ProjectKind::Node),
    ];
    let bookmarks = HashMap::from([
        (
            "work".into(),
            Bookmark {
                name: "work".into(),
                path: rust.clone(),
                created_at: 1,
                last_used: None,
                tags: Vec::new(),
            },
        ),
        (
            "old".into(),
            Bookmark {
                name: "old".into(),
                path: missing.clone(),
                created_at: 2,
                last_used: None,
                tags: Vec::new(),
            },
        ),
    ]);

    let batch = places(
        &records,
        &bookmarks,
        ProviderBudget::new(8, Duration::from_secs(1)),
    );

    assert_eq!(batch.source, PaletteSource::Places);
    assert_eq!(batch.items.len(), 3);
    let work = batch
        .items
        .iter()
        .find(|item| item.title == "@work")
        .expect("merged bookmark project");
    assert!(work.subtitle.contains("Bookmark"));
    assert!(work.subtitle.contains("Rust project"));
    assert_eq!(work.action, PaletteAction::Navigate { path: rust.clone() });
    let old = batch
        .items
        .iter()
        .find(|item| item.title == "@old")
        .expect("stale bookmark");
    assert!(old.subtitle.contains("missing"));
    assert!(batch.items.iter().any(|item| item.title == "node-project"));
}
