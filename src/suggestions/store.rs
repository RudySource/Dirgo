use std::{path::Path, thread, time::Duration};

use redb::{
    Database, ReadOnlyDatabase, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    TableDefinition,
};

use crate::{Result, config::SuggestionsConfig};

use super::{CommandHistoryEntry, privacy::is_sensitive_command};

const HISTORY: TableDefinition<&str, &[u8]> = TableDefinition::new("command_history");

pub struct CommandHistoryStore {
    db: Database,
}

impl CommandHistoryStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| crate::DirgoError::io(parent, error))?;
            set_user_only_directory(parent)?;
        }
        let db = open_database_with_retry(path)?;
        set_user_only_file(path)?;
        Ok(Self { db })
    }

    pub fn record(&self, command: &str, now: u64, config: &SuggestionsConfig) -> Result<bool> {
        let command = command.trim();
        if !config.command_history || is_sensitive_command(command, &config.deny_patterns) {
            return Ok(false);
        }

        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(HISTORY)?;
            let current = table
                .get(command)?
                .map(|value| serde_json::from_slice::<CommandHistoryEntry>(value.value()))
                .transpose()?;
            let entry = CommandHistoryEntry {
                command: command.to_owned(),
                use_count: current
                    .as_ref()
                    .map_or(1, |entry| entry.use_count.saturating_add(1)),
                last_used: now,
            };
            let encoded = serde_json::to_vec(&entry)?;
            table.insert(command, encoded.as_slice())?;
        }
        write.commit()?;
        self.prune(now, config)?;
        Ok(true)
    }

    pub fn entries(
        &self,
        now: u64,
        config: &SuggestionsConfig,
    ) -> Result<Vec<CommandHistoryEntry>> {
        if !config.command_history {
            return Ok(Vec::new());
        }
        self.prune(now, config)?;
        let read = self.db.begin_read()?;
        let table = match read.open_table(HISTORY) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut entries = Vec::with_capacity(table.len()? as usize);
        for item in table.iter()? {
            let (_, value) = item?;
            entries.push(serde_json::from_slice(value.value())?);
        }
        entries.sort_by(|left: &CommandHistoryEntry, right| {
            right
                .last_used
                .cmp(&left.last_used)
                .then_with(|| right.use_count.cmp(&left.use_count))
                .then_with(|| left.command.cmp(&right.command))
        });
        Ok(entries)
    }

    pub fn clear(&self) -> Result<usize> {
        let write = self.db.begin_write()?;
        let removed;
        {
            let mut table = write.open_table(HISTORY)?;
            let keys: Vec<String> = table
                .iter()?
                .map(|item| item.map(|(key, _)| key.value().to_owned()))
                .collect::<std::result::Result<_, _>>()?;
            removed = keys.len();
            for key in keys {
                table.remove(key.as_str())?;
            }
        }
        write.commit()?;
        Ok(removed)
    }

    fn prune(&self, now: u64, config: &SuggestionsConfig) -> Result<()> {
        let cutoff = now.saturating_sub(config.retention_days.saturating_mul(86_400));
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(HISTORY)?;
            let mut rows = Vec::<(String, u64)>::with_capacity(table.len()? as usize);
            for item in table.iter()? {
                let (key, value) = item?;
                let entry: CommandHistoryEntry = serde_json::from_slice(value.value())?;
                rows.push((key.value().to_owned(), entry.last_used));
            }
            rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
            for (index, (key, last_used)) in rows.into_iter().enumerate() {
                if last_used < cutoff || index >= config.retention_entries {
                    table.remove(key.as_str())?;
                }
            }
        }
        write.commit()?;
        Ok(())
    }
}

pub fn read_command_history(
    path: &Path,
    now: u64,
    config: &SuggestionsConfig,
) -> Result<Vec<CommandHistoryEntry>> {
    if !config.command_history || !path.exists() {
        return Ok(Vec::new());
    }
    let cutoff = now.saturating_sub(config.retention_days.saturating_mul(86_400));
    let db = match ReadOnlyDatabase::open(path) {
        Ok(db) => db,
        Err(redb::DatabaseError::DatabaseAlreadyOpen) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let read = db.begin_read()?;
    let table = match read.open_table(HISTORY) {
        Ok(table) => table,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut entries = Vec::with_capacity(table.len()? as usize);
    for item in table.iter()? {
        let (_, value) = item?;
        let entry: CommandHistoryEntry = serde_json::from_slice(value.value())?;
        if entry.last_used >= cutoff {
            entries.push(entry);
        }
    }
    entries.sort_by(|left, right| {
        right
            .last_used
            .cmp(&left.last_used)
            .then_with(|| right.use_count.cmp(&left.use_count))
            .then_with(|| left.command.cmp(&right.command))
    });
    entries.truncate(config.retention_entries);
    Ok(entries)
}

fn open_database_with_retry(path: &Path) -> Result<Database> {
    for attempt in 0..6 {
        match Database::create(path) {
            Ok(db) => return Ok(db),
            Err(redb::DatabaseError::DatabaseAlreadyOpen) if attempt < 5 => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!("bounded database retry always returns")
}

#[cfg(unix)]
fn set_user_only_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| crate::DirgoError::io(path, error))
}

#[cfg(not(unix))]
fn set_user_only_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_user_only_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| crate::DirgoError::io(path, error))
}

#[cfg(not(unix))]
fn set_user_only_file(_path: &Path) -> Result<()> {
    Ok(())
}
