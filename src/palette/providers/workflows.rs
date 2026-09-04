use std::{collections::BTreeSet, time::Instant};

use crate::{
    palette::{
        PaletteAction, PaletteItem, PaletteSource, ProviderBatch, ProviderBudget, WorkflowPreview,
    },
    suggestions::WorkflowSnapshot,
    workflows::{WorkflowScope, WorkflowSource},
};

pub fn workflows(
    snapshot: &WorkflowSnapshot,
    scope: WorkflowScope,
    project_commands: &BTreeSet<String>,
    budget: ProviderBudget,
) -> ProviderBatch {
    let started = Instant::now();
    let predecessors = snapshot.predecessors(&scope).to_vec();
    let items = snapshot
        .next_actions(scope, "", project_commands.clone(), budget.max_items)
        .into_iter()
        .take_while(|_| started.elapsed() < budget.deadline)
        .take(budget.max_items)
        .map(|action| {
            let (title, steps, next_index, kind) = action
                .workflow_id
                .and_then(|id| {
                    snapshot.saved_workflow(id).map(|workflow| {
                        let commands = workflow
                            .steps
                            .iter()
                            .map(|step| step.command.clone())
                            .collect::<Vec<_>>();
                        let index = commands
                            .iter()
                            .position(|command| command == &action.command)
                            .unwrap_or(0);
                        (workflow.name.clone(), commands, index, "Saved")
                    })
                })
                .unwrap_or_else(|| {
                    let mut steps = predecessors.clone();
                    steps.push(action.command.clone());
                    let index = steps.len().saturating_sub(1);
                    (action.command.clone(), steps, index, "Learned")
                });
            PaletteItem {
                id: action.workflow_id.map_or_else(
                    || format!("workflows:learned:{}", action.command),
                    |id| format!("workflows:saved:{id}"),
                ),
                source: PaletteSource::Workflows,
                title,
                subtitle: format!(
                    "{kind} · {} steps · confidence {}",
                    steps.len(),
                    action.confidence
                ),
                insert_text: Some(action.command.clone()),
                preview_key: Some(action.workflow_id.map_or_else(
                    || format!("workflow:learned:{}", action.command),
                    |id| format!("workflow:saved:{id}"),
                )),
                workflow_preview: Some(WorkflowPreview { steps, next_index }),
                action: PaletteAction::Insert {
                    text: action.command,
                },
                score: match action.source {
                    WorkflowSource::Saved => 30_000,
                    WorkflowSource::Learned => 20_000 + i64::from(action.confidence),
                },
            }
        })
        .collect();
    ProviderBatch::ready(PaletteSource::Workflows, items, started.elapsed())
}
