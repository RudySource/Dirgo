use std::{fs, process::Command, time::Duration};

use dirgo::palette::{PaletteAction, PaletteSource, ProviderBudget, ProviderState, providers::git};

fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .status()
        .expect("run git fixture command");
    assert!(status.success(), "git fixture command failed: {args:?}");
}

#[test]
fn git_provider_lists_branches_and_worktrees_as_explicit_safe_actions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    let linked = temp.path().join("linked worktree");
    fs::create_dir_all(&repo).expect("repo");
    run_git(&repo, &["init", "-b", "main"]);
    run_git(&repo, &["config", "user.email", "fixture@example.invalid"]);
    run_git(&repo, &["config", "user.name", "Dirgo Fixture"]);
    fs::write(repo.join("README.md"), "fixture").expect("fixture file");
    run_git(&repo, &["add", "README.md"]);
    run_git(&repo, &["commit", "-m", "fixture"]);
    run_git(&repo, &["branch", "feature/test"]);
    run_git(
        &repo,
        &[
            "worktree",
            "add",
            linked.to_str().expect("utf8 worktree"),
            "feature/test",
        ],
    );

    let batch = git(&repo, ProviderBudget::new(16, Duration::from_secs(2)));

    assert_eq!(batch.source, PaletteSource::Git);
    assert!(batch.error.is_none());
    let main = batch
        .items
        .iter()
        .find(|item| item.title == "main")
        .expect("main branch");
    assert!(main.subtitle.contains("Current branch"));
    let feature = batch
        .items
        .iter()
        .find(|item| item.title == "feature/test")
        .expect("feature branch");
    assert_eq!(
        feature.action,
        PaletteAction::InsertCommand {
            program: "git".into(),
            args: vec!["switch".into(), "--".into(), "feature/test".into()],
        }
    );
    assert!(batch.items.iter().any(|item| {
        item.subtitle.contains("Worktree")
            && item.action
                == PaletteAction::Navigate {
                    path: linked.canonicalize().expect("linked worktree"),
                }
    }));
}

#[test]
fn git_provider_failure_is_isolated_outside_a_repository() {
    let temp = tempfile::tempdir().expect("tempdir");
    let batch = git(
        temp.path(),
        ProviderBudget::new(8, Duration::from_millis(250)),
    );

    assert_eq!(batch.source, PaletteSource::Git);
    assert!(batch.items.is_empty());
    assert!(batch.error.is_some());
    let snapshot = dirgo::palette::PaletteCoordinator::new(std::collections::HashMap::from([(
        PaletteSource::Git,
        ProviderBudget::new(8, Duration::from_millis(250)),
    )]))
    .merge(vec![batch]);
    assert_eq!(snapshot.state(PaletteSource::Git), ProviderState::Failed);
}
