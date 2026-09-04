use std::io::{self, IsTerminal};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{DirgoError, Result};

use super::{Suggestion, SuggestionSource};

#[derive(Debug, Clone, Copy)]
pub struct PickerOptions {
    pub color: bool,
    pub unicode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerAccept {
    Enter,
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickerSelection {
    index: usize,
    accept: PickerAccept,
}

impl PickerSelection {
    pub fn index(self) -> usize {
        self.index
    }

    pub fn accept(self) -> PickerAccept {
        self.accept
    }
}

pub fn pick_suggestion(
    suggestions: &[Suggestion],
    options: PickerOptions,
) -> Result<Option<PickerSelection>> {
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
            KeyCode::Enter => {
                break Some(PickerSelection {
                    index: selected,
                    accept: PickerAccept::Enter,
                });
            }
            KeyCode::Tab => {
                break Some(PickerSelection {
                    index: selected,
                    accept: PickerAccept::Tab,
                });
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
    let area = picker_area(frame.area(), suggestions);
    let surface = if options.color {
        Style::default()
            .fg(Color::Rgb(244, 247, 248))
            .bg(Color::Rgb(11, 15, 18))
    } else {
        Style::default()
    };
    let border_style = if options.color {
        Style::default().fg(Color::Rgb(53, 65, 74))
    } else {
        Style::default()
    };
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_set(if options.unicode {
                border::ROUNDED
            } else {
                ASCII_BORDER
            })
            .border_style(border_style)
            .style(surface),
        area,
    );

    let inner = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(inner);
    let header = Layout::horizontal([Constraint::Min(18), Constraint::Length(16)]).split(rows[0]);
    let version = env!("CARGO_PKG_VERSION")
        .rsplit_once('.')
        .map_or(env!("CARGO_PKG_VERSION"), |(short, _)| short);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                if options.unicode { "● " } else { "* " },
                picker_accent(options.color),
            ),
            Span::styled(
                format!("DIRGO {version}"),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("  suggestions", muted(options.color)),
        ]))
        .style(surface),
        header[0],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "1{}{} of {}",
            range_dash(options),
            suggestions.len(),
            suggestions.len()
        ))
        .alignment(Alignment::Right)
        .style(muted(options.color)),
        header[1],
    );
    let selected_style = if options.color {
        Style::default()
            .fg(Color::Rgb(244, 247, 248))
            .bg(Color::Rgb(16, 32, 24))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    let detail_width = usize::from(rows[1].width.saturating_sub(34)).max(1);
    let items: Vec<ListItem<'_>> = suggestions
        .iter()
        .enumerate()
        .map(|(index, suggestion)| {
            let detail = suggestion
                .description
                .as_deref()
                .filter(|description| {
                    !description.eq_ignore_ascii_case(source_label(suggestion.source))
                })
                .unwrap_or(&suggestion.edit.replacement);
            let marker = if index == selected {
                if options.unicode { "›  " } else { ">  " }
            } else {
                "   "
            };
            let detail_lines = wrapped_detail(detail, detail_width);
            let mut lines = vec![Line::from(vec![
                Span::styled(marker, picker_accent(options.color)),
                Span::styled(
                    format!("{:<24}", clipped(&suggestion.display, 22, options.unicode)),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<7}", source_label(suggestion.source)),
                    source_style(suggestion.source, options.color),
                ),
                Span::styled(detail_lines[0].clone(), muted(options.color)),
            ])];
            if let Some(continuation) = detail_lines.get(1) {
                lines.push(Line::from(vec![
                    Span::raw("                                  "),
                    Span::styled(continuation.clone(), muted(options.color)),
                ]));
            }
            let item = ListItem::new(lines);
            if index == selected {
                item.style(selected_style)
            } else {
                item
            }
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(
        List::new(items)
            .style(surface)
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_style(Style::default()),
        rows[1],
        &mut state,
    );
    let selected_is_directory = suggestions.get(selected).is_some_and(|suggestion| {
        matches!(
            suggestion.source,
            SuggestionSource::Directory | SuggestionSource::NavigationHistory
        )
    });
    let action_spans = if selected_is_directory {
        vec![
            Span::styled("Enter", key_style(options.color)),
            Span::styled("  Open     ", muted(options.color)),
            Span::styled("Tab", key_style(options.color)),
            Span::styled("  Insert     ", muted(options.color)),
        ]
    } else {
        vec![
            Span::styled("Tab/Enter", key_style(options.color)),
            Span::styled("  Insert     ", muted(options.color)),
        ]
    };
    let mut footer = vec![
        Span::styled(
            if options.unicode { "↑↓" } else { "Up/Down" },
            key_style(options.color),
        ),
        Span::styled("  Select     ", muted(options.color)),
    ];
    footer.extend(action_spans);
    footer.extend([
        Span::styled("Esc", key_style(options.color)),
        Span::styled("  Close", muted(options.color)),
    ]);
    frame.render_widget(Paragraph::new(Line::from(footer)).style(surface), rows[2]);
}

fn picker_area(area: Rect, suggestions: &[Suggestion]) -> Rect {
    let width = area.width.min(112);
    let detail_width = usize::from(width.saturating_sub(38)).max(1);
    let item_rows = suggestions.iter().fold(0_u16, |rows, suggestion| {
        let detail = suggestion
            .description
            .as_deref()
            .filter(|description| {
                !description.eq_ignore_ascii_case(source_label(suggestion.source))
            })
            .unwrap_or(&suggestion.edit.replacement);
        rows.saturating_add(if UnicodeWidthStr::width(detail) > detail_width {
            2
        } else {
            1
        })
    });
    let height = area.height.min(item_rows.saturating_add(6).clamp(8, 18));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn wrapped_detail(value: &str, width: usize) -> Vec<String> {
    if UnicodeWidthStr::width(value) <= width {
        return vec![value.to_owned()];
    }

    let (first, remainder) = split_at_width(value, width);
    if UnicodeWidthStr::width(remainder) <= width {
        return vec![first.to_owned(), remainder.to_owned()];
    }

    let suffix_width = width.saturating_sub(1);
    let suffix = suffix_at_width(remainder, suffix_width);
    vec![first.to_owned(), format!("…{suffix}")]
}

fn split_at_width(value: &str, width: usize) -> (&str, &str) {
    let mut used = 0_usize;
    for (index, character) in value.char_indices() {
        let character_width = character.width().unwrap_or(0);
        if used.saturating_add(character_width) > width {
            return (&value[..index], &value[index..]);
        }
        used = used.saturating_add(character_width);
    }
    (value, "")
}

fn suffix_at_width(value: &str, width: usize) -> &str {
    let mut used = 0_usize;
    for (index, character) in value.char_indices().rev() {
        let character_width = character.width().unwrap_or(0);
        if used.saturating_add(character_width) > width {
            return &value[index + character.len_utf8()..];
        }
        used = used.saturating_add(character_width);
    }
    value
}

fn range_dash(options: PickerOptions) -> &'static str {
    if options.unicode { "–" } else { "-" }
}

fn clipped(value: &str, width: usize, unicode: bool) -> String {
    if value.chars().count() <= width {
        return value.to_owned();
    }
    let suffix = if unicode { '…' } else { '.' };
    let keep = width.saturating_sub(1);
    let mut output = value.chars().take(keep).collect::<String>();
    output.push(suffix);
    output
}

fn muted(color: bool) -> Style {
    if color {
        Style::default().fg(Color::Rgb(142, 154, 165))
    } else {
        Style::default()
    }
}

fn key_style(color: bool) -> Style {
    if color {
        Style::default()
            .fg(Color::Rgb(174, 186, 195))
            .bg(Color::Rgb(17, 24, 29))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    }
}

fn source_style(source: SuggestionSource, color: bool) -> Style {
    if color && source == SuggestionSource::ProjectCommand {
        Style::default()
            .fg(Color::Rgb(114, 223, 149))
            .add_modifier(Modifier::BOLD)
    } else {
        muted(color).add_modifier(Modifier::BOLD)
    }
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

fn picker_accent(color: bool) -> Style {
    if color {
        Style::default().fg(Color::Rgb(32, 191, 85))
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    }
}

#[cfg(test)]
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
        SuggestionSource::ProjectCommand => "PROJ",
        SuggestionSource::Workflow => "NEXT",
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
    use ratatui::backend::TestBackend;

    fn suggestion(display: &str, description: &str, source: SuggestionSource) -> Suggestion {
        Suggestion {
            id: format!("test:{display}"),
            edit: super::super::TextEdit {
                expected_before: "git c".into(),
                replacement: format!("git {display}"),
            },
            display: display.into(),
            description: Some(description.into()),
            source,
            score: 1.0,
        }
    }

    fn rendered_picker(options: PickerOptions) -> Terminal<TestBackend> {
        let suggestions = [
            suggestion(
                "checkout",
                "Switch branches or restore files",
                SuggestionSource::Subcommand,
            ),
            suggestion(
                "commit",
                "Record changes to the repository",
                SuggestionSource::Subcommand,
            ),
        ];
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(frame, &suggestions, 1, options))
            .expect("draw");
        terminal
    }

    fn rendered_long_directory_picker() -> Terminal<TestBackend> {
        let suggestions = [suggestion(
            "slash",
            "/workspace/projects/slash/frontend/native/android/src/final-component",
            SuggestionSource::Directory,
        )];
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                render(
                    frame,
                    &suggestions,
                    0,
                    PickerOptions {
                        color: false,
                        unicode: true,
                    },
                );
            })
            .expect("draw");
        terminal
    }

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
        assert_eq!(source_label(SuggestionSource::ProjectCommand), "PROJ");
        assert_eq!(source_label(SuggestionSource::Workflow), "NEXT");
        assert_eq!(centered(Rect::new(0, 0, 3, 2)), Rect::new(0, 0, 3, 2));
        assert_eq!(ASCII_BORDER.top_left, "+");
        assert_eq!(ASCII_BORDER.vertical_left, "|");
    }

    #[test]
    fn premium_picker_has_brand_counter_descriptions_and_explicit_actions() {
        let terminal = rendered_picker(PickerOptions {
            color: true,
            unicode: true,
        });
        let output = terminal.backend().to_string();

        let version = env!("CARGO_PKG_VERSION")
            .rsplit_once('.')
            .map_or(env!("CARGO_PKG_VERSION"), |(short, _)| short);
        assert!(output.contains(&format!("● DIRGO {version}")));
        assert!(output.contains("1–2 of 2"));
        assert!(output.contains("Record changes to the repository"));
        assert!(output.contains("Tab/Enter  Insert"));
        assert!(output.contains("Esc  Close"));

        let buffer = terminal.backend().buffer();
        let marker = (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .find_map(|position| {
                let cell = buffer.cell(position)?;
                (cell.symbol() == "›").then_some(cell)
            })
            .expect("selected marker");
        assert_eq!(marker.fg, Color::Rgb(32, 191, 85));
        assert_eq!(marker.bg, Color::Rgb(16, 32, 24));
        assert!(!marker.modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn monochrome_picker_keeps_selection_visible_without_color() {
        let terminal = rendered_picker(PickerOptions {
            color: false,
            unicode: false,
        });
        let output = terminal.backend().to_string();
        assert!(output.contains("+"));
        assert!(output.contains(">"));

        let buffer = terminal.backend().buffer();
        assert!(
            (0..buffer.area.height)
                .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
                .filter_map(|position| buffer.cell(position))
                .any(|cell| cell.symbol() == ">" && cell.modifier.contains(Modifier::BOLD))
        );
    }

    #[test]
    fn long_directory_detail_wraps_and_keeps_the_path_tail_visible() {
        let terminal = rendered_long_directory_picker();
        let buffer = terminal.backend().buffer();
        let rows = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .filter_map(|x| buffer.cell((x, y)))
                    .map(|cell| cell.symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        let first_row = rows
            .iter()
            .position(|row| row.contains("/workspace/projects"))
            .expect("first path row");
        let continuation_row = rows
            .iter()
            .position(|row| row.contains("final-component"))
            .expect("wrapped path tail");

        assert_eq!(continuation_row, first_row + 1);
        assert!(!rows[continuation_row].contains("slash  DIR"));
    }
}
