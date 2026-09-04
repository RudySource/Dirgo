use std::{collections::BTreeSet, path::PathBuf};

use dirgo::{
    suggestions::CommandOutcome,
    workflows::{
        SavedWorkflowV1, WorkflowQuery, WorkflowScope, WorkflowSource, WorkflowStep,
        WorkflowTransitionV1, rank_next_actions,
    },
};

#[test]
fn ranking_keeps_projects_isolated_and_bounds_global_fallback() {
    let alpha = "project:/fixture/alpha";
    let beta = "project:/fixture/beta";
    let mut transitions = vec![
        transition(alpha, &["cargo fmt"], "cargo test", 6, 5, 1),
        transition(beta, &["cargo fmt"], "npm test", 20, 20, 0),
    ];
    for index in 0..5 {
        transitions.push(transition(
            "global",
            &["cargo fmt"],
            &format!("global-{index}"),
            10,
            10,
            0,
        ));
    }
    let actions = rank_next_actions(
        &transitions,
        &[],
        &query(
            WorkflowScope::Project(PathBuf::from("/fixture/alpha")),
            &["cargo fmt"],
        ),
    );

    assert_eq!(actions[0].command, "cargo test");
    assert_eq!(
        actions
            .iter()
            .filter(|action| action.command.starts_with("global-"))
            .count(),
        1
    );
    assert!(actions.iter().all(|action| action.command != "npm test"));
    assert!(
        actions
            .iter()
            .all(|action| !action.reason.contains("/fixture"))
    );
}

#[test]
fn longer_exact_context_and_predecessor_outcome_are_deterministic() {
    let scope = "project:/fixture/alpha";
    let transitions = vec![
        transition(scope, &["cargo clippy"], "cargo test", 12, 12, 0),
        transition(
            scope,
            &["cargo fmt", "cargo clippy"],
            "cargo build",
            3,
            3,
            0,
        ),
        WorkflowTransitionV1 {
            predecessor_outcome: CommandOutcome::Failure,
            ..transition(scope, &["cargo clippy"], "cargo fix", 8, 7, 1)
        },
    ];
    let mut request = query(
        WorkflowScope::Project(PathBuf::from("/fixture/alpha")),
        &["cargo fmt", "cargo clippy"],
    );
    let first = rank_next_actions(&transitions, &[], &request);
    let second = rank_next_actions(&transitions, &[], &request);
    assert_eq!(first, second);
    assert_eq!(first[0].command, "cargo build");
    assert!(first.iter().all(|action| action.command != "cargo fix"));

    request.predecessor_outcome = CommandOutcome::Failure;
    let failed = rank_next_actions(&transitions, &[], &request);
    assert_eq!(failed[0].command, "cargo fix");
}

#[test]
fn weak_evidence_is_suppressed_and_success_is_explainable() {
    let mut weak = transition("global", &["git add ."], "git commit", 2, 2, 0);
    weak.evidence_sessions = vec!["only-one-session".into()];
    let request = query(WorkflowScope::Global, &["git add ."]);
    assert!(rank_next_actions(&[weak], &[], &request).is_empty());

    let strong = transition("global", &["git add ."], "git commit", 6, 5, 1);
    let actions = rank_next_actions(&[strong], &[], &request);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].source, WorkflowSource::Learned);
    assert!(actions[0].reason.contains("6 times"));
    assert!(actions[0].reason.contains("83% successful"));
    assert!(actions[0].confidence <= 1_000);
}

#[test]
fn saved_workflows_outrank_learned_but_never_displace_identical_proj_text() {
    let scope = "project:/fixture/alpha";
    let learned = transition(scope, &["cargo fmt"], "cargo test", 20, 20, 0);
    let saved = SavedWorkflowV1 {
        id: 42,
        name: "Verify".into(),
        scope_key: scope.into(),
        steps: vec![
            WorkflowStep {
                command: "cargo fmt".into(),
            },
            WorkflowStep {
                command: "cargo test".into(),
            },
        ],
        created_at: 100,
        updated_at: 100,
    };
    let mut request = query(
        WorkflowScope::Project(PathBuf::from("/fixture/alpha")),
        &["cargo fmt"],
    );
    let actions = rank_next_actions(
        std::slice::from_ref(&learned),
        std::slice::from_ref(&saved),
        &request,
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].source, WorkflowSource::Saved);
    assert_eq!(actions[0].workflow_id, Some(42));

    request.project_commands.insert("cargo test".into());
    assert!(rank_next_actions(&[learned], &[saved], &request).is_empty());
}

#[test]
fn saved_workflow_ambiguity_remains_visible_as_separate_choices() {
    let scope = "project:/fixture/alpha";
    let saved = [
        saved(1, "Fast", scope, &["cargo fmt", "cargo test"]),
        saved(2, "Release", scope, &["cargo fmt", "cargo build"]),
    ];
    let actions = rank_next_actions(
        &[],
        &saved,
        &query(
            WorkflowScope::Project(PathBuf::from("/fixture/alpha")),
            &["cargo fmt"],
        ),
    );
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].workflow_id, Some(1));
    assert_eq!(actions[1].workflow_id, Some(2));
}

fn query(scope: WorkflowScope, predecessors: &[&str]) -> WorkflowQuery {
    WorkflowQuery {
        scope,
        predecessors: predecessors.iter().map(|value| (*value).into()).collect(),
        predecessor_outcome: CommandOutcome::Success,
        prefix: String::new(),
        project_commands: BTreeSet::new(),
        limit: 8,
    }
}

fn transition(
    scope: &str,
    predecessors: &[&str],
    next: &str,
    observations: u64,
    successes: u64,
    failures: u64,
) -> WorkflowTransitionV1 {
    WorkflowTransitionV1 {
        scope_key: scope.into(),
        predecessors: predecessors.iter().map(|value| (*value).into()).collect(),
        predecessor_outcome: CommandOutcome::Success,
        next_command: next.into(),
        observations,
        evidence_sessions: vec!["session-a".into(), "session-b".into()],
        next_successes: successes,
        next_failures: failures,
        next_unknown: observations.saturating_sub(successes + failures),
        first_seen: 100,
        last_seen: 200,
    }
}

fn saved(id: u64, name: &str, scope: &str, commands: &[&str]) -> SavedWorkflowV1 {
    SavedWorkflowV1 {
        id,
        name: name.into(),
        scope_key: scope.into(),
        steps: commands
            .iter()
            .map(|command| WorkflowStep {
                command: (*command).into(),
            })
            .collect(),
        created_at: 100,
        updated_at: 100,
    }
}
