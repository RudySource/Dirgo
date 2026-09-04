use std::{
    borrow::Cow,
    collections::HashMap,
    env,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use unicode_width::UnicodeWidthStr;

use clap::Parser;

use crate::{
    DirgoError, Result,
    actions::Action,
    cli::{
        BookmarkCommand, Cli, Command, CompletionOutputFormat, ConfigCommand, HistoryScopeArgs,
        ImportSource, QueryArgs, ResolveArgs, SuggestionsCommand, SuggestionsHistoryCommand,
        UpdateNotificationMode,
    },
    config::Config,
    history_import,
    index::{self, IndexStore},
    model::{Candidate, PathHistory, QueryResponse, unix_now},
    palette::{
        PaletteAction, PaletteCoordinator, PaletteResultFrame, PaletteSession, PaletteSource,
        PaletteViewOptions, ProviderBudget,
    },
    paths::{self, AppPaths},
    search::{self, SearchContext},
    shell,
    state::{StateStore, read_suggestion_context},
    suggestions::{
        CommandCatalog, CommandHistoryEventV2, CommandHistoryScope, CommandHistoryStore,
        CommandOutcome, DecodedHistoryRecord, MAX_REQUEST_BYTES, SuggestionData, SuggestionEngine,
        SuggestionResponse, claim_project_command_refresh, decode_history_record_frame,
        decode_request_line, encode_response_line, is_sensitive_command,
        load_cached_project_command_snapshot, load_project_command_snapshot, pick_suggestion,
        read_bounded_frame, read_command_history, read_history_snapshot,
        refresh_project_command_cache, write_suggestions_config,
    },
    terminal,
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
    let mut cli = Cli::parse();
    cli.normalize_resolve_action().map_err(DirgoError::User)?;
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
        Some(Command::Support) => {
            print_support();
            return Ok(0);
        }
        _ => {}
    }

    let paths = AppPaths::discover()?;
    if cli.update {
        return crate::update::run_update();
    }
    if matches!(&cli.command, Some(Command::CheckUpdate)) {
        return crate::update::refresh_cache(&paths);
    }
    if let Some(Command::UpdateNotifications { mode }) = &cli.command {
        return crate::update::set_notifications(
            &paths,
            matches!(mode, UpdateNotificationMode::On),
        );
    }
    if let Some(Command::Setup(args)) = &cli.command {
        return crate::setup::run(&paths, args, cli.no_color, cli.no_unicode);
    }
    if let Some(Command::Roots { command }) = &cli.command {
        return crate::roots::run(&paths, command);
    }
    if matches!(
        &cli.command,
        Some(Command::Config {
            command: ConfigCommand::Path
        })
    ) {
        print_output_path(&paths.config_file);
        return Ok(0);
    }
    if matches!(
        &cli.command,
        None | Some(
            Command::Query(_)
                | Command::Root
                | Command::Repo { .. }
                | Command::Recent { .. }
                | Command::Back
                | Command::Forward
                | Command::Resolve(_)
        )
    ) {
        crate::update::notify_and_refresh_in_background(&paths);
    }
    let config_result = Config::load(&paths);
    if cli.doctor || matches!(&cli.command, Some(Command::Doctor)) {
        return doctor(&paths, config_result);
    }
    let mut config = config_result?;
    if cli.no_color {
        config.ui.accent = "none".into();
    }
    if cli.no_unicode {
        config.ui.icons = "never".into();
    }

    if let Some(Command::Suggestions { command }) = &cli.command {
        return suggestions_command(&paths, &mut config, command);
    }
    if let Some(Command::Workflows { command }) = &cli.command {
        return crate::workflows::commands::run(&paths, &mut config, command);
    }
    if matches!(cli.command, Some(Command::Suggest)) {
        return hidden_suggest(&paths, &config);
    }
    if let Some(Command::SuggestWorker { ready }) = &cli.command {
        return hidden_suggest_worker(&paths, &config, *ready);
    }
    if matches!(cli.command, Some(Command::SuggestRecord)) {
        return hidden_suggest_record(&paths, &config);
    }
    if matches!(cli.command, Some(Command::SuggestEnabled)) {
        return Ok(if config.suggestions.enabled { 0 } else { 1 });
    }
    if matches!(cli.command, Some(Command::SuggestLiveEnabled)) {
        return Ok(
            if config.suggestions.enabled && config.suggestions.live_panel {
                0
            } else {
                1
            },
        );
    }
    if matches!(cli.command, Some(Command::SuggestNativeEnabled)) {
        return Ok(
            if config.suggestions.enabled && config.suggestions.native_completions {
                0
            } else {
                1
            },
        );
    }
    if matches!(cli.command, Some(Command::SuggestDebounce)) {
        println!("{:.3}", config.suggestions.debounce_ms as f64 / 1_000.0);
        return Ok(0);
    }
    if matches!(cli.command, Some(Command::SuggestNativeTimeout)) {
        println!("{}", config.suggestions.native_timeout_ms);
        return Ok(0);
    }
    if matches!(cli.command, Some(Command::SuggestHistoryEnabled)) {
        return Ok(
            if config.suggestions.enabled && config.suggestions.command_history {
                0
            } else {
                1
            },
        );
    }
    if let Some(Command::SuggestProjectRefresh { cwd }) = &cli.command {
        refresh_project_command_cache(&paths.cache_dir, cwd)?;
        return Ok(0);
    }
    if let Some(Command::SuggestShell { shell, cwd }) = &cli.command {
        return hidden_suggest_shell(&paths, &config, *shell, cwd);
    }
    if let Some(Command::SuggestComplete {
        shell,
        cwd,
        terminal_rows,
        terminal_columns,
        format,
        page_offset,
        page_size,
        include_total,
        include_descriptions,
        frame_generation,
    }) = &cli.command
    {
        return hidden_suggest_complete(
            &paths,
            &config,
            *shell,
            cwd,
            CompletionOutputOptions {
                terminal_rows: *terminal_rows,
                terminal_columns: *terminal_columns,
                format: *format,
                page_offset: *page_offset,
                page_size: *page_size,
                include_total: *include_total,
                include_descriptions: *include_descriptions,
                frame_generation: *frame_generation,
            },
        );
    }
    if let Some(Command::SuggestPick {
        shell,
        cwd,
        request_path,
        output_path,
    }) = &cli.command
    {
        return hidden_suggest_pick(&paths, &config, *shell, cwd, request_path, output_path);
    }
    if let Some(Command::PaletteJson { cwd }) = &cli.command {
        return hidden_palette_json(&paths, &config, cwd);
    }
    if let Some(Command::PalettePick {
        shell,
        cwd,
        output_path,
        query,
    }) = &cli.command
    {
        return hidden_palette_pick(
            &paths,
            &config,
            *shell,
            cwd,
            output_path,
            query.as_deref().unwrap_or_default(),
        );
    }

    if cli.refresh || matches!(cli.command, Some(Command::Refresh)) {
        let summary = index::rebuild(&paths, &config)?;
        println!(
            "Indexed {} directories ({} projects).",
            summary.directories, summary.projects
        );
        return Ok(0);
    }
    if cli.bookmarks || matches!(cli.command, Some(Command::Bookmarks)) {
        return list_bookmarks(&paths);
    }
    if let Some(name) = cli.forget {
        return remove_bookmark(&paths, &name);
    }

    match cli.command {
        Some(Command::Init { .. } | Command::Completions { .. } | Command::Setup(_)) => {
            unreachable!("handled before storage access")
        }
        Some(Command::Query(args)) => query_command(&paths, &config, args, requested_action),
        Some(Command::Explain { query }) => explain_command(&paths, &config, query),
        Some(Command::Bench { query, samples }) => bench_command(&paths, &config, &query, samples),
        Some(Command::Root) => print_project_root(current_dir()?),
        Some(Command::Roots { .. }) => unreachable!("handled before configuration loading"),
        Some(Command::Palette { query }) => {
            palette_command(&paths, &config, &current_dir()?, query.join(" "))
        }
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
        Some(Command::Stats) => stats(&paths, &config),
        Some(Command::Config { command }) => config_command(&paths, &config, command),
        Some(Command::Support) => unreachable!("handled before storage access"),
        Some(
            Command::Suggestions { .. }
            | Command::Workflows { .. }
            | Command::Suggest
            | Command::SuggestWorker { .. }
            | Command::SuggestRecord
            | Command::SuggestEnabled
            | Command::SuggestLiveEnabled
            | Command::SuggestNativeEnabled
            | Command::SuggestDebounce
            | Command::SuggestNativeTimeout
            | Command::SuggestHistoryEnabled
            | Command::PaletteJson { .. }
            | Command::PalettePick { .. }
            | Command::SuggestProjectRefresh { .. }
            | Command::SuggestShell { .. }
            | Command::SuggestComplete { .. }
            | Command::SuggestPick { .. },
        ) => {
            unreachable!("handled before command dispatch")
        }
        Some(Command::Resolve(args)) => shell_resolve(&paths, &config, args, requested_action),
        Some(
            Command::Refresh
            | Command::Doctor
            | Command::Bookmarks
            | Command::UpdateNotifications { .. }
            | Command::CheckUpdate,
        ) => {
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

fn suggestions_command(
    paths: &AppPaths,
    config: &mut Config,
    command: &SuggestionsCommand,
) -> Result<i32> {
    match command {
        SuggestionsCommand::Enable => {
            config.suggestions.enabled = true;
            write_suggestions_config(&paths.config_file, config)?;
            println!(
                "Shell-native suggestions enabled. Open a new shell or reload Dirgo integration."
            );
        }
        SuggestionsCommand::Disable => {
            config.suggestions.enabled = false;
            write_suggestions_config(&paths.config_file, config)?;
            println!(
                "Shell-native suggestions disabled immediately. Reload Dirgo integration to remove key bindings. Command history was not deleted."
            );
        }
        SuggestionsCommand::Status => {
            println!(
                "Suggestions      {}\nCommand history  {}\nMaximum results  {}",
                enabled_label(config.suggestions.enabled),
                enabled_label(config.suggestions.command_history),
                config.suggestions.max_results,
            );
        }
        SuggestionsCommand::Doctor => {
            println!(
                "Dirgo Suggestions Doctor\n\nconfiguration   valid\nsuggestions     {}\ncommand history {}\nprotocol        v{}\nstate           {}\n\nhistory capture\n  zsh           full\n  fish          full\n  bash          partial\n  powershell    partial",
                enabled_label(config.suggestions.enabled),
                enabled_label(config.suggestions.command_history),
                crate::suggestions::PROTOCOL_VERSION,
                if paths.suggestions_state_file.exists() {
                    "present"
                } else {
                    "not created"
                },
            );
        }
        SuggestionsCommand::History { command } => match command {
            SuggestionsHistoryCommand::Enable => {
                config.suggestions.command_history = true;
                write_suggestions_config(&paths.config_file, config)?;
                println!("Filtered local command history enabled.");
            }
            SuggestionsHistoryCommand::Disable => {
                config.suggestions.command_history = false;
                write_suggestions_config(&paths.config_file, config)?;
                println!("Command history disabled. Stored entries were not deleted.");
            }
            SuggestionsHistoryCommand::Status { json } => {
                if !paths.suggestions_state_file.exists() {
                    if *json {
                        println!(
                            "{{\"schema_version\":{},\"event_count\":0,\"aggregate_count\":0}}",
                            crate::suggestions::HISTORY_SCHEMA_VERSION
                        );
                    } else {
                        println!(
                            "History schema  v{}\nEvents          0\nAggregates      0",
                            crate::suggestions::HISTORY_SCHEMA_VERSION
                        );
                    }
                } else {
                    let status = read_history_snapshot(&paths.suggestions_state_file)?.status();
                    if *json {
                        println!("{}", serde_json::to_string(&status)?);
                    } else {
                        println!(
                            "History schema  v{}\nEvents          {}\nAggregates      {}",
                            status.schema_version, status.event_count, status.aggregate_count
                        );
                    }
                }
            }
            SuggestionsHistoryCommand::List { scope, limit, json } => {
                let selected = resolve_history_scope(scope, false)?;
                let mut rows = if paths.suggestions_state_file.exists() {
                    read_history_snapshot(&paths.suggestions_state_file)?
                        .aggregates_in_scope(&selected)?
                } else {
                    Vec::new()
                };
                rows.truncate(usize::from(*limit));
                if *json {
                    println!("{}", serde_json::to_string(&rows)?);
                } else {
                    for row in rows {
                        println!(
                            "{}\t{}\t{}",
                            row.last_used,
                            row.use_count,
                            safe_history_text(&row.command)
                        );
                    }
                }
            }
            SuggestionsHistoryCommand::Inspect { event_id, json } => {
                let event = if paths.suggestions_state_file.exists() {
                    read_history_snapshot(&paths.suggestions_state_file)?
                        .events
                        .into_iter()
                        .find(|event| event.id == *event_id)
                } else {
                    None
                };
                let event = event.ok_or_else(|| {
                    DirgoError::User(format!("command-history event {event_id} does not exist"))
                })?;
                if *json {
                    println!("{}", serde_json::to_string(&event)?);
                } else {
                    println!(
                        "Event {}\nCommand   {}\nOutcome   {:?}\nStarted   {}\nDuration  {}",
                        event.id,
                        safe_history_text(&event.command),
                        event.outcome,
                        event.started_at,
                        event
                            .duration_ms
                            .map_or_else(|| "unknown".into(), |value| format!("{value} ms"))
                    );
                }
            }
            SuggestionsHistoryCommand::Clear { scope } => {
                if !paths.suggestions_state_file.exists() {
                    println!("Command history is already empty.");
                } else {
                    let selected = resolve_history_scope(scope, true)?;
                    let store = CommandHistoryStore::open(&paths.suggestions_state_file)?;
                    let events = store.events_in_scope(&selected)?.len();
                    let aggregates = store.aggregates_in_scope(&selected)?.len();
                    store.clear_scope(&selected)?;
                    println!("Removed {events} events and {aggregates} aggregates.");
                }
            }
            SuggestionsHistoryCommand::Export {
                scope,
                output,
                include_paths,
                force,
            } => {
                let selected = resolve_history_scope(scope, false)?;
                let snapshot = read_history_snapshot(&paths.suggestions_state_file)?;
                export_history(
                    &snapshot.events_in_scope(&selected),
                    output,
                    *include_paths,
                    *force,
                )?;
                println!(
                    "Exported command history to {}.",
                    terminal::safe_path(output)
                );
            }
        },
    }
    Ok(0)
}

fn resolve_history_scope(
    args: &HistoryScopeArgs,
    bare_means_all: bool,
) -> Result<CommandHistoryScope> {
    if args.all || (bare_means_all && args.project.is_none() && !args.global) {
        return Ok(CommandHistoryScope::All);
    }
    if args.global {
        return Ok(CommandHistoryScope::Global);
    }
    let path = args
        .project
        .clone()
        .unwrap_or(std::env::current_dir().map_err(|error| DirgoError::io(".", error))?);
    let path = path
        .canonicalize()
        .map_err(|error| DirgoError::io(&path, error))?;
    match index::find_project_root(&path) {
        Some((root, _)) => Ok(CommandHistoryScope::Project(root)),
        None if args.project.is_some() => Err(DirgoError::User(format!(
            "no project root found at or above {}",
            path.display()
        ))),
        None => Ok(CommandHistoryScope::Global),
    }
}

fn safe_history_text(command: &str) -> String {
    let escaped = command
        .chars()
        .flat_map(char::escape_default)
        .take(240)
        .collect::<String>();
    if escaped.len() < command.len() {
        format!("{escaped}…")
    } else {
        escaped
    }
}

fn export_history(
    events: &[CommandHistoryEventV2],
    output: &Path,
    include_paths: bool,
    force: bool,
) -> Result<()> {
    use std::fs::OpenOptions;
    if std::fs::symlink_metadata(output).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(DirgoError::User(
            "refusing to export through a symlink target".into(),
        ));
    }
    if output.exists() && !force {
        return Err(DirgoError::User(format!(
            "{} already exists; pass --force to replace it",
            output.display()
        )));
    }
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| DirgoError::io(parent, error))?;
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(DirgoError::NonUtf8Path)?;
    let temp = parent.join(format!(
        ".{name}.dirgo-{}-{}.tmp",
        std::process::id(),
        unix_now()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| DirgoError::io(&temp, error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| DirgoError::io(&temp, error))?;
    }
    for event in events {
        let mut exported_event = serde_json::Map::new();
        exported_event.insert("id".into(), event.id.into());
        exported_event.insert("command".into(), event.command.clone().into());
        exported_event.insert("started_at".into(), event.started_at.into());
        exported_event.insert(
            "duration_ms".into(),
            serde_json::to_value(event.duration_ms)?,
        );
        exported_event.insert("exit_code".into(), serde_json::to_value(event.exit_code)?);
        exported_event.insert("outcome".into(), serde_json::to_value(event.outcome)?);
        exported_event.insert(
            "session_id".into(),
            serde_json::to_value(&event.session_id)?,
        );
        if include_paths {
            exported_event.insert("cwd".into(), serde_json::to_value(&event.cwd)?);
            exported_event.insert(
                "project_root".into(),
                serde_json::to_value(&event.project_root)?,
            );
        }
        let row = serde_json::json!({
            "format": "dirgo-command-history",
            "version": 1,
            "event": exported_event,
        });
        writeln!(file, "{}", serde_json::to_string(&row)?)
            .map_err(|error| DirgoError::io(&temp, error))?;
    }
    file.sync_all()
        .map_err(|error| DirgoError::io(&temp, error))?;
    drop(file);
    crate::suggestions::settings::replace_file(&temp, output)?;
    Ok(())
}

fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "enabled" } else { "disabled" }
}

fn hidden_suggest(paths: &AppPaths, config: &Config) -> Result<i32> {
    let mut input = Vec::new();
    io::stdin()
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut input)
        .map_err(|error| DirgoError::io("stdin", error))?;
    let request = match decode_request_line(&input) {
        Ok(request) => request,
        Err(error) => {
            let response = SuggestionResponse::error(0, terminal::safe_text(&error.to_string()));
            print!(
                "{}",
                encode_response_line(&response)
                    .map_err(|error| DirgoError::User(error.to_string()))?
            );
            return Ok(2);
        }
    };
    let suggestions = if config.suggestions.enabled {
        let project = cached_project_commands(paths, &request.cwd);
        build_suggestion_engine(paths, config, needs_executables(&request))?
            .suggest_with_project(&request, project.as_ref())
    } else {
        Vec::new()
    };
    let response = SuggestionResponse::success(request.request_id, suggestions);
    print!(
        "{}",
        encode_response_line(&response).map_err(|error| DirgoError::User(error.to_string()))?
    );
    Ok(0)
}

fn hidden_suggest_worker(paths: &AppPaths, config: &Config, ready: bool) -> Result<i32> {
    let mut active_config = config.clone();
    let mut engine = build_indexed_suggestion_engine(paths, &active_config)?;
    let mut data_stamp = suggestion_data_stamp(paths);
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    let mut last_request_id = None;
    if ready {
        writer
            .write_all(format!("READY {}\n", crate::suggestions::PROTOCOL_VERSION).as_bytes())
            .and_then(|_| writer.flush())
            .map_err(|error| DirgoError::io("stdout", error))?;
    }
    loop {
        let frame = match read_bounded_frame(&mut reader) {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(error) => {
                let response =
                    SuggestionResponse::error(0, terminal::safe_text(&error.to_string()));
                let encoded = encode_response_line(&response)
                    .map_err(|error| DirgoError::User(error.to_string()))?;
                writer
                    .write_all(encoded.as_bytes())
                    .and_then(|_| writer.flush())
                    .map_err(|error| DirgoError::io("stdout", error))?;
                continue;
            }
        };
        let response = match decode_request_line(&frame) {
            Ok(request) => {
                if last_request_id.is_some_and(|last| request.request_id <= last) {
                    SuggestionResponse::error(request.request_id, "stale request id")
                } else {
                    last_request_id = Some(request.request_id);
                    let next_stamp = suggestion_data_stamp(paths);
                    if next_stamp != data_stamp
                        && let Ok(next_config) = Config::load(paths)
                        && let Ok(next_engine) =
                            build_indexed_suggestion_engine(paths, &next_config)
                    {
                        active_config = next_config;
                        engine = next_engine;
                        data_stamp = next_stamp;
                    }
                    let project = cached_project_commands(paths, &request.cwd);
                    SuggestionResponse::success(
                        request.request_id,
                        if active_config.suggestions.enabled {
                            engine.suggest_with_project(&request, project.as_ref())
                        } else {
                            Vec::new()
                        },
                    )
                }
            }
            Err(error) => SuggestionResponse::error(0, terminal::safe_text(&error.to_string())),
        };
        let encoded =
            encode_response_line(&response).map_err(|error| DirgoError::User(error.to_string()))?;
        writer
            .write_all(encoded.as_bytes())
            .and_then(|_| writer.flush())
            .map_err(|error| DirgoError::io("stdout", error))?;
    }
    Ok(0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SuggestionDataStamp {
    config: Option<(std::time::SystemTime, u64)>,
    index: Option<(std::time::SystemTime, u64)>,
    history: Option<(std::time::SystemTime, u64)>,
}

fn suggestion_data_stamp(paths: &AppPaths) -> SuggestionDataStamp {
    fn stamp(path: &Path) -> Option<(std::time::SystemTime, u64)> {
        let metadata = std::fs::metadata(path).ok()?;
        Some((metadata.modified().ok()?, metadata.len()))
    }
    SuggestionDataStamp {
        config: stamp(&paths.config_file),
        index: stamp(&paths.index_file),
        history: stamp(&paths.suggestions_state_file),
    }
}

fn cached_project_commands(
    paths: &AppPaths,
    cwd: &Path,
) -> Option<crate::suggestions::ProjectCommandSnapshot> {
    let snapshot = load_cached_project_command_snapshot(&paths.cache_dir, cwd);
    if claim_project_command_refresh(&paths.cache_dir, cwd)
        && let Ok(executable) = env::current_exe()
        && let Ok(mut child) = std::process::Command::new(executable)
            .arg("__suggest-project-refresh")
            .arg("--cwd")
            .arg(cwd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
    snapshot
}

fn hidden_suggest_record(paths: &AppPaths, config: &Config) -> Result<i32> {
    if !config.suggestions.enabled || !config.suggestions.command_history {
        return Ok(0);
    }
    let mut input = Vec::new();
    io::stdin()
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut input)
        .map_err(|error| DirgoError::io("stdin", error))?;
    if input.len() > MAX_REQUEST_BYTES {
        return Err(DirgoError::User(
            "command history entry exceeds 65536 bytes".into(),
        ));
    }
    let decoded =
        decode_history_record_frame(&input).map_err(|error| DirgoError::User(error.to_string()))?;
    let command = match &decoded {
        DecodedHistoryRecord::LegacyCommand(command) => command.as_str(),
        DecodedHistoryRecord::V2(frame) => frame.command.as_str(),
    };
    if command.starts_with(' ') || is_sensitive_command(command, &config.suggestions.deny_patterns)
    {
        return Ok(0);
    }
    let store = CommandHistoryStore::open(&paths.suggestions_state_file)?;
    match decoded {
        DecodedHistoryRecord::LegacyCommand(command) => {
            store.record(&command, unix_now(), &config.suggestions)?;
        }
        DecodedHistoryRecord::V2(frame) => {
            let cwd = frame
                .cwd
                .canonicalize()
                .map_err(|error| DirgoError::io(&frame.cwd, error))?;
            let project_root = index::find_project_root(&cwd).map(|(root, _)| root);
            store.record_event(
                CommandHistoryEventV2 {
                    id: 0,
                    command: frame.command,
                    started_at: frame.started_at,
                    duration_ms: frame.duration_ms,
                    cwd,
                    project_root,
                    exit_code: frame.exit_code,
                    outcome: CommandOutcome::from_exit_code(frame.exit_code),
                    session_id: frame.session_id,
                },
                &config.suggestions,
            )?;
        }
    }
    Ok(0)
}

fn hidden_suggest_shell(
    paths: &AppPaths,
    config: &Config,
    shell: crate::shell::Shell,
    cwd: &Path,
) -> Result<i32> {
    if !config.suggestions.enabled {
        return Ok(0);
    }
    let (before_cursor, after_cursor) = read_shell_buffer(None)?;
    let request = shell_suggestion_request(config, shell, cwd, before_cursor, after_cursor);
    let project = cached_project_commands(paths, &request.cwd);
    if let Some(suggestion) = build_suggestion_engine(paths, config, needs_executables(&request))?
        .suggest_with_project(&request, project.as_ref())
        .first()
    {
        println!("{}", suggestion.edit.replacement);
    }
    Ok(0)
}

fn hidden_suggest_pick(
    paths: &AppPaths,
    config: &Config,
    shell: crate::shell::Shell,
    cwd: &Path,
    request_path: &Path,
    output_path: &Path,
) -> Result<i32> {
    let (before_cursor, after_cursor) = read_shell_buffer(Some(request_path))?;
    if !config.suggestions.enabled {
        write_private_picker_result(output_path, b"")?;
        return Ok(0);
    }
    let request = shell_suggestion_request(config, shell, cwd, before_cursor, after_cursor);
    let project = cached_project_commands(paths, &request.cwd);
    let suggestions = build_suggestion_engine(paths, config, needs_executables(&request))?
        .suggest_with_project(&request, project.as_ref());
    let selection = pick_suggestion(
        &suggestions,
        crate::suggestions::PickerOptions {
            color: config.ui.accent != "none" && env::var_os("NO_COLOR").is_none(),
            unicode: config.ui.icons != "never",
        },
    )?;
    let contents = selection
        .map(|selection| {
            crate::suggestions::SuggestionPickerResultFrame::from_selection(
                &suggestions[selection.index()],
                selection.accept(),
                shell,
            )
            .map(|frame| frame.encode())
        })
        .transpose()?
        .unwrap_or_default();
    write_private_picker_result(output_path, contents.as_bytes())?;
    Ok(0)
}

fn palette_budgets() -> HashMap<PaletteSource, ProviderBudget> {
    HashMap::from([
        (
            PaletteSource::Files,
            ProviderBudget::new(256, Duration::from_millis(35)),
        ),
        (
            PaletteSource::Tasks,
            ProviderBudget::new(128, Duration::from_millis(20)),
        ),
        (
            PaletteSource::Workflows,
            ProviderBudget::new(64, Duration::from_millis(20)),
        ),
        (
            PaletteSource::Git,
            ProviderBudget::new(128, Duration::from_millis(120)),
        ),
        (
            PaletteSource::Compose,
            ProviderBudget::new(64, Duration::from_millis(20)),
        ),
        (
            PaletteSource::Places,
            ProviderBudget::new(128, Duration::from_millis(20)),
        ),
    ])
}

fn build_palette_snapshot(
    paths: &AppPaths,
    config: &Config,
    cwd: &Path,
) -> Result<crate::palette::PaletteSnapshot> {
    let cwd = cwd
        .canonicalize()
        .map_err(|error| DirgoError::io(cwd, error))?;
    let (records, bookmarks, _) = load_context(paths, config)?;
    let project_root = index::find_project_root(&cwd).map(|(root, _)| root);
    let file_root = project_root.as_deref().unwrap_or(&cwd);
    let project = project_root
        .as_deref()
        .map(load_project_command_snapshot)
        .transpose()?;
    let empty_project = crate::suggestions::ProjectCommandSnapshot::new(cwd.clone(), Vec::new());
    let project = project.as_ref().unwrap_or(&empty_project);
    let workflow = load_workflow_suggestion_snapshot(paths, config);
    let workflow_scope = project_root
        .clone()
        .map(crate::workflows::WorkflowScope::Project)
        .unwrap_or(crate::workflows::WorkflowScope::Global);
    let project_commands = project
        .commands()
        .iter()
        .map(|command| command.replacement.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let budgets = palette_budgets();
    let budget = |source| {
        budgets
            .get(&source)
            .copied()
            .expect("every palette provider has a budget")
    };
    let batches = std::thread::scope(|scope| {
        let files = scope
            .spawn(|| crate::palette::providers::files(file_root, budget(PaletteSource::Files)));
        let tasks =
            scope.spawn(|| crate::palette::providers::tasks(project, budget(PaletteSource::Tasks)));
        let workflows = scope.spawn(|| {
            workflow.as_ref().map_or_else(
                || {
                    crate::palette::ProviderBatch::ready(
                        PaletteSource::Workflows,
                        Vec::new(),
                        Duration::ZERO,
                    )
                },
                |snapshot| {
                    crate::palette::providers::workflows(
                        snapshot,
                        workflow_scope,
                        &project_commands,
                        budget(PaletteSource::Workflows),
                    )
                },
            )
        });
        let git = scope.spawn(|| crate::palette::providers::git(&cwd, budget(PaletteSource::Git)));
        let compose = scope
            .spawn(|| crate::palette::providers::compose(project, budget(PaletteSource::Compose)));
        let places = scope.spawn(|| {
            crate::palette::providers::places(&records, &bookmarks, budget(PaletteSource::Places))
        });
        vec![
            files.join().unwrap_or_else(|_| {
                crate::palette::ProviderBatch::failed(
                    PaletteSource::Files,
                    "files provider stopped unexpectedly",
                )
            }),
            tasks.join().unwrap_or_else(|_| {
                crate::palette::ProviderBatch::failed(
                    PaletteSource::Tasks,
                    "tasks provider stopped unexpectedly",
                )
            }),
            workflows.join().unwrap_or_else(|_| {
                crate::palette::ProviderBatch::failed(
                    PaletteSource::Workflows,
                    "workflows provider stopped unexpectedly",
                )
            }),
            git.join().unwrap_or_else(|_| {
                crate::palette::ProviderBatch::failed(
                    PaletteSource::Git,
                    "git provider stopped unexpectedly",
                )
            }),
            compose.join().unwrap_or_else(|_| {
                crate::palette::ProviderBatch::failed(
                    PaletteSource::Compose,
                    "compose provider stopped unexpectedly",
                )
            }),
            places.join().unwrap_or_else(|_| {
                crate::palette::ProviderBatch::failed(
                    PaletteSource::Places,
                    "places provider stopped unexpectedly",
                )
            }),
        ]
    });
    Ok(PaletteCoordinator::new(budgets).merge(batches))
}

fn hidden_palette_json(paths: &AppPaths, config: &Config, cwd: &Path) -> Result<i32> {
    let snapshot = build_palette_snapshot(paths, config, cwd)?;
    let states = PaletteSource::FILTERS
        .into_iter()
        .filter(|source| *source != PaletteSource::All)
        .map(|source| (source.as_str(), snapshot.state(source)))
        .collect::<HashMap<_, _>>();
    serde_json::to_writer(
        io::stdout().lock(),
        &serde_json::json!({
            "version": 1,
            "items": snapshot.items(PaletteSource::All),
            "states": states,
        }),
    )
    .map_err(|error| DirgoError::User(format!("could not encode palette snapshot: {error}")))?;
    println!();
    Ok(0)
}

fn palette_command(paths: &AppPaths, config: &Config, cwd: &Path, query: String) -> Result<i32> {
    let snapshot = build_palette_snapshot(paths, config, cwd)?;
    let session = PaletteSession::new(snapshot, query, Instant::now());
    let action = crate::palette::pick(
        session,
        PaletteViewOptions {
            color: config.ui.accent != "none" && env::var_os("NO_COLOR").is_none(),
            unicode: config.ui.icons != "never",
        },
    )?;
    if let Some(action) = action {
        handle_palette_action(config, action, None)?;
    }
    Ok(0)
}

fn hidden_palette_pick(
    paths: &AppPaths,
    config: &Config,
    shell: crate::shell::Shell,
    cwd: &Path,
    output_path: &Path,
    query: &str,
) -> Result<i32> {
    shell::validate_output_path(output_path)?;
    if query.contains(['\r', '\n']) || query.chars().any(char::is_control) {
        return Err(DirgoError::User(
            "palette query cannot contain terminal control characters".into(),
        ));
    }
    let snapshot = build_palette_snapshot(paths, config, cwd)?;
    let session = PaletteSession::new(snapshot, query.to_owned(), Instant::now());
    let action = crate::palette::pick(
        session,
        PaletteViewOptions {
            color: config.ui.accent != "none" && env::var_os("NO_COLOR").is_none(),
            unicode: config.ui.icons != "never",
        },
    )?;
    let contents = match action {
        Some(action) => handle_palette_action(config, action, Some(shell))?,
        None => String::new(),
    };
    write_private_picker_result(output_path, contents.as_bytes())?;
    Ok(0)
}

fn handle_palette_action(
    config: &Config,
    action: PaletteAction,
    shell: Option<crate::shell::Shell>,
) -> Result<String> {
    if let Some(shell) = shell
        && let Some(frame) = PaletteResultFrame::from_action(&action, shell)?
    {
        return Ok(frame.encode());
    }
    match action {
        PaletteAction::Navigate { path } => {
            print_output_path(&path);
        }
        PaletteAction::Insert { text } => println!("{text}"),
        PaletteAction::InsertCommand { program, args } => {
            println!("{} {}", program, args.join(" "));
        }
        PaletteAction::Open { path } => {
            crate::actions::execute(Action::Open, &path, &config.actions)?;
        }
        PaletteAction::CopyPath { path } => {
            crate::actions::execute(Action::Copy, &path, &config.actions)?;
        }
        PaletteAction::OpenEditor { path } => {
            crate::actions::execute(Action::Editor, &path, &config.actions)?;
        }
    }
    Ok(String::new())
}

fn write_private_picker_result(path: &Path, contents: &[u8]) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| DirgoError::io(path, error))?;
    file.write_all(contents)
        .map_err(|error| DirgoError::io(path, error))
}

fn read_shell_buffer(request_path: Option<&Path>) -> Result<(String, String)> {
    let mut input = Vec::new();
    if let Some(path) = request_path {
        std::fs::File::open(path)
            .map_err(|error| DirgoError::io(path, error))?
            .take((MAX_REQUEST_BYTES + 1) as u64)
            .read_to_end(&mut input)
            .map_err(|error| DirgoError::io(path, error))?;
    } else {
        io::stdin()
            .take((MAX_REQUEST_BYTES + 1) as u64)
            .read_to_end(&mut input)
            .map_err(|error| DirgoError::io("stdin", error))?;
    }
    if input.len() > MAX_REQUEST_BYTES {
        return Err(DirgoError::User(
            "suggestion input exceeds 65536 bytes".into(),
        ));
    }
    let mut fields = input.splitn(3, |byte| *byte == 0);
    let before = fields.next().unwrap_or_default();
    let after = fields.next().unwrap_or_default();
    if fields.next().is_some_and(|extra| !extra.is_empty()) {
        return Err(DirgoError::User(
            "suggestion input has too many fields".into(),
        ));
    }
    let before_cursor = std::str::from_utf8(before)
        .map_err(|_| DirgoError::User("suggestion buffer is not valid UTF-8".into()))?;
    let after_cursor = std::str::from_utf8(after)
        .map_err(|_| DirgoError::User("suggestion buffer is not valid UTF-8".into()))?;
    Ok((before_cursor.into(), after_cursor.into()))
}

fn shell_suggestion_request(
    config: &Config,
    shell: crate::shell::Shell,
    cwd: &Path,
    before_cursor: String,
    after_cursor: String,
) -> crate::suggestions::SuggestionRequest {
    crate::suggestions::SuggestionRequest {
        protocol_version: crate::suggestions::PROTOCOL_VERSION,
        request_id: 0,
        shell: shell.suggestion_kind(),
        cwd: cwd.to_path_buf(),
        before_cursor,
        after_cursor,
        max_results: config.suggestions.max_results,
        terminal_rows: None,
        terminal_columns: None,
        presentation: crate::suggestions::SuggestionPresentation::Explicit,
    }
}

#[derive(Debug, Clone, Copy)]
struct CompletionOutputOptions {
    terminal_rows: Option<u16>,
    terminal_columns: Option<u16>,
    format: CompletionOutputFormat,
    page_offset: usize,
    page_size: Option<u16>,
    include_total: bool,
    include_descriptions: bool,
    frame_generation: Option<u64>,
}

fn hidden_suggest_complete(
    paths: &AppPaths,
    config: &Config,
    shell: crate::shell::Shell,
    cwd: &Path,
    options: CompletionOutputOptions,
) -> Result<i32> {
    if !config.suggestions.enabled {
        return Ok(0);
    }
    let (before_cursor, after_cursor) = read_shell_buffer(None)?;
    let context =
        crate::suggestions::CompletionContext::parse(shell.suggestion_kind(), &before_cursor);
    let mut request =
        shell_suggestion_request(config, shell, cwd, before_cursor.clone(), after_cursor);
    request.terminal_rows = options.terminal_rows;
    request.terminal_columns = options.terminal_columns;
    request.presentation = crate::suggestions::SuggestionPresentation::List;
    let engine = build_suggestion_engine(paths, config, needs_executables(&request))?;
    let project = cached_project_commands(paths, &request.cwd);
    let (suggestions, total) = if let Some(page_size) = options.page_size {
        let page = engine.suggest_page_with_project(
            &request,
            project.as_ref(),
            options.page_offset,
            usize::from(page_size),
        );
        (page.suggestions, page.total)
    } else {
        let suggestions = engine.suggest_with_project(&request, project.as_ref());
        let total = suggestions.len();
        (suggestions, total)
    };
    let unchanged_prefix = &before_cursor[..context.replacement_start()];
    let mut frame = Vec::new();
    if let Some(generation) = options.frame_generation {
        write!(frame, "{generation}\0")
            .map_err(|error| DirgoError::io("completion frame", error))?;
    }
    if options.include_total {
        write!(frame, "{total}\0").map_err(|error| DirgoError::io("completion frame", error))?;
    }
    let ascii_descriptions =
        options.include_descriptions && env::var_os("DGO_NO_UNICODE").is_some();
    for suggestion in suggestions {
        let Some(token) = suggestion.edit.replacement.strip_prefix(unchanged_prefix) else {
            continue;
        };
        let label = suggestion_source_label(suggestion.source);
        let display = if options.include_descriptions {
            prepare_preview_display(
                &suggestion.display,
                label,
                options.terminal_columns,
                ascii_descriptions,
            )
        } else {
            Cow::Borrowed(suggestion.display.as_str())
        };
        let description = if options.include_descriptions {
            suggestion.description.as_deref().map(|description| {
                prepare_preview_description(
                    description,
                    options.terminal_columns,
                    ascii_descriptions,
                )
            })
        } else {
            None
        };
        match options.format {
            CompletionOutputFormat::Nul => frame
                .write_all(token.as_bytes())
                .and_then(|_| frame.write_all(&[0]))
                .and_then(|_| frame.write_all(display.as_bytes()))
                .and_then(|_| frame.write_all(&[0]))
                .and_then(|_| frame.write_all(label.as_bytes()))
                .and_then(|_| frame.write_all(&[0]))
                .and_then(|_| {
                    if options.include_descriptions {
                        frame.write_all(description.as_deref().unwrap_or("").as_bytes())
                    } else {
                        Ok(())
                    }
                })
                .and_then(|_| {
                    if options.include_descriptions {
                        frame.write_all(&[0])
                    } else {
                        Ok(())
                    }
                })
                .and_then(|_| frame.write_all(suggestion.edit.replacement.as_bytes()))
                .and_then(|_| frame.write_all(&[0]))
                .map_err(|error| DirgoError::io("completion frame", error))?,
            CompletionOutputFormat::Lines => {
                writeln!(frame, "{token}\t{label}  {}", suggestion.display)
                    .map_err(|error| DirgoError::io("completion frame", error))?
            }
        }
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    output
        .write_all(&frame)
        .map_err(|error| DirgoError::io("stdout", error))?;
    output
        .flush()
        .map_err(|error| DirgoError::io("stdout", error))?;
    Ok(0)
}

fn prepare_preview_description<'a>(
    description: &'a str,
    terminal_columns: Option<u16>,
    ascii: bool,
) -> Cow<'a, str> {
    let Some(columns) = terminal_columns.map(usize::from) else {
        return Cow::Borrowed(description);
    };
    let ellipsis = if ascii { "..." } else { "…" };
    if columns < 92 {
        let width = columns.saturating_sub(12).max(8);
        return truncate_to_cell_width(description, width, ellipsis);
    }

    let list_width = if columns >= 112 { 44 } else { 38 };
    let width = columns.saturating_sub(list_width + 11).max(8);
    if UnicodeWidthStr::width(description) <= width {
        return Cow::Borrowed(description);
    }

    let mut lines = Vec::new();
    let mut line = String::new();
    for word in description.split_whitespace() {
        let word = truncate_to_cell_width(word, width, ellipsis);
        let joined_width = if line.is_empty() {
            UnicodeWidthStr::width(word.as_ref())
        } else {
            UnicodeWidthStr::width(line.as_str()) + 1 + UnicodeWidthStr::width(word.as_ref())
        };
        if !line.is_empty() && joined_width > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    Cow::Owned(lines.join("\n"))
}

fn prepare_preview_display<'a>(
    display: &'a str,
    label: &str,
    terminal_columns: Option<u16>,
    ascii: bool,
) -> Cow<'a, str> {
    let Some(columns) = terminal_columns.map(usize::from) else {
        return Cow::Borrowed(display);
    };
    let ellipsis = if ascii { "..." } else { "…" };
    let wide = columns >= 92;
    let available = if wide {
        let list_width: usize = if columns >= 112 { 44 } else { 38 };
        list_width
            .saturating_sub(preview_kind(label).len() + 7)
            .max(8)
    } else {
        columns.saturating_sub(22).max(8)
    };
    let truncated = truncate_to_cell_width(display, available, ellipsis);
    if !wide {
        return truncated;
    }
    let padding = available.saturating_sub(UnicodeWidthStr::width(truncated.as_ref()));
    if padding == 0 {
        truncated
    } else {
        Cow::Owned(format!("{}{:<padding$}", truncated, ""))
    }
}

fn preview_kind(label: &str) -> &'static str {
    match label {
        "DIR" => "directory",
        "NAV" => "recent",
        "HIST" => "history",
        "PATH" => "executable",
        "CMD" => "command",
        "SUB" => "subcommand",
        "OPT" => "option",
        "BLT" => "builtin",
        "ALS" => "alias",
        "FILE" => "file",
        _ => "",
    }
}

fn truncate_to_cell_width<'a>(value: &'a str, width: usize, ellipsis: &str) -> Cow<'a, str> {
    if UnicodeWidthStr::width(value) <= width {
        return Cow::Borrowed(value);
    }
    let target = width.saturating_sub(UnicodeWidthStr::width(ellipsis));
    let mut end = 0;
    for (index, character) in value.char_indices() {
        let candidate_end = index + character.len_utf8();
        if UnicodeWidthStr::width(&value[..candidate_end]) > target {
            break;
        }
        end = candidate_end;
    }
    Cow::Owned(format!("{}{}", &value[..end], ellipsis))
}

fn suggestion_source_label(source: crate::suggestions::SuggestionSource) -> &'static str {
    use crate::suggestions::SuggestionSource;
    match source {
        SuggestionSource::Directory => "DIR",
        SuggestionSource::NavigationHistory => "NAV",
        SuggestionSource::CommandHistory => "HIST",
        SuggestionSource::Executable => "PATH",
        SuggestionSource::Command => "CMD",
        SuggestionSource::Subcommand => "SUB",
        SuggestionSource::Option => "OPT",
        SuggestionSource::Builtin => "BLT",
        SuggestionSource::Alias => "ALS",
        SuggestionSource::Filesystem => "FILE",
        SuggestionSource::ProjectCommand => "PROJ",
        SuggestionSource::Workflow => "NEXT",
    }
}

fn needs_executables(request: &crate::suggestions::SuggestionRequest) -> bool {
    !request
        .before_cursor
        .trim_start()
        .contains(char::is_whitespace)
}

fn build_suggestion_engine(
    paths: &AppPaths,
    config: &Config,
    include_executables: bool,
) -> Result<SuggestionEngine> {
    Ok(SuggestionEngine::new(load_suggestion_data(
        paths,
        config,
        include_executables,
    )?))
}

fn build_indexed_suggestion_engine(paths: &AppPaths, config: &Config) -> Result<SuggestionEngine> {
    Ok(SuggestionEngine::new_indexed(load_suggestion_data(
        paths, config, true,
    )?))
}

fn load_suggestion_data(
    paths: &AppPaths,
    config: &Config,
    include_executables: bool,
) -> Result<SuggestionData> {
    let records = if paths.index_file.exists() {
        open_index_with_recovery(paths, config)?.records()?
    } else {
        Vec::new()
    };
    let (bookmarks, navigation_history) = if paths.state_file.exists() {
        match read_suggestion_context(&paths.state_file) {
            Ok(context) => context,
            Err(DirgoError::Database(redb::DatabaseError::DatabaseAlreadyOpen)) => {
                Default::default()
            }
            Err(error) => return Err(error),
        }
    } else {
        Default::default()
    };
    let command_history = read_command_history(
        &paths.suggestions_state_file,
        unix_now(),
        &config.suggestions,
    )
    .unwrap_or_default();
    let workflow = load_workflow_suggestion_snapshot(paths, config);
    let mut catalog = if include_executables {
        CommandCatalog::discover(env::var_os("PATH").as_deref())
    } else {
        CommandCatalog::default()
    };
    if let Some(config_dir) = paths.config_file.parent() {
        catalog = catalog.with_user_specs(&config_dir.join("completions"));
    }
    Ok(SuggestionData {
        records,
        bookmarks,
        navigation_history,
        ranking: config.ranking.clone(),
        catalog,
        command_history,
        workflow,
    })
}

fn load_workflow_suggestion_snapshot(
    paths: &AppPaths,
    config: &Config,
) -> Option<crate::suggestions::WorkflowSnapshot> {
    if !config.suggestions.workflow_suggestions
        || !config.suggestions.command_history
        || !paths.suggestions_state_file.exists()
    {
        return None;
    }
    (|| {
        let workflow = crate::workflows::read_workflow_snapshot(&paths.suggestions_state_file)?;
        let history = read_history_snapshot(&paths.suggestions_state_file)?;
        let session = env::var("DGO_SESSION_ID").ok();
        Ok::<_, DirgoError>(session.map(|session| {
            crate::suggestions::WorkflowSnapshot::new(
                workflow.transitions,
                workflow.saved,
                history.events,
                &session,
            )
        }))
    })()
    .unwrap_or(None)
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
                terminal::safe_path(&backup)
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
                terminal::safe_path(&backup)
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
                    terminal::safe_path(&bookmark.path)
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
            println!("Saved @{name} → {}", terminal::safe_path(cwd));
        }
        return Ok(0);
    }
    if query.is_empty() && action != Action::Go {
        return complete_action(paths, config, cwd, cwd.to_path_buf(), action, shell_mode);
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
        if let Some(ignored) = crate::roots::ignored_query_segment(&response.query, &config.ignore)
        {
            eprintln!(
                "No indexed directory matches {:?}.\n\n\"{}\" is excluded from the default index.\nUse an exact path, or add only the directory you need:\n\n  dgo roots add <PATH>",
                response.query,
                terminal::safe_text(ignored)
            );
        } else {
            eprintln!(
                "No directories match {:?}.\nTry a shorter query or run `dgo refresh`.",
                response.query
            );
        }
        return Ok(EXIT_NO_MATCH);
    }
    let selection = match choose_candidate(
        &response.candidates,
        picker,
        &response.query,
        origin,
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
            print_output_path(&path);
            Ok(0)
        }
        Action::Print => {
            print_output_path(&path);
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
            terminal::safe_path(path)
        )))
    }
}

fn choose_candidate(
    initial_candidates: &[Candidate],
    picker: PickerCandidates,
    query: &str,
    origin: &Path,
    default_action: Action,
    config: &Config,
) -> Result<crate::tui::PickOutcome> {
    if crate::tui::is_supported()
        && let PickerCandidates::Records(records) = picker
    {
        return crate::tui::pick_stream(
            records.into_stream(),
            query,
            origin,
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
            eprintln!("{}", terminal::safe_path(&candidate.path));
        }
        eprintln!("Dirgo needs an interactive terminal to choose an ambiguous result.");
        return Ok(crate::tui::PickOutcome::Cancelled);
    }
    if crate::tui::is_supported() {
        return crate::tui::pick(
            picker_candidates,
            query,
            origin,
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
            terminal::safe_text(&candidate.basename),
            terminal::safe_text(&candidate.display_path)
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
        preview: config.ui.preview,
        height_percent: config.ui.height_percent,
    }
}

fn record_navigation(paths: &AppPaths, origin: &Path, destination: &Path) -> Result<()> {
    paths.ensure_dirs()?;
    let session = env::var("DGO_SESSION_ID").ok();
    StateStore::open(&paths.state_file)?.record_navigation(origin, destination, session.as_deref())
}

fn shell_resolve(
    paths: &AppPaths,
    config: &Config,
    args: ResolveArgs,
    action: Action,
) -> Result<i32> {
    match args.query.as_slice() {
        [command] if command == "root" => {
            let Some((path, _)) = index::find_project_root(&args.cwd) else {
                return Err(DirgoError::NoMatch("project root".into()));
            };
            record_navigation(paths, &args.cwd, &path)?;
            print_output_path(&path);
            Ok(0)
        }
        [command] if command == "back" => navigation_command(paths, Direction::Back),
        [command] if command == "forward" => navigation_command(paths, Direction::Forward),
        [command, rest @ ..] if command == "repo" => {
            repository_command(paths, config, &args.cwd, rest.to_vec(), true, action)
        }
        [command, rest @ ..] if command == "recent" => {
            recent_command(paths, config, &args.cwd, rest.to_vec(), true, action)
        }
        _ => default_query(paths, config, &args.cwd, args.query, true, action),
    }
}

fn print_project_root(cwd: PathBuf) -> Result<i32> {
    let Some((path, _)) = index::find_project_root(&cwd) else {
        return Err(DirgoError::NoMatch("project root".into()));
    };
    print_output_path(&path);
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
    print_output_path(&path);
    Ok(0)
}

fn bookmark_command(paths: &AppPaths, command: BookmarkCommand) -> Result<i32> {
    paths.ensure_dirs()?;
    let state = StateStore::open(&paths.state_file)?;
    match command {
        BookmarkCommand::Add { name, path } => {
            let path = path.unwrap_or(current_dir()?);
            let bookmark = state.add_bookmark(&name, &path)?;
            println!("Saved @{name} → {}", terminal::safe_path(&bookmark.path));
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
            println!(
                "@{:<20} {}",
                bookmark.name,
                terminal::safe_path(&bookmark.path)
            );
        }
    }
    Ok(0)
}

fn config_command(paths: &AppPaths, config: &Config, command: ConfigCommand) -> Result<i32> {
    match command {
        ConfigCommand::Path => print_output_path(&paths.config_file),
        ConfigCommand::Show => print!(
            "{}",
            toml::to_string_pretty(config)
                .map_err(|error| DirgoError::Config(error.to_string()))?
        ),
    }
    Ok(0)
}

fn print_support() {
    println!(
        "Dirgo support\n\nRun `dgo doctor`, remove personal paths from its output, then use the issue forms at:\nhttps://github.com/RudySource/Dirgo/issues\n\nFor vulnerabilities, follow SECURITY.md and do not open a public issue."
    );
}

fn print_output_path(path: &Path) {
    if io::stdout().is_terminal() {
        println!("{}", terminal::safe_path(path));
    } else {
        println!("{}", path.display());
    }
}

fn doctor(paths: &AppPaths, config: Result<Config>) -> Result<i32> {
    println!("Dirgo Doctor\n");
    println!("✓ version        {}", env!("CARGO_PKG_VERSION"));
    let (config, config_valid) = match config {
        Ok(config) => {
            println!("✓ config         valid (schema {})", config.schema_version);
            (config, true)
        }
        Err(error) => {
            println!(
                "! config         invalid at {}: {}",
                terminal::safe_path(&paths.config_file),
                terminal::safe_text(&error.to_string())
            );
            println!("  Repair or move that file, then rerun `dgo doctor`.");
            (Config::default(), false)
        }
    };
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
            "not detected; run `dgo setup`"
        }
    );
    paths.ensure_dirs()?;
    println!(
        "✓ storage        cache={} state={}",
        terminal::safe_path(&paths.cache_dir),
        terminal::safe_path(&paths.state_dir)
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
            Err(error) => println!(
                "! index          unhealthy ({}); run `dgo refresh`",
                terminal::safe_text(&error.to_string())
            ),
        }
    } else {
        println!("! index          missing; it will be built on first search");
    }
    let state = StateStore::open(&paths.state_file)?;
    println!(
        "✓ state          healthy ({} bookmarks)",
        state.bookmarks()?.len()
    );
    let root_statuses = crate::roots::statuses(&config.roots);
    let accessible_roots = root_statuses.iter().filter(|root| root.accessible).count();
    let focused_roots = root_statuses.iter().filter(|root| root.focused).count();
    let root_marker = if accessible_roots == root_statuses.len() {
        "✓"
    } else {
        "!"
    };
    println!(
        "{root_marker} roots          {} configured; {accessible_roots} accessible; {focused_roots} focused",
        root_statuses.len()
    );
    if accessible_roots != root_statuses.len() {
        println!("  Run `dgo roots list` for exact repair guidance.");
    }
    let update_view = crate::update::local_view(paths);
    let (update_marker, update_detail) = if matches!(
        update_view.refresh,
        crate::update::RefreshDisposition::Disabled
    ) {
        (
            "✓",
            "checks disabled; enable with `dgo update-notifications on`".into(),
        )
    } else {
        match update_view.relation {
            crate::update::VersionRelation::UpdateAvailable { latest } => (
                "!",
                format!(
                    "{latest} available ({}); run `dgo --update`",
                    if update_view.freshness == crate::update::CacheFreshness::Fresh {
                        "confirmed"
                    } else {
                        "cached"
                    }
                ),
            ),
            crate::update::VersionRelation::Current { latest }
                if update_view.freshness == crate::update::CacheFreshness::Fresh =>
            {
                ("✓", format!("current ({latest} confirmed)"))
            }
            crate::update::VersionRelation::Current { latest } => (
                "!",
                format!("cached stable {latest}; the next normal command will refresh it"),
            ),
            crate::update::VersionRelation::AheadOfLatest { latest } => {
                ("✓", format!("ahead of latest known stable {latest}"))
            }
            crate::update::VersionRelation::Unknown => (
                "!",
                "unknown; the next normal command will refresh it".into(),
            ),
        }
    };
    println!("{update_marker} update         {update_detail}");
    if paths.suggestions_state_file.exists() {
        match (
            read_history_snapshot(&paths.suggestions_state_file),
            crate::workflows::read_workflow_snapshot(&paths.suggestions_state_file),
        ) {
            (Ok(history), Ok(workflows)) => println!(
                "✓ learning       history={} events/{} aggregates ({}); workflows={} learned/{} saved ({}, schema v{}, rebuilt {})",
                history.events.len(),
                history.aggregates.len(),
                enabled_label(config.suggestions.command_history),
                workflows.transitions.len(),
                workflows.saved.len(),
                enabled_label(config.suggestions.workflow_suggestions),
                workflows.status.schema_version,
                workflows.status.last_rebuild,
            ),
            (Err(error), _) | (_, Err(error)) => {
                println!(
                    "! learning       unhealthy ({}); preserve suggestions.redb, then run `dgo suggestions doctor`",
                    terminal::safe_text(&error.to_string())
                );
            }
        }
    } else {
        println!(
            "✓ learning       history={} workflows={}; storage not created",
            enabled_label(config.suggestions.command_history),
            enabled_label(config.suggestions.workflow_suggestions),
        );
    }
    println!("✓ palette        files/tasks/git/compose/places; bounded on open");
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
            terminal::safe_path(&path),
            bytes as f64 / 1_048_576.0
        );
    } else {
        println!("✓ shell startup  no oversized startup file detected");
    }
    println!("✓ platform       {} {}", env::consts::OS, env::consts::ARCH);
    println!("\nDoctor completed. Lines marked ! need attention but do not block all commands.");
    Ok(if config_valid { 0 } else { 1 })
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

fn stats(paths: &AppPaths, config: &Config) -> Result<i32> {
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
        .map(|history| terminal::safe_path(&history.path))
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
            .unwrap_or(0)
        + std::fs::metadata(&paths.suggestions_state_file)
            .map(|meta| meta.len())
            .unwrap_or(0);
    let (history_events, history_aggregates, learned_transitions, saved_workflows) =
        if paths.suggestions_state_file.exists() {
            let history = read_history_snapshot(&paths.suggestions_state_file)?;
            let workflows =
                crate::workflows::read_workflow_snapshot(&paths.suggestions_state_file)?;
            (
                history.events.len(),
                history.aggregates.len(),
                workflows.transitions.len(),
                workflows.saved.len(),
            )
        } else {
            (0, 0, 0, 0)
        };
    let root_statuses = crate::roots::statuses(&config.roots);
    let accessible_roots = root_statuses.iter().filter(|root| root.accessible).count();
    let focused_roots = root_statuses.iter().filter(|root| root.focused).count();
    println!(
        "Dirgo stats\n\nIndexed directories   {directories}\nProjects              {projects}\nSearch roots          {}\nAccessible roots      {accessible_roots}\nFocused roots         {focused_roots}\nBookmarks             {}\nDirgo navigations     {jumps}\nHistory events        {history_events}\nHistory aggregates    {history_aggregates}\nLearned transitions   {learned_transitions}\nSaved workflows       {saved_workflows}\nMost visited          {most}\nIndex age             {age}\nDatabase size         {:.1} MB",
        root_statuses.len(),
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

#[cfg(test)]
mod preview_description_tests {
    use super::{prepare_preview_description, prepare_preview_display};
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn wide_preview_wraps_by_terminal_cells() {
        let prepared = prepare_preview_description(
            "部署生产环境 并验证所有服务 不会超出终端边界",
            Some(92),
            false,
        );

        assert!(prepared.contains('\n'));
        assert!(
            prepared
                .lines()
                .all(|line| UnicodeWidthStr::width(line) <= 43)
        );
    }

    #[test]
    fn narrow_ascii_preview_uses_cell_safe_ascii_ellipsis() {
        let description = "界".repeat(50);
        let prepared = prepare_preview_description(&description, Some(82), true);

        assert!(prepared.ends_with("..."));
        assert!(UnicodeWidthStr::width(prepared.as_ref()) <= 70);
    }

    #[test]
    fn wide_preview_pads_unicode_command_names_by_terminal_cells() {
        let prepared = prepare_preview_display("部署服务部署服务", "SUB", Some(92), false);

        assert_eq!(UnicodeWidthStr::width(prepared.as_ref()), 21);
        assert!(prepared.ends_with(' '));
    }
}
