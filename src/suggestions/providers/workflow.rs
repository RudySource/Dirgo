use std::collections::{BTreeSet, HashMap};

use crate::{
    index,
    suggestions::{CommandHistoryEventV2, CommandOutcome},
    workflows::{
        SavedWorkflowV1, WorkflowQuery, WorkflowScope, WorkflowTransitionV1, rank_next_actions,
    },
};

use super::super::{
    ProjectCommandSnapshot, Suggestion, SuggestionRequest, SuggestionSource, TextEdit,
};

#[derive(Debug, Clone, Default)]
pub struct WorkflowSnapshot {
    transitions: Vec<WorkflowTransitionV1>,
    saved: Vec<SavedWorkflowV1>,
    contexts: HashMap<String, WorkflowContext>,
}

#[derive(Debug, Clone)]
struct WorkflowContext {
    predecessors: Vec<String>,
    outcome: CommandOutcome,
}

impl WorkflowSnapshot {
    pub fn new(
        transitions: Vec<WorkflowTransitionV1>,
        saved: Vec<SavedWorkflowV1>,
        mut events: Vec<CommandHistoryEventV2>,
        session_id: &str,
    ) -> Self {
        events.sort_by_key(|event| event.id);
        let mut contexts = HashMap::<String, WorkflowContext>::new();
        for event in events
            .into_iter()
            .filter(|event| event.session_id.as_deref() == Some(session_id))
        {
            let key = event.project_root.as_ref().map_or_else(
                || "global".into(),
                |root| format!("project:{}", root.display()),
            );
            let context = contexts.entry(key).or_insert_with(|| WorkflowContext {
                predecessors: Vec::with_capacity(2),
                outcome: CommandOutcome::Unknown,
            });
            if context.predecessors.len() == 2 {
                context.predecessors.remove(0);
            }
            context.predecessors.push(event.command);
            context.outcome = event.outcome;
        }
        Self {
            transitions,
            saved,
            contexts,
        }
    }

    pub fn next_actions(
        &self,
        scope: WorkflowScope,
        prefix: &str,
        project_commands: BTreeSet<String>,
        limit: usize,
    ) -> Vec<crate::workflows::NextAction> {
        let scope_key = match &scope {
            WorkflowScope::Project(root) => format!("project:{}", root.display()),
            WorkflowScope::Global => "global".into(),
        };
        let Some(context) = self.contexts.get(&scope_key) else {
            return Vec::new();
        };
        rank_next_actions(
            &self.transitions,
            &self.saved,
            &WorkflowQuery {
                scope,
                predecessors: context.predecessors.clone(),
                predecessor_outcome: context.outcome,
                prefix: prefix.to_owned(),
                project_commands,
                limit,
            },
        )
    }

    pub fn saved_workflow(&self, id: u64) -> Option<&SavedWorkflowV1> {
        self.saved.iter().find(|workflow| workflow.id == id)
    }

    pub fn predecessors(&self, scope: &WorkflowScope) -> &[String] {
        let key = match scope {
            WorkflowScope::Project(root) => format!("project:{}", root.display()),
            WorkflowScope::Global => "global".into(),
        };
        self.contexts
            .get(&key)
            .map_or(&[], |context| context.predecessors.as_slice())
    }
}

pub fn workflow_suggestions(
    request: &SuggestionRequest,
    snapshot: Option<&WorkflowSnapshot>,
    project: Option<&ProjectCommandSnapshot>,
) -> Vec<Suggestion> {
    let Some(snapshot) = snapshot else {
        return Vec::new();
    };
    let prefix = request.before_cursor.trim_start();
    if prefix.is_empty() {
        return Vec::new();
    }
    let scope = index::find_project_root(&request.cwd)
        .map(|(root, _)| WorkflowScope::Project(root))
        .unwrap_or(WorkflowScope::Global);
    let project_commands = project
        .filter(|project| project.contains(&request.cwd))
        .map(|project| {
            project
                .commands()
                .iter()
                .map(|command| command.replacement.clone())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let leading = &request.before_cursor[..request.before_cursor.len() - prefix.len()];
    snapshot
        .next_actions(scope, prefix, project_commands, request.max_results)
        .into_iter()
        .map(|action| {
            let id = action.workflow_id.map_or_else(
                || format!("workflow:learned:{:016x}", stable_hash(&action.command)),
                |id| format!("workflow:saved:{id}"),
            );
            let score = match action.source {
                crate::workflows::WorkflowSource::Saved => 39_000.0,
                crate::workflows::WorkflowSource::Learned => 30_000.0,
            } + f64::from(action.confidence);
            Suggestion {
                id,
                edit: TextEdit {
                    expected_before: request.before_cursor.clone(),
                    replacement: format!("{leading}{}", action.command),
                },
                display: action.command,
                description: Some(action.reason),
                source: SuggestionSource::Workflow,
                score,
            }
        })
        .collect()
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    })
}
