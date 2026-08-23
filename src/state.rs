use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::{
    DirgoError, Result,
    model::{Bookmark, PathHistory, unix_now},
};

const BOOKMARKS: TableDefinition<&str, &[u8]> = TableDefinition::new("bookmarks");
const HISTORY: TableDefinition<&str, &[u8]> = TableDefinition::new("history");
const SESSIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("sessions");
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NavigationSession {
    entries: Vec<PathBuf>,
    cursor: usize,
}

pub struct StateStore {
    db: Database,
}

impl StateStore {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Database::create(path)?;
        let store = Self { db };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<()> {
        let write = self.db.begin_write()?;
        {
            let mut meta = write.open_table(META)?;
            if meta.get("schema_version")?.is_none() {
                meta.insert("schema_version", 1)?;
            }
            write.open_table(BOOKMARKS)?;
            write.open_table(HISTORY)?;
            write.open_table(SESSIONS)?;
        }
        write.commit()?;
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
        let now = unix_now();
        let bookmark = Bookmark {
            name: name.into(),
            path: path.to_path_buf(),
            created_at: now,
            last_used: None,
            tags: Vec::new(),
        };
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

    pub fn record_visit(&self, path: &Path, session_id: Option<&str>) -> Result<()> {
        let key = path.to_string_lossy();
        let now = unix_now();
        let mut history = self.history(path)?.unwrap_or(PathHistory {
            path: path.to_path_buf(),
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
            self.push_session(session_id, path)?;
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

    fn push_session(&self, id: &str, path: &Path) -> Result<()> {
        let mut session = self.session(id)?;
        if session
            .entries
            .get(session.cursor)
            .is_some_and(|current| current == path)
        {
            return Ok(());
        }
        if !session.entries.is_empty() {
            session.entries.truncate(session.cursor + 1);
        }
        session.entries.push(path.to_path_buf());
        session.cursor = session.entries.len().saturating_sub(1);
        self.save_session(id, &session)
    }

    pub fn back(&self, id: &str) -> Result<Option<PathBuf>> {
        let mut session = self.session(id)?;
        if session.entries.is_empty() || session.cursor == 0 {
            return Ok(None);
        }
        session.cursor -= 1;
        let path = session.entries[session.cursor].clone();
        self.save_session(id, &session)?;
        Ok(Some(path))
    }

    pub fn forward(&self, id: &str) -> Result<Option<PathBuf>> {
        let mut session = self.session(id)?;
        if session.entries.is_empty() || session.cursor + 1 >= session.entries.len() {
            return Ok(None);
        }
        session.cursor += 1;
        let path = session.entries[session.cursor].clone();
        self.save_session(id, &session)?;
        Ok(Some(path))
    }
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
            path
        );
    }

    #[test]
    fn navigation_discards_forward_branch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = store(&temp);
        for name in ["a", "b", "c"] {
            store
                .record_visit(Path::new(name), Some("s"))
                .expect("visit");
        }
        assert_eq!(store.back("s").expect("back"), Some(PathBuf::from("b")));
        store
            .record_visit(Path::new("d"), Some("s"))
            .expect("branch");
        assert_eq!(store.forward("s").expect("forward"), None);
    }
}
