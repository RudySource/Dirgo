use std::{
    collections::HashMap,
    env,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use clap::Parser;

use crate::{
    DirgoError, Result,
    actions::Action,
    cli::{BookmarkCommand, Cli, Command, ConfigCommand, QueryArgs, ResolveArgs},
    config::Config,
    index::{self, IndexStore},
    model::{Candidate, PathHistory, QueryResponse, unix_now},
    paths::AppPaths,
    search::{self, SearchContext},
    shell,
    state::StateStore,
};

const EXIT_NO_MATCH: i32 = 3;
const EXIT_AMBIGUOUS: i32 = 4;
const EXIT_ACTION_HANDLED: i32 = 10;

type LoadedContext = (
    Vec<crate::model::DirectoryRecord>,
    HashMap<String, crate::model::Bookmark>,
    HashMap<PathBuf, PathHistory>,
);
type LoadedState = (
    HashMap<String, crate::model::Bookmark>,
    HashMap<PathBuf, PathHistory>,
);

struct QueryOutcome {
    response: QueryResponse,
    picker_candidates: Vec<Candidate>,
}

impl QueryOutcome {
    fn resolved(response: QueryResponse) -> Self {
        Self {
            response,
            picker_candidates: Vec::new(),
        }
    }
}

pub fn run() -> Result<i32> {
    let cli = Cli::parse();
    let requested_action = cli.requested_action();
    init_logging(cli.verbose);
    let paths = AppPaths::discover()?;
    let config = Config::load(&paths)?;

    if cli.refresh || matches!(cli.command, Some(Command::Refresh)) {
        let summary = index::rebuild(&paths, &config)?;
        println!(
            "Indexed {} directories ({} projects).",
            summary.directories, summary.projects
        );
        return Ok(0);
    }
    if cli.doctor || matches!(cli.command, Some(Command::Doctor)) {
        return doctor(&paths, &config);
    }
    if cli.bookmarks || matches!(cli.command, Some(Command::Bookmarks)) {
        return list_bookmarks(&paths);
    }
    if let Some(name) = cli.forget {
        return remove_bookmark(&paths, &name);
    }

    match cli.command {
        Some(Command::Init { shell: selected }) => {
            print!("{}", shell::integration(selected));
            Ok(0)
        }
        Some(Command::Completions { shell: selected }) => {
            print!("{}", shell::completions(selected));
            Ok(0)
        }
        Some(Command::Query(args)) => query_command(&paths, &config, args, requested_action),
        Some(Command::Root) => print_project_root(current_dir()?),
        Some(Command::Repo { query }) => repository_command(
            &paths,
            &config,
            &current_dir()?,
            query,
            false,
            requested_action,
        ),
        Some(Command::Recent { query }) => {
            recent_command(&paths, &config, query, false, requested_action)
        }
        Some(Command::Back) => navigation_command(&paths, Direction::Back),
        Some(Command::Forward) => navigation_command(&paths, Direction::Forward),
        Some(Command::Bookmark { command }) => bookmark_command(&paths, command),
        Some(Command::Stats) => stats(&paths),
        Some(Command::Config { command }) => config_command(&paths, &config, command),
        Some(Command::Support) => {
            println!(
                "Support Dirgo\n\nSee SUPPORT.md in the Dirgo repository.\nNo donation address is configured yet."
            );
            Ok(0)
        }
        Some(Command::Resolve(args)) => shell_resolve(&paths, &config, args),
        Some(Command::Refresh | Command::Doctor | Command::Bookmarks) => {
            unreachable!("handled above")
        }
        None => default_query(
            &paths,
            &config,
            &current_dir()?,
            cli.query,
            false,
            requested_action,
        ),
    }
}

fn init_logging(verbose: u8) {
    let directive = env::var("DGO_LOG").unwrap_or_else(|_| {
        match verbose {
            0 => "off",
            1 => "info",
            _ => "debug",
        }
        .into()
    });
    let filter = tracing_subscriber::EnvFilter::try_new(directive)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .compact()
        .try_init();
}

fn current_dir() -> Result<PathBuf> {
    env::current_dir().map_err(|error| DirgoError::io(".", error))
}

fn ensure_data(paths: &AppPaths, config: &Config) -> Result<()> {
    paths.ensure_dirs()?;
    if !paths.index_file.exists() {
        eprintln!("Dirgo is building its first directory index…");
        let summary = index::rebuild(paths, config)?;
        eprintln!("Indexed {} directories.", summary.directories);
    }
    Ok(())
}

fn load_context(paths: &AppPaths, config: &Config) -> Result<LoadedContext> {
    ensure_data(paths, config)?;
    let records = IndexStore::open(&paths.index_file)?.records()?;
    let (bookmarks, history) = load_state_context(paths)?;
    Ok((records, bookmarks, history))
}

fn load_state_context(paths: &AppPaths) -> Result<LoadedState> {
    paths.ensure_dirs()?;
    let state = StateStore::open(&paths.state_file)?;
    let bookmarks = state.bookmark_map()?;
    let history = state
        .histories()?
        .into_iter()
        .map(|row| (row.path.clone(), row))
        .collect();
    Ok((bookmarks, history))
}

fn resolve_query(
    paths: &AppPaths,
    config: &Config,
    cwd: &Path,
    mut query: Vec<String>,
    force: bool,
    build_picker: bool,
) -> Result<QueryOutcome> {
    let explicit_force = query.first().is_some_and(|part| part == "?");
    if explicit_force {
        query.remove(0);
    }
    if !force && !explicit_force && query.len() == 1 {
        let raw = &query[0];
        if let Some(path) = crate::paths::absolute_directory(raw, cwd)? {
            return Ok(QueryOutcome::resolved(QueryResponse {
                query: raw.clone(),
                resolved: true,
                path: Some(path),
                confidence: Some(1.0),
                source: Some("existing_path"),
                candidates: Vec::new(),
            }));
        }
        if let Some(name) = raw.strip_prefix('@') {
            paths.ensure_dirs()?;
            let bookmark = StateStore::open(&paths.state_file)?
                .bookmark(name)?
                .ok_or_else(|| DirgoError::BookmarkMissing(name.into()))?;
            if !bookmark.path.is_dir() {
                return Err(DirgoError::User(format!(
                    "bookmark @{name} points to a missing directory: {}",
                    bookmark.path.display()
                )));
            }
            return Ok(QueryOutcome::resolved(QueryResponse {
                query: raw.clone(),
                resolved: true,
                path: Some(bookmark.path),
                confidence: Some(1.0),
                source: Some("bookmark"),
                candidates: Vec::new(),
            }));
        }
    }
    if query.first().is_some_and(|part| part == ".") {
        let records = index::crawl_local(cwd, config)?;
        let (bookmarks, history) = load_state_context(paths)?;
        let response = search::resolve(
            &query,
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd,
            },
            force || explicit_force,
        )?;
        let picker_candidates = if response.resolved || !build_picker {
            Vec::new()
        } else {
            search::picker_candidates(records, &bookmarks, &history, cwd)
        };
        return Ok(QueryOutcome {
            response,
            picker_candidates,
        });
    }
    let (records, bookmarks, history) = load_context(paths, config)?;
    let response = search::resolve(
        &query,
        &SearchContext {
            records: &records,
            bookmarks: &bookmarks,
            history: &history,
            cwd,
        },
        force || explicit_force,
    )?;
    let picker_candidates = if response.resolved || !build_picker {
        Vec::new()
    } else {
        search::picker_candidates(records, &bookmarks, &history, cwd)
    };
    Ok(QueryOutcome {
        response,
        picker_candidates,
    })
}

fn default_query(
    paths: &AppPaths,
    config: &Config,
    cwd: &Path,
    query: Vec<String>,
    shell_mode: bool,
    action: Action,
) -> Result<i32> {
    if let Some(name) = query.first().and_then(|part| part.strip_prefix('+'))
        && query.len() == 1
    {
        paths.ensure_dirs()?;
        StateStore::open(&paths.state_file)?.add_bookmark(name, cwd)?;
        if !shell_mode {
            println!("Saved @{name} → {}", cwd.display());
        }
        return Ok(0);
    }
    let outcome = resolve_query(paths, config, cwd, query, false, true)?;
    emit_resolution(paths, config, outcome, shell_mode, action)
}

fn query_command(
    paths: &AppPaths,
    config: &Config,
    args: QueryArgs,
    action: Action,
) -> Result<i32> {
    let outcome = resolve_query(
        paths,
        config,
        &current_dir()?,
        args.query,
        false,
        !args.json,
    )?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&outcome.response)?);
        return Ok(if outcome.response.resolved {
            0
        } else {
            EXIT_AMBIGUOUS
        });
    }
    emit_resolution(paths, config, outcome, false, action)
}

fn emit_resolution(
    paths: &AppPaths,
    config: &Config,
    outcome: QueryOutcome,
    shell_mode: bool,
    action: Action,
) -> Result<i32> {
    let QueryOutcome {
        response,
        picker_candidates,
    } = outcome;
    if let Some(path) = response.path {
        return complete_action(paths, config, path, action, shell_mode);
    }
    if response.candidates.is_empty()
        && (!io::stdin().is_terminal() || picker_candidates.is_empty())
    {
        eprintln!(
            "No directories match {:?}.\nTry a shorter query or run `dgo refresh`.",
            response.query
        );
        return Ok(EXIT_NO_MATCH);
    }
    let Some(selection) = choose_candidate(
        &response.candidates,
        picker_candidates,
        &response.query,
        action,
        config,
    )?
    else {
        return Ok(EXIT_AMBIGUOUS);
    };
    complete_action(paths, config, selection.path, selection.action, shell_mode)
}

fn complete_action(
    paths: &AppPaths,
    config: &Config,
    path: PathBuf,
    action: Action,
    shell_mode: bool,
) -> Result<i32> {
    shell::validate_output_path(&path)?;
    match action {
        Action::Go => {
            if shell_mode {
                record_navigation(paths, &path)?;
            }
            println!("{}", path.display());
            Ok(0)
        }
        Action::Print => {
            println!("{}", path.display());
            Ok(0)
        }
        Action::Open | Action::Copy | Action::Editor => {
            crate::actions::execute(action, &path, &config.actions)?;
            Ok(if shell_mode { EXIT_ACTION_HANDLED } else { 0 })
        }
    }
}

fn choose_candidate(
    initial_candidates: &[Candidate],
    picker_candidates: Vec<Candidate>,
    query: &str,
    default_action: Action,
    config: &Config,
) -> Result<Option<crate::tui::Selection>> {
    let candidates = if initial_candidates.is_empty() {
        &picker_candidates
    } else {
        initial_candidates
    };
    let visible = candidates.iter().take(12).collect::<Vec<_>>();
    if !io::stdin().is_terminal() {
        for candidate in visible {
            eprintln!("{}", candidate.path.display());
        }
        eprintln!("Dirgo needs an interactive terminal to choose an ambiguous result.");
        return Ok(None);
    }
    if crate::tui::is_supported() {
        return crate::tui::pick(
            picker_candidates,
            query,
            default_action,
            crate::actions::availability(&config.actions),
        );
    }
    eprintln!("Dirgo — choose a directory\n");
    for (index, candidate) in visible.iter().enumerate() {
        let marker = if candidate.bookmark.is_some() {
            "*"
        } else if candidate.is_project_root {
            ">"
        } else {
            " "
        };
        eprintln!(
            "{:>2}. {} {}\n    {}",
            index + 1,
            marker,
            candidate.basename,
            candidate.display_path
        );
    }
    eprint!("\nSelection [1-{}], or Enter to cancel: ", visible.len());
    io::stderr()
        .flush()
        .map_err(|error| DirgoError::io("stderr", error))?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| DirgoError::io("stdin", error))?;
    let selected = input
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|value| (1..=visible.len()).contains(value));
    Ok(selected.map(|value| crate::tui::Selection {
        path: visible[value - 1].path.clone(),
        action: default_action,
    }))
}

fn record_navigation(paths: &AppPaths, path: &Path) -> Result<()> {
    paths.ensure_dirs()?;
    let session = env::var("DGO_SESSION_ID").ok();
    StateStore::open(&paths.state_file)?.record_visit(path, session.as_deref())
}

fn shell_resolve(paths: &AppPaths, config: &Config, args: ResolveArgs) -> Result<i32> {
    match args.query.as_slice() {
        [command] if command == "root" => {
            let Some((path, _)) = index::find_project_root(&args.cwd) else {
                return Err(DirgoError::NoMatch("project root".into()));
            };
            record_navigation(paths, &path)?;
            println!("{}", path.display());
            Ok(0)
        }
        [command] if command == "back" => navigation_command(paths, Direction::Back),
        [command] if command == "forward" => navigation_command(paths, Direction::Forward),
        [command, rest @ ..] if command == "repo" => {
            repository_command(paths, config, &args.cwd, rest.to_vec(), true, Action::Go)
        }
        [command, rest @ ..] if command == "recent" => {
            recent_command(paths, config, rest.to_vec(), true, Action::Go)
        }
        _ => default_query(paths, config, &args.cwd, args.query, true, Action::Go),
    }
}

fn print_project_root(cwd: PathBuf) -> Result<i32> {
    let Some((path, _)) = index::find_project_root(&cwd) else {
        return Err(DirgoError::NoMatch("project root".into()));
    };
    println!("{}", path.display());
    Ok(0)
}

fn repository_command(
    paths: &AppPaths,
    config: &Config,
    cwd: &Path,
    query: Vec<String>,
    shell_mode: bool,
    action: Action,
) -> Result<i32> {
    let (mut records, bookmarks, history) = load_context(paths, config)?;
    records.retain(|record| record.is_project_root);
    let response = search::resolve(
        &query,
        &SearchContext {
            records: &records,
            bookmarks: &bookmarks,
            history: &history,
            cwd,
        },
        true,
    )?;
    let picker_candidates = search::picker_candidates(records, &bookmarks, &history, cwd);
    emit_resolution(
        paths,
        config,
        QueryOutcome {
            response,
            picker_candidates,
        },
        shell_mode,
        action,
    )
}

fn recent_command(
    paths: &AppPaths,
    config: &Config,
    query: Vec<String>,
    shell_mode: bool,
    action: Action,
) -> Result<i32> {
    paths.ensure_dirs()?;
    let state = StateStore::open(&paths.state_file)?;
    let needle = query.join(" ").to_lowercase();
    let picker_candidates: Vec<_> = state
        .histories()?
        .into_iter()
        .map(|history| {
            let basename = history
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            Candidate {
                display_path: history.path.display().to_string(),
                path: history.path,
                basename,
                score: history.last_visit as f64,
                source: "recent",
                is_project_root: false,
                bookmark: None,
            }
        })
        .collect();
    let candidates = picker_candidates
        .iter()
        .filter(|candidate| {
            needle.is_empty() || candidate.display_path.to_lowercase().contains(&needle)
        })
        .cloned()
        .collect();
    emit_resolution(
        paths,
        config,
        QueryOutcome {
            response: QueryResponse {
                query: query.join(" "),
                resolved: false,
                path: None,
                confidence: None,
                source: None,
                candidates,
            },
            picker_candidates,
        },
        shell_mode,
        action,
    )
}

enum Direction {
    Back,
    Forward,
}

fn navigation_command(paths: &AppPaths, direction: Direction) -> Result<i32> {
    paths.ensure_dirs()?;
    let session = env::var("DGO_SESSION_ID").map_err(|_| {
        DirgoError::User("DGO_SESSION_ID is missing; load `dgo init <shell>` first".into())
    })?;
    let state = StateStore::open(&paths.state_file)?;
    let path = match direction {
        Direction::Back => state.back(&session)?,
        Direction::Forward => state.forward(&session)?,
    };
    let Some(path) = path else {
        return Err(DirgoError::NoMatch("navigation history entry".into()));
    };
    println!("{}", path.display());
    Ok(0)
}

fn bookmark_command(paths: &AppPaths, command: BookmarkCommand) -> Result<i32> {
    paths.ensure_dirs()?;
    let state = StateStore::open(&paths.state_file)?;
    match command {
        BookmarkCommand::Add { name, path } => {
            let path = path.unwrap_or(current_dir()?);
            state.add_bookmark(&name, &path)?;
            println!("Saved @{name} → {}", path.display());
        }
        BookmarkCommand::Remove { name } => {
            if !state.remove_bookmark(&name)? {
                return Err(DirgoError::BookmarkMissing(name));
            }
            println!("Removed @{name}");
        }
        BookmarkCommand::Rename { old, new } => {
            state.rename_bookmark(&old, &new)?;
            println!("Renamed @{old} → @{new}");
        }
    }
    Ok(0)
}

fn remove_bookmark(paths: &AppPaths, name: &str) -> Result<i32> {
    paths.ensure_dirs()?;
    if !StateStore::open(&paths.state_file)?.remove_bookmark(name)? {
        return Err(DirgoError::BookmarkMissing(name.into()));
    }
    println!("Removed @{name}");
    Ok(0)
}

fn list_bookmarks(paths: &AppPaths) -> Result<i32> {
    paths.ensure_dirs()?;
    let bookmarks = StateStore::open(&paths.state_file)?.bookmarks()?;
    if bookmarks.is_empty() {
        println!("No bookmarks yet.\n\nCreate one with: dgo +name");
    } else {
        for bookmark in bookmarks {
            println!("@{:<20} {}", bookmark.name, bookmark.path.display());
        }
    }
    Ok(0)
}

fn config_command(paths: &AppPaths, config: &Config, command: ConfigCommand) -> Result<i32> {
    match command {
        ConfigCommand::Path => println!("{}", paths.config_file.display()),
        ConfigCommand::Show => print!(
            "{}",
            toml::to_string_pretty(config)
                .map_err(|error| DirgoError::Config(error.to_string()))?
        ),
    }
    Ok(0)
}

fn doctor(paths: &AppPaths, config: &Config) -> Result<i32> {
    println!("Dirgo Doctor\n");
    println!("✓ version        {}", env!("CARGO_PKG_VERSION"));
    println!("✓ config         valid (schema {})", config.schema_version);
    println!(
        "{} integration    {}",
        if env::var_os("DGO_SESSION_ID").is_some() {
            "✓"
        } else {
            "!"
        },
        if env::var_os("DGO_SESSION_ID").is_some() {
            "active"
        } else {
            "not detected; run eval \"$(dgo init <shell>)\""
        }
    );
    if paths.index_file.exists() {
        let summary = IndexStore::open(&paths.index_file)?.summary()?;
        println!("✓ index          {} directories", summary.directories);
    } else {
        println!("! index          missing; it will be built on first search");
    }
    paths.ensure_dirs()?;
    let state = StateStore::open(&paths.state_file)?;
    println!(
        "✓ state          healthy ({} bookmarks)",
        state.bookmarks()?.len()
    );
    println!("✓ platform       {} {}", env::consts::OS, env::consts::ARCH);
    println!("\nNo critical issues found.");
    Ok(0)
}

fn stats(paths: &AppPaths) -> Result<i32> {
    paths.ensure_dirs()?;
    let state = StateStore::open(&paths.state_file)?;
    let histories = state.histories()?;
    let bookmarks = state.bookmarks()?;
    let (directories, projects, built_at) = if paths.index_file.exists() {
        let summary = IndexStore::open(&paths.index_file)?.summary()?;
        (summary.directories, summary.projects, summary.built_at)
    } else {
        (0, 0, 0)
    };
    let jumps: u64 = histories.iter().map(|history| history.visit_count).sum();
    let most = histories
        .iter()
        .max_by_key(|history| history.visit_count)
        .map(|history| history.path.display().to_string())
        .unwrap_or_else(|| "—".into());
    let age = if built_at == 0 {
        "—".into()
    } else {
        format_age(unix_now().saturating_sub(built_at))
    };
    let db_size = std::fs::metadata(&paths.index_file)
        .map(|meta| meta.len())
        .unwrap_or(0)
        + std::fs::metadata(&paths.state_file)
            .map(|meta| meta.len())
            .unwrap_or(0);
    println!(
        "Dirgo stats\n\nIndexed directories   {directories}\nProjects              {projects}\nBookmarks             {}\nDirgo navigations     {jumps}\nMost visited          {most}\nIndex age             {age}\nDatabase size         {:.1} MB",
        bookmarks.len(),
        db_size as f64 / 1_048_576.0
    );
    Ok(0)
}

fn format_age(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
    } else {
        format!("{}d {}h", seconds / 86400, seconds % 86400 / 3600)
    }
}
