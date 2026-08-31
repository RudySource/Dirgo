use dirgo::shell::Shell;
use dirgo::suggestions::{
    PickerAccept, Suggestion, SuggestionPickerResultFrame, SuggestionPickerResultKind,
    SuggestionSource, TextEdit,
};

fn suggestion(source: SuggestionSource, id: &str, replacement: &str) -> Suggestion {
    Suggestion {
        id: id.into(),
        edit: TextEdit {
            expected_before: "dgo sla".into(),
            replacement: replacement.into(),
        },
        display: "slash".into(),
        description: None,
        source,
        score: 1.0,
    }
}

#[test]
fn enter_on_a_directory_returns_a_literal_navigation_frame() {
    let selected = suggestion(
        SuggestionSource::Directory,
        "directory:/tmp/Dirgo demo",
        "dgo '/tmp/Dirgo demo'",
    );

    let frame =
        SuggestionPickerResultFrame::from_selection(&selected, PickerAccept::Enter, Shell::Zsh)
            .expect("valid frame");

    assert_eq!(frame.kind(), SuggestionPickerResultKind::Navigate);
    assert_eq!(frame.encode(), "DGS1 navigate\n/tmp/Dirgo demo");
}

#[test]
fn tab_on_a_directory_and_enter_on_a_command_only_insert_text() {
    let directory = suggestion(
        SuggestionSource::NavigationHistory,
        "directory:/tmp/Dirgo demo",
        "cd '/tmp/Dirgo demo'",
    );
    let command = suggestion(
        SuggestionSource::CommandHistory,
        "history:git status",
        "git status",
    );

    let tab =
        SuggestionPickerResultFrame::from_selection(&directory, PickerAccept::Tab, Shell::Fish)
            .expect("valid tab frame");
    let enter =
        SuggestionPickerResultFrame::from_selection(&command, PickerAccept::Enter, Shell::Bash)
            .expect("valid command frame");

    assert_eq!(tab.kind(), SuggestionPickerResultKind::Insert);
    assert_eq!(tab.encode(), "DGS1 insert\ncd '/tmp/Dirgo demo'");
    assert_eq!(enter.kind(), SuggestionPickerResultKind::Insert);
    assert_eq!(enter.encode(), "DGS1 insert\ngit status");
}

#[test]
fn malformed_directory_identity_cannot_become_a_navigation_action() {
    let selected = suggestion(
        SuggestionSource::Directory,
        "directory:/tmp/safe\nDGS1 insert\nwhoami",
        "dgo safe",
    );

    assert!(
        SuggestionPickerResultFrame::from_selection(&selected, PickerAccept::Enter, Shell::Zsh,)
            .is_err()
    );
}
