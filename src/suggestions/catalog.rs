use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsStr,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use clap::{Command, CommandFactory};
use serde::Deserialize;

use crate::cli::Cli;

const MAX_PATH_ENTRIES: usize = 128;
const MAX_FILES_PER_ENTRY: usize = 4_096;
const MAX_EXECUTABLES: usize = 8_192;
const MAX_USER_SPEC_FILES: usize = 64;
const MAX_USER_SPEC_BYTES: u64 = 256 * 1024;
const MAX_USER_SPEC_DEPTH: usize = 8;
const MAX_USER_SPEC_NODES: usize = 2_048;
const MAX_CHILDREN_PER_NODE: usize = 256;
const MAX_OPTIONS_PER_NODE: usize = 256;
const MAX_NAME_BYTES: usize = 128;
const MAX_DESCRIPTION_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandOption {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub subcommands: Vec<CommandSpec>,
    #[serde(default)]
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
    command_index: HashMap<String, usize>,
}

impl Default for CommandCatalog {
    fn default() -> Self {
        let mut dirgo = Cli::command();
        dirgo.build();
        let mut commands = vec![command_spec(&dirgo)];
        commands.extend(super::specs::builtin_command_specs());
        let command_index = command_index(&commands);
        Self {
            executables: Vec::new(),
            commands,
            command_index,
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

    pub fn with_user_specs(mut self, directory: &Path) -> Self {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return self;
        };
        let mut paths = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(OsStr::to_str) == Some("toml"))
            .collect::<Vec<_>>();
        paths.sort_unstable();

        let mut remaining_nodes = MAX_USER_SPEC_NODES;
        for path in paths.into_iter().take(MAX_USER_SPEC_FILES) {
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.file_type().is_file() || metadata.len() > MAX_USER_SPEC_BYTES {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(spec) = toml::from_str::<CommandSpec>(&raw) else {
                continue;
            };
            let mut nodes = 0;
            if !valid_command_spec(&spec, 0, &mut nodes) || nodes > remaining_nodes {
                continue;
            }
            remaining_nodes -= nodes;
            merge_root(&mut self.commands, spec);
        }
        self.command_index = command_index(&self.commands);
        self
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
        self.command_index
            .get(name)
            .and_then(|index| self.commands.get(*index))
    }
}

fn command_index(commands: &[CommandSpec]) -> HashMap<String, usize> {
    let mut index = HashMap::new();
    for (position, command) in commands.iter().enumerate() {
        index.entry(command.name.clone()).or_insert(position);
        for alias in &command.aliases {
            index.entry(alias.clone()).or_insert(position);
        }
    }
    index
}

fn merge_root(commands: &mut Vec<CommandSpec>, incoming: CommandSpec) {
    if let Some(existing) = commands
        .iter_mut()
        .find(|command| command.name == incoming.name)
    {
        merge_command(existing, incoming);
    } else {
        commands.push(incoming);
    }
}

fn merge_command(existing: &mut CommandSpec, incoming: CommandSpec) {
    if incoming.description.is_some() {
        existing.description = incoming.description;
    }
    append_unique(&mut existing.aliases, incoming.aliases);
    for option in incoming.options {
        if let Some(current) = existing
            .options
            .iter_mut()
            .find(|current| current.name == option.name)
        {
            *current = option;
        } else {
            existing.options.push(option);
        }
    }
    for subcommand in incoming.subcommands {
        if let Some(current) = existing
            .subcommands
            .iter_mut()
            .find(|current| current.name == subcommand.name)
        {
            merge_command(current, subcommand);
        } else {
            existing.subcommands.push(subcommand);
        }
    }
}

fn append_unique(target: &mut Vec<String>, values: Vec<String>) {
    for value in values {
        if !target.contains(&value) {
            target.push(value);
        }
    }
}

fn valid_command_spec(spec: &CommandSpec, depth: usize, nodes: &mut usize) -> bool {
    *nodes += 1;
    if depth > MAX_USER_SPEC_DEPTH
        || *nodes > MAX_USER_SPEC_NODES
        || !valid_name(&spec.name)
        || spec.aliases.iter().any(|alias| !valid_name(alias))
        || spec
            .description
            .as_deref()
            .is_some_and(|description| !valid_description(description))
        || spec.subcommands.len() > MAX_CHILDREN_PER_NODE
        || spec.options.len() > MAX_OPTIONS_PER_NODE
    {
        return false;
    }
    if spec.options.iter().any(|option| {
        !valid_name(&option.name)
            || !option.name.starts_with('-')
            || option.aliases.iter().any(|alias| !valid_name(alias))
            || option
                .description
                .as_deref()
                .is_some_and(|description| !valid_description(description))
    }) {
        return false;
    }
    spec.subcommands
        .iter()
        .all(|child| valid_command_spec(child, depth + 1, nodes))
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_NAME_BYTES
        && !value.chars().any(|character| {
            character.is_control() || character.is_whitespace() || matches!(character, '\'' | '"')
        })
}

fn valid_description(value: &str) -> bool {
    value.len() <= MAX_DESCRIPTION_BYTES && !value.chars().any(char::is_control)
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
