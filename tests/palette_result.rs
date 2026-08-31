use std::path::PathBuf;

use dirgo::{
    palette::{PaletteAction, PaletteResultFrame},
    shell::Shell,
};

#[test]
fn result_frames_are_versioned_and_keep_literal_navigation_paths() {
    let action = PaletteAction::Navigate {
        path: PathBuf::from("/tmp/- Dirgo 'демо'"),
    };

    let frame = PaletteResultFrame::from_action(&action, Shell::Zsh)
        .expect("navigation frame")
        .expect("parent-shell action");

    assert_eq!(frame.encode(), "DGP1 navigate\n/tmp/- Dirgo 'демо'");
}

#[test]
fn inserted_tasks_are_returned_but_never_marked_for_execution() {
    let action = PaletteAction::Insert {
        text: "docker compose up api".into(),
    };

    let frame = PaletteResultFrame::from_action(&action, Shell::Bash)
        .expect("insert frame")
        .expect("parent-shell action");

    assert_eq!(frame.encode(), "DGP1 insert\ndocker compose up api");
    assert!(!frame.encode().contains("execute"));
}

#[test]
fn structured_git_commands_are_quoted_for_each_shell_without_eval() {
    let action = PaletteAction::InsertCommand {
        program: "git".into(),
        args: vec!["switch".into(), "--".into(), "feature/quo'te space".into()],
    };

    let zsh = PaletteResultFrame::from_action(&action, Shell::Zsh)
        .expect("zsh frame")
        .expect("insert action")
        .encode();
    let fish = PaletteResultFrame::from_action(&action, Shell::Fish)
        .expect("fish frame")
        .expect("insert action")
        .encode();
    let powershell = PaletteResultFrame::from_action(&action, Shell::PowerShell)
        .expect("powershell frame")
        .expect("insert action")
        .encode();

    assert_eq!(
        zsh,
        "DGP1 insert\n'git' 'switch' '--' 'feature/quo'\\''te space'"
    );
    assert_eq!(
        fish,
        "DGP1 insert\n'git' 'switch' '--' 'feature/quo\\'te space'"
    );
    assert_eq!(
        powershell,
        "DGP1 insert\n'git' 'switch' '--' 'feature/quo''te space'"
    );
    for frame in [zsh, fish, powershell] {
        assert!(!frame.contains("eval"));
        assert!(!frame.contains("Invoke-Expression"));
    }
}

#[test]
fn frames_reject_newlines_and_controls_at_the_shell_boundary() {
    let newline = PaletteAction::Insert {
        text: "echo safe\necho unsafe".into(),
    };
    let control = PaletteAction::Navigate {
        path: PathBuf::from("/tmp/dirgo\u{1b}[2J"),
    };

    assert!(PaletteResultFrame::from_action(&newline, Shell::Bash).is_err());
    assert!(PaletteResultFrame::from_action(&control, Shell::Zsh).is_err());
}
