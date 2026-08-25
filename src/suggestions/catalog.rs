use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use clap::{Command, CommandFactory};

use crate::cli::Cli;

const MAX_PATH_ENTRIES: usize = 128;
const MAX_FILES_PER_ENTRY: usize = 4_096;
const MAX_EXECUTABLES: usize = 8_192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOption {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: Option<String>,
    pub subcommands: Vec<CommandSpec>,
    pub options: Vec<CommandOption>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExecutableSpec {
    name: String,
    path: PathBuf,
    modified_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct CommandCatalog {
    executables: Vec<ExecutableSpec>,
    commands: Vec<CommandSpec>,
}

impl Default for CommandCatalog {
    fn default() -> Self {
        Self {
            executables: Vec::new(),
            commands: vec![command_spec(&Cli::command())],
        }
    }
}

impl CommandCatalog {
    pub fn discover(path: Option<&OsStr>) -> Self {
        let mut catalog = Self::default();
        let Some(path) = path else {
            return catalog;
        };
        let mut executables = BTreeMap::<String, ExecutableSpec>::new();
        for directory in std::env::split_paths(path).take(MAX_PATH_ENTRIES) {
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.take(MAX_FILES_PER_ENTRY).flatten() {
                if executables.len() >= MAX_EXECUTABLES || !is_executable(&entry.path()) {
                    continue;
                }
                let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                let name = portable_command_name(&file_name, cfg!(windows));
                executables.entry(name.clone()).or_insert_with(|| {
                    let path = entry.path();
                    ExecutableSpec {
                        name,
                        modified_seconds: modified_seconds(&path),
                        path,
                    }
                });
            }
        }
        catalog.executables = executables.into_values().collect();
        catalog
    }

    pub fn from_executable_names(names: impl IntoIterator<Item = String>) -> Self {
        let mut catalog = Self::default();
        let unique = names
            .into_iter()
            .map(|name| (name.clone(), name))
            .collect::<BTreeMap<_, _>>();
        catalog.executables = unique
            .into_values()
            .take(MAX_EXECUTABLES)
            .map(|name| ExecutableSpec {
                path: PathBuf::from(&name),
                name,
                modified_seconds: None,
            })
            .collect();
        catalog
    }

    pub fn executable_names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.executables
            .iter()
            .map(|executable| executable.name.as_str())
    }

    pub fn executable_count(&self) -> usize {
        self.executables.len()
    }

    pub fn executable_metadata(&self, name: &str) -> Option<(&Path, Option<u64>)> {
        self.executables
            .iter()
            .find(|executable| executable.name == name)
            .map(|executable| (executable.path.as_path(), executable.modified_seconds))
    }

    pub fn command(&self, name: &str) -> Option<&CommandSpec> {
        self.commands.iter().find(|command| {
            command.name == name || command.aliases.iter().any(|alias| alias == name)
        })
    }
}

pub fn portable_command_name(name: &str, windows: bool) -> String {
    if windows {
        let path = Path::new(name);
        if path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "exe" | "cmd" | "bat" | "com"
                )
            })
        {
            return path
                .file_stem()
                .and_then(OsStr::to_str)
                .unwrap_or(name)
                .to_owned();
        }
    }
    name.to_owned()
}

fn command_spec(command: &Command) -> CommandSpec {
    CommandSpec {
        name: command.get_name().to_owned(),
        aliases: command.get_visible_aliases().map(str::to_owned).collect(),
        description: command.get_about().map(ToString::to_string),
        subcommands: command
            .get_subcommands()
            .filter(|subcommand| !subcommand.is_hide_set())
            .map(command_spec)
            .collect(),
        options: command
            .get_arguments()
            .filter(|argument| !argument.is_hide_set())
            .flat_map(|argument| {
                let description = argument.get_help().map(ToString::to_string);
                let mut options = Vec::with_capacity(2);
                if let Some(long) = argument.get_long() {
                    options.push(CommandOption {
                        name: format!("--{long}"),
                        aliases: argument
                            .get_visible_aliases()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|alias| format!("--{alias}"))
                            .collect(),
                        description: description.clone(),
                    });
                }
                if let Some(short) = argument.get_short() {
                    options.push(CommandOption {
                        name: format!("-{short}"),
                        aliases: Vec::new(),
                        description,
                    });
                }
                options
            })
            .collect(),
    }
}

fn modified_seconds(path: &Path) -> Option<u64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(windows)]
fn is_executable(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "exe" | "cmd" | "bat" | "com"
                )
            })
}

#[cfg(not(any(unix, windows)))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
