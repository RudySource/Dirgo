use std::io::{self, IsTerminal};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{DirgoError, Result};

use super::{Suggestion, SuggestionSource};

#[derive(Debug, Clone, Copy)]
pub struct PickerOptions {
    pub color: bool,
    pub unicode: bool,
}

pub fn pick_suggestion(
    suggestions: &[Suggestion],
    options: PickerOptions,
) -> Result<Option<String>> {
    if suggestions.is_empty()
        || !io::stderr().is_terminal()
        || std::env::var("TERM").is_ok_and(|term| term == "dumb")
    {
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

    let mut selected = 0_usize;
    let result = loop {
        terminal
            .draw(|frame| render(frame, suggestions, selected, options))
            .map_err(|error| DirgoError::io("terminal", error))?;
        let event = event::read().map_err(|error| DirgoError::io("terminal", error))?;
        let Event::Key(key) = event else { continue };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                selected = selected.checked_sub(1).unwrap_or(suggestions.len() - 1);
            }
            KeyCode::Down | KeyCode::Char('j') => selected = (selected + 1) % suggestions.len(),
            KeyCode::Enter | KeyCode::Tab => {
                break Some(suggestions[selected].edit.replacement.clone());
            }
            KeyCode::Esc | KeyCode::Char('q') => break None,
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

fn render(
    frame: &mut Frame<'_>,
    suggestions: &[Suggestion],
    selected: usize,
    options: PickerOptions,
) {
    let area = centered(frame.area());
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Dirgo", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" suggestions"),
        ]))
        .block(picker_block(
            Borders::TOP | Borders::LEFT | Borders::RIGHT,
            options,
        )),
        rows[0],
    );
    let items: Vec<ListItem<'_>> = suggestions
        .iter()
        .map(|suggestion| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<4} ", source_label(suggestion.source)),
                    picker_accent(options.color),
                ),
                Span::raw(suggestion.display.as_str()),
                Span::styled(
                    format!("  {}", suggestion.edit.replacement),
                    if options.color {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        Style::default()
                    },
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .block(picker_block(Borders::LEFT | Borders::RIGHT, options)),
        rows[1],
        &mut state,
    );
    frame.render_widget(
        Paragraph::new("Up/Down select   Enter insert   Esc cancel").block(picker_block(
            Borders::BOTTOM | Borders::LEFT | Borders::RIGHT,
            options,
        )),
        rows[2],
    );
}

const ASCII_BORDER: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

fn picker_block(borders: Borders, options: PickerOptions) -> Block<'static> {
    let block = Block::default().borders(borders);
    if options.unicode {
        block
    } else {
        block.border_set(ASCII_BORDER)
    }
}

fn picker_accent(color: bool) -> Style {
    if color {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    }
}

fn centered(area: Rect) -> Rect {
    let width = area.width.min(100);
    let height = area.height.min(15);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn source_label(source: SuggestionSource) -> &'static str {
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
    // Suggestion requests may arrive on a private pipe while stderr remains
    // attached to the controlling PTY. Duplicating that already-open terminal
    // descriptor avoids reopening /dev/tty, which is restricted in some
    // sandboxes and terminal multiplexers.
    // SAFETY: stderr is verified as a terminal above and dup2 only duplicates
    // the descriptor for the lifetime of this short-lived picker process.
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
    use super::*;

    #[test]
    fn picker_layout_is_bounded_and_source_labels_are_textual() {
        assert_eq!(
            centered(Rect::new(0, 0, 200, 50)),
            Rect::new(50, 17, 100, 15)
        );
        assert_eq!(source_label(SuggestionSource::CommandHistory), "HIST");
        assert_eq!(source_label(SuggestionSource::Filesystem), "FILE");
        assert_eq!(source_label(SuggestionSource::Command), "CMD");
        assert_eq!(source_label(SuggestionSource::Subcommand), "SUB");
        assert_eq!(source_label(SuggestionSource::Option), "OPT");
        assert_eq!(centered(Rect::new(0, 0, 3, 2)), Rect::new(0, 0, 3, 2));
        assert_eq!(ASCII_BORDER.top_left, "+");
        assert_eq!(ASCII_BORDER.vertical_left, "|");
    }
}
