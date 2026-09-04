use std::collections::{BTreeMap, BTreeSet};

use crate::suggestions::CommandOutcome;

use super::{NextAction, SavedWorkflowV1, WorkflowScope, WorkflowSource, WorkflowTransitionV1};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowQuery {
    pub scope: WorkflowScope,
    pub predecessors: Vec<String>,
    pub predecessor_outcome: CommandOutcome,
    pub prefix: String,
    pub project_commands: BTreeSet<String>,
    pub limit: usize,
}

pub fn rank_next_actions(
    transitions: &[WorkflowTransitionV1],
    saved_workflows: &[SavedWorkflowV1],
    query: &WorkflowQuery,
) -> Vec<NextAction> {
    let scope_key = query_scope_key(&query.scope);
    let mut saved = saved_candidates(saved_workflows, query, &scope_key);
    let saved_commands = saved
        .iter()
        .map(|candidate| candidate.action.command.clone())
        .collect::<BTreeSet<_>>();
    let mut learned = learned_candidates(transitions, query, &scope_key)
        .into_iter()
        .filter(|candidate| !saved_commands.contains(&candidate.action.command))
        .collect::<Vec<_>>();

    let has_local = saved
        .iter()
        .chain(&learned)
        .any(|candidate| !candidate.global);
    if has_local {
        let mut retained_global = false;
        saved.retain(|candidate| {
            if !candidate.global {
                return true;
            }
            if retained_global {
                false
            } else {
                retained_global = true;
                true
            }
        });
        learned.retain(|candidate| {
            if !candidate.global {
                return true;
            }
            if retained_global {
                false
            } else {
                retained_global = true;
                true
            }
        });
    }

    saved.sort_by(|left, right| {
        left.global
            .cmp(&right.global)
            .then_with(|| left.action.workflow_id.cmp(&right.action.workflow_id))
            .then_with(|| left.action.command.cmp(&right.action.command))
    });
    learned.sort_by(|left, right| {
        left.global
            .cmp(&right.global)
            .then_with(|| right.context_len.cmp(&left.context_len))
            .then_with(|| right.action.confidence.cmp(&left.action.confidence))
            .then_with(|| left.action.command.cmp(&right.action.command))
    });
    saved
        .into_iter()
        .chain(learned)
        .map(|candidate| candidate.action)
        .take(query.limit.clamp(1, 20))
        .collect()
}

#[derive(Debug, Clone)]
struct RankedAction {
    action: NextAction,
    global: bool,
    context_len: usize,
}

fn saved_candidates(
    workflows: &[SavedWorkflowV1],
    query: &WorkflowQuery,
    scope_key: &str,
) -> Vec<RankedAction> {
    let mut candidates = Vec::new();
    for workflow in workflows {
        let global = workflow.scope_key == "global";
        if workflow.scope_key != scope_key && !(global && scope_key != "global") {
            continue;
        }
        for next_index in 1..workflow.steps.len() {
            let context_len = next_index.min(2);
            if !matches_saved_predecessors(
                &query.predecessors,
                &workflow.steps[next_index - context_len..next_index],
            ) {
                continue;
            }
            let command = &workflow.steps[next_index].command;
            if !matches_prefix(command, &query.prefix) || query.project_commands.contains(command) {
                continue;
            }
            candidates.push(RankedAction {
                action: NextAction {
                    command: command.clone(),
                    source: WorkflowSource::Saved,
                    workflow_id: Some(workflow.id),
                    confidence: if global { 950 } else { 1_000 },
                    reason: format!(
                        "Saved workflow · {} · inserted, never executed",
                        workflow.name
                    ),
                },
                global,
                context_len,
            });
            break;
        }
    }
    candidates
}

fn learned_candidates(
    transitions: &[WorkflowTransitionV1],
    query: &WorkflowQuery,
    scope_key: &str,
) -> Vec<RankedAction> {
    let mut by_command = BTreeMap::<String, RankedAction>::new();
    for transition in transitions {
        if transition.observations < 3
            || transition.evidence_sessions.len() < 2
            || transition.predecessor_outcome != query.predecessor_outcome
            || !matches_predecessors(&query.predecessors, &transition.predecessors)
            || !matches_prefix(&transition.next_command, &query.prefix)
            || query.project_commands.contains(&transition.next_command)
        {
            continue;
        }
        let global = transition.scope_key == "global";
        if transition.scope_key != scope_key && !(global && scope_key != "global") {
            continue;
        }
        let known = transition
            .next_successes
            .saturating_add(transition.next_failures);
        let success_percent = if known == 0 {
            0
        } else {
            transition.next_successes.saturating_mul(100) / known
        };
        let confidence = learned_confidence(transition, global, success_percent);
        let action = RankedAction {
            action: NextAction {
                command: transition.next_command.clone(),
                source: WorkflowSource::Learned,
                workflow_id: None,
                confidence,
                reason: format!(
                    "Next in {} · {} times · {}% successful",
                    if global {
                        "global history"
                    } else {
                        "this project"
                    },
                    transition.observations,
                    success_percent
                ),
            },
            global,
            context_len: transition.predecessors.len(),
        };
        match by_command.entry(transition.next_command.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(action);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let current = entry.get();
                if action.context_len > current.context_len
                    || (action.context_len == current.context_len
                        && action.action.confidence > current.action.confidence)
                {
                    entry.insert(action);
                }
            }
        }
    }
    by_command.into_values().collect()
}

fn learned_confidence(
    transition: &WorkflowTransitionV1,
    global: bool,
    success_percent: u64,
) -> u16 {
    let scope = if global { 0 } else { 100 };
    let context = if transition.predecessors.len() == 2 {
        400
    } else {
        0
    };
    let observations = transition.observations.min(20) * 12;
    let sessions = transition.evidence_sessions.len().min(8) as u64 * 20;
    let outcome = success_percent * 2;
    (scope + context + observations + sessions + outcome).min(999) as u16
}

fn matches_predecessors(actual: &[String], expected: &[String]) -> bool {
    actual.len() >= expected.len()
        && actual[actual.len() - expected.len()..]
            .iter()
            .eq(expected.iter())
}

fn matches_saved_predecessors(actual: &[String], expected: &[super::WorkflowStep]) -> bool {
    actual.len() >= expected.len()
        && actual[actual.len() - expected.len()..]
            .iter()
            .map(String::as_str)
            .eq(expected.iter().map(|step| step.command.as_str()))
}

fn matches_prefix(command: &str, prefix: &str) -> bool {
    prefix.is_empty() || command.starts_with(prefix)
}

fn query_scope_key(scope: &WorkflowScope) -> String {
    match scope {
        WorkflowScope::Project(root) => format!("project:{}", root.display()),
        WorkflowScope::Global => "global".into(),
    }
}
