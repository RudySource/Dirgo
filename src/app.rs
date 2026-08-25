use std::{
    collections::HashMap,
    env,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use clap::Parser;

use crate::{
    DirgoError, Result,
    actions::Action,
    cli::{
        BookmarkCommand, Cli, Command, ConfigCommand, ImportSource, QueryArgs, ResolveArgs,
        SuggestionsCommand, SuggestionsHistoryCommand, UpdateNotificationMode,
    },
    config::Config,
    history_import,
    index::{self, IndexStore},
    model::{Candidate, PathHistory, QueryResponse, unix_now},
    paths::{self, AppPaths},
    search::{self, SearchContext},
    shell,
    state::{StateStore, read_suggestion_context},
    suggestions::{
        CommandCatalog, CommandHistoryStore, MAX_REQUEST_BYTES, SuggestionData, SuggestionEngine,
        SuggestionResponse, decode_request_line, encode_response_line, pick_suggestion,
        read_bounded_frame, read_command_history, write_suggestions_config,
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
    if matches!(cli.command, Some(Command::SuggestHistoryEnabled)) {
        return Ok(
            if config.suggestions.enabled && config.suggestions.command_history {
                0
            } else {
                1
            },
        );
    }
    if let Some(Command::SuggestShell { shell, cwd }) = &cli.command {
        return hidden_suggest_shell(&paths, &config, *shell, cwd);
    }
    if let Some(Command::SuggestComplete {
        shell,
        cwd,
        terminal_rows,
        terminal_columns,
    }) = &cli.command
    {
        return hidden_suggest_complete(
            &paths,
            &config,
            *shell,
            cwd,
            *terminal_rows,
            *terminal_columns,
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
        Some(Command::Support) => unreachable!("handled before storage access"),
        Some(
            Command::Suggestions { .. }
            | Command::Suggest
            | Command::SuggestWorker { .. }
            | Command::SuggestRecord
            | Command::SuggestEnabled
            | Command::SuggestLiveEnabled
            | Command::SuggestHistoryEnabled
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
                "Dirgo Suggestions Doctor\n\nconfiguration  valid\nsuggestions    {}\ncommand history {}\nprotocol       v{}\nstate          {}",
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
            SuggestionsHistoryCommand::Clear => {
                if !paths.suggestions_state_file.exists() {
                    println!("Command history is already empty.");
                } else {
                    let removed =
                        CommandHistoryStore::open(&paths.suggestions_state_file)?.clear()?;
                    println!("Removed {removed} command-history entries.");
                }
            }
        },
    }
    Ok(0)
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
        build_suggestion_engine(paths, config, needs_executables(&request))?.suggest(&request)
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
                    SuggestionResponse::success(
                        request.request_id,
                        if active_config.suggestions.enabled {
                            engine.suggest(&request)
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
    while matches!(input.last(), Some(b'\n' | b'\r')) {
        input.pop();
    }
    let command = std::str::from_utf8(&input)
        .map_err(|_| DirgoError::User("command history entry is not valid UTF-8".into()))?;
    CommandHistoryStore::open(&paths.suggestions_state_file)?.record(
        command,
        unix_now(),
        &config.suggestions,
    )?;
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
    if let Some(suggestion) = build_suggestion_engine(paths, config, needs_executables(&request))?
        .suggest(&request)
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
    let suggestions =
        build_suggestion_engine(paths, config, needs_executables(&request))?.suggest(&request);
    let replacement = pick_suggestion(
        &suggestions,
        crate::suggestions::PickerOptions {
            color: config.ui.accent != "none" && env::var_os("NO_COLOR").is_none(),
            unicode: config.ui.icons != "never",
        },
    )?
    .unwrap_or_default();
    write_private_picker_result(output_path, replacement.as_bytes())?;
    Ok(0)
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

fn hidden_suggest_complete(
    paths: &AppPaths,
    config: &Config,
    shell: crate::shell::Shell,
    cwd: &Path,
    terminal_rows: Option<u16>,
    terminal_columns: Option<u16>,
) -> Result<i32> {
    if !config.suggestions.enabled {
        return Ok(0);
    }
    let (before_cursor, after_cursor) = read_shell_buffer(None)?;
    let context =
        crate::suggestions::CompletionContext::parse(shell.suggestion_kind(), &before_cursor);
    let mut request =
        shell_suggestion_request(config, shell, cwd, before_cursor.clone(), after_cursor);
    request.terminal_rows = terminal_rows;
    request.terminal_columns = terminal_columns;
    request.presentation = crate::suggestions::SuggestionPresentation::List;
    let suggestions =
        build_suggestion_engine(paths, config, needs_executables(&request))?.suggest(&request);
    let unchanged_prefix = &before_cursor[..context.replacement_start()];
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for suggestion in suggestions {
        let Some(token) = suggestion.edit.replacement.strip_prefix(unchanged_prefix) else {
            continue;
        };
        let label = suggestion_source_label(suggestion.source);
        output
            .write_all(token.as_bytes())
            .and_then(|_| output.write_all(&[0]))
            .and_then(|_| output.write_all(suggestion.display.as_bytes()))
            .and_then(|_| output.write_all(&[0]))
            .and_then(|_| output.write_all(label.as_bytes()))
            .and_then(|_| output.write_all(&[0]))
            .and_then(|_| output.write_all(suggestion.edit.replacement.as_bytes()))
            .and_then(|_| output.write_all(&[0]))
            .map_err(|error| DirgoError::io("stdout", error))?;
    }
    output
        .flush()
        .map_err(|error| DirgoError::io("stdout", error))?;
    Ok(0)
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
    )?;
    Ok(SuggestionData {
        records,
        bookmarks,
        navigation_history,
        ranking: config.ranking.clone(),
        catalog: if include_executables {
            CommandCatalog::discover(env::var_os("PATH").as_deref())
        } else {
            CommandCatalog::default()
        },
        command_history,
    })
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
            eprintln!("{}", terminal::safe_path(&candidate.path));
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
