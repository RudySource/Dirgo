use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::suggestions::CommandOutcome;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowScope {
    Project(PathBuf),
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowStep {
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowTransitionV1 {
    pub scope_key: String,
    pub predecessors: Vec<String>,
    pub predecessor_outcome: CommandOutcome,
    pub next_command: String,
    pub observations: u64,
    pub evidence_sessions: Vec<String>,
    pub next_successes: u64,
    pub next_failures: u64,
    pub next_unknown: u64,
    pub first_seen: u64,
    pub last_seen: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedWorkflowV1 {
    pub id: u64,
    pub name: String,
    pub scope_key: String,
    pub steps: Vec<WorkflowStep>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowSource {
    Learned,
    Saved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NextAction {
    pub command: String,
    pub source: WorkflowSource,
    pub workflow_id: Option<u64>,
    pub confidence: u16,
    pub reason: String,
}
