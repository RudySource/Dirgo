use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::{
    DirgoError, Result,
    model::{Bookmark, PathHistory, unix_now},
    paths,
};

const BOOKMARKS: TableDefinition<&str, &[u8]> = TableDefinition::new("bookmarks");
const HISTORY: TableDefinition<&str, &[u8]> = TableDefinition::new("history");
const SESSIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("sessions");
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
const SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NavigationSession {
    entries: Vec<PathBuf>,
    cursor: usize,
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
                    backup.display()
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
                path.display()
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
        let Some(mut bookmark) = self.bookmark(old)? else {
            return Err(DirgoError::BookmarkMissing(old.into()));
        };
        if old != new && self.bookmark(new)?.is_some() {
            return Err(DirgoError::User(format!(
                "bookmark @{new} already exists; remove it explicitly before renaming"
            )));
        }
        bookmark.name = new.into();
        let value = serde_json::to_vec(&bookmark)?;
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(BOOKMARKS)?;
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
        let mut history = self.history(destination)?.unwrap_or(PathHistory {
            path: destination.to_path_buf(),
            visit_count: 0,
            first_visit: now,
            last_visit: now,
        });
        history.visit_count += 1;
        history.last_visit = now;
        let value = serde_json::to_vec(&history)?;
        let write = self.db.begin_write()?;
        {
            write
                .open_table(HISTORY)?
                .insert(key.as_ref(), value.as_slice())?;
        }
        write.commit()?;
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
        Ok(HistoryImportSummary {
            imported,
            unchanged,
        })
    }

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
        let value = serde_json::to_vec(session)?;
        let write = self.db.begin_write()?;
        {
            write.open_table(SESSIONS)?.insert(id, value.as_slice())?;
        }
        write.commit()?;
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
