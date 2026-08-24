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
        allow_hyphen_values = true,
        help = "Directory name, path, bookmark, or fuzzy query"
    )]
    pub query: Vec<String>,
}

impl Cli {
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

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print parent-shell integration for Zsh, Bash, or Fish
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
    #[command(name = "__resolve", hide = true)]
    Resolve(ResolveArgs),
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
