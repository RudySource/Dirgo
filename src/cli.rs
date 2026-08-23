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

    #[arg(value_name = "QUERY", num_args = 0.., allow_hyphen_values = true)]
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
    Init {
        shell: Shell,
    },
    Completions {
        shell: Shell,
    },
    Refresh,
    Query(QueryArgs),
    Explain {
        #[arg(value_name = "QUERY", required = true)]
        query: Vec<String>,
    },
    Bench {
        #[arg(long, default_value = "a")]
        query: String,
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u8).range(1..=20))]
        samples: u8,
    },
    Root,
    Repo {
        #[arg(value_name = "QUERY")]
        query: Vec<String>,
    },
    Recent {
        #[arg(value_name = "QUERY")]
        query: Vec<String>,
    },
    Back,
    Forward,
    Import {
        source: ImportSource,
    },
    Bookmarks,
    Bookmark {
        #[command(subcommand)]
        command: BookmarkCommand,
    },
    Doctor,
    Stats,
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Support,
    #[command(name = "__resolve", hide = true)]
    Resolve(ResolveArgs),
}

#[derive(Debug, Args)]
pub struct QueryArgs {
    #[arg(value_name = "QUERY", required = true)]
    pub query: Vec<String>,
    #[arg(long)]
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
    Add {
        name: String,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Remove {
        name: String,
    },
    Rename {
        old: String,
        new: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Path,
    Show,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ImportSource {
    Zoxide,
}
