use std::{collections::BTreeSet, ffi::OsStr};

use super::super::{Suggestion, SuggestionRequest, SuggestionSource, TextEdit};

pub fn discover_executables(path: Option<&OsStr>) -> Vec<String> {
    let Some(path) = path else {
        return Vec::new();
    };
    let mut names = BTreeSet::new();
    for directory in std::env::split_paths(path).take(128) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.take(4_096).flatten() {
            if names.len() >= 8_192 || !is_executable(&entry.path()) {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                names.insert(executable_name(name));
            }
        }
    }
    names.into_iter().collect()
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(windows)]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "exe" | "cmd" | "bat" | "com"
                )
            })
}

#[cfg(not(any(unix, windows)))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

#[cfg(windows)]
fn executable_name(name: &str) -> String {
    std::path::Path::new(name)
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or(name)
        .to_owned()
}

#[cfg(not(windows))]
fn executable_name(name: &str) -> String {
    name.to_owned()
}

pub fn executable_suggestions(
    request: &SuggestionRequest,
    executables: &[String],
) -> Vec<Suggestion> {
    let input = request.before_cursor.trim_start();
    if input.contains(char::is_whitespace) {
        return Vec::new();
    }
    executables
        .iter()
        .filter(|executable| executable.starts_with(input))
        .map(|executable| Suggestion {
            id: format!("executable:{executable}"),
            edit: TextEdit {
                expected_before: request.before_cursor.clone(),
                replacement: executable.clone(),
            },
            display: executable.clone(),
            description: Some("PATH".into()),
            source: SuggestionSource::Executable,
            score: 10_000.0 - executable.len() as f64,
        })
        .collect()
}
