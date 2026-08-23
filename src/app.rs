use std::{
    collections::HashMap,
    env,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use clap::Parser;

use crate::{
    DirgoError, Result,
    actions::Action,
    cli::{BookmarkCommand, Cli, Command, ConfigCommand, ImportSource, QueryArgs, ResolveArgs},
    config::Config,
    history_import,
    index::{self, IndexStore},
    model::{Candidate, PathHistory, QueryResponse, unix_now},
    paths::{self, AppPaths},
    search::{self, SearchContext},
    shell,
    state::StateStore,
};

const EXIT_NO_MATCH: i32 = 3;
const EXIT_AMBIGUOUS: i32 = 4;
const EXIT_ACTION_HANDLED: i32 = 10;
const STALE_INDEX_AFTER_SECONDS: u64 = 7 * 24 * 60 * 60;
const SLOW_SHELL_STARTUP_BYTES: u64 = 1_048_576;
const BACKGROUND_INDEX_STREAM_THRESHOLD: usize = 20_000;

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
    picker: PickerCandidates,
}

enum PickerCandidates {
    Ready(Vec<Candidate>),
    Records(PickerRecords),
    IndexStream(IndexPickerStream),
}

struct IndexPickerStream {
    index_path: PathBuf,
    record_count: usize,
    bookmarks: HashMap<PathBuf, String>,
    history: HashMap<PathBuf, PathHistory>,
    cwd: PathBuf,
    ranking: crate::config::RankingConfig,
}

struct PickerRecords {
    records: Vec<crate::model::DirectoryRecord>,
    bookmarks: HashMap<String, crate::model::Bookmark>,
    history: HashMap<PathBuf, PathHistory>,
    cwd: PathBuf,
    ranking: crate::config::RankingConfig,
}

impl PickerCandidates {
    fn is_empty(&self) -> bool {
        match self {
            Self::Ready(candidates) => candidates.is_empty(),
            Self::Records(records) => records.records.is_empty(),
            Self::IndexStream(stream) => stream.record_count == 0,
        }
    }
}

impl IndexPickerStream {
    fn from_state(
        index_path: PathBuf,
        record_count: usize,
        bookmarks: HashMap<String, crate::model::Bookmark>,
        history: HashMap<PathBuf, PathHistory>,
        cwd: PathBuf,
        ranking: crate::config::RankingConfig,
    ) -> Self {
        Self {
            index_path,
            record_count,
            bookmarks: bookmarks
                .into_values()
                .map(|bookmark| (bookmark.path, bookmark.name))
                .collect(),
            history,
            cwd,
            ranking,
        }
    }
}

impl PickerRecords {
    fn into_stream(self) -> search::PickerCandidateStream {
        search::picker_candidate_stream(
            self.records,
            self.bookmarks,
            self.history,
            self.cwd,
            self.ranking,
        )
    }

    fn into_candidates(self) -> Vec<Candidate> {
        search::picker_candidates(
            self.records,
            &self.bookmarks,
            &self.history,
            &self.cwd,
            &self.ranking,
        )
    }
}

impl QueryOutcome {
    fn resolved(response: QueryResponse) -> Self {
        Self {
            response,
            picker: PickerCandidates::Ready(Vec::new()),
        }
    }
}

pub fn run() -> Result<i32> {
    let cli = Cli::parse();
    let requested_action = cli.requested_action();
    init_logging(cli.verbose);

    // These commands emit static shell text and must work in a restricted
    // shell, before XDG directories exist, or when state storage is damaged.
    // In particular, installers commonly invoke `dgo completions` before the
    // user has ever navigated with Dirgo.
    match &cli.command {
        Some(Command::Init { shell: selected }) => {
            print!("{}", shell::integration(*selected));
            return Ok(0);
        }
        Some(Command::Completions { shell: selected }) => {
            print!("{}", shell::completions(*selected));
            return Ok(0);
        }
        _ => {}
    }

    let paths = AppPaths::discover()?;
    let mut config = Config::load(&paths)?;
    if cli.no_color {
        config.ui.accent = "none".into();
    }
    if cli.no_unicode {
        config.ui.icons = "never".into();
    }

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
        Some(Command::Init { .. } | Command::Completions { .. }) => {
            unreachable!("handled before storage access")
        }
        Some(Command::Query(args)) => query_command(&paths, &config, args, requested_action),
        Some(Command::Explain { query }) => explain_command(&paths, &config, query),
        Some(Command::Bench { query, samples }) => bench_command(&paths, &config, &query, samples),
        Some(Command::Root) => print_project_root(current_dir()?),
        Some(Command::Repo { query }) => repository_command(
            &paths,
            &config,
            &current_dir()?,
            query,
            false,
            requested_action,
        ),
        Some(Command::Recent { query }) => recent_command(
            &paths,
            &config,
            &current_dir()?,
            query,
            false,
            requested_action,
        ),
        Some(Command::Back) => navigation_command(&paths, Direction::Back),
        Some(Command::Forward) => navigation_command(&paths, Direction::Forward),
        Some(Command::Import { source }) => import_history(&paths, source),
        Some(Command::Bookmark { command }) => bookmark_command(&paths, command),
        Some(Command::Stats) => stats(&paths),
        Some(Command::Config { command }) => config_command(&paths, &config, command),
        Some(Command::Support) => {
            println!(
                "Dirgo support\n\nRun `dgo doctor`, remove personal paths from its output, then use the issue forms at:\nhttps://github.com/RudySource/Dirgo/issues\n\nFor vulnerabilities, follow SECURITY.md and do not open a public issue."
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
    let records = open_index_with_recovery(paths, config)?.records()?;
    let (bookmarks, history) = load_state_context(paths)?;
    Ok((records, bookmarks, history))
}

fn open_index_with_recovery(paths: &AppPaths, config: &Config) -> Result<IndexStore> {
    match IndexStore::open(&paths.index_file) {
        Ok(store) => Ok(store),
        Err(DirgoError::IndexUpgradeRequired { found, expected }) => {
            eprintln!(
                "Dirgo is rebuilding its disposable index for schema {expected} (was {found})."
            );
            index::rebuild(paths, config)?;
            IndexStore::open(&paths.index_file)
        }
        Err(error) if recoverable_storage_error(&error) => {
            let backup = paths::preserve_for_recovery(&paths.index_file, "corrupt", unix_now())?;
            eprintln!(
                "Dirgo quarantined a corrupt index at {} and is rebuilding it.",
                backup.display()
            );
            index::rebuild(paths, config)?;
            IndexStore::open(&paths.index_file)
        }
        Err(error) => Err(error),
    }
}

fn load_state_context(paths: &AppPaths) -> Result<LoadedState> {
    paths.ensure_dirs()?;
    let load = || -> Result<LoadedState> {
        let state = StateStore::open(&paths.state_file)?;
        let bookmarks = state.bookmark_map()?;
        let history = state
            .histories()?
            .into_iter()
            .map(|row| (row.path.clone(), row))
            .collect();
        Ok((bookmarks, history))
    };
    match load() {
        Ok(context) => Ok(context),
        Err(error) if recoverable_storage_error(&error) && paths.state_file.exists() => {
            let backup = paths::preserve_for_recovery(&paths.state_file, "corrupt", unix_now())?;
            eprintln!(
                "Dirgo backed up corrupt state to {} and started with empty state.",
                backup.display()
            );
            load()
        }
        Err(error) => Err(error),
    }
}

fn recoverable_storage_error(error: &DirgoError) -> bool {
    matches!(
        error,
        DirgoError::Database(_)
            | DirgoError::Storage(_)
            | DirgoError::Table(_)
            | DirgoError::Data(_)
            | DirgoError::IndexData(_)
    )
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
                    "bookmark @{name} points to a missing directory: {}. Repair it with `dgo bookmark add {name} --path <directory>` or run `dgo bookmark remove {name}`",
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
                ranking: &config.ranking,
            },
            force || explicit_force,
            !build_picker,
        )?;
        let picker = if response.resolved || !build_picker {
            PickerCandidates::Ready(Vec::new())
        } else {
            PickerCandidates::Records(PickerRecords {
                records,
                bookmarks,
                history,
                cwd: cwd.to_path_buf(),
                ranking: config.ranking.clone(),
            })
        };
        return Ok(QueryOutcome { response, picker });
    }
    if build_picker && io::stdin().is_terminal() {
        ensure_data(paths, config)?;
        let store = open_index_with_recovery(paths, config)?;
        let record_count = store.record_count()?;
        if record_count >= BACKGROUND_INDEX_STREAM_THRESHOLD {
            if !force
                && !explicit_force
                && query.len() == 1
                && let Some(path) = store.unique_basename(&query[0])?
                && path.is_dir()
            {
                return Ok(QueryOutcome::resolved(QueryResponse {
                    query: query.join(" "),
                    resolved: true,
                    path: Some(path),
                    confidence: Some(1.0),
                    source: Some("exact_basename"),
                    candidates: Vec::new(),
                }));
            }
            let (bookmarks, history) = load_state_context(paths)?;
            return Ok(QueryOutcome {
                response: QueryResponse {
                    query: query.join(" "),
                    resolved: false,
                    path: None,
                    confidence: None,
                    source: None,
                    candidates: Vec::new(),
                },
                picker: PickerCandidates::IndexStream(IndexPickerStream::from_state(
                    paths.index_file.clone(),
                    record_count,
                    bookmarks,
                    history,
                    cwd.to_path_buf(),
                    config.ranking.clone(),
                )),
            });
        }
    }
    let (records, bookmarks, history) = load_context(paths, config)?;
    let response = search::resolve(
        &query,
        &SearchContext {
            records: &records,
            bookmarks: &bookmarks,
            history: &history,
            cwd,
            ranking: &config.ranking,
        },
        force || explicit_force,
        !build_picker,
    )?;
    let picker = if response.resolved || !build_picker {
        PickerCandidates::Ready(Vec::new())
    } else {
        PickerCandidates::Records(PickerRecords {
            records,
            bookmarks,
            history,
            cwd: cwd.to_path_buf(),
            ranking: config.ranking.clone(),
        })
    };
    Ok(QueryOutcome { response, picker })
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
    let outcome = resolve_query(paths, config, cwd, query, false, io::stdin().is_terminal())?;
    emit_resolution(paths, config, outcome, cwd, shell_mode, action)
}

fn query_command(
    paths: &AppPaths,
    config: &Config,
    args: QueryArgs,
    action: Action,
) -> Result<i32> {
    let cwd = current_dir()?;
    let outcome = resolve_query(
        paths,
        config,
        &cwd,
        args.query,
        false,
        !args.json && io::stdin().is_terminal(),
    )?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&outcome.response)?);
        return Ok(if outcome.response.resolved {
            0
        } else {
            EXIT_AMBIGUOUS
        });
    }
    emit_resolution(paths, config, outcome, &cwd, false, action)
}

fn explain_command(paths: &AppPaths, config: &Config, query: Vec<String>) -> Result<i32> {
    let cwd = current_dir()?;
    let outcome = resolve_query(paths, config, &cwd, query, true, false)?;
    println!("{}", serde_json::to_string_pretty(&outcome.response)?);
    Ok(0)
}

fn bench_command(paths: &AppPaths, config: &Config, query: &str, samples: u8) -> Result<i32> {
    let cwd = current_dir()?;
    let started = Instant::now();
    let (records, bookmarks, history) = load_context(paths, config)?;
    let load_context_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let mut picker_samples = Vec::with_capacity(samples as usize);
    let mut resolve_samples = Vec::with_capacity(samples as usize);
    for _ in 0..samples {
        let (picker_records, picker_bookmarks, picker_history) = load_context(paths, config)?;
        let started = Instant::now();
        let _ = search::picker_candidates(
            picker_records,
            &picker_bookmarks,
            &picker_history,
            &cwd,
            &config.ranking,
        );
        picker_samples.push(started.elapsed().as_secs_f64() * 1_000.0);

        let started = Instant::now();
        let _ = search::resolve(
            &[query.to_owned()],
            &SearchContext {
                records: &records,
                bookmarks: &bookmarks,
                history: &history,
                cwd: &cwd,
                ranking: &config.ranking,
            },
            true,
            true,
        )?;
        resolve_samples.push(started.elapsed().as_secs_f64() * 1_000.0);
    }
    println!(
        "Dirgo local benchmark\n\nDataset directories  {}\nSamples              {samples}\nQuery                {query:?}\nLoad context          {:.3} ms\nFallback candidate build {:.3} ms median\nFuzzy resolution      {:.3} ms median",
        records.len(),
        load_context_ms,
        median_ms(&mut picker_samples),
        median_ms(&mut resolve_samples),
    );
    Ok(0)
}

fn median_ms(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

fn emit_resolution(
    paths: &AppPaths,
    config: &Config,
    outcome: QueryOutcome,
    origin: &Path,
    shell_mode: bool,
    action: Action,
) -> Result<i32> {
    let QueryOutcome { response, picker } = outcome;
    if let Some(path) = response.path {
        return complete_action(paths, config, origin, path, action, shell_mode);
    }
    if response.candidates.is_empty() && (!io::stdin().is_terminal() || picker.is_empty()) {
        eprintln!(
            "No directories match {:?}.\nTry a shorter query or run `dgo refresh`.",
            response.query
        );
        return Ok(EXIT_NO_MATCH);
    }
    let selection = match choose_candidate(
        &response.candidates,
        picker,
        &response.query,
        action,
        config,
    )? {
        crate::tui::PickOutcome::Selected(selection) => selection,
        crate::tui::PickOutcome::Refresh => {
            let summary = index::rebuild(paths, config)?;
            eprintln!(
                "Indexed {} directories ({} projects). Reopen Dirgo to search the refreshed index.",
                summary.directories, summary.projects
            );
            return Ok(if shell_mode { EXIT_ACTION_HANDLED } else { 0 });
        }
        crate::tui::PickOutcome::Cancelled => return Ok(EXIT_AMBIGUOUS),
    };
    complete_action(
        paths,
        config,
        origin,
        selection.path,
        selection.action,
        shell_mode,
    )
}

fn complete_action(
    paths: &AppPaths,
    config: &Config,
    origin: &Path,
    path: PathBuf,
    action: Action,
    shell_mode: bool,
) -> Result<i32> {
    ensure_existing_directory(&path)?;
    shell::validate_output_path(&path)?;
    match action {
        Action::Go => {
            if shell_mode {
                record_navigation(paths, origin, &path)?;
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

fn ensure_existing_directory(path: &Path) -> Result<()> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(DirgoError::User(format!(
            "directory no longer exists: {}. Run `dgo refresh`; repair or remove a bookmark if it points here",
            path.display()
        )))
    }
}

fn choose_candidate(
    initial_candidates: &[Candidate],
    picker: PickerCandidates,
    query: &str,
    default_action: Action,
    config: &Config,
) -> Result<crate::tui::PickOutcome> {
    if crate::tui::is_supported()
        && let PickerCandidates::Records(records) = picker
    {
        return crate::tui::pick_stream(
            records.into_stream(),
            query,
            default_action,
            &config.actions,
            picker_options(config),
        );
    }
    if crate::tui::is_supported()
        && let PickerCandidates::IndexStream(stream) = picker
    {
        return crate::tui::pick_index_stream(
            crate::tui::IndexStreamSource {
                index_path: stream.index_path,
                record_count: stream.record_count,
                bookmarks: stream.bookmarks,
                history: stream.history,
                cwd: stream.cwd,
                ranking: stream.ranking,
            },
            query,
            default_action,
            &config.actions,
            picker_options(config),
        );
    }
    let picker_candidates = match picker {
        PickerCandidates::Ready(candidates) => candidates,
        PickerCandidates::Records(records) => records.into_candidates(),
        PickerCandidates::IndexStream(_) => unreachable!("handled by interactive picker"),
    };
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
        return Ok(crate::tui::PickOutcome::Cancelled);
    }
    if crate::tui::is_supported() {
        return crate::tui::pick(
            picker_candidates,
            query,
            default_action,
            &config.actions,
            picker_options(config),
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
    Ok(
        selected.map_or(crate::tui::PickOutcome::Cancelled, |value| {
            crate::tui::PickOutcome::Selected(crate::tui::Selection {
                path: visible[value - 1].path.clone(),
                action: default_action,
            })
        }),
    )
}

fn picker_options(config: &Config) -> crate::tui::Options {
    crate::tui::Options {
        color: config.ui.accent != "none" && env::var_os("NO_COLOR").is_none(),
        unicode: config.ui.icons != "never",
    }
}

fn record_navigation(paths: &AppPaths, origin: &Path, destination: &Path) -> Result<()> {
    paths.ensure_dirs()?;
    let session = env::var("DGO_SESSION_ID").ok();
    StateStore::open(&paths.state_file)?.record_navigation(origin, destination, session.as_deref())
}

fn shell_resolve(paths: &AppPaths, config: &Config, args: ResolveArgs) -> Result<i32> {
    match args.query.as_slice() {
        [command] if command == "root" => {
            let Some((path, _)) = index::find_project_root(&args.cwd) else {
                return Err(DirgoError::NoMatch("project root".into()));
            };
            record_navigation(paths, &args.cwd, &path)?;
            println!("{}", path.display());
            Ok(0)
        }
        [command] if command == "back" => navigation_command(paths, Direction::Back),
        [command] if command == "forward" => navigation_command(paths, Direction::Forward),
        [command, rest @ ..] if command == "repo" => {
            repository_command(paths, config, &args.cwd, rest.to_vec(), true, Action::Go)
        }
        [command, rest @ ..] if command == "recent" => {
            recent_command(paths, config, &args.cwd, rest.to_vec(), true, Action::Go)
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
            ranking: &config.ranking,
        },
        true,
        !io::stdin().is_terminal(),
    )?;
    let picker_candidates =
        search::picker_candidates(records, &bookmarks, &history, cwd, &config.ranking);
    emit_resolution(
        paths,
        config,
        QueryOutcome {
            response,
            picker: PickerCandidates::Ready(picker_candidates),
        },
        cwd,
        shell_mode,
        action,
    )
}

fn recent_command(
    paths: &AppPaths,
    config: &Config,
    cwd: &Path,
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
                score_breakdown: crate::model::ScoreBreakdown::from_total(
                    history.last_visit as f64,
                ),
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
            picker: PickerCandidates::Ready(picker_candidates),
        },
        cwd,
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
    ensure_existing_directory(&path)?;
    shell::validate_output_path(&path)?;
    println!("{}", path.display());
    Ok(0)
}

fn bookmark_command(paths: &AppPaths, command: BookmarkCommand) -> Result<i32> {
    paths.ensure_dirs()?;
    let state = StateStore::open(&paths.state_file)?;
    match command {
        BookmarkCommand::Add { name, path } => {
            let path = path.unwrap_or(current_dir()?);
            let bookmark = state.add_bookmark(&name, &path)?;
            println!("Saved @{name} → {}", bookmark.path.display());
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

fn import_history(paths: &AppPaths, source: ImportSource) -> Result<i32> {
    match source {
        ImportSource::Zoxide => {
            let snapshot = history_import::read_zoxide()?;
            paths.ensure_dirs()?;
            let summary = StateStore::open(&paths.state_file)?.import_history(&snapshot.entries)?;
            println!(
                "Imported {} zoxide entries ({} unchanged, {} stale skipped).",
                summary.imported, summary.unchanged, snapshot.skipped_stale
            );
            Ok(0)
        }
    }
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
    paths.ensure_dirs()?;
    println!(
        "✓ storage        cache={} state={}",
        paths.cache_dir.display(),
        paths.state_dir.display()
    );
    if paths.index_file.exists() {
        match IndexStore::open(&paths.index_file).and_then(|store| store.summary()) {
            Ok(summary) => {
                let age = unix_now().saturating_sub(summary.built_at);
                let marker = if summary.built_at == 0 || age > STALE_INDEX_AFTER_SECONDS {
                    "!"
                } else {
                    "✓"
                };
                let detail = if marker == "!" {
                    format!(
                        "{} directories; stale ({}) — run `dgo refresh`",
                        summary.directories,
                        format_age(age)
                    )
                } else {
                    format!(
                        "{} directories; built {} ago",
                        summary.directories,
                        format_age(age)
                    )
                };
                println!("{marker} index          {detail}");
            }
            Err(error) => println!("! index          unhealthy ({error}); run `dgo refresh`"),
        }
    } else {
        println!("! index          missing; it will be built on first search");
    }
    let state = StateStore::open(&paths.state_file)?;
    println!(
        "✓ state          healthy ({} bookmarks)",
        state.bookmarks()?.len()
    );
    let availability = crate::actions::availability(&config.actions);
    println!(
        "{} actions        open={} copy={} editor={}",
        if availability.open && availability.copy && availability.editor {
            "✓"
        } else {
            "!"
        },
        availability.open,
        availability.copy,
        availability.editor
    );
    if let Some((path, bytes)) = oversized_shell_startup() {
        println!(
            "! shell startup  {} is {:.1} MiB; a large shell config can delay or crash terminals",
            path.display(),
            bytes as f64 / 1_048_576.0
        );
    } else {
        println!("✓ shell startup  no oversized startup file detected");
    }
    println!("✓ platform       {} {}", env::consts::OS, env::consts::ARCH);
    println!("\nDoctor completed. Lines marked ! need attention but do not block all commands.");
    Ok(0)
}

fn oversized_shell_startup() -> Option<(PathBuf, u64)> {
    let home = dirs::home_dir()?;
    let shell = env::var_os("SHELL")?;
    let shell = Path::new(&shell).file_name()?.to_string_lossy();
    let startup = match shell.as_ref() {
        "zsh" => home.join(".zshrc"),
        "bash" => home.join(".bashrc"),
        "fish" => home.join(".config/fish/config.fish"),
        _ => return None,
    };
    let bytes = std::fs::metadata(&startup).ok()?.len();
    (bytes > SLOW_SHELL_STARTUP_BYTES).then_some((startup, bytes))
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
