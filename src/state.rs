use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use redb::{
    Database, ReadOnlyDatabase, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    TableDefinition,
};
use serde::{Deserialize, Serialize};

use crate::{
    DirgoError, Result,
    model::{Bookmark, PathHistory, unix_now},
    paths, terminal,
};

const BOOKMARKS: TableDefinition<&str, &[u8]> = TableDefinition::new("bookmarks");
const HISTORY: TableDefinition<&str, &[u8]> = TableDefinition::new("history");
const SESSIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("sessions");
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
const SCHEMA_VERSION: u64 = 1;
const MAX_HISTORY_ENTRIES: usize = 50_000;
const RETAINED_HISTORY_ENTRIES: usize = 45_000;
const MAX_SESSION_ENTRIES: usize = 256;
const MAX_SESSIONS: usize = 256;
const RETAINED_SESSIONS: usize = 192;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct NavigationSession {
    entries: Vec<PathBuf>,
    cursor: usize,
    updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryImportSummary {
    pub imported: usize,
    pub unchanged: usize,
}

pub struct StateStore {
    db: Database,
}

impl StateStore {
    pub fn open(path: &Path) -> Result<Self> {
        match Self::open_checked(path) {
            Ok(store) => Ok(store),
            Err(error) if recoverable_storage_error(&error) && path.exists() => {
                let backup = paths::preserve_for_recovery(path, "corrupt", unix_now())?;
                eprintln!(
                    "Dirgo backed up corrupt state to {} and started with empty state.",
                    terminal::safe_path(&backup)
                );
                Self::open_checked(path)
            }
            Err(error) => Err(error),
        }
    }

    fn open_checked(path: &Path) -> Result<Self> {
        let db = Database::create(path)?;
        let store = Self { db };
        store.ensure_schema()?;
        store.validate_content()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<()> {
        let write = self.db.begin_write()?;
        {
            let mut meta = write.open_table(META)?;
            let schema_version = {
                let value = meta.get("schema_version")?;
                value.map(|value| value.value())
            };
            match schema_version {
                None => {
                    meta.insert("schema_version", SCHEMA_VERSION)?;
                }
                Some(0) => {
                    meta.insert("schema_version", SCHEMA_VERSION)?;
                }
                Some(SCHEMA_VERSION) => {}
                Some(version) => {
                    return Err(DirgoError::User(format!(
                        "state schema version {version} is unsupported; preserve the file and upgrade Dirgo"
                    )));
                }
            }
            write.open_table(BOOKMARKS)?;
            write.open_table(HISTORY)?;
            write.open_table(SESSIONS)?;
        }
        write.commit()?;
        Ok(())
    }

    fn validate_content(&self) -> Result<()> {
        self.bookmarks()?;
        self.histories()?;
        let read = self.db.begin_read()?;
        let table = read.open_table(SESSIONS)?;
        for item in table.iter()? {
            let (_, value) = item?;
            let _: NavigationSession = serde_json::from_slice(value.value())?;
        }
        Ok(())
    }

    pub fn bookmarks(&self) -> Result<Vec<Bookmark>> {
        let read = self.db.begin_read()?;
        let table = read.open_table(BOOKMARKS)?;
        let mut bookmarks: Vec<Bookmark> = Vec::with_capacity(table.len()? as usize);
        for item in table.iter()? {
            let (_, value) = item?;
            bookmarks.push(serde_json::from_slice(value.value())?);
        }
        bookmarks.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        Ok(bookmarks)
    }

    pub fn bookmark_map(&self) -> Result<HashMap<String, Bookmark>> {
        Ok(self
            .bookmarks()?
            .into_iter()
            .map(|bookmark| (bookmark.name.clone(), bookmark))
            .collect())
    }

    pub fn bookmark(&self, name: &str) -> Result<Option<Bookmark>> {
        let read = self.db.begin_read()?;
        let table = read.open_table(BOOKMARKS)?;
        table
            .get(name)?
            .map(|value| serde_json::from_slice(value.value()).map_err(Into::into))
            .transpose()
    }

    pub fn add_bookmark(&self, name: &str, path: &Path) -> Result<Bookmark> {
        validate_bookmark_name(name)?;
        if !path.is_dir() {
            return Err(DirgoError::User(format!(
                "{} is not a directory",
                terminal::safe_path(path)
            )));
        }
        let path = path
            .canonicalize()
            .map_err(|error| DirgoError::io(path, error))?;
        paths::validate_shell_path(&path)?;
        let now = unix_now();
        let bookmark = self.bookmark(name)?.map_or_else(
            || Bookmark {
                name: name.into(),
                path: path.clone(),
                created_at: now,
                last_used: None,
                tags: Vec::new(),
            },
            |mut bookmark| {
                bookmark.path = path.clone();
                bookmark
            },
        );
        let value = serde_json::to_vec(&bookmark)?;
        let write = self.db.begin_write()?;
        {
            write
                .open_table(BOOKMARKS)?
                .insert(name, value.as_slice())?;
        }
        write.commit()?;
        Ok(bookmark)
    }

    pub fn remove_bookmark(&self, name: &str) -> Result<bool> {
        let write = self.db.begin_write()?;
        let removed = { write.open_table(BOOKMARKS)?.remove(name)?.is_some() };
        write.commit()?;
        Ok(removed)
    }

    pub fn rename_bookmark(&self, old: &str, new: &str) -> Result<()> {
        validate_bookmark_name(new)?;
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(BOOKMARKS)?;
            let Some(mut bookmark): Option<Bookmark> = table
                .get(old)?
                .map(|value| serde_json::from_slice(value.value()))
                .transpose()?
            else {
                return Err(DirgoError::BookmarkMissing(old.into()));
            };
            if old != new && table.get(new)?.is_some() {
                return Err(DirgoError::User(format!(
                    "bookmark @{new} already exists; remove it explicitly before renaming"
                )));
            }
            bookmark.name = new.into();
            let value = serde_json::to_vec(&bookmark)?;
            table.remove(old)?;
            table.insert(new, value.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn record_navigation(
        &self,
        from: &Path,
        destination: &Path,
        session_id: Option<&str>,
    ) -> Result<()> {
        if from == destination {
            return Ok(());
        }
        let key = destination.to_string_lossy();
        let now = unix_now();
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(HISTORY)?;
            let mut history: PathHistory = table
                .get(key.as_ref())?
                .map(|value| serde_json::from_slice(value.value()))
                .transpose()?
                .unwrap_or(PathHistory {
                    path: destination.to_path_buf(),
                    visit_count: 0,
                    first_visit: now,
                    last_visit: now,
                });
            history.visit_count = history.visit_count.saturating_add(1);
            history.last_visit = now;
            let value = serde_json::to_vec(&history)?;
            table.insert(key.as_ref(), value.as_slice())?;
        }
        write.commit()?;
        self.prune_history(MAX_HISTORY_ENTRIES, RETAINED_HISTORY_ENTRIES)?;
        if let Some(session_id) = session_id {
            self.push_transition(session_id, from, destination)?;
        }
        Ok(())
    }

    pub fn histories(&self) -> Result<Vec<PathHistory>> {
        let read = self.db.begin_read()?;
        let table = read.open_table(HISTORY)?;
        let mut rows: Vec<PathHistory> = Vec::with_capacity(table.len()? as usize);
        for item in table.iter()? {
            let (_, value) = item?;
            rows.push(serde_json::from_slice(value.value())?);
        }
        rows.sort_unstable_by(|a, b| b.last_visit.cmp(&a.last_visit));
        Ok(rows)
    }

    pub fn import_history(&self, entries: &[(PathBuf, u64)]) -> Result<HistoryImportSummary> {
        let write = self.db.begin_write()?;
        let mut imported = 0;
        let mut unchanged = 0;
        {
            let mut table = write.open_table(HISTORY)?;
            for (path, imported_visits) in entries {
                let key = path.to_string_lossy();
                let existing: Option<PathHistory> = {
                    let value = table.get(key.as_ref())?;
                    value
                        .map(|value| serde_json::from_slice(value.value()))
                        .transpose()?
                };
                if existing
                    .as_ref()
                    .is_some_and(|row| row.visit_count >= *imported_visits)
                {
                    unchanged += 1;
                    continue;
                }
                let row = existing.map_or_else(
                    || PathHistory {
                        path: path.clone(),
                        visit_count: *imported_visits,
                        first_visit: 0,
                        last_visit: 0,
                    },
                    |mut row| {
                        row.visit_count = *imported_visits;
                        row
                    },
                );
                let value = serde_json::to_vec(&row)?;
                table.insert(key.as_ref(), value.as_slice())?;
                imported += 1;
            }
        }
        write.commit()?;
        self.prune_history(MAX_HISTORY_ENTRIES, RETAINED_HISTORY_ENTRIES)?;
        Ok(HistoryImportSummary {
            imported,
            unchanged,
        })
    }

    #[cfg(test)]
    fn history(&self, path: &Path) -> Result<Option<PathHistory>> {
        let read = self.db.begin_read()?;
        let table = read.open_table(HISTORY)?;
        let key = path.to_string_lossy();
        table
            .get(key.as_ref())?
            .map(|value| serde_json::from_slice(value.value()).map_err(Into::into))
            .transpose()
    }

    fn session(&self, id: &str) -> Result<NavigationSession> {
        let read = self.db.begin_read()?;
        let table = read.open_table(SESSIONS)?;
        Ok(table
            .get(id)?
            .map(|value| serde_json::from_slice(value.value()))
            .transpose()?
            .unwrap_or_default())
    }

    fn save_session(&self, id: &str, session: &NavigationSession) -> Result<()> {
        let mut session = session.clone();
        session.updated_at = unix_now();
        let value = serde_json::to_vec(&session)?;
        let write = self.db.begin_write()?;
        {
            write.open_table(SESSIONS)?.insert(id, value.as_slice())?;
        }
        write.commit()?;
        self.prune_sessions(MAX_SESSIONS, RETAINED_SESSIONS, Some(id))?;
        Ok(())
    }

    fn push_transition(&self, id: &str, from: &Path, destination: &Path) -> Result<()> {
        let mut session = self.session(id)?;
        let current_matches_origin = session
            .entries
            .get(session.cursor)
            .is_some_and(|current| current == from);
        if session.entries.is_empty() {
            session.entries.push(from.to_path_buf());
            session.cursor = 0;
        } else if !current_matches_origin {
            session.entries.truncate(session.cursor + 1);
            session.entries.push(from.to_path_buf());
            session.cursor = session.entries.len() - 1;
        }
        if session.entries[session.cursor] != destination {
            session.entries.truncate(session.cursor + 1);
            session.entries.push(destination.to_path_buf());
            session.cursor = session.entries.len() - 1;
        }
        if session.entries.len() > MAX_SESSION_ENTRIES {
            let excess = session.entries.len() - MAX_SESSION_ENTRIES;
            session.entries.drain(..excess);
            session.cursor = session.cursor.saturating_sub(excess);
        }
        self.save_session(id, &session)
    }

    pub fn back(&self, id: &str) -> Result<Option<PathBuf>> {
        let mut session = self.session(id)?;
        if session.entries.is_empty() || session.cursor == 0 {
            return Ok(None);
        }
        let mut cursor = session.cursor;
        while cursor > 0 {
            cursor -= 1;
            if session.entries[cursor].is_dir() {
                session.cursor = cursor;
                let path = session.entries[cursor].clone();
                self.save_session(id, &session)?;
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    pub fn forward(&self, id: &str) -> Result<Option<PathBuf>> {
        let mut session = self.session(id)?;
        if session.entries.is_empty() || session.cursor + 1 >= session.entries.len() {
            return Ok(None);
        }
        let mut cursor = session.cursor + 1;
        while cursor < session.entries.len() {
            if session.entries[cursor].is_dir() {
                session.cursor = cursor;
                let path = session.entries[cursor].clone();
                self.save_session(id, &session)?;
                return Ok(Some(path));
            }
            cursor += 1;
        }
        Ok(None)
    }

    fn prune_history(&self, maximum: usize, retained: usize) -> Result<()> {
        let read = self.db.begin_read()?;
        let table = read.open_table(HISTORY)?;
        if table.len()? as usize <= maximum {
            return Ok(());
        }
        drop(table);
        drop(read);

        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(HISTORY)?;
            let mut rows = Vec::with_capacity(table.len()? as usize);
            for item in table.iter()? {
                let (key, value) = item?;
                let history: PathHistory = serde_json::from_slice(value.value())?;
                rows.push((
                    key.value().to_owned(),
                    history.last_visit,
                    history.visit_count,
                ));
            }
            rows.sort_unstable_by(|a, b| {
                a.1.cmp(&b.1)
                    .then_with(|| a.2.cmp(&b.2))
                    .then_with(|| a.0.cmp(&b.0))
            });
            let remove = rows.len().saturating_sub(retained.min(maximum));
            for (key, _, _) in rows.into_iter().take(remove) {
                table.remove(key.as_str())?;
            }
        }
        write.commit()?;
        Ok(())
    }

    fn prune_sessions(
        &self,
        maximum: usize,
        retained: usize,
        protected_id: Option<&str>,
    ) -> Result<()> {
        let read = self.db.begin_read()?;
        let table = read.open_table(SESSIONS)?;
        if table.len()? as usize <= maximum {
            return Ok(());
        }
        drop(table);
        drop(read);

        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(SESSIONS)?;
            let mut sessions = Vec::with_capacity(table.len()? as usize);
            for item in table.iter()? {
                let (key, value) = item?;
                let session: NavigationSession = serde_json::from_slice(value.value())?;
                sessions.push((key.value().to_owned(), session.updated_at));
            }
            sessions.sort_unstable_by(|a, b| {
                (protected_id == Some(a.0.as_str()))
                    .cmp(&(protected_id == Some(b.0.as_str())))
                    .then_with(|| a.1.cmp(&b.1))
                    .then_with(|| a.0.cmp(&b.0))
            });
            let remove = sessions.len().saturating_sub(retained.min(maximum));
            for (key, _) in sessions.into_iter().take(remove) {
                table.remove(key.as_str())?;
            }
        }
        write.commit()?;
        Ok(())
    }
}

/// Loads suggestion inputs without taking the exclusive database lock used by
/// state mutations. This keeps concurrent shell redraws independent and never
/// creates state merely because the prompt requested a suggestion.
pub fn read_suggestion_context(
    path: &Path,
) -> Result<(HashMap<String, Bookmark>, HashMap<PathBuf, PathHistory>)> {
    let db = ReadOnlyDatabase::open(path)?;
    let read = db.begin_read()?;

    let bookmark_table = read.open_table(BOOKMARKS)?;
    let mut bookmarks = HashMap::with_capacity(bookmark_table.len()? as usize);
    for item in bookmark_table.iter()? {
        let (_, value) = item?;
        let bookmark: Bookmark = serde_json::from_slice(value.value())?;
        bookmarks.insert(bookmark.name.clone(), bookmark);
    }
    drop(bookmark_table);

    let history_table = read.open_table(HISTORY)?;
    let mut history = HashMap::with_capacity(history_table.len()? as usize);
    for item in history_table.iter()? {
        let (_, value) = item?;
        let row: PathHistory = serde_json::from_slice(value.value())?;
        history.insert(row.path.clone(), row);
    }
    Ok((bookmarks, history))
}

fn recoverable_storage_error(error: &DirgoError) -> bool {
    matches!(
        error,
        DirgoError::Database(_)
            | DirgoError::Storage(_)
            | DirgoError::Table(_)
            | DirgoError::Data(_)
    )
}

fn validate_bookmark_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '.' | '_' | '-'))
    {
        return Err(DirgoError::InvalidBookmark(name.into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(temp: &tempfile::TempDir) -> StateStore {
        StateStore::open(&temp.path().join("state.redb")).expect("state")
    }

    #[test]
    fn suggestion_context_supports_multiple_concurrent_readers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("state.redb");
        drop(StateStore::open(&path).expect("initialize state"));

        let first = ReadOnlyDatabase::open(&path).expect("first reader");
        let context = read_suggestion_context(&path).expect("second reader");

        assert!(context.0.is_empty());
        assert!(context.1.is_empty());
        drop(first);
    }

    #[test]
    fn bookmarks_survive_reopen_and_support_unicode() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("Каталог с пробелом");
        std::fs::create_dir(&path).expect("dir");
        store(&temp).add_bookmark("работа", &path).expect("add");
        assert_eq!(
            store(&temp)
                .bookmark("работа")
                .expect("read")
                .expect("exists")
                .path,
            path.canonicalize().expect("canonical path")
        );
    }

    #[test]
    fn bookmark_repairs_store_an_absolute_path_and_preserve_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        std::fs::create_dir(&first).expect("first");
        std::fs::create_dir(&second).expect("second");
        let store = store(&temp);
        let original = store.add_bookmark("work", &first).expect("original");
        let repaired = store.add_bookmark("work", &second).expect("repair");
        assert!(repaired.path.is_absolute());
        assert_eq!(repaired.created_at, original.created_at);
    }

    #[test]
    fn rename_never_overwrites_an_existing_bookmark() {
        let temp = tempfile::tempdir().expect("tempdir");
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        std::fs::create_dir(&first).expect("first");
        std::fs::create_dir(&second).expect("second");
        let store = store(&temp);
        store.add_bookmark("first", &first).expect("first bookmark");
        store
            .add_bookmark("second", &second)
            .expect("second bookmark");

        let error = store
            .rename_bookmark("first", "second")
            .expect_err("collision must fail");
        assert!(error.to_string().contains("already exists"));
        assert_eq!(
            store
                .bookmark("first")
                .expect("first")
                .expect("present")
                .path,
            first.canonicalize().expect("canonical first")
        );
        assert_eq!(
            store
                .bookmark("second")
                .expect("second")
                .expect("present")
                .path,
            second.canonicalize().expect("canonical second")
        );
    }

    #[test]
    fn navigation_discards_forward_branch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = store(&temp);
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        let c = temp.path().join("c");
        let d = temp.path().join("d");
        for path in [&a, &b, &c, &d] {
            std::fs::create_dir(path).expect("navigation directory");
        }
        store
            .record_navigation(&a, &b, Some("s"))
            .expect("first transition");
        store
            .record_navigation(&b, &c, Some("s"))
            .expect("second transition");
        assert_eq!(store.back("s").expect("back"), Some(b.clone()));
        store.record_navigation(&b, &d, Some("s")).expect("branch");
        assert_eq!(store.forward("s").expect("forward"), None);
        assert_eq!(store.back("s").expect("back"), Some(b));
        assert_eq!(store.back("s").expect("back"), Some(a));
    }

    #[test]
    fn first_transition_preserves_origin_and_forward_destination() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = store(&temp);
        let origin = temp.path().join("origin");
        let destination = temp.path().join("destination");
        std::fs::create_dir(&origin).expect("origin");
        std::fs::create_dir(&destination).expect("destination");

        store
            .record_navigation(&origin, &destination, Some("s"))
            .expect("transition");
        assert_eq!(store.back("s").expect("back"), Some(origin));
        assert_eq!(store.forward("s").expect("forward"), Some(destination));
    }

    #[test]
    fn sessions_are_isolated() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = store(&temp);
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        let x = temp.path().join("x");
        let y = temp.path().join("y");
        for path in [&a, &b, &x, &y] {
            std::fs::create_dir(path).expect("navigation directory");
        }

        store
            .record_navigation(&a, &b, Some("first"))
            .expect("first session");
        store
            .record_navigation(&x, &y, Some("second"))
            .expect("second session");
        assert_eq!(store.back("first").expect("first back"), Some(a));
        assert_eq!(store.back("second").expect("second back"), Some(x));
    }

    #[test]
    fn navigation_skips_deleted_entries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = store(&temp);
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        let c = temp.path().join("c");
        for path in [&a, &b, &c] {
            std::fs::create_dir(path).expect("navigation directory");
        }
        store
            .record_navigation(&a, &b, Some("s"))
            .expect("first transition");
        store
            .record_navigation(&b, &c, Some("s"))
            .expect("second transition");
        std::fs::remove_dir(&b).expect("delete middle entry");

        assert_eq!(store.back("s").expect("back"), Some(a));
        assert_eq!(store.forward("s").expect("forward"), Some(c));
    }

    #[test]
    fn history_import_is_idempotent_and_preserves_native_recency() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = store(&temp);
        let origin = temp.path().join("origin");
        let destination = temp.path().join("destination");
        std::fs::create_dir(&origin).expect("origin");
        std::fs::create_dir(&destination).expect("destination");

        let first = store
            .import_history(&[(destination.clone(), 5)])
            .expect("first import");
        assert_eq!(first.imported, 1);
        let repeated = store
            .import_history(&[(destination.clone(), 5)])
            .expect("repeated import");
        assert_eq!(repeated.unchanged, 1);
        store
            .record_navigation(&origin, &destination, None)
            .expect("native visit");
        let native = store.history(&destination).expect("history").expect("row");
        assert_eq!(native.visit_count, 6);
        assert!(native.last_visit > 0);

        store
            .import_history(&[(destination.clone(), 10)])
            .expect("larger import");
        let merged = store.history(&destination).expect("history").expect("row");
        assert_eq!(merged.visit_count, 10);
        assert_eq!(merged.last_visit, native.last_visit);
        assert_eq!(merged.first_visit, native.first_visit);
    }

    #[test]
    fn concurrent_navigation_updates_do_not_lose_visits() {
        use std::sync::{Arc, Barrier};

        let temp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(store(&temp));
        let origin = temp.path().join("origin");
        let destination = temp.path().join("destination");
        std::fs::create_dir(&origin).expect("origin");
        std::fs::create_dir(&destination).expect("destination");
        let workers = 8;
        let iterations = 40;
        let barrier = Arc::new(Barrier::new(workers));
        let mut threads = Vec::new();
        for _ in 0..workers {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            let origin = origin.clone();
            let destination = destination.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..iterations {
                    store
                        .record_navigation(&origin, &destination, None)
                        .expect("concurrent navigation");
                }
            }));
        }
        for thread in threads {
            thread.join().expect("navigation worker");
        }

        assert_eq!(
            store
                .history(&destination)
                .expect("history")
                .expect("row")
                .visit_count,
            (workers * iterations) as u64
        );
    }

    #[test]
    fn concurrent_renames_cannot_overwrite_the_same_bookmark() {
        use std::sync::{Arc, Barrier};

        let temp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(store(&temp));
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        std::fs::create_dir(&first).expect("first");
        std::fs::create_dir(&second).expect("second");
        store.add_bookmark("first", &first).expect("first bookmark");
        store
            .add_bookmark("second", &second)
            .expect("second bookmark");
        let barrier = Arc::new(Barrier::new(2));
        let mut threads = Vec::new();
        for name in ["first", "second"] {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                store.rename_bookmark(name, "shared")
            }));
        }
        let succeeded = threads
            .into_iter()
            .map(|thread| thread.join().expect("rename worker").is_ok())
            .filter(|succeeded| *succeeded)
            .count();

        assert_eq!(succeeded, 1);
        let bookmarks = store.bookmarks().expect("bookmarks");
        assert_eq!(bookmarks.len(), 2);
        assert!(bookmarks.iter().any(|bookmark| bookmark.name == "shared"));
    }

    #[test]
    fn navigation_sessions_keep_only_the_latest_entries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = store(&temp);
        for index in 0..300 {
            store
                .push_transition(
                    "bounded",
                    &PathBuf::from(format!("entry-{index:03}")),
                    &PathBuf::from(format!("entry-{:03}", index + 1)),
                )
                .expect("transition");
        }

        let session = store.session("bounded").expect("session");
        assert_eq!(session.entries.len(), MAX_SESSION_ENTRIES);
        assert_eq!(session.cursor, MAX_SESSION_ENTRIES - 1);
        assert_eq!(session.entries.last(), Some(&PathBuf::from("entry-300")));
    }

    #[test]
    fn history_pruning_retains_the_strongest_recent_rows() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = store(&temp);
        let write = store.db.begin_write().expect("write");
        {
            let mut table = write.open_table(HISTORY).expect("history table");
            for index in 0..6_u64 {
                let path = PathBuf::from(format!("history-{index}"));
                let row = PathHistory {
                    path,
                    visit_count: index + 1,
                    first_visit: index,
                    last_visit: index,
                };
                let value = serde_json::to_vec(&row).expect("history row");
                table
                    .insert(format!("history-{index}").as_str(), value.as_slice())
                    .expect("insert history");
            }
        }
        write.commit().expect("commit");

        store.prune_history(5, 3).expect("prune history");
        let rows = store.histories().expect("histories");
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter().map(|row| row.last_visit).collect::<Vec<_>>(),
            vec![5, 4, 3]
        );
    }

    #[test]
    fn session_pruning_bounds_abandoned_shell_sessions() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = store(&temp);
        for index in 0..300_u64 {
            store
                .save_session(
                    &format!("session-{index:03}"),
                    &NavigationSession {
                        entries: vec![PathBuf::from(format!("path-{index}"))],
                        cursor: 0,
                        updated_at: index,
                    },
                )
                .expect("save session");
        }

        let read = store.db.begin_read().expect("read");
        let table = read.open_table(SESSIONS).expect("sessions");
        assert!(table.len().expect("session count") <= MAX_SESSIONS as u64);
        assert!(table.get("session-000").expect("old session").is_none());
        assert!(table.get("session-299").expect("new session").is_some());
    }

    #[test]
    fn existing_sessions_without_timestamps_remain_readable() {
        let session: NavigationSession =
            serde_json::from_str(r#"{"entries":["one"],"cursor":0}"#).expect("legacy session");
        assert_eq!(session.entries, vec![PathBuf::from("one")]);
        assert_eq!(session.updated_at, 0);
    }

    #[test]
    fn rejects_unknown_schema_without_overwriting_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("state.redb");
        let db = Database::create(&path).expect("database");
        let write = db.begin_write().expect("write");
        {
            let mut meta = write.open_table(META).expect("meta");
            meta.insert("schema_version", SCHEMA_VERSION + 1)
                .expect("version");
        }
        write.commit().expect("commit");
        drop(db);

        let error = match StateStore::open(&path) {
            Ok(_) => panic!("unsupported schema was accepted"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("schema version 2 is unsupported")
        );
        assert!(path.exists());
        assert!(
            std::fs::read_dir(temp.path())
                .expect("entries")
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().contains("corrupt"))
        );
    }

    #[test]
    fn migrates_schema_zero_without_losing_bookmarks() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("state.redb");
        let bookmark_path = temp.path().join("bookmark");
        std::fs::create_dir(&bookmark_path).expect("bookmark directory");
        let db = Database::create(&path).expect("database");
        let bookmark = Bookmark {
            name: "work".into(),
            path: bookmark_path.clone(),
            created_at: 1,
            last_used: None,
            tags: Vec::new(),
        };
        let write = db.begin_write().expect("write");
        {
            let mut meta = write.open_table(META).expect("meta");
            meta.insert("schema_version", 0).expect("version");
            let mut bookmarks = write.open_table(BOOKMARKS).expect("bookmarks");
            let value = serde_json::to_vec(&bookmark).expect("serialize");
            bookmarks
                .insert("work", value.as_slice())
                .expect("bookmark");
        }
        write.commit().expect("commit");
        drop(db);

        let store = StateStore::open(&path).expect("migrated state");
        assert_eq!(
            store
                .bookmark("work")
                .expect("bookmark")
                .expect("exists")
                .path,
            bookmark_path
        );
        let read = store.db.begin_read().expect("read");
        let meta = read.open_table(META).expect("meta");
        assert_eq!(
            meta.get("schema_version")
                .expect("version")
                .expect("present")
                .value(),
            SCHEMA_VERSION
        );
    }
}
