use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use redb::{
    Database, ReadOnlyDatabase, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    TableDefinition,
};

use crate::{Result, config::SuggestionsConfig, model::unix_now};

use super::{
    CommandHistoryAggregateV2, CommandHistoryEventV2, CommandOutcome,
    privacy::is_sensitive_command, providers::CommandHistoryEntry,
};

pub const HISTORY_SCHEMA_VERSION: u64 = 2;
const LEGACY_GLOBAL_SCOPE: &str = "legacy_global";
const GLOBAL_SCOPE: &str = "global";
const HISTORY: TableDefinition<&str, &[u8]> = TableDefinition::new("command_history");
const HISTORY_META: TableDefinition<&str, u64> = TableDefinition::new("history_meta");
const EVENTS: TableDefinition<u64, &[u8]> = TableDefinition::new("command_events_v2");
const AGGREGATES: TableDefinition<&str, &[u8]> = TableDefinition::new("command_aggregates_v2");

pub struct CommandHistoryStore {
    db: Database,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandHistoryScope {
    Project(PathBuf),
    Global,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct CommandHistoryStatus {
    pub schema_version: u64,
    pub event_count: usize,
    pub aggregate_count: usize,
}

#[derive(Debug, Clone)]
pub struct CommandHistorySnapshot {
    pub schema_version: u64,
    pub events: Vec<CommandHistoryEventV2>,
    pub aggregates: Vec<CommandHistoryAggregateV2>,
}

impl CommandHistorySnapshot {
    pub fn status(&self) -> CommandHistoryStatus {
        CommandHistoryStatus {
            schema_version: self.schema_version,
            event_count: self.events.len(),
            aggregate_count: self.aggregates.len(),
        }
    }

    pub fn events_in_scope(&self, scope: &CommandHistoryScope) -> Vec<CommandHistoryEventV2> {
        self.events
            .iter()
            .filter(|event| event_matches_scope(event, scope))
            .cloned()
            .collect()
    }

    pub fn aggregates_in_scope(
        &self,
        scope: &CommandHistoryScope,
    ) -> Result<Vec<CommandHistoryAggregateV2>> {
        let project_scope = match scope {
            CommandHistoryScope::Project(root) => Some(scope_key(Some(root))?),
            _ => None,
        };
        Ok(self
            .aggregates
            .iter()
            .filter(|aggregate| match scope {
                CommandHistoryScope::All => true,
                CommandHistoryScope::Global => {
                    aggregate.scope_key == GLOBAL_SCOPE
                        || aggregate.scope_key == LEGACY_GLOBAL_SCOPE
                }
                CommandHistoryScope::Project(_) => {
                    Some(aggregate.scope_key.as_str()) == project_scope.as_deref()
                }
            })
            .cloned()
            .collect())
    }
}

pub fn read_history_snapshot(path: &Path) -> Result<CommandHistorySnapshot> {
    let db = ReadOnlyDatabase::open(path)?;
    let read = db.begin_read()?;
    let meta = read.open_table(HISTORY_META)?;
    let schema_version = meta
        .get("schema_version")?
        .map(|value| value.value())
        .ok_or_else(|| {
            crate::DirgoError::User("command-history schema marker is missing".into())
        })?;
    if schema_version != HISTORY_SCHEMA_VERSION {
        return Err(crate::DirgoError::User(format!(
            "history schema version {schema_version} is unsupported; preserve the file and upgrade Dirgo"
        )));
    }
    let event_table = read.open_table(EVENTS)?;
    let mut events = Vec::with_capacity(event_table.len()? as usize);
    for item in event_table.iter()? {
        let (_, value) = item?;
        events.push(serde_json::from_slice(value.value())?);
    }
    events.sort_by_key(|event: &CommandHistoryEventV2| event.id);
    let aggregate_table = read.open_table(AGGREGATES)?;
    let mut aggregates = Vec::with_capacity(aggregate_table.len()? as usize);
    for item in aggregate_table.iter()? {
        let (_, value) = item?;
        aggregates.push(serde_json::from_slice(value.value())?);
    }
    aggregates.sort_by(|left: &CommandHistoryAggregateV2, right| {
        right
            .last_used
            .cmp(&left.last_used)
            .then_with(|| left.command.cmp(&right.command))
    });
    Ok(CommandHistorySnapshot {
        schema_version,
        events,
        aggregates,
    })
}

impl CommandHistoryStore {
    pub fn open(path: &Path) -> Result<Self> {
        if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(crate::DirgoError::User(
                "refusing to open command history through a symlink".into(),
            ));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| crate::DirgoError::io(parent, error))?;
            set_user_only_directory(parent)?;
        }
        preflight_existing_schema(path)?;
        let db = open_database_with_retry(path)?;
        set_user_only_file(path)?;
        let store = Self {
            db,
            path: path.to_path_buf(),
        };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<()> {
        match self.schema_version_if_present()? {
            Some(HISTORY_SCHEMA_VERSION) => return Ok(()),
            Some(version) => {
                return Err(crate::DirgoError::User(format!(
                    "history schema version {version} is unsupported; preserve the file and upgrade Dirgo"
                )));
            }
            None => {}
        }

        let legacy_exists = self.legacy_table_exists()?;
        let legacy = match self.read_legacy_entries() {
            Ok(entries) => entries,
            Err(error) if legacy_exists => {
                preserve_recovery_copy(&self.path)?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let write = self.db.begin_write()?;
        {
            let mut aggregates = write.open_table(AGGREGATES)?;
            for entry in &legacy {
                let aggregate = CommandHistoryAggregateV2 {
                    scope_key: LEGACY_GLOBAL_SCOPE.into(),
                    command: entry.command.clone(),
                    use_count: entry.use_count,
                    success_count: 0,
                    failure_count: 0,
                    unknown_count: entry.use_count,
                    last_used: entry.last_used,
                    last_success: None,
                    last_failure: None,
                    total_duration_ms: 0,
                    measured_duration_count: 0,
                };
                let key = aggregate_key(&aggregate.scope_key, &aggregate.command)?;
                let encoded = serde_json::to_vec(&aggregate)?;
                aggregates.insert(key.as_str(), encoded.as_slice())?;
            }
        }
        {
            let _events = write.open_table(EVENTS)?;
        }
        {
            let mut meta = write.open_table(HISTORY_META)?;
            meta.insert("schema_version", HISTORY_SCHEMA_VERSION)?;
            meta.insert("next_event_id", 1)?;
            meta.insert("last_successful_migration", unix_now())?;
        }
        if legacy_exists {
            write.delete_table(HISTORY)?;
        }
        write.commit()?;
        Ok(())
    }

    fn schema_version_if_present(&self) -> Result<Option<u64>> {
        let read = self.db.begin_read()?;
        let table = match read.open_table(HISTORY_META) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        Ok(table.get("schema_version")?.map(|value| value.value()))
    }

    fn legacy_table_exists(&self) -> Result<bool> {
        let read = self.db.begin_read()?;
        match read.open_table(HISTORY) {
            Ok(_) => Ok(true),
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    fn read_legacy_entries(&self) -> Result<Vec<CommandHistoryEntry>> {
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
        Ok(entries)
    }

    pub fn schema_version(&self) -> Result<u64> {
        self.schema_version_if_present()?.ok_or_else(|| {
            crate::DirgoError::User("command-history schema marker is missing".into())
        })
    }

    pub fn all_events(&self) -> Result<Vec<CommandHistoryEventV2>> {
        let read = self.db.begin_read()?;
        let table = read.open_table(EVENTS)?;
        let mut events = Vec::<CommandHistoryEventV2>::with_capacity(table.len()? as usize);
        for item in table.iter()? {
            let (_, value) = item?;
            events.push(serde_json::from_slice(value.value())?);
        }
        events.sort_by_key(|event| event.id);
        Ok(events)
    }

    pub fn all_aggregates(&self) -> Result<Vec<CommandHistoryAggregateV2>> {
        let read = self.db.begin_read()?;
        let table = read.open_table(AGGREGATES)?;
        let mut aggregates = Vec::<CommandHistoryAggregateV2>::with_capacity(table.len()? as usize);
        for item in table.iter()? {
            let (_, value) = item?;
            aggregates.push(serde_json::from_slice(value.value())?);
        }
        aggregates.sort_by(|left, right| {
            right
                .last_used
                .cmp(&left.last_used)
                .then_with(|| left.command.cmp(&right.command))
                .then_with(|| left.scope_key.cmp(&right.scope_key))
        });
        Ok(aggregates)
    }

    pub fn status(&self) -> Result<CommandHistoryStatus> {
        Ok(CommandHistoryStatus {
            schema_version: self.schema_version()?,
            event_count: self.all_events()?.len(),
            aggregate_count: self.all_aggregates()?.len(),
        })
    }

    pub fn event(&self, event_id: u64) -> Result<Option<CommandHistoryEventV2>> {
        let read = self.db.begin_read()?;
        let table = read.open_table(EVENTS)?;
        table
            .get(event_id)?
            .map(|value| serde_json::from_slice(value.value()).map_err(Into::into))
            .transpose()
    }

    pub fn events_in_scope(
        &self,
        scope: &CommandHistoryScope,
    ) -> Result<Vec<CommandHistoryEventV2>> {
        Ok(self
            .all_events()?
            .into_iter()
            .filter(|event| event_matches_scope(event, scope))
            .collect())
    }

    pub fn aggregates_in_scope(
        &self,
        scope: &CommandHistoryScope,
    ) -> Result<Vec<CommandHistoryAggregateV2>> {
        let project_scope = match scope {
            CommandHistoryScope::Project(root) => Some(scope_key(Some(root))?),
            _ => None,
        };
        Ok(self
            .all_aggregates()?
            .into_iter()
            .filter(|aggregate| match scope {
                CommandHistoryScope::All => true,
                CommandHistoryScope::Global => {
                    aggregate.scope_key == GLOBAL_SCOPE
                        || aggregate.scope_key == LEGACY_GLOBAL_SCOPE
                }
                CommandHistoryScope::Project(_) => {
                    Some(aggregate.scope_key.as_str()) == project_scope.as_deref()
                }
            })
            .collect())
    }

    pub fn record(&self, command: &str, now: u64, config: &SuggestionsConfig) -> Result<bool> {
        let cwd = std::env::current_dir()
            .ok()
            .and_then(|path| path.canonicalize().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let project_root = crate::index::find_project_root(&cwd).map(|(root, _)| root);
        self.record_event(
            CommandHistoryEventV2 {
                id: 0,
                command: command.trim().to_owned(),
                started_at: now,
                duration_ms: None,
                cwd,
                project_root,
                exit_code: None,
                outcome: CommandOutcome::Unknown,
                session_id: None,
            },
            config,
        )
    }

    pub fn record_event(
        &self,
        mut event: CommandHistoryEventV2,
        config: &SuggestionsConfig,
    ) -> Result<bool> {
        if !config.command_history
            || event.command.starts_with(' ')
            || is_sensitive_command(&event.command, &config.deny_patterns)
        {
            return Ok(false);
        }
        event.command = event.command.trim().to_owned();
        let write = self.db.begin_write()?;
        event.id = {
            let mut meta = write.open_table(HISTORY_META)?;
            let next = meta.get("next_event_id")?.map_or(1, |value| value.value());
            meta.insert("next_event_id", next.saturating_add(1))?;
            next
        };
        {
            let mut events = write.open_table(EVENTS)?;
            let encoded = serde_json::to_vec(&event)?;
            events.insert(event.id, encoded.as_slice())?;
        }
        prune_transaction(&write, event.started_at, config)?;
        write.commit()?;
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
        aggregates_to_entries(self.all_aggregates()?, self.all_events()?, now, config)
    }

    pub fn clear(&self) -> Result<usize> {
        let removed = self.all_aggregates()?.len();
        let write = self.db.begin_write()?;
        clear_events(&write)?;
        clear_aggregates(&write)?;
        {
            let mut meta = write.open_table(HISTORY_META)?;
            meta.insert("next_event_id", 1)?;
        }
        write.commit()?;
        Ok(removed)
    }

    pub fn clear_scope(&self, scope: &CommandHistoryScope) -> Result<usize> {
        if matches!(scope, CommandHistoryScope::All) {
            return self.clear();
        }
        let write = self.db.begin_write()?;
        let all_events = {
            let table = write.open_table(EVENTS)?;
            let mut rows = Vec::with_capacity(table.len()? as usize);
            for item in table.iter()? {
                let (_, value) = item?;
                rows.push(serde_json::from_slice::<CommandHistoryEventV2>(
                    value.value(),
                )?);
            }
            rows
        };
        let all_aggregates = {
            let table = write.open_table(AGGREGATES)?;
            let mut rows = Vec::with_capacity(table.len()? as usize);
            for item in table.iter()? {
                let (_, value) = item?;
                rows.push(serde_json::from_slice::<CommandHistoryAggregateV2>(
                    value.value(),
                )?);
            }
            rows
        };
        let removed = all_events
            .iter()
            .filter(|event| event_matches_scope(event, scope))
            .count()
            + all_aggregates
                .iter()
                .filter(|aggregate| aggregate_matches_scope(aggregate, scope))
                .count();
        let retained_events = all_events
            .into_iter()
            .filter(|event| !event_matches_scope(event, scope))
            .collect::<Vec<_>>();
        let retained_legacy = all_aggregates
            .into_iter()
            .filter(|aggregate| aggregate.scope_key == LEGACY_GLOBAL_SCOPE)
            .filter(|_| !matches!(scope, CommandHistoryScope::Global))
            .collect::<Vec<_>>();
        clear_events(&write)?;
        clear_aggregates(&write)?;
        {
            let mut events = write.open_table(EVENTS)?;
            for event in &retained_events {
                let encoded = serde_json::to_vec(event)?;
                events.insert(event.id, encoded.as_slice())?;
            }
        }
        let mut rebuilt = HashMap::<(String, String), CommandHistoryAggregateV2>::new();
        for aggregate in retained_legacy {
            rebuilt.insert(
                (aggregate.scope_key.clone(), aggregate.command.clone()),
                aggregate,
            );
        }
        for event in &retained_events {
            let event_scope = scope_key(event.project_root.as_deref())?;
            let aggregate = rebuilt
                .entry((event_scope.clone(), event.command.clone()))
                .or_insert_with(|| empty_aggregate(&event_scope, &event.command));
            apply_event(aggregate, event);
        }
        {
            let mut aggregates = write.open_table(AGGREGATES)?;
            for aggregate in rebuilt.into_values() {
                let key = aggregate_key(&aggregate.scope_key, &aggregate.command)?;
                let encoded = serde_json::to_vec(&aggregate)?;
                aggregates.insert(key.as_str(), encoded.as_slice())?;
            }
        }
        write.commit()?;
        Ok(removed)
    }

    fn prune(&self, now: u64, config: &SuggestionsConfig) -> Result<()> {
        let write = self.db.begin_write()?;
        prune_transaction(&write, now, config)?;
        write.commit()?;
        Ok(())
    }
}

fn prune_transaction(
    write: &redb::WriteTransaction,
    now: u64,
    config: &SuggestionsConfig,
) -> Result<()> {
    let cutoff = now.saturating_sub(config.retention_days.saturating_mul(86_400));
    let mut retained = {
        let table = write.open_table(EVENTS)?;
        let mut events = Vec::with_capacity(table.len()? as usize);
        for item in table.iter()? {
            let (_, value) = item?;
            let event: CommandHistoryEventV2 = serde_json::from_slice(value.value())?;
            if event.started_at >= cutoff {
                events.push(event);
            }
        }
        events
    };
    retained.sort_by(|left, right| {
        right
            .started_at
            .cmp(&left.started_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    retained.truncate(config.retention_entries);
    retained.sort_by_key(|event| event.id);
    let legacy = {
        let table = write.open_table(AGGREGATES)?;
        let mut rows = Vec::new();
        for item in table.iter()? {
            let (_, value) = item?;
            let aggregate: CommandHistoryAggregateV2 = serde_json::from_slice(value.value())?;
            if aggregate.scope_key == LEGACY_GLOBAL_SCOPE && aggregate.last_used >= cutoff {
                rows.push(aggregate);
            }
        }
        rows
    };
    clear_events(write)?;
    clear_aggregates(write)?;
    {
        let mut table = write.open_table(EVENTS)?;
        for event in &retained {
            let encoded = serde_json::to_vec(event)?;
            table.insert(event.id, encoded.as_slice())?;
        }
    }
    let mut rebuilt = HashMap::<(String, String), CommandHistoryAggregateV2>::new();
    for aggregate in legacy {
        rebuilt.insert(
            (aggregate.scope_key.clone(), aggregate.command.clone()),
            aggregate,
        );
    }
    for event in &retained {
        let scope = scope_key(event.project_root.as_deref())?;
        let aggregate = rebuilt
            .entry((scope.clone(), event.command.clone()))
            .or_insert_with(|| empty_aggregate(&scope, &event.command));
        apply_event(aggregate, event);
    }
    let mut rebuilt = rebuilt.into_values().collect::<Vec<_>>();
    rebuilt.sort_by(|left, right| {
        right
            .last_used
            .cmp(&left.last_used)
            .then_with(|| right.use_count.cmp(&left.use_count))
            .then_with(|| left.command.cmp(&right.command))
    });
    rebuilt.truncate(config.retention_entries);
    let mut table = write.open_table(AGGREGATES)?;
    for aggregate in rebuilt {
        let key = aggregate_key(&aggregate.scope_key, &aggregate.command)?;
        let encoded = serde_json::to_vec(&aggregate)?;
        table.insert(key.as_str(), encoded.as_slice())?;
    }
    Ok(())
}

fn event_matches_scope(event: &CommandHistoryEventV2, scope: &CommandHistoryScope) -> bool {
    match scope {
        CommandHistoryScope::All => true,
        CommandHistoryScope::Global => event.project_root.is_none(),
        CommandHistoryScope::Project(root) => event.project_root.as_deref() == Some(root.as_path()),
    }
}

fn aggregate_matches_scope(
    aggregate: &CommandHistoryAggregateV2,
    scope: &CommandHistoryScope,
) -> bool {
    match scope {
        CommandHistoryScope::All => true,
        CommandHistoryScope::Global => {
            aggregate.scope_key == GLOBAL_SCOPE || aggregate.scope_key == LEGACY_GLOBAL_SCOPE
        }
        CommandHistoryScope::Project(root) => {
            scope_key(Some(root)).ok().as_deref() == Some(aggregate.scope_key.as_str())
        }
    }
}

fn preflight_existing_schema(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let db = ReadOnlyDatabase::open(path)?;
    let read = db.begin_read()?;
    let meta = match read.open_table(HISTORY_META) {
        Ok(meta) => meta,
        Err(redb::TableError::TableDoesNotExist(_)) => {
            let legacy = match read.open_table(HISTORY) {
                Ok(legacy) => legacy,
                Err(redb::TableError::TableDoesNotExist(_)) => return Ok(()),
                Err(error) => return Err(error.into()),
            };
            let malformed = {
                let mut malformed = None;
                for item in legacy.iter()? {
                    let (_, value) = item?;
                    if let Err(error) = serde_json::from_slice::<CommandHistoryEntry>(value.value())
                    {
                        malformed = Some(error);
                        break;
                    }
                }
                malformed
            };
            if let Some(error) = malformed {
                drop(legacy);
                drop(read);
                drop(db);
                preserve_recovery_copy(path)?;
                return Err(error.into());
            }
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    let version = meta.get("schema_version")?.map(|value| value.value());
    if let Some(version) = version
        && version != HISTORY_SCHEMA_VERSION
    {
        return Err(crate::DirgoError::User(format!(
            "history schema version {version} is unsupported; preserve the file and upgrade Dirgo"
        )));
    }
    Ok(())
}

pub fn read_command_history(
    path: &Path,
    now: u64,
    config: &SuggestionsConfig,
) -> Result<Vec<CommandHistoryEntry>> {
    if !config.command_history || !path.exists() {
        return Ok(Vec::new());
    }
    let db = match ReadOnlyDatabase::open(path) {
        Ok(db) => db,
        Err(redb::DatabaseError::DatabaseAlreadyOpen) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let read = db.begin_read()?;
    let version = match read.open_table(HISTORY_META) {
        Ok(meta) => meta.get("schema_version")?.map(|value| value.value()),
        Err(redb::TableError::TableDoesNotExist(_)) => None,
        Err(error) => return Err(error.into()),
    };
    if version.is_none() {
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
        return filter_legacy_entries(entries, now, config);
    }
    if version != Some(HISTORY_SCHEMA_VERSION) {
        return Err(crate::DirgoError::User(format!(
            "history schema version {} is unsupported; preserve the file and upgrade Dirgo",
            version.unwrap_or_default()
        )));
    }
    let table = read.open_table(AGGREGATES)?;
    let mut aggregates = Vec::with_capacity(table.len()? as usize);
    for item in table.iter()? {
        let (_, value) = item?;
        aggregates.push(serde_json::from_slice(value.value())?);
    }
    const CONTEXT_EVENT_WINDOW: usize = 256;
    let event_table = read.open_table(EVENTS)?;
    let mut events = Vec::with_capacity(CONTEXT_EVENT_WINDOW.min(event_table.len()? as usize));
    for item in event_table.iter()?.rev().take(CONTEXT_EVENT_WINDOW) {
        let (_, value) = item?;
        events.push(serde_json::from_slice(value.value())?);
    }
    events.sort_by_key(|event: &CommandHistoryEventV2| event.id);
    aggregates_to_entries(aggregates, events, now, config)
}

fn aggregates_to_entries(
    aggregates: Vec<CommandHistoryAggregateV2>,
    events: Vec<CommandHistoryEventV2>,
    now: u64,
    config: &SuggestionsConfig,
) -> Result<Vec<CommandHistoryEntry>> {
    let cutoff = now.saturating_sub(config.retention_days.saturating_mul(86_400));
    let mut entries = aggregates
        .into_iter()
        .filter(|aggregate| aggregate.last_used >= cutoff)
        .map(|aggregate| {
            let mut entry = CommandHistoryEntry::new(
                &aggregate.command,
                aggregate.use_count,
                aggregate.last_used,
            );
            entry.scope_key = aggregate.scope_key;
            entry.success_count = aggregate.success_count;
            entry.failure_count = aggregate.failure_count;
            entry.unknown_count = aggregate.unknown_count;
            entry.last_success = aggregate.last_success;
            entry.last_failure = aggregate.last_failure;
            for event in events
                .iter()
                .rev()
                .filter(|event| event.command == entry.command)
            {
                if scope_key(event.project_root.as_deref()).ok().as_deref()
                    != Some(entry.scope_key.as_str())
                {
                    continue;
                }
                if !entry.recent_cwds.contains(&event.cwd) && entry.recent_cwds.len() < 4 {
                    entry.recent_cwds.push(event.cwd.clone());
                }
                if let Some(session) = event.session_id.as_ref()
                    && !entry.recent_sessions.contains(session)
                    && entry.recent_sessions.len() < 4
                {
                    entry.recent_sessions.push(session.clone());
                }
            }
            entry
        })
        .collect::<Vec<_>>();
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

fn filter_legacy_entries(
    mut entries: Vec<CommandHistoryEntry>,
    now: u64,
    config: &SuggestionsConfig,
) -> Result<Vec<CommandHistoryEntry>> {
    let cutoff = now.saturating_sub(config.retention_days.saturating_mul(86_400));
    entries.retain(|entry| entry.last_used >= cutoff);
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

fn empty_aggregate(scope: &str, command: &str) -> CommandHistoryAggregateV2 {
    CommandHistoryAggregateV2 {
        scope_key: scope.into(),
        command: command.into(),
        use_count: 0,
        success_count: 0,
        failure_count: 0,
        unknown_count: 0,
        last_used: 0,
        last_success: None,
        last_failure: None,
        total_duration_ms: 0,
        measured_duration_count: 0,
    }
}

fn apply_event(aggregate: &mut CommandHistoryAggregateV2, event: &CommandHistoryEventV2) {
    aggregate.use_count = aggregate.use_count.saturating_add(1);
    aggregate.last_used = aggregate.last_used.max(event.started_at);
    match event.outcome {
        CommandOutcome::Success => {
            aggregate.success_count = aggregate.success_count.saturating_add(1);
            aggregate.last_success = Some(
                aggregate
                    .last_success
                    .map_or(event.started_at, |value| value.max(event.started_at)),
            );
        }
        CommandOutcome::Failure => {
            aggregate.failure_count = aggregate.failure_count.saturating_add(1);
            aggregate.last_failure = Some(
                aggregate
                    .last_failure
                    .map_or(event.started_at, |value| value.max(event.started_at)),
            );
        }
        CommandOutcome::Unknown => {
            aggregate.unknown_count = aggregate.unknown_count.saturating_add(1);
        }
    }
    if let Some(duration) = event.duration_ms {
        aggregate.total_duration_ms = aggregate.total_duration_ms.saturating_add(duration);
        aggregate.measured_duration_count = aggregate.measured_duration_count.saturating_add(1);
    }
}

fn scope_key(project_root: Option<&Path>) -> Result<String> {
    match project_root {
        Some(root) => {
            let root = root.to_str().ok_or(crate::DirgoError::NonUtf8Path)?;
            Ok(format!("project:{root}"))
        }
        None => Ok(GLOBAL_SCOPE.into()),
    }
}

fn aggregate_key(scope: &str, command: &str) -> Result<String> {
    Ok(serde_json::to_string(&(scope, command))?)
}

fn clear_events(write: &redb::WriteTransaction) -> Result<()> {
    let mut table = write.open_table(EVENTS)?;
    let keys = table
        .iter()?
        .map(|item| item.map(|(key, _)| key.value()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for key in keys {
        table.remove(key)?;
    }
    Ok(())
}

fn clear_aggregates(write: &redb::WriteTransaction) -> Result<()> {
    let mut table = write.open_table(AGGREGATES)?;
    let keys = table
        .iter()?
        .map(|item| item.map(|(key, _)| key.value().to_owned()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for key in keys {
        table.remove(key.as_str())?;
    }
    Ok(())
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

fn preserve_recovery_copy(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(crate::DirgoError::NonUtf8Path)?;
    let recovery = parent.join(format!(
        "{name}.recovery-{}-{}",
        unix_now(),
        std::process::id()
    ));
    std::fs::copy(path, &recovery).map_err(|error| crate::DirgoError::io(&recovery, error))?;
    set_user_only_file(&recovery)?;
    Ok(recovery)
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
