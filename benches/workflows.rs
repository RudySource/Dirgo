use std::collections::BTreeSet;

use criterion::{Criterion, criterion_group, criterion_main};
use dirgo::{
    suggestions::CommandOutcome,
    workflows::{WorkflowQuery, WorkflowScope, WorkflowTransitionV1, rank_next_actions},
};

fn workflows(c: &mut Criterion) {
    let transitions = (0..10_000)
        .map(|index| WorkflowTransitionV1 {
            scope_key: if index % 2 == 0 {
                "project:/benchmark".into()
            } else {
                "global".into()
            },
            predecessors: vec![format!("command-{index}")],
            predecessor_outcome: CommandOutcome::Success,
            next_command: format!("next-command-{index}"),
            observations: 6,
            evidence_sessions: vec!["session-a".into(), "session-b".into()],
            next_successes: 5,
            next_failures: 1,
            next_unknown: 0,
            first_seen: 100,
            last_seen: 200 + index,
        })
        .collect::<Vec<_>>();
    let query = WorkflowQuery {
        scope: WorkflowScope::Project("/benchmark".into()),
        predecessors: vec!["command-9998".into()],
        predecessor_outcome: CommandOutcome::Success,
        prefix: "next".into(),
        project_commands: BTreeSet::new(),
        limit: 8,
    };
    c.bench_function("workflow_next/10k_transitions", |bencher| {
        bencher.iter(|| rank_next_actions(&transitions, &[], &query))
    });
}

criterion_group!(benches, workflows);
criterion_main!(benches);
