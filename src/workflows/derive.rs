use std::collections::BTreeMap;

use crate::{
    DirgoError, Result,
    suggestions::{CommandHistoryEventV2, CommandOutcome, is_sensitive_command},
};

use super::WorkflowTransitionV1;

const MAX_TRANSITION_GAP_SECONDS: u64 = 30 * 60;
const MAX_EVIDENCE_SESSIONS: usize = 8;
pub(crate) const MAX_WORKFLOW_TRANSITIONS: usize = 10_000;

pub fn rebuild_transitions(
    events: impl IntoIterator<Item = CommandHistoryEventV2>,
) -> Result<Vec<WorkflowTransitionV1>> {
    let mut by_id = BTreeMap::new();
    for event in events {
        if let Some(existing) = by_id.insert(event.id, event.clone())
            && existing != event
        {
            return Err(DirgoError::User(format!(
                "command history contains conflicting event id {}",
                event.id
            )));
        }
    }
    let events = by_id.into_values().collect::<Vec<_>>();
    let mut transitions = BTreeMap::<TransitionKey, WorkflowTransitionV1>::new();
    for next_index in 1..events.len() {
        let next = &events[next_index];
        let predecessor = &events[next_index - 1];
        if !events_are_consecutive(predecessor, next) {
            continue;
        }
        record_transition(
            &mut transitions,
            next,
            vec![predecessor.command.clone()],
            predecessor.outcome,
        )?;

        if next_index >= 2 {
            let first = &events[next_index - 2];
            if events_are_consecutive(first, predecessor) {
                record_transition(
                    &mut transitions,
                    next,
                    vec![first.command.clone(), predecessor.command.clone()],
                    predecessor.outcome,
                )?;
            }
        }
    }
    let mut transitions = transitions.into_values().collect::<Vec<_>>();
    transitions.sort_by(|left, right| {
        right
            .last_seen
            .cmp(&left.last_seen)
            .then_with(|| right.observations.cmp(&left.observations))
            .then_with(|| left.scope_key.cmp(&right.scope_key))
            .then_with(|| left.predecessors.cmp(&right.predecessors))
            .then_with(|| left.next_command.cmp(&right.next_command))
    });
    transitions.truncate(MAX_WORKFLOW_TRANSITIONS);
    transitions.sort_by(|left, right| {
        left.scope_key
            .cmp(&right.scope_key)
            .then_with(|| left.predecessors.cmp(&right.predecessors))
            .then_with(|| {
                outcome_key(left.predecessor_outcome).cmp(&outcome_key(right.predecessor_outcome))
            })
            .then_with(|| left.next_command.cmp(&right.next_command))
    });
    Ok(transitions)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TransitionKey {
    scope_key: String,
    predecessors: Vec<String>,
    predecessor_outcome: u8,
    next_command: String,
}

fn record_transition(
    transitions: &mut BTreeMap<TransitionKey, WorkflowTransitionV1>,
    next: &CommandHistoryEventV2,
    predecessors: Vec<String>,
    predecessor_outcome: CommandOutcome,
) -> Result<()> {
    let scope_key = scope_key(next)?;
    let key = TransitionKey {
        scope_key: scope_key.clone(),
        predecessors: predecessors.clone(),
        predecessor_outcome: outcome_key(predecessor_outcome),
        next_command: next.command.clone(),
    };
    let transition = transitions
        .entry(key)
        .or_insert_with(|| WorkflowTransitionV1 {
            scope_key,
            predecessors,
            predecessor_outcome,
            next_command: next.command.clone(),
            observations: 0,
            evidence_sessions: Vec::new(),
            next_successes: 0,
            next_failures: 0,
            next_unknown: 0,
            first_seen: next.started_at,
            last_seen: next.started_at,
        });
    transition.observations = transition.observations.saturating_add(1);
    let session = next
        .session_id
        .as_ref()
        .expect("consecutive events always have a session");
    if !transition.evidence_sessions.contains(session)
        && transition.evidence_sessions.len() < MAX_EVIDENCE_SESSIONS
    {
        transition.evidence_sessions.push(session.clone());
        transition.evidence_sessions.sort();
    }
    match next.outcome {
        CommandOutcome::Success => {
            transition.next_successes = transition.next_successes.saturating_add(1)
        }
        CommandOutcome::Failure => {
            transition.next_failures = transition.next_failures.saturating_add(1)
        }
        CommandOutcome::Unknown => {
            transition.next_unknown = transition.next_unknown.saturating_add(1)
        }
    }
    transition.first_seen = transition.first_seen.min(next.started_at);
    transition.last_seen = transition.last_seen.max(next.started_at);
    Ok(())
}

fn events_are_consecutive(
    predecessor: &CommandHistoryEventV2,
    next: &CommandHistoryEventV2,
) -> bool {
    predecessor.id.checked_add(1) == Some(next.id)
        && predecessor.session_id.is_some()
        && predecessor.session_id == next.session_id
        && predecessor.project_root == next.project_root
        && next.started_at >= predecessor.started_at
        && next.started_at - predecessor.started_at <= MAX_TRANSITION_GAP_SECONDS
        && eligible_command(&predecessor.command)
        && eligible_command(&next.command)
}

fn eligible_command(command: &str) -> bool {
    !command.trim().is_empty()
        && !command.starts_with(' ')
        && !is_sensitive_command(command, &[])
        && !command.chars().any(char::is_control)
}

fn scope_key(event: &CommandHistoryEventV2) -> Result<String> {
    match event.project_root.as_deref() {
        Some(root) => root
            .to_str()
            .map(|root| format!("project:{root}"))
            .ok_or(DirgoError::NonUtf8Path),
        None => Ok("global".into()),
    }
}

fn outcome_key(outcome: CommandOutcome) -> u8 {
    match outcome {
        CommandOutcome::Success => 0,
        CommandOutcome::Failure => 1,
        CommandOutcome::Unknown => 2,
    }
}
