use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    DirgoError, Result,
    cli::RootsCommand,
    config::{Config, default_ignores},
    config_edit::{ConfigMutation, mutate_config},
    index,
    paths::{self, AppPaths},
    terminal,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RootStatus {
    pub path: PathBuf,
    pub accessible: bool,
    pub focused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
}

pub fn run(paths: &AppPaths, command: &RootsCommand) -> Result<i32> {
    match command {
        RootsCommand::List { json } => list(paths, *json),
        RootsCommand::Add { path, no_refresh } => add(paths, path, *no_refresh),
        RootsCommand::Remove { path, no_refresh } => remove(paths, path, *no_refresh),
    }
}

fn list(paths: &AppPaths, json: bool) -> Result<i32> {
    let config = Config::load(paths)?;
    let statuses = statuses(&config.roots);
    if json {
        println!("{}", serde_json::to_string(&statuses)?);
    } else {
        println!("Search roots");
        for status in statuses {
            let marker = if status.accessible { "✓" } else { "!" };
            let focused = if status.focused {
                " · focused root"
            } else {
                ""
            };
            let issue = status
                .issue
                .map_or_else(String::new, |issue| format!(" · {issue}"));
            println!(
                "{marker} {}{focused}{issue}",
                terminal::safe_path(&status.path)
            );
        }
    }
    Ok(0)
}

fn add(paths: &AppPaths, input: &Path, no_refresh: bool) -> Result<i32> {
    let root = resolve_input(input, true)?;
    let outcome = mutate_config(&paths.config_file, ConfigMutation::AddRoot(root.clone()))?;
    if outcome.changed {
        println!("Added search root:\n{}", terminal::safe_path(&root));
    } else {
        println!(
            "Search root is already configured:\n{}",
            terminal::safe_path(&root)
        );
    }
    if no_refresh || !outcome.changed {
        return Ok(0);
    }
    let config = Config::load(paths)?;
    match index::rebuild(paths, &config) {
        Ok(summary) => println!(
            "\nIndexed {} directories ({} projects).",
            summary.directories, summary.projects
        ),
        Err(error) => eprintln!(
            "\nRoot was saved, but the index refresh failed. The last good index remains active.\n{}",
            terminal::safe_text(&error.to_string())
        ),
    }
    Ok(0)
}

fn remove(paths: &AppPaths, input: &Path, no_refresh: bool) -> Result<i32> {
    let config = Config::load(paths)?;
    if config.roots.len() == 1 {
        return Err(DirgoError::User(
            "cannot remove the final search root".into(),
        ));
    }
    let root = resolve_input(input, false)?;
    let outcome = mutate_config(&paths.config_file, ConfigMutation::RemoveRoot(root.clone()))?;
    if outcome.changed {
        println!(
            "Removed search root:\n{}\n\nBookmarks and navigation history were not changed.",
            terminal::safe_path(&root)
        );
    } else {
        println!(
            "Search root was not configured:\n{}",
            terminal::safe_path(&root)
        );
    }
    if no_refresh || !outcome.changed {
        return Ok(0);
    }
    let config = Config::load(paths)?;
    match index::rebuild(paths, &config) {
        Ok(summary) => println!(
            "\nIndexed {} directories ({} projects).",
            summary.directories, summary.projects
        ),
        Err(error) => eprintln!(
            "\nRoot was removed, but the index refresh failed. The last good index remains active.\n{}",
            terminal::safe_text(&error.to_string())
        ),
    }
    Ok(0)
}

fn resolve_input(input: &Path, must_exist: bool) -> Result<PathBuf> {
    let text = input
        .to_str()
        .ok_or_else(|| DirgoError::User("root path is not valid UTF-8".into()))?;
    let expanded = paths::expand_path(text)?;
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        env::current_dir()
            .map_err(|error| DirgoError::io("current directory", error))?
            .join(expanded)
    };
    match absolute.canonicalize() {
        Ok(path) => Ok(path),
        Err(_) if must_exist => Err(DirgoError::User(format!(
            "search root must be an existing directory: {}",
            terminal::safe_path(&absolute)
        ))),
        Err(_) => Ok(absolute),
    }
}

pub fn statuses(roots: &[PathBuf]) -> Vec<RootStatus> {
    let canonical = roots
        .iter()
        .map(|path| path.canonicalize().unwrap_or_else(|_| path.clone()))
        .collect::<Vec<_>>();
    canonical
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let metadata = fs::metadata(path);
            let accessible = metadata.as_ref().is_ok_and(fs::Metadata::is_dir);
            let issue = match metadata {
                Ok(metadata) if metadata.is_file() => Some("path is a file".into()),
                Ok(_) if !accessible => Some("path is not accessible".into()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Some("directory no longer exists".into())
                }
                Err(_) => Some("directory is not accessible".into()),
                _ => None,
            };
            let nested = canonical.iter().enumerate().any(|(other_index, other)| {
                index != other_index && path != other && path.starts_with(other)
            });
            let ignored_ancestor = path.components().any(|component| {
                let name = component.as_os_str().to_string_lossy();
                default_ignores().contains(&name.as_ref())
            });
            RootStatus {
                path: path.clone(),
                accessible,
                focused: nested || ignored_ancestor,
                issue,
            }
        })
        .collect()
}

pub fn ignored_query_segment<'a>(query: &str, ignores: &'a [String]) -> Option<&'a str> {
    let segments = query
        .split(|character: char| character.is_whitespace() || matches!(character, '/' | '\\'))
        .filter(|segment| !segment.is_empty());
    for segment in segments {
        if let Some(ignored) = ignores
            .iter()
            .find(|ignored| ignored.eq_ignore_ascii_case(segment))
        {
            return Some(ignored);
        }
    }
    None
}

pub fn default_ignored_query_segment(query: &str) -> Option<&'static str> {
    let segments = query
        .split(|character: char| character.is_whitespace() || matches!(character, '/' | '\\'))
        .filter(|segment| !segment.is_empty());
    for segment in segments {
        if let Some(ignored) = default_ignores()
            .into_iter()
            .find(|ignored| ignored.eq_ignore_ascii_case(segment))
        {
            return Some(ignored);
        }
    }
    None
}
