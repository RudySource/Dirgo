use std::{
    env, fs, io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use nucleo::{
    Config as NucleoConfig, Nucleo, Utf32String,
    pattern::{CaseMatching, Normalization},
};
use ratatui::{
    Frame, Terminal, TerminalOptions, Viewport,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph, Wrap},
};

use crate::{
    DirgoError, Result,
    actions::{Action, Availability},
    model::Candidate,
};

const RESULT_CACHE_LIMIT: u32 = 2_000;
const PREVIEW_DEBOUNCE: Duration = Duration::from_millis(90);
const PREVIEW_ENTRY_LIMIT: usize = 20;

pub fn is_supported() -> bool {
    use std::io::IsTerminal;

    io::stdin().is_terminal()
        && io::stderr().is_terminal()
        && env::var("TERM").map_or(true, |term| term != "dumb")
}

pub struct Selection {
    pub path: PathBuf,
    pub action: Action,
}

pub fn pick(
    candidates: Vec<Candidate>,
    query: &str,
    default_action: Action,
    actions: Availability,
) -> Result<Option<Selection>> {
    if candidates.is_empty() {
        return Ok(None);
    }
    let (_, terminal_rows) =
        crossterm::terminal::size().map_err(|error| DirgoError::io("terminal", error))?;
    let height = ((u32::from(terminal_rows) * 70) / 100) as u16;
    let height = height.clamp(7, 24).min(terminal_rows.max(1));

    enable_raw_mode().map_err(|error| DirgoError::io("terminal", error))?;
    let mut terminal_mode = TerminalModeGuard {
        alternate_screen: false,
    };
    let force_fullscreen = env::var_os("DGO_TUI_FULLSCREEN").is_some();
    let mut terminal = if force_fullscreen {
        fullscreen_terminal(&mut terminal_mode)?
    } else {
        match Terminal::with_options(
            CrosstermBackend::new(io::stderr()),
            TerminalOptions {
                viewport: Viewport::Inline(height),
            },
        ) {
            Ok(terminal) => terminal,
            Err(inline_error) => {
                tracing::debug!(%inline_error, "inline viewport unavailable; using fullscreen picker");
                fullscreen_terminal(&mut terminal_mode)?
            }
        }
    };
    terminal
        .hide_cursor()
        .map_err(|error| DirgoError::io("terminal", error))?;

    let mut matcher = LiveMatcher::new(candidates, query);
    let preview_loader = PreviewLoader::new();
    let mut state = PickerState::new(query);
    let mut visible = Vec::new();
    let selection = loop {
        let status = matcher.tick(6);
        if status.changed || visible.is_empty() {
            visible = matcher.cached_results();
            state.update_match_status(matcher.matched_count(), matcher.is_running());
            state.clamp_selection(visible.len());
        } else {
            state.matching = matcher.is_running();
        }
        while let Some(preview) = preview_loader.try_recv() {
            state.accept_preview(preview, &visible);
        }
        state.request_preview(&visible, &preview_loader);

        terminal
            .draw(|frame| render(frame, &visible, &mut state, actions))
            .map_err(|error| DirgoError::io("terminal", error))?;

        if !event::poll(Duration::from_millis(16))
            .map_err(|error| DirgoError::io("terminal", error))?
        {
            continue;
        }
        let Event::Key(key) = event::read().map_err(|error| DirgoError::io("terminal", error))?
        else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match handle_key(key, &mut state, visible.len(), actions) {
            PickerAction::Continue => {}
            PickerAction::QueryChanged => {
                matcher.set_query(&state.query);
                visible.clear();
                state.update_match_status(0, true);
            }
            PickerAction::Select(action) => {
                break state
                    .selected()
                    .and_then(|index| visible.get(index))
                    .map(|candidate| Selection {
                        path: candidate.path.clone(),
                        action: if action == Action::Go {
                            default_action
                        } else {
                            action
                        },
                    });
            }
            PickerAction::Cancel => break None,
        }
    };
    terminal
        .clear()
        .map_err(|error| DirgoError::io("terminal", error))?;
    terminal
        .show_cursor()
        .map_err(|error| DirgoError::io("terminal", error))?;
    Ok(selection)
}

struct LiveMatcher {
    nucleo: Nucleo<Candidate>,
    query: String,
    path_mode: bool,
    total_items: usize,
    cancel_injection: Arc<AtomicBool>,
    injection_thread: Option<JoinHandle<()>>,
    running: bool,
}

impl LiveMatcher {
    fn new(candidates: Vec<Candidate>, query: &str) -> Self {
        let total_items = candidates.len();
        let nucleo = Nucleo::new(NucleoConfig::DEFAULT, Arc::new(|| {}), None, 2);
        let injector = nucleo.injector();
        let cancel_injection = Arc::new(AtomicBool::new(false));
        let thread_cancel = Arc::clone(&cancel_injection);
        let injection_thread = thread::spawn(move || {
            for candidate in candidates {
                if thread_cancel.load(Ordering::Relaxed) {
                    break;
                }
                injector.push(candidate, |candidate, columns| {
                    columns[0] = Utf32String::from(candidate.basename.as_str());
                    columns[1] = Utf32String::from(fuzzy_path_text(&candidate.display_path));
                });
            }
        });
        let mut matcher = Self {
            nucleo,
            query: String::new(),
            path_mode: false,
            total_items,
            cancel_injection,
            injection_thread: Some(injection_thread),
            running: true,
        };
        matcher.set_query(query);
        matcher
    }

    fn set_query(&mut self, query: &str) {
        let path_mode = is_path_query(query);
        let append = path_mode == self.path_mode && query.starts_with(&self.query);
        let path_query = path_mode.then(|| fuzzy_path_text(query));
        self.nucleo.pattern.reparse(
            0,
            if path_mode { "" } else { query },
            CaseMatching::Smart,
            Normalization::Smart,
            append,
        );
        self.nucleo.pattern.reparse(
            1,
            path_query.as_deref().unwrap_or(""),
            CaseMatching::Smart,
            Normalization::Smart,
            append,
        );
        self.query.clear();
        self.query.push_str(query);
        self.path_mode = path_mode;
        self.running = true;
    }

    fn tick(&mut self, timeout_ms: u64) -> nucleo::Status {
        let status = self.nucleo.tick(timeout_ms);
        self.running = status.running
            || self.nucleo.active_injectors() > 0
            || self.nucleo.snapshot().item_count() < self.total_items as u32;
        status
    }

    fn cached_results(&self) -> Vec<Candidate> {
        let snapshot = self.nucleo.snapshot();
        let end = snapshot.matched_item_count().min(RESULT_CACHE_LIMIT);
        snapshot
            .matched_items(0..end)
            .map(|item| item.data.clone())
            .collect()
    }

    fn matched_count(&self) -> usize {
        self.nucleo.snapshot().matched_item_count() as usize
    }

    fn is_running(&self) -> bool {
        self.running
    }
}

impl Drop for LiveMatcher {
    fn drop(&mut self) {
        self.cancel_injection.store(true, Ordering::Relaxed);
        if let Some(thread) = self.injection_thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug)]
struct DirectoryPreview {
    path: PathBuf,
    project_type: Option<&'static str>,
    entries: Vec<String>,
    error: Option<String>,
}

struct PreviewLoader {
    requests: Sender<PathBuf>,
    results: Receiver<DirectoryPreview>,
}

impl PreviewLoader {
    fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<PathBuf>();
        let (result_tx, result_rx) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(mut path) = request_rx.recv() {
                for newer in request_rx.try_iter() {
                    path = newer;
                }
                if result_tx.send(load_directory_preview(path)).is_err() {
                    break;
                }
            }
        });
        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    fn request(&self, path: PathBuf) {
        let _ = self.requests.send(path);
    }

    fn try_recv(&self) -> Option<DirectoryPreview> {
        self.results.try_recv().ok()
    }
}

fn load_directory_preview(path: PathBuf) -> DirectoryPreview {
    let mut preview = DirectoryPreview {
        path: path.clone(),
        project_type: None,
        entries: Vec::new(),
        error: None,
    };
    let read_dir = match fs::read_dir(&path) {
        Ok(read_dir) => read_dir,
        Err(error) => {
            preview.error = Some(error.to_string());
            return preview;
        }
    };
    let mut entries = Vec::new();
    let mut marker_names = Vec::new();
    for entry in read_dir {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        marker_names.push(name.clone());
        let suffix = entry
            .file_type()
            .ok()
            .filter(|kind| kind.is_dir())
            .map_or("", |_| "/");
        entries.push(format!("{name}{suffix}"));
    }
    entries.sort_unstable_by_key(|entry| entry.to_lowercase());
    entries.truncate(PREVIEW_ENTRY_LIMIT);
    preview.project_type = detect_project_type(&marker_names);
    preview.entries = entries;
    preview
}

fn detect_project_type(entries: &[String]) -> Option<&'static str> {
    [
        ("Cargo.toml", "Rust project"),
        ("go.mod", "Go project"),
        ("package.json", "Node.js project"),
        ("pyproject.toml", "Python project"),
        (".git", "Git repository"),
    ]
    .into_iter()
    .find_map(|(marker, kind)| entries.iter().any(|entry| entry == marker).then_some(kind))
}

fn is_path_query(query: &str) -> bool {
    query
        .chars()
        .any(|character| character.is_whitespace() || matches!(character, '/' | '\\'))
}

fn fuzzy_path_text(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if matches!(character, '/' | '\\') {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn fullscreen_terminal(
    terminal_mode: &mut TerminalModeGuard,
) -> Result<Terminal<CrosstermBackend<io::Stderr>>> {
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen).map_err(|error| DirgoError::io("terminal", error))?;
    terminal_mode.alternate_screen = true;
    Terminal::new(CrosstermBackend::new(stderr)).map_err(|error| DirgoError::io("terminal", error))
}

struct TerminalModeGuard {
    alternate_screen: bool,
}

impl Drop for TerminalModeGuard {
    fn drop(&mut self) {
        if self.alternate_screen {
            let _ = execute!(io::stderr(), LeaveAlternateScreen);
        }
        let _ = disable_raw_mode();
    }
}

#[derive(Debug)]
struct PickerState {
    query: String,
    cursor: usize,
    list: ListState,
    preview: bool,
    color: bool,
    matching: bool,
    matched_total: usize,
    selection_changed_at: Instant,
    requested_preview: Option<PathBuf>,
    preview_data: Option<DirectoryPreview>,
}

impl PickerState {
    fn new(query: &str) -> Self {
        let mut list = ListState::default();
        list.select(Some(0));
        Self {
            query: query.to_owned(),
            cursor: query.len(),
            list,
            preview: true,
            color: env::var_os("NO_COLOR").is_none(),
            matching: true,
            matched_total: 0,
            selection_changed_at: Instant::now(),
            requested_preview: None,
            preview_data: None,
        }
    }

    fn selected(&self) -> Option<usize> {
        self.list.selected()
    }

    fn update_match_status(&mut self, matched_total: usize, matching: bool) {
        self.matched_total = matched_total;
        self.matching = matching;
    }

    fn clamp_selection(&mut self, length: usize) {
        let selected = self.selected().unwrap_or(0);
        let next = length.checked_sub(1).map(|last| selected.min(last));
        if self.selected() != next {
            self.list.select(next);
            self.selection_changed();
        }
    }

    fn move_by(&mut self, amount: isize, length: usize) {
        if length == 0 {
            self.list.select(None);
            return;
        }
        let current = self.selected().unwrap_or(0) as isize;
        let last = length.saturating_sub(1) as isize;
        let next = (current + amount).clamp(0, last) as usize;
        if self.selected() != Some(next) {
            self.list.select(Some(next));
            self.selection_changed();
        }
    }

    fn reset_selection(&mut self) {
        self.list.select(Some(0));
        self.selection_changed();
    }

    fn selection_changed(&mut self) {
        self.selection_changed_at = Instant::now();
        self.requested_preview = None;
        self.preview_data = None;
    }

    fn insert(&mut self, character: char) {
        self.query.insert(self.cursor, character);
        self.cursor += character.len_utf8();
        self.reset_selection();
    }

    fn backspace(&mut self) -> bool {
        let Some(previous) = previous_boundary(&self.query, self.cursor) else {
            return false;
        };
        self.query.drain(previous..self.cursor);
        self.cursor = previous;
        self.reset_selection();
        true
    }

    fn delete(&mut self) -> bool {
        let Some(next) = next_boundary(&self.query, self.cursor) else {
            return false;
        };
        self.query.drain(self.cursor..next);
        self.reset_selection();
        true
    }

    fn clear_query(&mut self) -> bool {
        if self.query.is_empty() {
            return false;
        }
        self.query.clear();
        self.cursor = 0;
        self.reset_selection();
        true
    }

    fn preview_ready(&self) -> bool {
        self.preview && self.selection_changed_at.elapsed() >= PREVIEW_DEBOUNCE
    }

    fn request_preview(&mut self, candidates: &[Candidate], loader: &PreviewLoader) {
        if !self.preview_ready() {
            return;
        }
        let Some(path) = self
            .selected()
            .and_then(|index| candidates.get(index))
            .map(|candidate| &candidate.path)
        else {
            return;
        };
        if self.requested_preview.as_ref() == Some(path) {
            return;
        }
        self.requested_preview = Some(path.clone());
        loader.request(path.clone());
    }

    fn accept_preview(&mut self, preview: DirectoryPreview, candidates: &[Candidate]) {
        let selected_path = self
            .selected()
            .and_then(|index| candidates.get(index))
            .map(|candidate| &candidate.path);
        if selected_path == Some(&preview.path) {
            self.preview_data = Some(preview);
        }
    }
}

fn previous_boundary(value: &str, index: usize) -> Option<usize> {
    value[..index]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
}

fn next_boundary(value: &str, index: usize) -> Option<usize> {
    value[index..]
        .chars()
        .next()
        .map(|character| index + character.len_utf8())
}

enum PickerAction {
    Continue,
    QueryChanged,
    Select(Action),
    Cancel,
}

fn handle_key(
    key: KeyEvent,
    state: &mut PickerState,
    length: usize,
    actions: Availability,
) -> PickerAction {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => PickerAction::Select(Action::Go),
        (KeyCode::Char('o'), KeyModifiers::CONTROL) if actions.open => {
            PickerAction::Select(Action::Open)
        }
        (KeyCode::Char('y'), KeyModifiers::CONTROL) if actions.copy => {
            PickerAction::Select(Action::Copy)
        }
        (KeyCode::Char('e'), KeyModifiers::CONTROL) if actions.editor => {
            PickerAction::Select(Action::Editor)
        }
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => PickerAction::Cancel,
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
            if state.clear_query() {
                PickerAction::QueryChanged
            } else {
                PickerAction::Continue
            }
        }
        (KeyCode::Backspace, _) => {
            if state.backspace() {
                PickerAction::QueryChanged
            } else {
                PickerAction::Continue
            }
        }
        (KeyCode::Delete, _) => {
            if state.delete() {
                PickerAction::QueryChanged
            } else {
                PickerAction::Continue
            }
        }
        (KeyCode::Left, _) => {
            if let Some(previous) = previous_boundary(&state.query, state.cursor) {
                state.cursor = previous;
            }
            PickerAction::Continue
        }
        (KeyCode::Right, _) => {
            if let Some(next) = next_boundary(&state.query, state.cursor) {
                state.cursor = next;
            }
            PickerAction::Continue
        }
        (KeyCode::Char(character), modifiers)
            if !modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) =>
        {
            state.insert(character);
            PickerAction::QueryChanged
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
            state.move_by(-1, length);
            PickerAction::Continue
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
            state.move_by(1, length);
            PickerAction::Continue
        }
        (KeyCode::Home, _) => {
            let next = (length > 0).then_some(0);
            if state.selected() != next {
                state.list.select(next);
                state.selection_changed();
            }
            PickerAction::Continue
        }
        (KeyCode::End, _) => {
            let next = length.checked_sub(1);
            if state.selected() != next {
                state.list.select(next);
                state.selection_changed();
            }
            PickerAction::Continue
        }
        (KeyCode::PageUp, _) => {
            state.move_by(-8, length);
            PickerAction::Continue
        }
        (KeyCode::PageDown, _) => {
            state.move_by(8, length);
            PickerAction::Continue
        }
        (KeyCode::Tab, _) => {
            state.preview = !state.preview;
            PickerAction::Continue
        }
        _ => PickerAction::Continue,
    }
}

fn render(
    frame: &mut Frame<'_>,
    candidates: &[Candidate],
    state: &mut PickerState,
    actions: Availability,
) {
    let area = frame.area();
    let compact = area.width < 52;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if compact { 2 } else { 3 }),
            Constraint::Min(2),
            Constraint::Length(1),
        ])
        .split(area);

    render_query(frame, rows[0], state, compact);
    if candidates.is_empty() {
        render_empty(frame, rows[1], state);
    } else if area.width >= 88 && state.preview_ready() {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
            .split(rows[1]);
        render_results(frame, columns[0], candidates, state);
        render_preview(frame, columns[1], candidates, state);
    } else {
        render_results(frame, rows[1], candidates, state);
    }
    render_footer(frame, rows[2], compact, state, actions);
}

fn render_query(frame: &mut Frame<'_>, area: Rect, state: &PickerState, compact: bool) {
    let (before, after) = state.query.split_at(state.cursor);
    let mut spans = vec![
        Span::styled("› ", accent(state.color)),
        Span::raw(before),
        Span::styled("▏", accent(state.color)),
    ];
    if after.is_empty() && state.query.is_empty() {
        spans.push(Span::styled("Type to search", muted()));
    } else {
        spans.push(Span::raw(after));
    }
    let block = if compact {
        Block::default().padding(Padding::horizontal(1))
    } else {
        Block::default()
            .title(Span::styled(
                " Dirgo ",
                Style::default().add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::BOTTOM)
            .border_style(muted())
            .padding(Padding::horizontal(1))
    };
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn render_results(
    frame: &mut Frame<'_>,
    area: Rect,
    candidates: &[Candidate],
    state: &PickerState,
) {
    let selected = state.selected().unwrap_or(0);
    let visible_count = usize::from((area.height / 2).max(1));
    let start = selected.saturating_add(1).saturating_sub(visible_count);
    let items = candidates
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_count)
        .map(|(index, candidate)| {
            let marker = if candidate.bookmark.is_some() {
                "* "
            } else if candidate.is_project_root {
                "> "
            } else {
                "  "
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        if index == selected { "│ " } else { "  " },
                        accent(state.color),
                    ),
                    Span::styled(marker, muted()),
                    Span::styled(
                        &candidate.basename,
                        if index == selected {
                            Style::default().add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        },
                    ),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(&candidate.display_path, muted()),
                ]),
            ])
        });
    frame.render_widget(List::new(items), area);
}

fn render_preview(
    frame: &mut Frame<'_>,
    area: Rect,
    candidates: &[Candidate],
    state: &PickerState,
) {
    let Some(candidate) = state.selected().and_then(|index| candidates.get(index)) else {
        return;
    };
    let mut lines = vec![
        Line::from(Span::styled("Destination", muted())),
        Line::from(Span::styled(
            &candidate.basename,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(candidate.path.display().to_string()),
    ];
    if candidate.is_project_root {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Project root",
            accent(state.color),
        )));
    }
    if let Some(bookmark) = &candidate.bookmark {
        lines.push(Line::from(format!("Bookmark  @{bookmark}")));
    }
    match state
        .preview_data
        .as_ref()
        .filter(|preview| preview.path == candidate.path)
    {
        Some(preview) => {
            if let Some(project_type) = preview.project_type {
                lines.push(Line::from(project_type));
            }
            lines.push(Line::from(""));
            if preview.error.is_some() {
                lines.push(Line::from(Span::styled("Preview unavailable", muted())));
            } else if preview.entries.is_empty() {
                lines.push(Line::from(Span::styled("Empty directory", muted())));
            } else {
                lines.push(Line::from(Span::styled("Contents", muted())));
                lines.extend(
                    preview
                        .entries
                        .iter()
                        .map(|entry| Line::from(entry.as_str())),
                );
            }
        }
        None => {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Loading directory…", muted())));
        }
    }
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(muted())
        .padding(Padding::new(2, 1, 0, 0));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn render_empty(frame: &mut Frame<'_>, area: Rect, state: &PickerState) {
    let (title, hint) = if state.matching {
        ("Finding directories…", "Results update while you type")
    } else {
        ("No directories match", "Try a shorter query")
    };
    let message = Paragraph::new(vec![
        Line::from(Span::styled(
            title,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(hint, accent(state.color))),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(message, area);
}

fn render_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    compact: bool,
    state: &PickerState,
    actions: Availability,
) {
    let count = if state.matching {
        "matching…".to_owned()
    } else {
        format!("{} matches", state.matched_total)
    };
    let mut controls = if compact {
        "↑↓  Enter  Esc".to_owned()
    } else {
        "↑↓ move   Enter go".to_owned()
    };
    if !compact {
        if actions.open {
            controls.push_str("   ^O open");
        }
        if actions.copy {
            controls.push_str("   ^Y copy");
        }
        if actions.editor {
            controls.push_str("   ^E editor");
        }
        controls.push_str("   Tab preview   Esc close");
    }
    let line = Line::from(vec![
        Span::styled(controls, muted()),
        Span::raw("   "),
        Span::styled(count, accent(state.color)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn accent(color: bool) -> Style {
    if color {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}

fn muted() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use std::path::Path;

    fn candidate(path: &str, project: bool) -> Candidate {
        Candidate {
            path: PathBuf::from(path),
            display_path: path.into(),
            basename: Path::new(path)
                .file_name()
                .expect("basename")
                .to_string_lossy()
                .into_owned(),
            score: 1.0,
            source: "test",
            is_project_root: project,
            bookmark: None,
        }
    }

    fn rendered(width: u16, height: u16, candidates: &[Candidate]) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut state = PickerState::new("pun");
        state.color = false;
        state.matching = false;
        state.matched_total = candidates.len();
        state.selection_changed_at = Instant::now() - PREVIEW_DEBOUNCE;
        terminal
            .draw(|frame| render(frame, candidates, &mut state, Availability::default()))
            .expect("draw");
        terminal.backend().to_string()
    }

    #[test]
    fn wide_layout_has_navigation_rail_and_preview() {
        let output = rendered(
            100,
            18,
            &[
                candidate("/work/Punk", true),
                candidate("/work/punk-api", false),
            ],
        );
        assert!(output.contains("│"));
        assert!(output.contains("Destination"));
        assert!(output.contains("Project root"));
    }

    #[test]
    fn compact_layout_removes_preview_and_keeps_controls() {
        let output = rendered(44, 9, &[candidate("/work/Punk", true)]);
        assert!(!output.contains("Destination"));
        assert!(output.contains("Enter"));
        assert!(output.contains("Punk"));
    }

    #[test]
    fn empty_state_is_actionable() {
        let output = rendered(72, 12, &[]);
        assert!(output.contains("No directories match"));
        assert!(output.contains("Try a shorter query"));
    }

    #[test]
    fn unicode_query_editing_respects_character_boundaries() {
        let mut state = PickerState::new("a界");
        assert!(matches!(
            handle_key(
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                &mut state,
                1,
                Availability::default(),
            ),
            PickerAction::QueryChanged
        ));
        assert_eq!(state.query, "a");
        state.cursor = 0;
        assert!(matches!(
            handle_key(
                KeyEvent::new(KeyCode::Char('界'), KeyModifiers::NONE),
                &mut state,
                1,
                Availability::default(),
            ),
            PickerAction::QueryChanged
        ));
        assert_eq!(state.query, "界a");
    }

    #[test]
    fn action_shortcuts_are_exposed_only_when_available() {
        let mut state = PickerState::new("api");
        let available = Availability {
            open: true,
            copy: false,
            editor: true,
        };
        assert!(matches!(
            handle_key(
                KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
                &mut state,
                1,
                available,
            ),
            PickerAction::Select(Action::Open)
        ));
        assert!(matches!(
            handle_key(
                KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
                &mut state,
                1,
                available,
            ),
            PickerAction::Continue
        ));
        assert!(matches!(
            handle_key(
                KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
                &mut state,
                1,
                available,
            ),
            PickerAction::Select(Action::Editor)
        ));
    }

    #[test]
    fn high_level_nucleo_switches_between_basename_and_path_queries() {
        let candidates = vec![
            candidate("/work/punk/apps/frontend", true),
            candidate("/work/other/apps/backend", false),
        ];
        let mut matcher = LiveMatcher::new(candidates, "front");
        let deadline = Instant::now() + Duration::from_secs(2);
        while matcher.is_running() && Instant::now() < deadline {
            matcher.tick(10);
        }
        let results = matcher.cached_results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].basename, "frontend");

        matcher.set_query("punk frontend");
        let deadline = Instant::now() + Duration::from_secs(2);
        while matcher.is_running() && Instant::now() < deadline {
            matcher.tick(10);
        }
        let results = matcher.cached_results();
        assert_eq!(results.len(), 1);
        assert!(results[0].display_path.contains("punk/apps/frontend"));
    }

    #[test]
    fn directory_preview_is_shallow_bounded_and_detects_project_type() {
        let temp = tempfile::tempdir().expect("preview tempdir");
        fs::write(temp.path().join("Cargo.toml"), "[package]").expect("project marker");
        for index in 0..25 {
            fs::create_dir(temp.path().join(format!("entry-{index:02}"))).expect("preview entry");
        }
        fs::write(temp.path().join("entry-00/nested.txt"), "not top level").expect("nested file");

        let preview = load_directory_preview(temp.path().to_path_buf());

        assert_eq!(preview.project_type, Some("Rust project"));
        assert_eq!(preview.entries.len(), PREVIEW_ENTRY_LIMIT);
        assert!(!preview.entries.iter().any(|entry| entry == "nested.txt"));
        assert!(preview.entries.iter().any(|entry| entry.ends_with('/')));
        assert!(preview.error.is_none());
    }
}
