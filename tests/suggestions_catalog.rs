use std::ffi::OsStr;

use dirgo::suggestions::{CommandCatalog, portable_command_name};

#[test]
fn portable_command_names_strip_windows_executable_extensions_case_insensitively() {
    assert_eq!(portable_command_name("Dirgo.EXE", true), "Dirgo");
    assert_eq!(portable_command_name("build.CmD", true), "build");
    assert_eq!(portable_command_name("script.sh", false), "script.sh");
}

#[cfg(unix)]
#[test]
fn catalog_discovers_sorted_unique_executables_with_bounded_metadata() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::create_dir_all(&first).expect("first");
    std::fs::create_dir_all(&second).expect("second");
    for path in [
        first.join("zeta"),
        first.join("alpha"),
        second.join("alpha"),
    ] {
        std::fs::write(&path, "#!/bin/sh\n").expect("fixture");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
            .expect("permissions");
    }
    let path = std::env::join_paths([first, second]).expect("PATH");

    let catalog = CommandCatalog::discover(Some(OsStr::new(&path)));
    assert_eq!(
        catalog.executable_names().collect::<Vec<_>>(),
        ["alpha", "zeta"]
    );
    assert!(catalog.executable_metadata("alpha").is_some());
    assert!(catalog.executable_count() <= 8_192);
}

#[test]
fn dirgo_command_graph_contains_public_nested_metadata_but_not_hidden_protocol_commands() {
    let catalog = CommandCatalog::default();
    let dirgo = catalog.command("dgo").expect("Dirgo spec");
    let suggestions = dirgo
        .subcommands
        .iter()
        .find(|command| command.name == "suggestions")
        .expect("suggestions command");

    assert!(
        suggestions
            .subcommands
            .iter()
            .any(|command| command.name == "history")
    );
    assert!(dirgo.options.iter().any(|option| option.name == "--update"));
    assert!(
        dirgo
            .subcommands
            .iter()
            .all(|command| !command.name.starts_with("__"))
    );
}
