use std::{
    fs,
    io::{self, IsTerminal},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{DirgoError, Result, terminal};

use super::{
    PaletteAction, PaletteSession, PaletteSource, PreviewRequest, PreviewResponse, ProviderState,
};

#[derive(Debug, Clone, Copy)]
pub struct PaletteViewOptions {
    pub color: bool,
    pub unicode: bool,
}

impl Default for PaletteViewOptions {
    fn default() -> Self {
        Self {
            color: std::env::var_os("NO_COLOR").is_none(),
            unicode: true,
        }
    }
}

pub fn pick(
    mut session: PaletteSession,
    options: PaletteViewOptions,
) -> Result<Option<PaletteAction>> {
    if !io::stderr().is_terminal() || std::env::var("TERM").is_ok_and(|term| term == "dumb") {
        return Ok(None);
    }
    use_terminal_stdin()?;
    enable_raw_mode().map_err(|error| DirgoError::io("terminal", error))?;
    let mut guard = TerminalGuard;
    execute!(io::stderr(), EnterAlternateScreen)
        .map_err(|error| DirgoError::io("terminal", error))?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stderr()))
        .map_err(|error| DirgoError::io("terminal", error))?;
    terminal
        .hide_cursor()
        .map_err(|error| DirgoError::io("terminal", error))?;
    let loader = PreviewLoader::new();
    let mut cursor = session.query().len();
    let result = loop {
        while let Some(response) = loader.try_recv() {
            session.accept_preview(response);
        }
        if let Some(request) = session.preview_request(Instant::now()) {
            loader.request(request);
        }
        terminal
            .draw(|frame| render(frame, &mut session, options))
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
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }
        let now = Instant::now();
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => break None,
            (KeyCode::Enter, _) => break session.selected().map(|item| item.action.clone()),
            (KeyCode::Tab, _) => session.switch_next(now),
            (KeyCode::BackTab, _) => session.switch_previous(now),
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
                session.move_selection(-1, now);
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
                session.move_selection(1, now);
            }
            (KeyCode::Left, _) => {
                cursor = previous_boundary(session.query(), cursor).unwrap_or(cursor)
            }
            (KeyCode::Right, _) => {
                cursor = next_boundary(session.query(), cursor).unwrap_or(cursor)
            }
            (KeyCode::Backspace, _) => {
                if let Some(previous) = previous_boundary(session.query(), cursor) {
                    let mut query = session.query().to_owned();
                    query.drain(previous..cursor);
                    cursor = previous;
                    session.set_query(query, now);
                }
            }
            (KeyCode::Delete, _) => {
                if let Some(next) = next_boundary(session.query(), cursor) {
                    let mut query = session.query().to_owned();
                    query.drain(cursor..next);
                    session.set_query(query, now);
                }
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                cursor = 0;
                session.set_query(String::new(), now);
            }
            (KeyCode::Char(character), modifiers)
                if !modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                let mut query = session.query().to_owned();
                query.insert(cursor, character);
                cursor += character.len_utf8();
                session.set_query(query, now);
            }
            _ => {}
        }
    };
    terminal
        .show_cursor()
        .map_err(|error| DirgoError::io("terminal", error))?;
    drop(terminal);
    guard.restore()?;
    std::mem::forget(guard);
    Ok(result)
}

fn render(frame: &mut Frame<'_>, session: &mut PaletteSession, options: PaletteViewOptions) {
    let area = frame.area();
    let compact = area.width < 72;
    let surface = if options.color {
        Style::default()
            .fg(Color::Rgb(244, 247, 248))
            .bg(Color::Rgb(11, 15, 18))
    } else {
        Style::default()
    };
    let block = if options.unicode {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(muted(options))
            .style(surface)
    } else {
        Block::default().style(surface)
    };
    frame.render_widget(block, area);
    let inner = if options.unicode {
        area.inner(Margin {
            horizontal: 2,
            vertical: 1,
        })
    } else {
        area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        })
    };
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(2),
        Constraint::Length(1),
    ])
    .split(inner);
    render_brand(frame, rows[0], options, surface);
    render_query(frame, rows[1], session, options, surface);
    render_sources(frame, rows[2], session, options, surface);
    if !compact && area.width >= 96 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
            .split(rows[3]);
        render_results(frame, columns[0], session, options, surface);
        render_preview(frame, columns[1], session, options, surface);
    } else {
        render_results(frame, rows[3], session, options, surface);
    }
    frame.render_widget(
        Paragraph::new(if compact {
            "Tab source  Enter select  Esc close"
        } else {
            "↑↓ move   Tab source   Shift+Tab back   Enter select   Esc close"
        })
        .style(muted(options)),
        rows[4],
    );
}

fn render_brand(frame: &mut Frame<'_>, area: Rect, options: PaletteViewOptions, surface: Style) {
    let marker = if options.unicode { "● " } else { "* " };
    let short_version = env!("CARGO_PKG_VERSION")
        .rsplit_once('.')
        .map_or(env!("CARGO_PKG_VERSION"), |(version, _)| version);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(marker, accent(options)),
            Span::styled(
                format!("DIRGO {short_version}"),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Workspace Palette", muted(options)),
        ]))
        .style(surface),
        area,
    );
}

fn render_query(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &PaletteSession,
    options: PaletteViewOptions,
    surface: Style,
) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(if options.unicode { "› " } else { "> " }, accent(options)),
            Span::raw(terminal::safe_text(session.query()).into_owned()),
            Span::styled(if options.unicode { "▏" } else { "|" }, accent(options)),
        ]))
        .style(surface),
        area,
    );
}

fn render_sources(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &PaletteSession,
    options: PaletteViewOptions,
    surface: Style,
) {
    let mut spans = Vec::new();
    for source in PaletteSource::FILTERS {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        let status = if source != PaletteSource::All
            && session.provider_state(source) != ProviderState::Ready
        {
            "!"
        } else {
            ""
        };
        let label = format!("{}{status}", title_case(source));
        if source == session.source() {
            spans.push(Span::styled(
                format!("[{label}]"),
                accent(options).add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(label, muted(options)));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(surface), area);
}

fn render_results(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &PaletteSession,
    options: PaletteViewOptions,
    surface: Style,
) {
    if session.visible().is_empty() {
        frame.render_widget(
            Paragraph::new("No matches\n\nTry another query or source")
                .style(muted(options))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }
    let selected = session.selected_index().unwrap_or(0);
    let items = session
        .visible()
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let marker = if index == selected {
                if options.unicode { "› " } else { "> " }
            } else {
                "  "
            };
            let line = Line::from(vec![
                Span::styled(marker, accent(options)),
                Span::styled(
                    terminal::safe_text(&item.title).into_owned(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {:<7}  ", title_case(item.source)),
                    muted(options).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    terminal::safe_text(&item.subtitle).into_owned(),
                    muted(options),
                ),
            ]);
            let row = ListItem::new(line);
            if index == selected && options.color {
                row.style(Style::default().bg(Color::Rgb(16, 32, 24)))
            } else {
                row
            }
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(List::new(items).style(surface), area, &mut state);
}

fn render_preview(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &PaletteSession,
    options: PaletteViewOptions,
    surface: Style,
) {
    let lines = session.preview().map_or_else(
        || {
            vec![Line::from(Span::styled(
                "Preview loads on selection",
                muted(options),
            ))]
        },
        |preview| {
            preview
                .lines
                .iter()
                .map(|line| Line::from(terminal::safe_text(line).into_owned()))
                .collect()
        },
    );
    let block = if options.unicode {
        Block::default()
            .borders(Borders::LEFT)
            .border_style(muted(options))
            .title(" Preview ")
    } else {
        Block::default().title("Preview")
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(surface)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn title_case(source: PaletteSource) -> &'static str {
    match source {
        PaletteSource::All => "All",
        PaletteSource::Files => "Files",
        PaletteSource::Tasks => "Tasks",
        PaletteSource::Workflows => "Workflows",
        PaletteSource::Git => "Git",
        PaletteSource::Compose => "Compose",
        PaletteSource::Places => "Places",
    }
}

fn accent(options: PaletteViewOptions) -> Style {
    if options.color {
        Style::default().fg(Color::Rgb(52, 199, 89))
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    }
}

fn muted(options: PaletteViewOptions) -> Style {
    if options.color {
        Style::default().fg(Color::Rgb(142, 154, 165))
    } else {
        Style::default()
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

struct PreviewLoader {
    shared: Arc<(Mutex<PreviewWorkerState>, Condvar)>,
    receiver: Receiver<PreviewResponse>,
    worker: Option<thread::JoinHandle<()>>,
}

#[derive(Default)]
struct PreviewWorkerState {
    pending: Option<PreviewRequest>,
    stopped: bool,
}

impl PreviewLoader {
    fn new() -> Self {
        let shared = Arc::new((Mutex::new(PreviewWorkerState::default()), Condvar::new()));
        let worker_shared = Arc::clone(&shared);
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            loop {
                let request = {
                    let (lock, ready) = &*worker_shared;
                    let mut state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    while state.pending.is_none() && !state.stopped {
                        state = ready
                            .wait(state)
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                    }
                    if state.stopped {
                        break;
                    }
                    state.pending.take()
                };
                let Some(request) = request else { continue };
                let lines = preview_lines(&request.item);
                if sender
                    .send(PreviewResponse {
                        generation: request.generation,
                        key: request.key,
                        lines,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            shared,
            receiver,
            worker: Some(worker),
        }
    }

    fn request(&self, request: PreviewRequest) {
        let (lock, ready) = &*self.shared;
        let mut state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending = Some(request);
        ready.notify_one();
    }

    fn try_recv(&self) -> Option<PreviewResponse> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for PreviewLoader {
    fn drop(&mut self) {
        let (lock, ready) = &*self.shared;
        {
            let mut state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.stopped = true;
            state.pending = None;
            ready.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn preview_lines(item: &super::PaletteItem) -> Vec<String> {
    let mut lines = vec![item.subtitle.clone(), item.title.clone()];
    if let Some(preview) = &item.workflow_preview {
        lines.push("Complete sequence".into());
        lines.extend(preview.steps.iter().enumerate().map(|(index, step)| {
            if index == preview.next_index {
                format!("> {}. {step}  NEXT", index + 1)
            } else {
                format!("  {}. {step}", index + 1)
            }
        }));
        lines.push("Inserted, never executed".into());
        lines.truncate(24);
        return lines;
    }
    match &item.action {
        PaletteAction::Navigate { path }
        | PaletteAction::Open { path }
        | PaletteAction::CopyPath { path }
        | PaletteAction::OpenEditor { path } => {
            lines.push(path.display().to_string());
            if path.is_dir()
                && let Ok(entries) = fs::read_dir(path)
            {
                let mut names = entries
                    .filter_map(|entry| entry.ok())
                    .take(20)
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                names.sort();
                lines.extend(names);
            }
        }
        PaletteAction::Insert { text } => {
            lines.push(text.clone());
            lines.push("Inserted, not executed".into());
        }
        PaletteAction::InsertCommand { program, args } => {
            lines.push(format!("{} {}", program, args.join(" ")));
            lines.push("Inserted, not executed".into());
        }
    }
    lines.truncate(24);
    lines
}

struct TerminalGuard;

impl TerminalGuard {
    fn restore(&mut self) -> Result<()> {
        execute!(io::stderr(), LeaveAlternateScreen)
            .map_err(|error| DirgoError::io("terminal", error))?;
        disable_raw_mode().map_err(|error| DirgoError::io("terminal", error))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stderr(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

#[cfg(unix)]
fn use_terminal_stdin() -> Result<()> {
    if unsafe { libc::dup2(libc::STDERR_FILENO, libc::STDIN_FILENO) } == -1 {
        return Err(DirgoError::io("terminal", io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(windows)]
fn use_terminal_stdin() -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        time::{Duration, Instant},
    };

    use ratatui::{Terminal, backend::TestBackend};

    use crate::palette::{
        PaletteAction, PaletteCoordinator, PaletteItem, PaletteSession, PaletteSource,
        PreviewResponse, ProviderBatch, ProviderBudget,
    };

    use super::{PaletteViewOptions, render};

    fn item(source: PaletteSource, title: &str) -> PaletteItem {
        PaletteItem {
            id: format!("{}:{title}", source.as_str()),
            source,
            title: title.into(),
            subtitle: format!("{title} detail"),
            insert_text: Some(title.into()),
            preview_key: Some(format!("preview:{title}")),
            workflow_preview: None,
            action: PaletteAction::Insert { text: title.into() },
            score: 100,
        }
    }

    fn session(now: Instant) -> PaletteSession {
        let budgets = PaletteSource::FILTERS
            .into_iter()
            .filter(|source| *source != PaletteSource::All)
            .map(|source| (source, ProviderBudget::new(8, Duration::from_secs(1))))
            .collect::<HashMap<_, _>>();
        let batches = [
            (PaletteSource::Files, "src/main.rs"),
            (PaletteSource::Tasks, "cargo test"),
            (PaletteSource::Git, "feature/palette"),
            (PaletteSource::Compose, "api"),
            (PaletteSource::Places, "@work"),
        ]
        .into_iter()
        .map(|(source, title)| {
            ProviderBatch::ready(source, vec![item(source, title)], Duration::ZERO)
        })
        .collect();
        PaletteSession::new(
            PaletteCoordinator::new(budgets).merge(batches),
            String::new(),
            now,
        )
    }

    fn rendered(
        width: u16,
        height: u16,
        session: &mut PaletteSession,
        options: PaletteViewOptions,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(frame, session, options))
            .expect("draw");
        terminal.backend().to_string()
    }

    #[test]
    fn wide_palette_has_source_strip_results_preview_and_clear_controls() {
        let now = Instant::now();
        let mut session = session(now);
        let request = session
            .preview_request(now + Duration::from_millis(90))
            .expect("preview request");
        assert!(session.accept_preview(PreviewResponse {
            generation: request.generation,
            key: request.key,
            lines: vec![
                "File".into(),
                "src/main.rs".into(),
                "Inserted, not executed".into()
            ],
        }));

        let output = rendered(112, 18, &mut session, PaletteViewOptions::default());

        let (major_minor, _) = env!("CARGO_PKG_VERSION")
            .rsplit_once('.')
            .expect("Cargo package version has a patch component");
        assert!(output.contains(&format!("DIRGO {major_minor}")));
        assert!(output.contains("All"));
        assert!(output.contains("Files"));
        assert!(output.contains("Tasks"));
        assert!(output.contains("Git"));
        assert!(output.contains("Compose"));
        assert!(output.contains("Places"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("Inserted, not executed"));
        assert!(output.contains("Tab source"));
        assert!(output.contains("Enter select"));
    }

    #[test]
    fn compact_ascii_palette_keeps_query_results_and_controls_without_unicode() {
        let now = Instant::now();
        let mut session = session(now);
        session.set_query("cargo".into(), now);
        let output = rendered(
            48,
            10,
            &mut session,
            PaletteViewOptions {
                color: false,
                unicode: false,
            },
        );

        assert!(output.contains("> cargo|"));
        assert!(output.contains("cargo test"));
        assert!(output.contains("Tab source"));
        assert!(!output.contains('›'));
        assert!(!output.contains('●'));
        assert!(!output.contains('│'));
    }
}
