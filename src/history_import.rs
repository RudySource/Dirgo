use std::{collections::BTreeMap, path::PathBuf, process::Command};

use crate::{DirgoError, Result};

const MAX_IMPORTED_VISITS: f64 = 1_000_000.0;

pub struct ZoxideSnapshot {
    pub entries: Vec<(PathBuf, u64)>,
    pub skipped_stale: usize,
}

pub fn read_zoxide() -> Result<ZoxideSnapshot> {
    let output = Command::new("zoxide")
        .args(["query", "--list", "--score"])
        .output()
        .map_err(|error| {
            DirgoError::User(format!(
                "could not run `zoxide query --list --score`: {error}. Install zoxide or omit the import"
            ))
        })?;
    if !output.status.success() {
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        return Err(DirgoError::User(format!(
            "`zoxide query --list --score` failed: {}",
            diagnostic.trim()
        )));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| DirgoError::User("zoxide output is not valid UTF-8".into()))?;
    let parsed = parse_zoxide(&stdout)?;
    let mut entries = Vec::with_capacity(parsed.len());
    let mut skipped_stale = 0;
    for (path, visits) in parsed {
        if path.is_dir() {
            entries.push((path, visits));
        } else {
            skipped_stale += 1;
        }
    }
    Ok(ZoxideSnapshot {
        entries,
        skipped_stale,
    })
}

fn parse_zoxide(value: &str) -> Result<Vec<(PathBuf, u64)>> {
    let mut entries = BTreeMap::<PathBuf, u64>::new();
    for (index, raw_line) in value.lines().enumerate() {
        let line = raw_line.trim_start();
        if line.is_empty() {
            continue;
        }
        let split_at = line.find(char::is_whitespace).ok_or_else(|| {
            DirgoError::User(format!(
                "invalid zoxide output on line {}: expected `<score> <absolute path>`",
                index + 1
            ))
        })?;
        let score_raw = &line[..split_at];
        let path_raw = line[split_at..].trim_start();
        let score = score_raw.parse::<f64>().map_err(|_| {
            DirgoError::User(format!(
                "invalid zoxide score on line {}: {score_raw:?}",
                index + 1
            ))
        })?;
        if !score.is_finite() || score <= 0.0 || score > MAX_IMPORTED_VISITS {
            return Err(DirgoError::User(format!(
                "zoxide score on line {} must be finite and between 0 and {MAX_IMPORTED_VISITS}",
                index + 1
            )));
        }
        if path_raw.is_empty() {
            return Err(DirgoError::User(format!(
                "zoxide path is empty on line {}",
                index + 1
            )));
        }
        let path = PathBuf::from(path_raw);
        if !path.is_absolute() {
            return Err(DirgoError::User(format!(
                "zoxide path on line {} is not absolute: {path_raw:?}",
                index + 1
            )));
        }
        let visits = score.ceil() as u64;
        entries
            .entry(path)
            .and_modify(|existing| *existing = (*existing).max(visits))
            .or_insert(visits);
    }
    Ok(entries.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scores_and_preserves_spaces_in_absolute_paths() {
        let one = std::env::temp_dir().join("dirgo zoxide one space");
        let two = std::env::temp_dir().join("dirgo-zoxide-two");
        let input = format!("   4.0 {}\n12.2 {}\n", one.display(), two.display());
        let parsed = parse_zoxide(&input).expect("parse");
        let mut expected = vec![(one, 4), (two, 13)];
        expected.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(parsed, expected);
    }

    #[test]
    fn rejects_malformed_relative_or_unbounded_entries_before_import() {
        for invalid in [
            "not-a-score /tmp/path",
            "4.0 relative/path",
            "inf /tmp/path",
            "1000001 /tmp/path",
            "4.0",
        ] {
            assert!(parse_zoxide(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn duplicate_paths_merge_idempotently_with_the_larger_score() {
        let path = std::env::temp_dir().join("dirgo-zoxide-duplicate");
        let input = format!("2 {}\n8 {}\n", path.display(), path.display());
        let parsed = parse_zoxide(&input).expect("parse");
        assert_eq!(parsed, vec![(path, 8)]);
    }
}
