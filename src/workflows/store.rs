use std::path::Path;

use redb::{
    ReadOnlyDatabase, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition,
};

use crate::{
    Result,
    suggestions::{CommandHistoryStore, CommandOutcome},
    workflows::{SavedWorkflowV1, WorkflowTransitionV1, rebuild_transitions},
};

const MAX_SAVED_WORKFLOWS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkflowStatus {
    pub schema_version: u64,
    pub learned_count: usize,
    pub saved_count: usize,
    pub last_rebuild: u64,
}

#[derive(Debug, Clone)]
pub struct WorkflowStorageSnapshot {
    pub status: WorkflowStatus,
    pub transitions: Vec<WorkflowTransitionV1>,
    pub saved: Vec<SavedWorkflowV1>,
}

pub fn read_workflow_snapshot(path: &Path) -> Result<WorkflowStorageSnapshot> {
    let db = ReadOnlyDatabase::open(path)?;
    let read = db.begin_read()?;
    let meta = read.open_table(HISTORY_META)?;
    let schema_version = meta
        .get("schema_version")?
        .map(|value| value.value())
        .ok_or_else(|| crate::DirgoError::User("workflow schema marker is missing".into()))?;
    if !matches!(schema_version, 2 | WORKFLOW_SCHEMA_VERSION) {
        return Err(crate::DirgoError::User(format!(
            "workflow schema version {schema_version} is unsupported; preserve the file and upgrade Dirgo"
        )));
    }
    let last_rebuild = meta
        .get("last_workflow_rebuild")?
        .map_or(0, |value| value.value());
    let mut transitions = Vec::new();
    match read.open_table(WORKFLOW_TRANSITIONS) {
        Ok(table) => {
            transitions.reserve(table.len()? as usize);
            for item in table.iter()? {
                let (_, value) = item?;
                let transition = serde_json::from_slice(value.value())?;
                validate_transition(&transition)?;
                transitions.push(transition);
            }
        }
        Err(redb::TableError::TableDoesNotExist(_)) if schema_version == 2 => {}
        Err(error) => return Err(error.into()),
    }
    transitions.sort_by(|left: &WorkflowTransitionV1, right| {
        left.scope_key
            .cmp(&right.scope_key)
            .then_with(|| left.predecessors.cmp(&right.predecessors))
            .then_with(|| left.next_command.cmp(&right.next_command))
    });
    let mut saved = Vec::new();
    match read.open_table(SAVED_WORKFLOWS) {
        Ok(table) => {
            saved.reserve(table.len()? as usize);
            for item in table.iter()? {
                let (_, value) = item?;
                let workflow = serde_json::from_slice(value.value())?;
                validate_saved_workflow(&workflow)?;
                saved.push(workflow);
            }
        }
        Err(redb::TableError::TableDoesNotExist(_)) if schema_version == 2 => {}
        Err(error) => return Err(error.into()),
    }
    saved.sort_by_key(|workflow: &SavedWorkflowV1| workflow.id);
    Ok(WorkflowStorageSnapshot {
        status: WorkflowStatus {
            schema_version,
            learned_count: transitions.len(),
            saved_count: saved.len(),
            last_rebuild,
        },
        transitions,
        saved,
    })
}

pub const WORKFLOW_SCHEMA_VERSION: u64 = 3;
const WORKFLOW_RETENTION_SECONDS: u64 = 180 * 86_400;
const MAX_COMMAND_BYTES: usize = 65_536;
const MAX_SESSION_BYTES: usize = 256;
const HISTORY_META: TableDefinition<&str, u64> = TableDefinition::new("history_meta");
const EVENTS: TableDefinition<u64, &[u8]> = TableDefinition::new("command_events_v2");
pub(crate) const WORKFLOW_TRANSITIONS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("workflow_transitions_v1");
pub(crate) const SAVED_WORKFLOWS: TableDefinition<u64, &[u8]> =
    TableDefinition::new("saved_workflows_v1");

pub struct WorkflowStore {
    history: CommandHistoryStore,
}

impl WorkflowStore {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            history: CommandHistoryStore::open(path)?,
        })
    }

    pub fn schema_version(&self) -> Result<u64> {
        self.history.schema_version()
    }

    pub fn transitions(&self) -> Result<Vec<WorkflowTransitionV1>> {
        let read = self.history.database().begin_read()?;
        let table = read.open_table(WORKFLOW_TRANSITIONS)?;
        let mut rows = Vec::with_capacity(table.len()? as usize);
        for item in table.iter()? {
            let (_, value) = item?;
            let transition = serde_json::from_slice(value.value())?;
            validate_transition(&transition)?;
            rows.push(transition);
        }
        rows.sort_by(|left: &WorkflowTransitionV1, right| {
            left.scope_key
                .cmp(&right.scope_key)
                .then_with(|| left.predecessors.cmp(&right.predecessors))
                .then_with(|| left.next_command.cmp(&right.next_command))
        });
        Ok(rows)
    }

    pub fn rebuild_transitions(&self, now: u64) -> Result<usize> {
        let write = self.history.database().begin_write()?;
        let count = rebuild_transition_table(&write, now)?;
        write.commit()?;
        Ok(count)
    }

    pub fn saved_workflows(&self) -> Result<Vec<SavedWorkflowV1>> {
        let read = self.history.database().begin_read()?;
        let table = read.open_table(SAVED_WORKFLOWS)?;
        let mut rows = Vec::with_capacity(table.len()? as usize);
        for item in table.iter()? {
            let (_, value) = item?;
            let workflow = serde_json::from_slice(value.value())?;
            validate_saved_workflow(&workflow)?;
            rows.push(workflow);
        }
        rows.sort_by_key(|workflow: &SavedWorkflowV1| workflow.id);
        Ok(rows)
    }

    pub fn status(&self) -> Result<WorkflowStatus> {
        let read = self.history.database().begin_read()?;
        let meta = read.open_table(HISTORY_META)?;
        Ok(WorkflowStatus {
            schema_version: self.schema_version()?,
            learned_count: read.open_table(WORKFLOW_TRANSITIONS)?.len()? as usize,
            saved_count: read.open_table(SAVED_WORKFLOWS)?.len()? as usize,
            last_rebuild: meta
                .get("last_workflow_rebuild")?
                .map_or(0, |value| value.value()),
        })
    }

    pub fn save_workflow(
        &self,
        name: &str,
        scope_key: &str,
        steps: Vec<crate::workflows::WorkflowStep>,
        now: u64,
    ) -> Result<SavedWorkflowV1> {
        validate_name(name)?;
        let write = self.history.database().begin_write()?;
        let id = {
            let table = write.open_table(SAVED_WORKFLOWS)?;
            if table.len()? as usize >= MAX_SAVED_WORKFLOWS {
                return Err(crate::DirgoError::User(
                    "saved workflow limit reached (256); remove one before saving another".into(),
                ));
            }
            for item in table.iter()? {
                let (_, value) = item?;
                let existing: SavedWorkflowV1 = serde_json::from_slice(value.value())?;
                if existing.scope_key == scope_key && existing.name == name {
                    return Err(crate::DirgoError::User(format!(
                        "workflow name {name:?} already exists in this scope"
                    )));
                }
            }
            let mut meta = write.open_table(HISTORY_META)?;
            let id = meta
                .get("next_saved_workflow_id")?
                .map_or(1, |value| value.value());
            meta.insert("next_saved_workflow_id", id.saturating_add(1))?;
            id
        };
        let workflow = SavedWorkflowV1 {
            id,
            name: name.to_owned(),
            scope_key: scope_key.to_owned(),
            steps,
            created_at: now,
            updated_at: now,
        };
        validate_saved_workflow(&workflow)?;
        {
            let mut table = write.open_table(SAVED_WORKFLOWS)?;
            let encoded = serde_json::to_vec(&workflow)?;
            table.insert(id, encoded.as_slice())?;
        }
        write.commit()?;
        Ok(workflow)
    }

    pub fn rename_workflow(&self, id: u64, name: &str, now: u64) -> Result<()> {
        validate_name(name)?;
        let write = self.history.database().begin_write()?;
        let mut workflow = {
            let table = write.open_table(SAVED_WORKFLOWS)?;
            let Some(value) = table.get(id)? else {
                return Err(crate::DirgoError::User(format!(
                    "saved workflow {id} does not exist"
                )));
            };
            serde_json::from_slice::<SavedWorkflowV1>(value.value())?
        };
        {
            let table = write.open_table(SAVED_WORKFLOWS)?;
            for item in table.iter()? {
                let (_, value) = item?;
                let existing: SavedWorkflowV1 = serde_json::from_slice(value.value())?;
                if existing.id != id
                    && existing.scope_key == workflow.scope_key
                    && existing.name == name
                {
                    return Err(crate::DirgoError::User(format!(
                        "workflow name {name:?} already exists in this scope"
                    )));
                }
            }
        }
        workflow.name = name.to_owned();
        workflow.updated_at = now;
        validate_saved_workflow(&workflow)?;
        {
            let mut table = write.open_table(SAVED_WORKFLOWS)?;
            let encoded = serde_json::to_vec(&workflow)?;
            table.insert(id, encoded.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn remove_workflow(&self, id: u64) -> Result<()> {
        let write = self.history.database().begin_write()?;
        let removed = write.open_table(SAVED_WORKFLOWS)?.remove(id)?.is_some();
        if !removed {
            return Err(crate::DirgoError::User(format!(
                "saved workflow {id} does not exist"
            )));
        }
        write.commit()?;
        Ok(())
    }

    pub fn clear_learned(&self, scope_key: Option<&str>) -> Result<usize> {
        let write = self.history.database().begin_write()?;
        let removed = {
            let mut table = write.open_table(WORKFLOW_TRANSITIONS)?;
            let mut keys = Vec::with_capacity(table.len()? as usize);
            for item in table.iter()? {
                let (key, value) = item?;
                let transition = serde_json::from_slice::<WorkflowTransitionV1>(value.value())?;
                validate_transition(&transition)?;
                keys.push((key.value().to_owned(), transition));
            }
            let mut removed = 0;
            for (key, transition) in keys {
                if scope_key.is_none_or(|scope| transition.scope_key == scope) {
                    table.remove(key.as_str())?;
                    removed += 1;
                }
            }
            removed
        };
        write.commit()?;
        Ok(removed)
    }
}

pub(crate) fn create_schema_tables(write: &redb::WriteTransaction) -> Result<()> {
    let _transitions = write.open_table(WORKFLOW_TRANSITIONS)?;
    let _saved = write.open_table(SAVED_WORKFLOWS)?;
    let mut meta = write.open_table(HISTORY_META)?;
    if meta.get("next_saved_workflow_id")?.is_none() {
        meta.insert("next_saved_workflow_id", 1)?;
    }
    if meta.get("last_workflow_rebuild")?.is_none() {
        meta.insert("last_workflow_rebuild", 0)?;
    }
    Ok(())
}

pub(crate) fn clear_transition_table(write: &redb::WriteTransaction) -> Result<()> {
    let mut table = write.open_table(WORKFLOW_TRANSITIONS)?;
    let keys = table
        .iter()?
        .map(|item| item.map(|(key, _)| key.value().to_owned()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for key in keys {
        table.remove(key.as_str())?;
    }
    Ok(())
}

pub(crate) fn rebuild_transition_table(write: &redb::WriteTransaction, now: u64) -> Result<usize> {
    let cutoff = now.saturating_sub(WORKFLOW_RETENTION_SECONDS);
    let events = {
        let table = write.open_table(EVENTS)?;
        let mut events = Vec::with_capacity(table.len()? as usize);
        for item in table.iter()? {
            let (_, value) = item?;
            let event: crate::suggestions::CommandHistoryEventV2 =
                serde_json::from_slice(value.value())?;
            if event.started_at >= cutoff && event.started_at <= now {
                events.push(event);
            }
        }
        events
    };
    let transitions = rebuild_transitions(events)?;
    {
        clear_transition_table(write)?;
        let mut table = write.open_table(WORKFLOW_TRANSITIONS)?;
        for transition in &transitions {
            let key = transition_key(transition)?;
            let encoded = serde_json::to_vec(transition)?;
            table.insert(key.as_str(), encoded.as_slice())?;
        }
    }
    {
        let mut meta = write.open_table(HISTORY_META)?;
        meta.insert("last_workflow_rebuild", now)?;
    }
    Ok(transitions.len())
}

fn validate_transition(transition: &WorkflowTransitionV1) -> Result<()> {
    let valid_scope = transition.scope_key == "global"
        || transition
            .scope_key
            .strip_prefix("project:")
            .is_some_and(valid_scalar);
    let valid_predecessors = matches!(transition.predecessors.len(), 1 | 2)
        && transition
            .predecessors
            .iter()
            .all(|command| valid_command(command));
    let sessions_are_valid = !transition.evidence_sessions.is_empty()
        && transition.evidence_sessions.len() <= 8
        && transition
            .evidence_sessions
            .iter()
            .all(|session| session.len() <= MAX_SESSION_BYTES && valid_scalar(session))
        && transition
            .evidence_sessions
            .windows(2)
            .all(|pair| pair[0] < pair[1]);
    let outcomes = transition
        .next_successes
        .saturating_add(transition.next_failures)
        .saturating_add(transition.next_unknown);
    if !valid_scope
        || !valid_predecessors
        || !valid_command(&transition.next_command)
        || transition.observations == 0
        || outcomes != transition.observations
        || !sessions_are_valid
        || transition.first_seen > transition.last_seen
    {
        return Err(crate::DirgoError::User(
            "workflow transition row is malformed; preserve the database and rebuild learned workflows"
                .into(),
        ));
    }
    Ok(())
}

pub fn validate_name(name: &str) -> Result<()> {
    let visible = name.chars().count();
    let hostile = name.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
    });
    if name.trim() != name || !(1..=64).contains(&visible) || hostile {
        return Err(crate::DirgoError::User(
            "workflow name must contain 1 to 64 visible characters without controls or bidi overrides"
                .into(),
        ));
    }
    Ok(())
}

fn validate_saved_workflow(workflow: &SavedWorkflowV1) -> Result<()> {
    validate_name(&workflow.name)?;
    if workflow.id == 0
        || !(2..=8).contains(&workflow.steps.len())
        || !(workflow.scope_key == "global"
            || workflow
                .scope_key
                .strip_prefix("project:")
                .is_some_and(valid_scalar))
        || workflow
            .steps
            .iter()
            .any(|step| !valid_command(&step.command))
        || workflow.created_at > workflow.updated_at
    {
        return Err(crate::DirgoError::User(
            "saved workflow row is malformed; preserve the database and inspect it before recovery"
                .into(),
        ));
    }
    Ok(())
}

fn valid_command(command: &str) -> bool {
    command == command.trim()
        && !command.is_empty()
        && command.len() <= MAX_COMMAND_BYTES
        && valid_scalar(command)
        && !crate::suggestions::is_sensitive_command(command, &[])
}

fn valid_scalar(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}

fn transition_key(transition: &WorkflowTransitionV1) -> Result<String> {
    Ok(serde_json::to_string(&(
        &transition.scope_key,
        &transition.predecessors,
        outcome_key(transition.predecessor_outcome),
        &transition.next_command,
    ))?)
}

fn outcome_key(outcome: CommandOutcome) -> u8 {
    match outcome {
        CommandOutcome::Success => 0,
        CommandOutcome::Failure => 1,
        CommandOutcome::Unknown => 2,
    }
}
