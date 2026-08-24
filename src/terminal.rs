use std::{borrow::Cow, fmt::Write as _, path::Path};

/// Makes untrusted filesystem text safe to render in a terminal.
///
/// Paths remain unchanged for navigation and machine-readable stdout. Only
/// human-facing output passes through this function, preventing filenames from
/// injecting ANSI controls or invisible bidirectional overrides.
pub fn safe_text(value: &str) -> Cow<'_, str> {
    if !value.chars().any(is_unsafe_terminal_character) {
        return Cow::Borrowed(value);
    }

    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{1b}' => escaped.push_str("\\x1b"),
            character if is_unsafe_terminal_character(character) => {
                let _ = write!(escaped, "\\u{{{:x}}}", character as u32);
            }
            character => escaped.push(character),
        }
    }
    Cow::Owned(escaped)
}

pub fn safe_path(path: &Path) -> String {
    safe_text(&path.to_string_lossy()).into_owned()
}

fn is_unsafe_terminal_character(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_normal_unicode_paths() {
        assert_eq!(safe_text("~/Проекты/音乐/🚀"), "~/Проекты/音乐/🚀");
    }

    #[test]
    fn escapes_terminal_controls_and_direction_overrides() {
        assert_eq!(
            safe_text("project\u{1b}]8;;https://example.test\u{7}x\u{202e}txt"),
            "project\\x1b]8;;https://example.test\\u{7}x\\u{202e}txt"
        );
        assert_eq!(safe_text("line\nname\tend"), "line\\nname\\tend");
    }
}
