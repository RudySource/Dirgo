use std::{fs, path::Path};

use crate::{DirgoError, Result, config::Config};

pub fn write_suggestions_config(path: &Path, config: &Config) -> Result<()> {
    config.validate()?;
    let existing = if path.exists() {
        fs::read_to_string(path).map_err(|error| DirgoError::io(path, error))?
    } else {
        toml::to_string_pretty(config).map_err(|error| DirgoError::Config(error.to_string()))?
    };
    let section = render_section(config)?;
    let updated = replace_section(&existing, "suggestions", &section);
    crate::config_edit::atomic_write(path, updated.as_bytes())
}

fn render_section(config: &Config) -> Result<String> {
    let body = toml::to_string_pretty(&config.suggestions)
        .map_err(|error| DirgoError::Config(error.to_string()))?;
    Ok(format!("[suggestions]\n{body}"))
}

fn replace_section(input: &str, name: &str, replacement: &str) -> String {
    let header = format!("[{name}]");
    let lines: Vec<&str> = input.lines().collect();
    let start = lines.iter().position(|line| line.trim() == header);
    let mut output = String::new();
    if let Some(start) = start {
        let end = lines
            .iter()
            .enumerate()
            .skip(start + 1)
            .find_map(|(index, line)| {
                let line = line.trim();
                (line.starts_with('[') && line.ends_with(']')).then_some(index)
            })
            .unwrap_or(lines.len());
        for line in &lines[..start] {
            output.push_str(line);
            output.push('\n');
        }
        output.push_str(replacement.trim_end());
        output.push('\n');
        for line in &lines[end..] {
            output.push_str(line);
            output.push('\n');
        }
    } else {
        output.push_str(input.trim_end());
        output.push_str("\n\n");
        output.push_str(replacement.trim_end());
        output.push('\n');
    }
    output
}

pub(crate) fn replace_file(temporary: &Path, path: &Path) -> Result<()> {
    crate::config_edit::replace_file(temporary, path)
}
