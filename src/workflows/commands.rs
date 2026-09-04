use std::{collections::BTreeSet, env, io::IsTerminal, path::PathBuf};

use serde::Serialize;

use crate::{
    DirgoError, Result,
    cli::{HistoryScopeArgs, WorkflowReadScopeArgs, WorkflowsCommand},
    config::Config,
    index,
    model::unix_now,
    paths::AppPaths,
    suggestions::{CommandHistoryEventV2, CommandOutcome, read_history_snapshot},
    terminal,
};

use super::{
    SavedWorkflowV1, WorkflowQuery, WorkflowScope, WorkflowStep, WorkflowStore,
    WorkflowTransitionV1, export::export_workflows, rank_next_actions, read_workflow_snapshot,
};

#[derive(Debug, Clone)]
enum ScopeSelection {
    One(WorkflowScope),
    All,
}

#[derive(Serialize)]
struct StatusOutput {
    enabled: bool,
    history_enabled: bool,
    schema_version: u64,
    learned_count: usize,
    saved_count: usize,
    last_rebuild: u64,
}

pub fn run(paths: &AppPaths, config: &mut Config, command: &WorkflowsCommand) -> Result<i32> {
    match command {
        WorkflowsCommand::Enable => {
            if !config.suggestions.command_history {
                return Err(DirgoError::User(
                    "Workflow suggestions require filtered command history. Run: dgo suggestions history enable"
                        .into(),
                ));
            }
            config.suggestions.workflow_suggestions = true;
            crate::suggestions::write_suggestions_config(&paths.config_file, config)?;
            println!("Workflow suggestions enabled. Commands are suggested, never executed.");
        }
        WorkflowsCommand::Disable => {
            config.suggestions.workflow_suggestions = false;
            crate::suggestions::write_suggestions_config(&paths.config_file, config)?;
            println!(
                "Workflow suggestions disabled. Stored history and workflows were not deleted."
            );
        }
        WorkflowsCommand::Status { json } => {
            let status = if paths.suggestions_state_file.exists() {
                let stored = read_workflow_snapshot(&paths.suggestions_state_file)?.status;
                StatusOutput {
                    enabled: config.suggestions.workflow_suggestions,
                    history_enabled: config.suggestions.command_history,
                    schema_version: stored.schema_version,
                    learned_count: stored.learned_count,
                    saved_count: stored.saved_count,
                    last_rebuild: stored.last_rebuild,
                }
            } else {
                StatusOutput {
                    enabled: config.suggestions.workflow_suggestions,
                    history_enabled: config.suggestions.command_history,
                    schema_version: super::WORKFLOW_SCHEMA_VERSION,
                    learned_count: 0,
                    saved_count: 0,
                    last_rebuild: 0,
                }
            };
            if *json {
                println!("{}", serde_json::to_string(&status)?);
            } else {
                println!(
                    "Workflow suggestions  {}\nCommand history       {}\nWorkflow schema       v{}\nLearned transitions   {}\nSaved workflows       {}\nLast rebuild          {}",
                    enabled_label(status.enabled),
                    enabled_label(status.history_enabled),
                    status.schema_version,
                    status.learned_count,
                    status.saved_count,
                    status.last_rebuild
                );
            }
        }
        WorkflowsCommand::Next { scope, json } => next(paths, config, scope, *json)?,
        WorkflowsCommand::List { scope, json } => list(paths, scope, *json)?,
        WorkflowsCommand::Show { id, json } => show(paths, *id, *json)?,
        WorkflowsCommand::Save { name, last, yes } => {
            save(paths, config, name, usize::from(*last), *yes)?
        }
        WorkflowsCommand::Rename { id, name } => {
            WorkflowStore::open(&paths.suggestions_state_file)?.rename_workflow(
                *id,
                name,
                unix_now(),
            )?;
            println!("Renamed saved workflow {id}.");
        }
        WorkflowsCommand::Remove { id } => {
            WorkflowStore::open(&paths.suggestions_state_file)?.remove_workflow(*id)?;
            println!("Removed saved workflow {id}. No command was executed.");
        }
        WorkflowsCommand::ClearLearned { scope } => {
            if !paths.suggestions_state_file.exists() {
                println!("Learned workflows are already empty.");
            } else {
                let selected = resolve_scope(scope, true)?;
                let key = selection_key(&selected)?;
                let removed = WorkflowStore::open(&paths.suggestions_state_file)?
                    .clear_learned(key.as_deref())?;
                println!(
                    "Removed {removed} learned transitions. Command history and saved workflows were preserved."
                );
            }
        }
        WorkflowsCommand::Export {
            scope,
            output,
            include_paths,
            force,
        } => {
            let selected = resolve_scope(scope, false)?;
            let (transitions, saved) = selected_rows(paths, &selected)?;
            export_workflows(&transitions, &saved, output, *include_paths, *force)?;
            println!("Exported workflows to {}.", terminal::safe_path(output));
        }
    }
    Ok(0)
}

fn next(paths: &AppPaths, config: &Config, args: &WorkflowReadScopeArgs, json: bool) -> Result<()> {
    if !config.suggestions.workflow_suggestions || !paths.suggestions_state_file.exists() {
        if json {
            println!("[]");
        } else {
            println!("No next workflow action is available.");
        }
        return Ok(());
    }
    let scope = resolve_read_scope(args)?;
    let session = env::var("DGO_SESSION_ID").ok();
    let events = read_history_snapshot(&paths.suggestions_state_file)?.events;
    let mut matching = session.map_or_else(Vec::new, |session| {
        events
            .into_iter()
            .filter(|event| event.session_id.as_deref() == Some(session.as_str()))
            .filter(|event| event_in_scope(event, &scope))
            .collect::<Vec<_>>()
    });
    matching.sort_by_key(|event| event.id);
    let outcome = matching
        .last()
        .map_or(CommandOutcome::Unknown, |event| event.outcome);
    let predecessors = matching
        .iter()
        .rev()
        .take(2)
        .map(|event| event.command.clone())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let snapshot = read_workflow_snapshot(&paths.suggestions_state_file)?;
    let actions = rank_next_actions(
        &snapshot.transitions,
        &snapshot.saved,
        &WorkflowQuery {
            scope,
            predecessors,
            predecessor_outcome: outcome,
            prefix: String::new(),
            project_commands: BTreeSet::new(),
            limit: config.suggestions.max_results,
        },
    );
    if json {
        println!("{}", serde_json::to_string(&actions)?);
    } else if actions.is_empty() {
        println!("No next workflow action is available.");
    } else {
        for action in actions {
            println!("{}\t{}", safe_command(&action.command), action.reason);
        }
    }
    Ok(())
}

fn list(paths: &AppPaths, args: &HistoryScopeArgs, json: bool) -> Result<()> {
    let selected = resolve_scope(args, false)?;
    let (transitions, saved) = selected_rows(paths, &selected)?;
    if json {
        println!(
            "{}",
            serde_json::json!({"saved": saved, "learned": transitions})
        );
    } else {
        for workflow in saved {
            println!(
                "{}\tSAVED\t{}\t{} steps",
                workflow.id,
                terminal::safe_text(&workflow.name),
                workflow.steps.len()
            );
        }
        for transition in transitions {
            println!(
                "-\tLEARNED\t{}\t{} observations",
                safe_command(&transition.next_command),
                transition.observations
            );
        }
    }
    Ok(())
}

fn show(paths: &AppPaths, id: u64, json: bool) -> Result<()> {
    let workflow = read_workflow_snapshot(&paths.suggestions_state_file)?
        .saved
        .into_iter()
        .find(|workflow| workflow.id == id)
        .ok_or_else(|| DirgoError::User(format!("saved workflow {id} does not exist")))?;
    if json {
        println!("{}", serde_json::to_string(&workflow)?);
    } else {
        println!(
            "Workflow {}: {}",
            workflow.id,
            terminal::safe_text(&workflow.name)
        );
        for (index, step) in workflow.steps.iter().enumerate() {
            println!("  {}. {}", index + 1, safe_command(&step.command));
        }
        println!("Inserted, never executed.");
    }
    Ok(())
}

fn save(paths: &AppPaths, config: &Config, name: &str, last: usize, yes: bool) -> Result<()> {
    super::store::validate_name(name)?;
    let session = env::var("DGO_SESSION_ID").map_err(|_| {
        DirgoError::User(
            "DGO_SESSION_ID is missing; open a shell with Dirgo integration and try again".into(),
        )
    })?;
    let scope = current_scope()?;
    if !paths.suggestions_state_file.exists() {
        return Err(DirgoError::User(format!(
            "no completed commands were found for DGO_SESSION_ID={}; run commands in this shell and try again",
            terminal::safe_text(&session)
        )));
    }
    let mut events = read_history_snapshot(&paths.suggestions_state_file)?
        .events
        .into_iter()
        .filter(|event| event.session_id.as_deref() == Some(session.as_str()))
        .filter(|event| event_in_scope(event, &scope))
        .collect::<Vec<_>>();
    events.sort_by_key(|event| event.id);
    let events = events.into_iter().rev().take(last).collect::<Vec<_>>();
    if events.len() != last {
        return Err(DirgoError::User(format!(
            "this shell session has only {} eligible commands in the current scope; {last} required",
            events.len()
        )));
    }
    let steps = events
        .into_iter()
        .rev()
        .map(|event| WorkflowStep {
            command: event.command,
        })
        .collect::<Vec<_>>();
    if steps.iter().any(|step| {
        crate::suggestions::is_sensitive_command(&step.command, &config.suggestions.deny_patterns)
    }) {
        return Err(DirgoError::User(
            "the selected history contains a command blocked by privacy filters; it was not displayed or saved"
                .into(),
        ));
    }
    println!("Workflow preview: {}", terminal::safe_text(name));
    for (index, step) in steps.iter().enumerate() {
        println!("  {}. {}", index + 1, safe_command(&step.command));
    }
    if !yes {
        if !std::io::stdin().is_terminal() {
            return Err(DirgoError::User(
                "non-interactive save requires --yes after reviewing the workflow preview".into(),
            ));
        }
        eprint!("Save this workflow? [y/N] ");
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .map_err(|error| DirgoError::io("stdin", error))?;
        if !matches!(answer.trim(), "y" | "Y" | "yes" | "YES") {
            println!("Workflow was not saved.");
            return Ok(());
        }
    }
    let key = scope_key(&scope)?;
    let workflow = WorkflowStore::open(&paths.suggestions_state_file)?.save_workflow(
        name,
        &key,
        steps,
        unix_now(),
    )?;
    println!("Saved workflow {}. No command was executed.", workflow.id);
    Ok(())
}

fn selected_rows(
    paths: &AppPaths,
    selected: &ScopeSelection,
) -> Result<(Vec<WorkflowTransitionV1>, Vec<SavedWorkflowV1>)> {
    if !paths.suggestions_state_file.exists() {
        return Ok((Vec::new(), Vec::new()));
    }
    let snapshot = read_workflow_snapshot(&paths.suggestions_state_file)?;
    let key = selection_key(selected)?;
    let transitions = snapshot
        .transitions
        .into_iter()
        .filter(|transition| key.as_ref().is_none_or(|key| &transition.scope_key == key))
        .collect();
    let saved = snapshot
        .saved
        .into_iter()
        .filter(|workflow| key.as_ref().is_none_or(|key| &workflow.scope_key == key))
        .collect();
    Ok((transitions, saved))
}

fn resolve_scope(args: &HistoryScopeArgs, bare_means_all: bool) -> Result<ScopeSelection> {
    if args.all || (bare_means_all && args.project.is_none() && !args.global) {
        return Ok(ScopeSelection::All);
    }
    if args.global {
        return Ok(ScopeSelection::One(WorkflowScope::Global));
    }
    resolve_project_or_global(args.project.clone()).map(ScopeSelection::One)
}

fn resolve_read_scope(args: &WorkflowReadScopeArgs) -> Result<WorkflowScope> {
    if args.global {
        Ok(WorkflowScope::Global)
    } else {
        resolve_project_or_global(args.project.clone())
    }
}

fn resolve_project_or_global(path: Option<PathBuf>) -> Result<WorkflowScope> {
    let explicit = path.is_some();
    let path = path.unwrap_or(std::env::current_dir().map_err(|error| DirgoError::io(".", error))?);
    let canonical = path
        .canonicalize()
        .map_err(|error| DirgoError::io(&path, error))?;
    match index::find_project_root(&canonical) {
        Some((root, _)) => Ok(WorkflowScope::Project(root)),
        None if explicit => Err(DirgoError::User(format!(
            "no project root found at or above {}",
            terminal::safe_path(&canonical)
        ))),
        None => Ok(WorkflowScope::Global),
    }
}

fn current_scope() -> Result<WorkflowScope> {
    resolve_project_or_global(None)
}

fn selection_key(selection: &ScopeSelection) -> Result<Option<String>> {
    match selection {
        ScopeSelection::All => Ok(None),
        ScopeSelection::One(scope) => scope_key(scope).map(Some),
    }
}

fn scope_key(scope: &WorkflowScope) -> Result<String> {
    match scope {
        WorkflowScope::Global => Ok("global".into()),
        WorkflowScope::Project(root) => root
            .to_str()
            .map(|root| format!("project:{root}"))
            .ok_or(DirgoError::NonUtf8Path),
    }
}

fn event_in_scope(event: &CommandHistoryEventV2, scope: &WorkflowScope) -> bool {
    match scope {
        WorkflowScope::Global => event.project_root.is_none(),
        WorkflowScope::Project(root) => event.project_root.as_ref() == Some(root),
    }
}

fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "enabled" } else { "disabled" }
}

fn safe_command(command: &str) -> String {
    command
        .chars()
        .flat_map(char::escape_default)
        .take(512)
        .collect()
}
