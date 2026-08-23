use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use fs2::FileExt;
use ignore::{DirEntry, WalkBuilder, WalkState};
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};

use crate::{
    DirgoError, Result,
    config::Config,
    model::{DirectoryRecord, ProjectKind, unix_now},
    paths::AppPaths,
};

const DIRECTORIES: TableDefinition<&str, &[u8]> = TableDefinition::new("directories");
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");

#[derive(Debug, Clone, Copy)]
pub struct IndexSummary {
    pub directories: usize,
    pub projects: usize,
    pub built_at: u64,
}

pub struct IndexStore {
    db: Database,
}

impl IndexStore {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            db: Database::create(path)?,
        })
    }

    pub fn records(&self) -> Result<Vec<DirectoryRecord>> {
        let read = self.db.begin_read()?;
        let table = match read.open_table(DIRECTORIES) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut records = Vec::with_capacity(table.len()? as usize);
        for item in table.iter()? {
            let (_, value) = item?;
            records.push(serde_json::from_slice(value.value())?);
        }
        Ok(records)
    }

    pub fn summary(&self) -> Result<IndexSummary> {
        let records = self.records()?;
        let read = self.db.begin_read()?;
        let built_at = match read.open_table(META) {
            Ok(table) => table
                .get("built_at")?
                .map(|value| value.value())
                .unwrap_or(0),
            Err(redb::TableError::TableDoesNotExist(_)) => 0,
            Err(error) => return Err(error.into()),
        };
        Ok(IndexSummary {
            directories: records.len(),
            projects: records
                .iter()
                .filter(|record| record.is_project_root)
                .count(),
            built_at,
        })
    }
}

#[derive(Default)]
struct ScanOutput {
    directories: Vec<PathBuf>,
    projects: HashMap<PathBuf, ProjectKind>,
    accessible_roots: usize,
}

pub fn rebuild(paths: &AppPaths, config: &Config) -> Result<IndexSummary> {
    paths.ensure_dirs()?;
    let lock_path = paths.cache_dir.join("refresh.lock");
    let lock = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| DirgoError::io(&lock_path, error))?;
    lock.try_lock_exclusive()
        .map_err(|_| DirgoError::RefreshBusy)?;

    let records = crawl(config)?;
    let now = unix_now();

    let temp_path = paths
        .cache_dir
        .join(format!("index.redb.tmp.{}", std::process::id()));
    if temp_path.exists() {
        fs::remove_file(&temp_path).map_err(|error| DirgoError::io(&temp_path, error))?;
    }
    write_index(&temp_path, &records, now)?;
    let validation = IndexStore::open(&temp_path)?.summary()?;
    if validation.directories != records.len() {
        return Err(DirgoError::User(
            "new index failed validation; the previous index was preserved".into(),
        ));
    }
    fs::rename(&temp_path, &paths.index_file)
        .map_err(|error| DirgoError::io(&paths.index_file, error))?;
    drop(lock);
    Ok(validation)
}

pub fn crawl_local(root: &Path, config: &Config) -> Result<Vec<DirectoryRecord>> {
    let mut local = config.clone();
    local.roots = vec![root.to_path_buf()];
    crawl(&local)
}

fn crawl(config: &Config) -> Result<Vec<DirectoryRecord>> {
    let scan = scan(config)?;
    if scan.accessible_roots == 0 {
        return Err(DirgoError::User(
            "none of the configured index roots is accessible; the previous index was preserved"
                .into(),
        ));
    }
    let now = unix_now();
    let home = dirs::home_dir();
    let mut records: Vec<_> = scan
        .directories
        .into_iter()
        .map(|path| {
            let basename = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let project_kind = scan.projects.get(&path).copied();
            let display_path = display_path(&path, home.as_deref());
            DirectoryRecord {
                parent: path.parent().unwrap_or(&path).to_path_buf(),
                depth: path.components().count(),
                path,
                display_path,
                basename,
                is_project_root: project_kind.is_some(),
                project_kind,
                last_seen: now,
            }
        })
        .collect();
    records.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    records.dedup_by(|a, b| a.path == b.path);
    Ok(records)
}

fn write_index(path: &Path, records: &[DirectoryRecord], now: u64) -> Result<()> {
    let db = Database::create(path)?;
    let write = db.begin_write()?;
    {
        let mut table = write.open_table(DIRECTORIES)?;
        for record in records {
            let key = record.path.to_string_lossy();
            let value = serde_json::to_vec(record)?;
            table.insert(key.as_ref(), value.as_slice())?;
        }
        let mut meta = write.open_table(META)?;
        meta.insert("built_at", now)?;
        meta.insert("schema_version", 1)?;
    }
    write.commit()?;
    Ok(())
}

fn scan(config: &Config) -> Result<ScanOutput> {
    let output = Arc::new(Mutex::new(ScanOutput::default()));
    let ignores = Arc::new(config.ignore.clone());
    for root in &config.roots {
        let root = crate::paths::expand_path(&root.to_string_lossy())?;
        if !root.is_dir() {
            tracing::warn!(path = %root.display(), "index root is not accessible");
            continue;
        }
        if let Ok(mut output) = output.lock() {
            output.accessible_roots += 1;
        }
        let mut builder = WalkBuilder::new(&root);
        builder
            .hidden(false)
            .follow_links(config.follow_symlinks)
            .git_ignore(config.respect_gitignore)
            .git_global(config.respect_gitignore)
            .git_exclude(config.respect_gitignore);
        let filter_output = Arc::clone(&output);
        let filter_ignores = Arc::clone(&ignores);
        builder.filter_entry(move |entry| filter_entry(entry, &filter_ignores, &filter_output));
        let walker = builder.build_parallel();
        let worker_output = Arc::clone(&output);
        walker.run(|| {
            let worker_output = Arc::clone(&worker_output);
            Box::new(move |entry| {
                match entry {
                    Ok(entry) => collect_entry(&entry, &worker_output),
                    Err(error) => tracing::debug!(%error, "skipping inaccessible index entry"),
                }
                WalkState::Continue
            })
        });
    }
    let output = Arc::try_unwrap(output)
        .map_err(|_| DirgoError::User("index workers did not shut down cleanly".into()))?
        .into_inner()
        .map_err(|_| DirgoError::User("index worker state was poisoned".into()))?;
    Ok(output)
}

fn filter_entry(entry: &DirEntry, ignores: &[String], output: &Mutex<ScanOutput>) -> bool {
    let name = entry.file_name().to_string_lossy();
    if name == ".git" && entry.path().is_dir() {
        if let Some(parent) = entry.path().parent() {
            if let Ok(mut output) = output.lock() {
                output
                    .projects
                    .insert(parent.to_path_buf(), ProjectKind::Git);
            }
        }
    }
    entry.depth() == 0 || !ignores.iter().any(|ignored| ignored == name.as_ref())
}

fn collect_entry(entry: &DirEntry, output: &Mutex<ScanOutput>) {
    if entry.file_type().is_some_and(|kind| kind.is_dir()) {
        if let Ok(path) = entry.path().canonicalize()
            && let Ok(mut output) = output.lock()
        {
            output.directories.push(path);
        }
        return;
    }
    if !entry.file_type().is_some_and(|kind| kind.is_file()) {
        return;
    }
    let Some(kind) = marker_kind(entry.file_name().to_str().unwrap_or_default()) else {
        return;
    };
    let Some(parent) = entry.path().parent() else {
        return;
    };
    if let Ok(mut output) = output.lock() {
        output
            .projects
            .entry(parent.to_path_buf())
            .and_modify(|current| {
                if marker_priority(kind) > marker_priority(*current) {
                    *current = kind;
                }
            })
            .or_insert(kind);
    }
}

fn marker_kind(name: &str) -> Option<ProjectKind> {
    match name {
        "Cargo.toml" => Some(ProjectKind::Rust),
        "package.json" | "pnpm-workspace.yaml" => Some(ProjectKind::Node),
        "go.mod" => Some(ProjectKind::Go),
        "pyproject.toml" => Some(ProjectKind::Python),
        "pom.xml" | "build.gradle" => Some(ProjectKind::Java),
        "Gemfile" => Some(ProjectKind::Ruby),
        "composer.json" => Some(ProjectKind::Php),
        "Makefile" => Some(ProjectKind::Generic),
        _ => None,
    }
}

fn marker_priority(kind: ProjectKind) -> u8 {
    match kind {
        ProjectKind::Git => 9,
        ProjectKind::Generic => 1,
        _ => 5,
    }
}

fn display_path(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && let Ok(relative) = path.strip_prefix(home)
    {
        return if relative.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~/{}", relative.display())
        };
    }
    path.display().to_string()
}

pub fn find_project_root(cwd: &Path) -> Option<(PathBuf, ProjectKind)> {
    for directory in cwd.ancestors() {
        if directory.join(".git").exists() {
            return Some((directory.to_path_buf(), ProjectKind::Git));
        }
        for marker in [
            "Cargo.toml",
            "package.json",
            "pnpm-workspace.yaml",
            "go.mod",
            "pyproject.toml",
            "pom.xml",
            "build.gradle",
            "Makefile",
            "Gemfile",
            "composer.json",
        ] {
            if directory.join(marker).is_file() {
                return marker_kind(marker).map(|kind| (directory.to_path_buf(), kind));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_paths(temp: &tempfile::TempDir) -> AppPaths {
        let cache_dir = temp.path().join("cache");
        let state_dir = temp.path().join("state");
        AppPaths {
            config_file: temp.path().join("config.toml"),
            index_file: cache_dir.join("index.redb"),
            state_file: state_dir.join("state.redb"),
            cache_dir,
            state_dir,
        }
    }

    fn config(root: &Path) -> Config {
        Config {
            roots: vec![root.to_path_buf()],
            ..Config::default()
        }
    }

    #[test]
    fn closest_project_marker_wins() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("project");
        let nested = root.join("src/module");
        fs::create_dir_all(&nested).expect("dirs");
        fs::write(root.join("Cargo.toml"), "").expect("marker");
        assert_eq!(find_project_root(&nested), Some((root, ProjectKind::Rust)));
    }

    #[test]
    fn rebuild_atomically_replaces_old_snapshot() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        fs::create_dir_all(root.join("old")).expect("old directory");
        let paths = app_paths(&temp);
        rebuild(&paths, &config(&root)).expect("first rebuild");

        fs::remove_dir(root.join("old")).expect("remove old fixture");
        fs::create_dir(root.join("new")).expect("new directory");
        rebuild(&paths, &config(&root)).expect("second rebuild");

        let records = IndexStore::open(&paths.index_file)
            .expect("open index")
            .records()
            .expect("records");
        assert!(records.iter().any(|record| record.basename == "new"));
        assert!(!records.iter().any(|record| record.basename == "old"));
        assert!(
            !paths
                .cache_dir
                .join(format!("index.redb.tmp.{}", std::process::id()))
                .exists()
        );
    }

    #[test]
    fn inaccessible_roots_preserve_previous_index() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        fs::create_dir_all(root.join("kept")).expect("fixture");
        let paths = app_paths(&temp);
        let config = config(&root);
        rebuild(&paths, &config).expect("first rebuild");
        fs::rename(&root, temp.path().join("root-unavailable")).expect("hide root");

        assert!(rebuild(&paths, &config).is_err());
        let records = IndexStore::open(&paths.index_file)
            .expect("open preserved index")
            .records()
            .expect("records");
        assert!(records.iter().any(|record| record.basename == "kept"));
    }

    #[test]
    fn refresh_lock_rejects_a_second_writer() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        fs::create_dir(&root).expect("root");
        let paths = app_paths(&temp);
        paths.ensure_dirs().expect("app dirs");
        let lock_path = paths.cache_dir.join("refresh.lock");
        let lock = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(lock_path)
            .expect("lock file");
        lock.lock_exclusive().expect("exclusive lock");

        assert!(matches!(
            rebuild(&paths, &config(&root)),
            Err(DirgoError::RefreshBusy)
        ));
    }
}
