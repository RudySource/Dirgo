use std::{
    ffi::OsStr,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::palette::{PaletteAction, PaletteItem, PaletteSource, ProviderBatch, ProviderBudget};

const MAX_GIT_OUTPUT: u64 = 256 * 1024;

pub fn git(cwd: &Path, budget: ProviderBudget) -> ProviderBatch {
    let started = Instant::now();
    let root_output = match run_git(
        cwd,
        ["rev-parse", "--show-toplevel"],
        remaining(started, budget.deadline),
    ) {
        Ok(output) => output,
        Err(error) => return ProviderBatch::failed(PaletteSource::Git, error),
    };
    let root_text = match std::str::from_utf8(&root_output) {
        Ok(value) => value.trim_end_matches(['\r', '\n']),
        Err(_) => return ProviderBatch::failed(PaletteSource::Git, "Git root is not UTF-8"),
    };
    if root_text.is_empty() || root_text.contains(['\r', '\n']) {
        return ProviderBatch::failed(PaletteSource::Git, "Git root is not a safe path");
    }
    let root = match PathBuf::from(root_text).canonicalize() {
        Ok(root) if root.is_dir() => root,
        _ => return ProviderBatch::failed(PaletteSource::Git, "Git root is unavailable"),
    };
    let branches = match run_git(
        &root,
        [
            "for-each-ref",
            "--count=256",
            "--format=%(refname:short)%09%(HEAD)",
            "refs/heads",
        ],
        remaining(started, budget.deadline),
    ) {
        Ok(output) => output,
        Err(error) => return ProviderBatch::failed(PaletteSource::Git, error),
    };
    let worktrees = match run_git(
        &root,
        ["worktree", "list", "--porcelain", "-z"],
        remaining(started, budget.deadline),
    ) {
        Ok(output) => output,
        Err(error) => return ProviderBatch::failed(PaletteSource::Git, error),
    };
    let mut items = parse_branches(&branches);
    items.extend(parse_worktrees(&worktrees));
    items.truncate(budget.max_items);
    ProviderBatch::ready(PaletteSource::Git, items, started.elapsed())
}

fn remaining(started: Instant, deadline: Duration) -> Duration {
    deadline.saturating_sub(started.elapsed())
}

fn run_git<I, S>(cwd: &Path, args: I, timeout: Duration) -> Result<Vec<u8>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if timeout.is_zero() {
        return Err("Git provider timed out".into());
    }
    let mut child = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Git is unavailable: {error}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Git output pipe is unavailable".to_owned())?;
    let reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout
            .by_ref()
            .take(MAX_GIT_OUTPUT + 1)
            .read_to_end(&mut output)
            .map(|_| output)
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(2)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("Git provider timed out".into());
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(format!("Git provider failed: {error}"));
            }
        }
    };
    let output = reader
        .join()
        .map_err(|_| "Git output reader stopped unexpectedly".to_owned())?
        .map_err(|error| format!("Git output could not be read: {error}"))?;
    if output.len() as u64 > MAX_GIT_OUTPUT {
        return Err("Git provider output exceeded 256 KiB".into());
    }
    if !status.success() {
        return Err("Git command did not complete successfully".into());
    }
    Ok(output)
}

fn parse_branches(output: &[u8]) -> Vec<PaletteItem> {
    let Ok(output) = std::str::from_utf8(output) else {
        return Vec::new();
    };
    output
        .lines()
        .filter_map(|line| {
            let (name, head) = line.split_once('\t')?;
            safe_git_label(name)?;
            Some(PaletteItem {
                id: format!("git:branch:{name}"),
                source: PaletteSource::Git,
                title: name.into(),
                subtitle: if head.trim() == "*" {
                    "Current branch"
                } else {
                    "Branch"
                }
                .into(),
                insert_text: None,
                preview_key: Some(format!("git:branch:{name}")),
                action: PaletteAction::InsertCommand {
                    program: "git".into(),
                    args: vec!["switch".into(), "--".into(), name.into()],
                },
                score: if head.trim() == "*" { 30_000 } else { 25_000 },
            })
        })
        .collect()
}

fn parse_worktrees(output: &[u8]) -> Vec<PaletteItem> {
    let mut items = Vec::new();
    let mut path = None;
    let mut branch = None;
    for field in output.split(|byte| *byte == 0) {
        if field.is_empty() {
            push_worktree(&mut items, path.take(), branch.take());
            continue;
        }
        if let Some(value) = field.strip_prefix(b"worktree ") {
            push_worktree(&mut items, path.take(), branch.take());
            if let Ok(value) = std::str::from_utf8(value)
                && !value.contains(['\r', '\n'])
            {
                path = Some(PathBuf::from(value));
            }
        } else if let Some(value) = field.strip_prefix(b"branch refs/heads/")
            && let Ok(value) = std::str::from_utf8(value)
            && safe_git_label(value).is_some()
        {
            branch = Some(value.to_owned());
        }
    }
    push_worktree(&mut items, path, branch);
    items
}

fn push_worktree(items: &mut Vec<PaletteItem>, path: Option<PathBuf>, branch: Option<String>) {
    let Some(path) = path.and_then(|path| path.canonicalize().ok()) else {
        return;
    };
    let title = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("worktree")
        .to_owned();
    let subtitle = branch.map_or_else(
        || "Worktree".into(),
        |branch| format!("Worktree · {branch}"),
    );
    items.push(PaletteItem {
        id: format!("git:worktree:{}", path.display()),
        source: PaletteSource::Git,
        title,
        subtitle,
        insert_text: None,
        preview_key: Some(format!("git:worktree:{}", path.display())),
        action: PaletteAction::Navigate { path },
        score: 27_000,
    });
}

fn safe_git_label(value: &str) -> Option<&str> {
    (!value.is_empty() && value.len() <= 1_024 && !value.chars().any(char::is_control))
        .then_some(value)
}
