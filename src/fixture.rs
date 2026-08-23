//! Deterministic directory trees for local performance measurements.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{DirgoError, Result};

pub const MAX_DIRECTORIES: u64 = 1_000_000;
const PROGRESS_FILE: &str = ".dirgo-fixture-progress.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixtureProgress {
    pub completed: u64,
    pub target: u64,
}

/// Creates exactly `directories` child directories below a new, empty `root`.
///
/// Node names encode their position in a breadth-first tree. This keeps a
/// million-directory fixture realistic enough to exercise traversal while
/// avoiding a single enormous parent directory. The root itself is not part of
/// the returned count.
pub fn create(root: &Path, directories: u64, fanout: u16) -> Result<()> {
    let progress = create_batch(root, directories, fanout, directories, false)?;
    debug_assert_eq!(progress.completed, directories);
    Ok(())
}

/// Creates at most `batch_size` additional nodes and records enough state to
/// resume safely after an interrupted benchmark preparation. Resuming is only
/// allowed when Dirgo's own progress marker matches the requested target and
/// fanout; an arbitrary existing directory is never adopted as a fixture.
pub fn create_batch(
    root: &Path,
    directories: u64,
    fanout: u16,
    batch_size: u64,
    resume: bool,
) -> Result<FixtureProgress> {
    if directories == 0 || directories > MAX_DIRECTORIES {
        return Err(DirgoError::User(format!(
            "fixture directory count must be between 1 and {MAX_DIRECTORIES}"
        )));
    }
    if fanout < 2 {
        return Err(DirgoError::User("fixture fanout must be at least 2".into()));
    }
    if batch_size == 0 {
        return Err(DirgoError::User(
            "fixture batch size must be at least 1".into(),
        ));
    }
    let parent = root.parent().ok_or_else(|| {
        DirgoError::User(format!("fixture path {} has no parent", root.display()))
    })?;
    if !parent.is_dir() {
        return Err(DirgoError::User(format!(
            "fixture parent {} does not exist",
            parent.display()
        )));
    }

    let mut completed = match fs::symlink_metadata(root) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() {
                return Err(DirgoError::User(format!(
                    "fixture root must be a real directory, not a file or symlink: {}",
                    root.display()
                )));
            }
            if !resume {
                return Err(DirgoError::User(format!(
                    "refusing to write fixture into existing path {}",
                    root.display()
                )));
            }
            read_progress(root, directories, fanout)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(root).map_err(|error| DirgoError::io(root, error))?;
            write_progress(root, directories, fanout, 0)?;
            0
        }
        Err(error) => return Err(DirgoError::io(root, error)),
    };

    let batch_end = completed.saturating_add(batch_size).min(directories);
    for node in completed + 1..=batch_end {
        let path = node_path(root, node, u64::from(fanout));
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => {
                return Err(DirgoError::User(format!(
                    "fixture node is not a real directory: {}",
                    path.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&path).map_err(|error| DirgoError::io(&path, error))?;
            }
            Err(error) => return Err(DirgoError::io(&path, error)),
        }
    }
    completed = batch_end;
    if completed == directories {
        fs::write(
            root.join("dirgo-fixture.toml"),
            fixture_metadata(directories, fanout, completed),
        )
        .map_err(|error| DirgoError::io(root, error))?;
        fs::remove_file(root.join(PROGRESS_FILE)).map_err(|error| DirgoError::io(root, error))?;
    } else {
        write_progress(root, directories, fanout, completed)?;
    }
    Ok(FixtureProgress {
        completed,
        target: directories,
    })
}

fn fixture_metadata(directories: u64, fanout: u16, completed: u64) -> String {
    format!(
        "schema_version = 1\ndirectories = {directories}\nfanout = {fanout}\ncompleted = {completed}\nlayout = \"breadth-first\"\n"
    )
}

fn write_progress(root: &Path, directories: u64, fanout: u16, completed: u64) -> Result<()> {
    let path = root.join(PROGRESS_FILE);
    let temporary = root.join(format!("{PROGRESS_FILE}.tmp"));
    fs::write(&temporary, fixture_metadata(directories, fanout, completed))
        .map_err(|error| DirgoError::io(&temporary, error))?;
    fs::rename(&temporary, &path).map_err(|error| DirgoError::io(&path, error))
}

fn read_progress(root: &Path, directories: u64, fanout: u16) -> Result<u64> {
    let path = root.join(PROGRESS_FILE);
    let text = fs::read_to_string(&path).map_err(|_| {
        DirgoError::User(format!(
            "refusing to resume {} without a valid Dirgo progress marker",
            root.display()
        ))
    })?;
    let value: toml::Value = toml::from_str(&text)
        .map_err(|_| DirgoError::User("fixture progress marker is invalid".into()))?;
    let marker_directories = value.get("directories").and_then(toml::Value::as_integer);
    let marker_fanout = value.get("fanout").and_then(toml::Value::as_integer);
    let completed = value.get("completed").and_then(toml::Value::as_integer);
    if marker_directories != i64::try_from(directories).ok()
        || marker_fanout != Some(i64::from(fanout))
        || completed.is_none_or(|value| value < 0 || value as u64 > directories)
    {
        return Err(DirgoError::User(
            "fixture progress marker does not match the requested target and fanout".into(),
        ));
    }
    Ok(completed.unwrap_or_default() as u64)
}

fn node_path(root: &Path, mut node: u64, fanout: u64) -> PathBuf {
    let mut nodes = Vec::new();
    while node > 0 {
        nodes.push(node);
        node = (node - 1) / fanout;
    }
    nodes.reverse();
    nodes.into_iter().fold(root.to_path_buf(), |path, node| {
        path.join(format!("node-{node:07}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_exact_count_without_reusing_a_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("fixture");
        create(&root, 9, 3).expect("fixture");
        let directories = walkdir_count(&root);
        assert_eq!(directories, 9);
        assert!(root.join("node-0000001").is_dir());
        assert!(root.join("node-0000001/node-0000004").is_dir());
        assert!(root.join("dirgo-fixture.toml").is_file());
        assert!(create(&root, 1, 2).is_err());
    }

    #[test]
    fn resumes_only_a_matching_marked_fixture() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("fixture");
        let first = create_batch(&root, 9, 3, 4, false).expect("first batch");
        assert_eq!(first.completed, 4);
        assert!(root.join(PROGRESS_FILE).is_file());

        let second = create_batch(&root, 9, 3, 4, true).expect("second batch");
        assert_eq!(second.completed, 8);
        assert!(create_batch(&root, 10, 3, 4, true).is_err());
        assert!(create_batch(&root, 9, 4, 4, true).is_err());

        let final_batch = create_batch(&root, 9, 3, 4, true).expect("final batch");
        assert_eq!(final_batch.completed, 9);
        assert_eq!(walkdir_count(&root), 9);
        assert!(!root.join(PROGRESS_FILE).exists());
        assert!(root.join("dirgo-fixture.toml").is_file());

        let unknown = temp.path().join("unknown");
        fs::create_dir(&unknown).expect("unknown root");
        assert!(create_batch(&unknown, 9, 3, 4, true).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn never_resumes_through_a_symlinked_root() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("target");
        fs::create_dir(&target).expect("target");
        fs::write(target.join(PROGRESS_FILE), fixture_metadata(9, 3, 0)).expect("marker");
        let linked_root = temp.path().join("linked");
        symlink(&target, &linked_root).expect("symlink");

        let error = create_batch(&linked_root, 9, 3, 4, true).expect_err("reject symlink root");
        assert!(error.to_string().contains("real directory"));
        assert_eq!(walkdir_count(&target), 0);
    }

    fn walkdir_count(root: &Path) -> usize {
        let mut pending = vec![root.to_path_buf()];
        let mut count = 0;
        while let Some(path) = pending.pop() {
            for entry in fs::read_dir(path).expect("read fixture") {
                let entry = entry.expect("entry");
                if entry.file_type().expect("type").is_dir() {
                    count += 1;
                    pending.push(entry.path());
                }
            }
        }
        count
    }
}
