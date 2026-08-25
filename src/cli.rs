use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};

use crate::{actions::Action, shell::Shell};

#[derive(Debug, Parser)]
#[command(name = "dgo", version, about = "Dirgo — go anywhere, instantly.", long_about = None, subcommand_precedence_over_arg = true)]
#[command(group(ArgGroup::new("action").args(["open", "finder", "code", "copy", "print"]).multiple(false)))]
pub struct Cli {
    #[arg(long, global = true, action = clap::ArgAction::Count, help = "Show diagnostic logging")]
    pub verbose: u8,

    #[arg(
        short = 'r',
        long = "refresh",
        conflicts_with = "query",
        help = "Compatibility alias for `dgo refresh`"
    )]
    pub refresh: bool,

    #[arg(
        long = "doctor",
        conflicts_with = "query",
        help = "Compatibility alias for `dgo doctor`"
    )]
    pub doctor: bool,

    #[arg(
        long = "bookmarks",
        conflicts_with = "query",
        help = "Compatibility alias for `dgo bookmarks`"
    )]
    pub bookmarks: bool,

    #[arg(
        long,
        conflicts_with = "query",
        help = "Update Dirgo using its detected installation source"
    )]
    pub update: bool,

    #[arg(
        long = "forget",
        value_name = "NAME",
        conflicts_with = "query",
        help = "Compatibility alias for `dgo bookmark remove`"
    )]
    pub forget: Option<String>,

    #[arg(long, global = true, help = "Open the selected directory with the OS")]
    pub open: bool,

    #[arg(
        long,
        global = true,
        help = "Open the selected directory in Finder or the OS file browser"
    )]
    pub finder: bool,

    #[arg(
        long,
        global = true,
        help = "Open the selected directory in the configured editor"
    )]
    pub code: bool,

    #[arg(long, global = true, help = "Copy the selected path to the clipboard")]
    pub copy: bool,

    #[arg(
        long,
        global = true,
        help = "Print the selected path without navigating"
    )]
    pub print: bool,

    #[arg(long, global = true, help = "Disable colored TUI output")]
    pub no_color: bool,

    #[arg(long, global = true, help = "Use ASCII-only TUI symbols")]
    pub no_unicode: bool,

    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(
        value_name = "QUERY",
        num_args = 0..,
        help = "Directory name, path, bookmark, or fuzzy query"
    )]
    pub query: Vec<String>,
}

impl Cli {
    /// Shell wrappers pass user arguments through the hidden resolver after a
    /// `--` separator so leading-dash paths remain data. Recover action flags
    /// from that positional tail while preserving a user-supplied separator.
    pub fn normalize_resolve_action(&mut self) -> std::result::Result<(), String> {
        let already_has_action = self.open || self.finder || self.code || self.copy || self.print;
        let Some(Command::Resolve(args)) = &mut self.command else {
            return Ok(());
        };
        let literal_from = args
            .query
            .iter()
            .position(|argument| argument == "--")
            .unwrap_or(args.query.len());
        let mut action = None;
        let mut query = Vec::with_capacity(args.query.len());
        for (index, argument) in args.query.drain(..).enumerate() {
            let parsed = (index < literal_from)
                .then(|| action_flag(&argument))
                .flatten();
            if let Some(next) = parsed {
                if action.replace(next).is_some() {
                    return Err(
                        "only one of --open, --finder, --code, --copy, or --print may be used"
                            .into(),
                    );
                }
            } else if index != literal_from || argument != "--" {
                query.push(argument);
            }
        }
        args.query = query;
        if action.is_some() && already_has_action {
            return Err(
                "only one of --open, --finder, --code, --copy, or --print may be used".into(),
            );
        }
        match action {
            Some("open") => self.open = true,
            Some("finder") => self.finder = true,
            Some("code") => self.code = true,
            Some("copy") => self.copy = true,
            Some("print") => self.print = true,
            Some(_) | None => {}
        }
        Ok(())
    }

    pub fn requested_action(&self) -> Action {
        if self.open || self.finder {
            Action::Open
        } else if self.code {
            Action::Editor
        } else if self.copy {
            Action::Copy
        } else if self.print {
            Action::Print
        } else {
            Action::Go
        }
    }
}

fn action_flag(argument: &str) -> Option<&'static str> {
    match argument {
        "--open" => Some("open"),
        "--finder" => Some("finder"),
        "--code" => Some("code"),
        "--copy" => Some("copy"),
        "--print" => Some("print"),
        _ => None,
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Connect Dirgo to your shell safely
    Setup(SetupArgs),
    /// Print parent-shell integration for Zsh, Bash, Fish, or PowerShell
    Init {
        /// Shell whose integration script should be generated
        shell: Shell,
    },
    /// Print command completion definitions for a supported shell
    Completions {
        /// Shell whose completion definitions should be generated
        shell: Shell,
    },
    /// Rebuild the disposable filesystem index atomically
    Refresh,
    /// Resolve a query without changing the current shell directory
    Query(QueryArgs),
    /// Show ranked candidates and score components as JSON
    Explain {
        #[arg(
            value_name = "QUERY",
            required = true,
            help = "Directory query to explain"
        )]
        query: Vec<String>,
    },
    /// Measure local context loading and fuzzy-resolution work
    Bench {
        /// Query used for the local measurement
        #[arg(long, default_value = "a")]
        query: String,
        /// Number of samples used for the reported median
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u8).range(1..=20))]
        samples: u8,
    },
    /// Print the nearest project root
    Root,
    /// Resolve or choose among indexed project roots
    Repo {
        #[arg(value_name = "QUERY", help = "Optional project query")]
        query: Vec<String>,
    },
    /// Resolve or choose among previously visited directories
    Recent {
        #[arg(value_name = "QUERY", help = "Optional history query")]
        query: Vec<String>,
    },
    /// Move backward in this shell session's navigation history
    Back,
    /// Move forward in this shell session's navigation history
    Forward,
    /// Import an optional local history source explicitly
    Import {
        /// History provider to import
        source: ImportSource,
    },
    /// List saved directory bookmarks
    Bookmarks,
    /// Create, repair, rename, or remove bookmarks
    Bookmark {
        #[command(subcommand)]
        command: BookmarkCommand,
    },
    /// Diagnose configuration, storage, integration, index, and actions
    Doctor,
    /// Show local index and navigation statistics
    Stats,
    /// Print the active configuration or its path
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Show support and private vulnerability-reporting guidance
    Support,
    /// Enable or disable new-version notifications
    UpdateNotifications {
        /// Whether version notifications should be shown
        mode: UpdateNotificationMode,
    },
    /// Manage shell-native suggestions and their local history
    Suggestions {
        #[command(subcommand)]
        command: SuggestionsCommand,
    },
    #[command(name = "__check-update", hide = true)]
    CheckUpdate,
    #[command(name = "__suggest", hide = true)]
    Suggest,
    #[command(name = "__suggest-worker", hide = true)]
    SuggestWorker {
        #[arg(long, hide = true)]
        ready: bool,
    },
    #[command(name = "__suggest-record", hide = true)]
    SuggestRecord,
    #[command(name = "__suggest-enabled", hide = true)]
    SuggestEnabled,
    #[command(name = "__suggest-history-enabled", hide = true)]
    SuggestHistoryEnabled,
    #[command(name = "__suggest-shell", hide = true)]
    SuggestShell {
        #[arg(long, value_enum)]
        shell: Shell,
        #[arg(long)]
        cwd: PathBuf,
    },
    #[command(name = "__suggest-pick", hide = true)]
    SuggestPick {
        #[arg(long, value_enum)]
        shell: Shell,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        request_path: PathBuf,
        #[arg(long)]
        output_path: PathBuf,
    },
    #[command(name = "__resolve", hide = true)]
    Resolve(ResolveArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum UpdateNotificationMode {
    On,
    Off,
}

#[derive(Debug, Subcommand)]
pub enum SuggestionsCommand {
    /// Enable suggestion hooks for new shell sessions
    Enable,
    /// Disable suggestion hooks without deleting local history
    Disable,
    /// Show effective suggestion and history settings
    Status,
    /// Inspect suggestion storage and integration prerequisites
    Doctor,
    /// Manage opt-in command history
    History {
        #[command(subcommand)]
        command: SuggestionsHistoryCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum SuggestionsHistoryCommand {
    /// Allow filtered command history to improve suggestions
    Enable,
    /// Stop recording and reading command history
    Disable,
    /// Remove all command-history records
    Clear,
}

#[derive(Debug, Args)]
pub struct SetupArgs {
    /// Shell to configure; detected from SHELL when omitted
    #[arg(long, value_enum)]
    pub shell: Option<Shell>,
    /// Shell startup file to update instead of the detected default
    #[arg(long, value_name = "FILE")]
    pub rc: Option<PathBuf>,
    /// Show the exact change without writing files
    #[arg(long)]
    pub dry_run: bool,
    /// Apply without an interactive confirmation
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Remove only the block previously managed by Dirgo
    #[arg(long)]
    pub remove: bool,
}

#[derive(Debug, Args)]
pub struct QueryArgs {
    #[arg(
        value_name = "QUERY",
        required = true,
        help = "Directory query to resolve"
    )]
    pub query: Vec<String>,
    #[arg(long, help = "Emit the complete resolution response as JSON")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ResolveArgs {
    #[arg(long)]
    pub cwd: PathBuf,
    #[arg(value_name = "QUERY", num_args = 0.., allow_hyphen_values = true, trailing_var_arg = true)]
    pub query: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum BookmarkCommand {
    /// Create a bookmark or repair its destination
    Add {
        /// Bookmark name used as @NAME
        name: String,
        /// Destination directory; defaults to the current directory
        #[arg(long, value_name = "DIRECTORY")]
        path: Option<PathBuf>,
    },
    /// Remove a bookmark without deleting its directory
    Remove {
        /// Bookmark name to remove
        name: String,
    },
    /// Rename a bookmark without changing its destination
    Rename {
        /// Existing bookmark name
        old: String,
        /// New bookmark name
        new: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the configuration file location
    Path,
    /// Print the effective configuration, including defaults
    Show,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ImportSource {
    Zoxide,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_flags_parse_before_and_after_a_public_query() {
        for flag in ["--open", "--finder", "--code", "--copy", "--print"] {
            for arguments in [["dgo", flag, "project"], ["dgo", "project", flag]] {
                let cli = Cli::try_parse_from(arguments).expect("valid action placement");
                assert_eq!(cli.query, ["project"]);
                assert_ne!(cli.requested_action(), Action::Go);
            }
        }
    }

    #[test]
    fn hidden_resolver_recovers_actions_but_preserves_literal_flag_queries() {
        let mut action = Cli::try_parse_from([
            "dgo",
            "__resolve",
            "--cwd",
            "/work",
            "--",
            "project",
            "--finder",
        ])
        .expect("resolver action");
        action.normalize_resolve_action().expect("normalize action");
        assert_eq!(action.requested_action(), Action::Open);
        let Some(Command::Resolve(args)) = action.command else {
            panic!("resolve command");
        };
        assert_eq!(args.query, ["project"]);

        let mut literal =
            Cli::try_parse_from(["dgo", "__resolve", "--cwd", "/work", "--", "--", "--finder"])
                .expect("literal resolver query");
        literal
            .normalize_resolve_action()
            .expect("normalize literal");
        assert_eq!(literal.requested_action(), Action::Go);
        let Some(Command::Resolve(args)) = literal.command else {
            panic!("resolve command");
        };
        assert_eq!(args.query, ["--finder"]);
    }

    #[test]
    fn hidden_resolver_rejects_actions_on_both_sides_of_the_separator() {
        let mut cli = Cli::try_parse_from([
            "dgo",
            "--open",
            "__resolve",
            "--cwd",
            "/work",
            "--",
            "project",
            "--copy",
        ])
        .expect("trailing resolver arguments are positional");
        assert!(cli.normalize_resolve_action().is_err());
    }
}
