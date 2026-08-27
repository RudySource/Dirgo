use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use fs2::FileExt;
use ignore::{DirEntry, WalkBuilder, WalkState};
use redb::{
    Database, ReadOnlyDatabase, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    TableDefinition,
};

use crate::{
    DirgoError, Result,
    config::Config,
    model::{DirectoryRecord, ProjectKind, unix_now},
    paths::AppPaths,
};

const DIRECTORIES: TableDefinition<&str, &[u8]> = TableDefinition::new("directories");
const UNIQUE_BASENAMES: TableDefinition<&str, &[u8]> = TableDefinition::new("unique_basenames");
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
const SCHEMA_VERSION: u64 = 3;
const RECORD_BYTES: usize = 10;
const RECORD_FORMAT_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy)]
pub struct IndexSummary {
    pub directories: usize,
    pub projects: usize,
    pub built_at: u64,
}

pub struct IndexStore {
    db: ReadOnlyDatabase,
}

impl IndexStore {
    pub fn open(path: &Path) -> Result<Self> {
        let store = Self {
            db: ReadOnlyDatabase::open(path)?,
        };
        store.validate_schema()?;
        Ok(store)
    }

    fn validate_schema(&self) -> Result<()> {
        let read = self.db.begin_read()?;
        let schema_version = match read.open_table(META) {
            Ok(table) => table.get("schema_version")?.map(|value| value.value()),
            Err(redb::TableError::TableDoesNotExist(_)) => None,
            Err(error) => return Err(error.into()),
        };
        match schema_version {
            None => Err(DirgoError::IndexData(
                "index schema marker is missing".into(),
            )),
            Some(SCHEMA_VERSION) => Ok(()),
            Some(version) if version < SCHEMA_VERSION => Err(DirgoError::IndexUpgradeRequired {
                found: version,
                expected: SCHEMA_VERSION,
            }),
            Some(version) => Err(DirgoError::User(format!(
                "index schema version {version} is unsupported; run `dgo refresh` to rebuild it"
            ))),
        }
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
            let (key, value) = item?;
            records.push(decode_record(key.value(), value.value())?);
        }
        Ok(records)
    }

    pub fn record_count(&self) -> Result<usize> {
        let read = self.db.begin_read()?;
        match read.open_table(DIRECTORIES) {
            Ok(table) => Ok(table.len()? as usize),
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(0),
            Err(error) => Err(error.into()),
        }
    }

    /// Calls `visit` for each decoded directory record without first building a
    /// full in-memory vector. Intended for the interactive picker's worker.
    /// The callback returns whether the scan should continue. This lets an
    /// interactive picker stop I/O promptly after the user closes it.
    pub fn visit_records(&self, mut visit: impl FnMut(DirectoryRecord) -> bool) -> Result<()> {
        let read = self.db.begin_read()?;
        let table = match read.open_table(DIRECTORIES) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for item in table.iter()? {
            let (key, value) = item?;
            if !visit(decode_record(key.value(), value.value())?) {
                break;
            }
        }
        Ok(())
    }

    /// Finds a basename only when it maps to one indexed directory. Keys are
    /// normalized with Unicode lowercasing, matching the search smart-case
    /// exact-match semantics without decoding the complete index.
    pub fn unique_basename(&self, basename: &str) -> Result<Option<PathBuf>> {
        let read = self.db.begin_read()?;
        let table = match read.open_table(UNIQUE_BASENAMES) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let key = basename.to_lowercase();
        let Some(value) = table.get(key.as_str())? else {
            return Ok(None);
        };
        let value = value.value();
        if value.is_empty() {
            return Ok(None);
        }
        let path = std::str::from_utf8(value).map_err(|_| {
            DirgoError::IndexData("unique basename entry is not valid UTF-8".into())
        })?;
        Ok(Some(PathBuf::from(path)))
    }

    pub fn summary(&self) -> Result<IndexSummary> {
        let read = self.db.begin_read()?;
        let (directories, projects) = match read.open_table(DIRECTORIES) {
            Ok(table) => {
                let mut projects = 0;
                for item in table.iter()? {
                    let (_, value) = item?;
                    if decode_project_kind(value.value())?.is_some() {
                        projects += 1;
                    }
                }
                (table.len()? as usize, projects)
            }
            Err(redb::TableError::TableDoesNotExist(_)) => (0, 0),
            Err(error) => return Err(error.into()),
        };
        let built_at = match read.open_table(META) {
            Ok(table) => table
                .get("built_at")?
                .map(|value| value.value())
                .unwrap_or(0),
            Err(redb::TableError::TableDoesNotExist(_)) => 0,
            Err(error) => return Err(error.into()),
        };
        Ok(IndexSummary {
            directories,
            projects,
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
    cleanup_stale_temp_indexes(&paths.cache_dir)?;

    let records = crawl(config)?;
    let now = unix_now();

    let temp_path = paths
        .cache_dir
        .join(format!("index.redb.tmp.{}", std::process::id()));
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

fn cleanup_stale_temp_indexes(cache_dir: &Path) -> Result<()> {
    for entry in fs::read_dir(cache_dir).map_err(|error| DirgoError::io(cache_dir, error))? {
        let entry = entry.map_err(|error| DirgoError::io(cache_dir, error))?;
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("index.redb.tmp.") {
            continue;
        }
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| DirgoError::io(&path, error))?
            .is_file()
        {
            fs::remove_file(&path).map_err(|error| DirgoError::io(&path, error))?;
        }
    }
    Ok(())
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
        let mut unique_basenames = write.open_table(UNIQUE_BASENAMES)?;
        for record in records {
            let key = record.path.to_string_lossy();
            let value = encode_record(record);
            table.insert(key.as_ref(), value.as_slice())?;
            let basename = record.basename.to_lowercase();
            let path = key.as_bytes();
            let existing_is_unique = unique_basenames
                .get(basename.as_str())?
                .is_some_and(|value| !value.value().is_empty());
            if existing_is_unique {
                unique_basenames.insert(basename.as_str(), &[][..])?;
            } else if unique_basenames.get(basename.as_str())?.is_none() {
                unique_basenames.insert(basename.as_str(), path)?;
            }
        }
        let mut meta = write.open_table(META)?;
        meta.insert("built_at", now)?;
        meta.insert("schema_version", SCHEMA_VERSION)?;
    }
    write.commit()?;
    Ok(())
}

fn encode_record(record: &DirectoryRecord) -> [u8; RECORD_BYTES] {
    let mut value = [0; RECORD_BYTES];
    value[0] = RECORD_FORMAT_VERSION;
    value[1] = project_kind_code(record.project_kind);
    value[2..].copy_from_slice(&record.last_seen.to_le_bytes());
    value
}

fn decode_record(path: &str, value: &[u8]) -> Result<DirectoryRecord> {
    let project_kind = decode_project_kind(value)?;
    let path = PathBuf::from(path);
    let basename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let parent = path.parent().unwrap_or(&path).to_path_buf();
    let last_seen = u64::from_le_bytes(
        value[2..]
            .try_into()
            .map_err(|_| DirgoError::IndexData("record timestamp is truncated".into()))?,
    );
    Ok(DirectoryRecord {
        display_path: display_path(&path, dirs::home_dir().as_deref()),
        depth: path.components().count(),
        path,
        basename,
        parent,
        is_project_root: project_kind.is_some(),
        project_kind,
        last_seen,
    })
}

fn decode_project_kind(value: &[u8]) -> Result<Option<ProjectKind>> {
    if value.len() != RECORD_BYTES {
        return Err(DirgoError::IndexData(format!(
            "record has {} bytes; expected {RECORD_BYTES}",
            value.len()
        )));
    }
    if value[0] != RECORD_FORMAT_VERSION {
        return Err(DirgoError::IndexData(format!(
            "record format {} is unsupported",
            value[0]
        )));
    }
    let kind = match value[1] {
        0 => None,
        1 => Some(ProjectKind::Git),
        2 => Some(ProjectKind::Rust),
        3 => Some(ProjectKind::Node),
        4 => Some(ProjectKind::Go),
        5 => Some(ProjectKind::Python),
        6 => Some(ProjectKind::Java),
        7 => Some(ProjectKind::Ruby),
        8 => Some(ProjectKind::Php),
        9 => Some(ProjectKind::Generic),
        code => {
            return Err(DirgoError::IndexData(format!(
                "unknown project kind code {code}"
            )));
        }
    };
    Ok(kind)
}

fn project_kind_code(kind: Option<ProjectKind>) -> u8 {
    match kind {
        None => 0,
        Some(ProjectKind::Git) => 1,
        Some(ProjectKind::Rust) => 2,
        Some(ProjectKind::Node) => 3,
        Some(ProjectKind::Go) => 4,
        Some(ProjectKind::Python) => 5,
        Some(ProjectKind::Java) => 6,
        Some(ProjectKind::Ruby) => 7,
        Some(ProjectKind::Php) => 8,
        Some(ProjectKind::Generic) => 9,
    }
}

fn scan(config: &Config) -> Result<ScanOutput> {
    let output = Arc::new(Mutex::new(ScanOutput::default()));
    let ignores = Arc::new(config.ignore.clone());
    for root in &config.roots {
        let root = crate::paths::expand_path(&root.to_string_lossy())?;
        if !root.is_dir() || fs::read_dir(&root).is_err() {
            tracing::warn!(
                path = %crate::terminal::safe_path(&root),
                "index root is not readable"
            );
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
    if name == ".git"
        && entry.path().is_dir()
        && let Some(parent) = entry.path().parent()
        && let Ok(parent) = parent.canonicalize()
        && let Ok(mut output) = output.lock()
    {
        output.projects.insert(parent, ProjectKind::Git);
    }
    entry.depth() == 0 || !ignores.iter().any(|ignored| ignored == name.as_ref())
}

fn collect_entry(entry: &DirEntry, output: &Mutex<ScanOutput>) {
    if entry.file_type().is_some_and(|kind| kind.is_dir()) {
        if let Ok(path) = entry.path().canonicalize()
            && path.to_str().is_some()
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
    let Ok(parent) = parent.canonicalize() else {
        return;
    };
    if let Ok(mut output) = output.lock() {
        output
            .projects
            .entry(parent)
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
        "Makefile"
        | "justfile"
        | "Justfile"
        | "compose.yaml"
        | "compose.yml"
        | "docker-compose.yaml"
        | "docker-compose.yml" => Some(ProjectKind::Generic),
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
            "justfile",
            "Justfile",
            "compose.yaml",
            "compose.yml",
            "docker-compose.yaml",
            "docker-compose.yml",
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
            suggestions_state_file: state_dir.join("suggestions.redb"),
            update_cache_file: cache_dir.join("update.json"),
            update_check_file: cache_dir.join("update-check"),
            update_notice_disabled_file: state_dir.join("update-notifications-disabled"),
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
    fn compact_records_round_trip_unicode_paths_and_project_metadata() {
        let record = DirectoryRecord {
            path: PathBuf::from("/workspace/quo'te 子"),
            display_path: "unused when encoding".into(),
            basename: "unused when encoding".into(),
            parent: PathBuf::from("/workspace"),
            depth: 2,
            is_project_root: true,
            project_kind: Some(ProjectKind::Rust),
            last_seen: 42,
        };

        let decoded = decode_record(&record.path.to_string_lossy(), &encode_record(&record))
            .expect("decode compact record");
        assert_eq!(decoded.path, record.path);
        assert_eq!(decoded.basename, "quo'te 子");
        assert_eq!(decoded.parent, PathBuf::from("/workspace"));
        assert_eq!(decoded.project_kind, Some(ProjectKind::Rust));
        assert_eq!(decoded.last_seen, 42);
    }

    #[test]
    fn malformed_compact_record_is_recoverable_index_data() {
        let error = decode_record("/workspace/broken", &[RECORD_FORMAT_VERSION])
            .expect_err("truncated record must fail");
        assert!(matches!(error, DirgoError::IndexData(_)));
    }

    #[test]
    fn index_without_a_schema_marker_is_recoverable_index_data() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("index.redb");
        Database::create(&path).expect("empty database");

        assert!(matches!(
            IndexStore::open(&path),
            Err(DirgoError::IndexData(_))
        ));
    }

    #[test]
    fn unique_basename_lookup_is_case_insensitive_and_rejects_duplicates() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        fs::create_dir_all(root.join("only-one")).expect("unique fixture");
        fs::create_dir_all(root.join("first/shared")).expect("first duplicate");
        fs::create_dir_all(root.join("second/shared")).expect("second duplicate");
        let paths = app_paths(&temp);
        rebuild(&paths, &config(&root)).expect("rebuild");

        let store = IndexStore::open(&paths.index_file).expect("open index");
        assert_eq!(
            store.unique_basename("ONLY-ONE").expect("lookup"),
            Some(
                root.join("only-one")
                    .canonicalize()
                    .expect("canonical path")
            )
        );
        assert_eq!(store.unique_basename("shared").expect("lookup"), None);
        assert_eq!(store.unique_basename("missing").expect("lookup"), None);
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

    #[test]
    fn interrupted_refresh_temp_file_is_removed_before_a_new_publish() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        fs::create_dir_all(root.join("fresh")).expect("fixture");
        let paths = app_paths(&temp);
        paths.ensure_dirs().expect("app dirs");
        let stale = paths.cache_dir.join("index.redb.tmp.interrupted");
        fs::write(&stale, "partial redb write").expect("stale temp");

        rebuild(&paths, &config(&root)).expect("rebuild");
        assert!(!stale.exists());
        assert!(
            IndexStore::open(&paths.index_file)
                .expect("index")
                .records()
                .expect("records")
                .iter()
                .any(|record| record.basename == "fresh")
        );
    }

    #[test]
    fn open_reader_keeps_its_previous_snapshot_during_refresh() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        fs::create_dir_all(root.join("old")).expect("old directory");
        let paths = app_paths(&temp);
        rebuild(&paths, &config(&root)).expect("first rebuild");
        let old_reader = IndexStore::open(&paths.index_file).expect("old reader");

        fs::remove_dir(root.join("old")).expect("remove old");
        fs::create_dir(root.join("new")).expect("new directory");
        rebuild(&paths, &config(&root)).expect("second rebuild");

        let old_records = old_reader.records().expect("old snapshot");
        let new_records = IndexStore::open(&paths.index_file)
            .expect("new reader")
            .records()
            .expect("new snapshot");
        assert!(old_records.iter().any(|record| record.basename == "old"));
        assert!(!old_records.iter().any(|record| record.basename == "new"));
        assert!(new_records.iter().any(|record| record.basename == "new"));
        assert!(!new_records.iter().any(|record| record.basename == "old"));
    }

    #[test]
    fn multiple_readers_can_open_the_published_index_concurrently() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        fs::create_dir_all(root.join("shared")).expect("fixture");
        let paths = app_paths(&temp);
        rebuild(&paths, &config(&root)).expect("rebuild");

        let first = IndexStore::open(&paths.index_file).expect("first reader");
        let second = IndexStore::open(&paths.index_file).expect("second reader");

        assert_eq!(
            first.record_count().expect("first count"),
            second.record_count().expect("second count")
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_root_preserves_previous_index() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        fs::create_dir_all(root.join("kept")).expect("fixture");
        let paths = app_paths(&temp);
        let config = config(&root);
        rebuild(&paths, &config).expect("first rebuild");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).expect("deny root");
        let result = rebuild(&paths, &config);
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).expect("restore root");

        assert!(result.is_err());
        assert!(
            IndexStore::open(&paths.index_file)
                .expect("preserved index")
                .records()
                .expect("records")
                .iter()
                .any(|record| record.basename == "kept")
        );
    }

    #[cfg(unix)]
    #[test]
    fn broken_symlinks_and_cycles_do_not_block_following_links() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        let nested = root.join("real/nested");
        fs::create_dir_all(&nested).expect("fixture");
        symlink(root.join("missing"), root.join("broken")).expect("broken link");
        symlink(root.join("real"), nested.join("cycle")).expect("cycle link");
        let paths = app_paths(&temp);
        let mut config = config(&root);
        config.follow_symlinks = true;

        rebuild(&paths, &config).expect("rebuild despite links");
        let canonical_nested = nested.canonicalize().expect("canonical nested");
        let records = IndexStore::open(&paths.index_file)
            .expect("index")
            .records()
            .expect("records");
        assert!(records.iter().any(|record| record.path == canonical_nested));
        assert!(records.len() < 10, "cycle made the index unbounded");
    }

    #[cfg(unix)]
    #[test]
    fn project_markers_survive_a_symlinked_configured_root() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let real_root = temp.path().join("real-root");
        let project = real_root.join("project");
        fs::create_dir_all(&project).expect("project");
        fs::write(project.join("Cargo.toml"), "").expect("marker");
        let linked_root = temp.path().join("linked-root");
        symlink(&real_root, &linked_root).expect("linked root");
        let paths = app_paths(&temp);

        rebuild(&paths, &config(&linked_root)).expect("rebuild");
        let canonical_project = project.canonicalize().expect("canonical project");
        let records = IndexStore::open(&paths.index_file)
            .expect("index")
            .records()
            .expect("records");
        assert!(records.iter().any(|record| {
            record.path == canonical_project && record.project_kind == Some(ProjectKind::Rust)
        }));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_directories_are_not_stored_lossily() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("root");
        fs::create_dir(&root).expect("root");
        fs::create_dir(root.join(OsString::from_vec(vec![b'b', b'a', b'd', 0xff])))
            .expect("non-utf8 directory");
        let paths = app_paths(&temp);

        rebuild(&paths, &config(&root)).expect("rebuild");
        let records = IndexStore::open(&paths.index_file)
            .expect("index")
            .records()
            .expect("records");
        assert_eq!(records.len(), 1, "non-UTF-8 child was stored lossily");
    }

    #[test]
    fn unsupported_schema_requires_an_explicit_refresh() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("index.redb");
        let db = Database::create(&path).expect("database");
        let write = db.begin_write().expect("write");
        {
            let mut meta = write.open_table(META).expect("meta");
            meta.insert("schema_version", SCHEMA_VERSION + 1)
                .expect("version");
        }
        write.commit().expect("commit");
        drop(db);

        let error = match IndexStore::open(&path) {
            Ok(_) => panic!("unsupported schema was accepted"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("run `dgo refresh`"));
        assert!(path.exists());
    }

    #[test]
    fn older_schema_requests_a_safe_disposable_rebuild() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("index.redb");
        let db = Database::create(&path).expect("database");
        let write = db.begin_write().expect("write");
        {
            let mut meta = write.open_table(META).expect("meta");
            meta.insert("schema_version", SCHEMA_VERSION - 1)
                .expect("version");
        }
        write.commit().expect("commit");
        drop(db);

        assert!(matches!(
            IndexStore::open(&path),
            Err(DirgoError::IndexUpgradeRequired {
                found,
                expected: SCHEMA_VERSION
            }) if found == SCHEMA_VERSION - 1
        ));
        assert!(path.exists());
    }
}
