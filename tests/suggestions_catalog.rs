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

#[test]
fn catalog_contains_curated_external_command_trees_and_aliases() {
    let catalog = CommandCatalog::default();
    let docker = catalog.command("docker").expect("Docker spec");
    let compose = docker
        .subcommands
        .iter()
        .find(|command| command.name == "compose")
        .expect("Docker Compose spec");

    assert!(
        compose
            .subcommands
            .iter()
            .any(|command| command.name == "up")
    );
    assert!(catalog.command("kubectl").is_some());
    assert!(catalog.command("k").is_some());
    assert!(catalog.command("cargo").is_some());
    assert!(catalog.command("npm").is_some());
}

#[test]
fn catalog_loads_bounded_data_only_user_command_specs() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        temp.path().join("acme.toml"),
        r#"
name = "acme"
description = "Company developer tool"
aliases = ["a"]

[[subcommands]]
name = "deploy"
description = "Deploy the current service"

[[subcommands.options]]
name = "--production"
description = "Deploy to production"
"#,
    )
    .expect("custom spec");

    let catalog = CommandCatalog::default().with_user_specs(temp.path());
    let acme = catalog.command("a").expect("custom alias");
    let deploy = acme
        .subcommands
        .iter()
        .find(|command| command.name == "deploy")
        .expect("custom subcommand");
    assert_eq!(
        deploy.description.as_deref(),
        Some("Deploy the current service")
    );
    assert_eq!(deploy.options[0].name, "--production");
}

#[test]
fn user_specs_merge_with_builtins_and_invalid_or_oversized_files_are_ignored() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        temp.path().join("git.toml"),
        r#"
name = "git"

[[subcommands]]
name = "checkout"
description = "Custom checkout help"

[[subcommands]]
name = "company-sync"
description = "Synchronize company repositories"
"#,
    )
    .expect("override spec");
    std::fs::write(temp.path().join("invalid.toml"), "name = [broken").expect("invalid spec");
    std::fs::write(
        temp.path().join("oversized.toml"),
        vec![b'x'; 256 * 1024 + 1],
    )
    .expect("oversized spec");

    let catalog = CommandCatalog::default().with_user_specs(temp.path());
    let git = catalog.command("git").expect("Git spec remains available");
    assert!(
        git.subcommands
            .iter()
            .any(|command| command.name == "commit")
    );
    assert_eq!(
        git.subcommands
            .iter()
            .find(|command| command.name == "checkout")
            .and_then(|command| command.description.as_deref()),
        Some("Custom checkout help")
    );
    assert!(
        git.subcommands
            .iter()
            .any(|command| command.name == "company-sync")
    );
    assert!(catalog.command("invalid").is_none());
}

#[test]
fn user_spec_limits_and_alias_collisions_cannot_displace_builtin_commands() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut excessive_options = String::from("name = \"unbounded\"\n");
    for index in 0..257 {
        excessive_options.push_str(&format!("[[options]]\nname = \"--flag-{index}\"\n"));
    }
    std::fs::write(temp.path().join("00-unbounded.toml"), excessive_options)
        .expect("over-limit option spec");
    std::fs::write(
        temp.path().join("01-alias.toml"),
        "name = \"shadow\"\naliases = [\"git\"]\n",
    )
    .expect("colliding alias spec");
    for index in 2..=65 {
        std::fs::write(
            temp.path().join(format!("{index:02}-tool.toml")),
            format!("name = \"tool-{index:02}\"\n"),
        )
        .expect("bounded file fixture");
    }

    let catalog = CommandCatalog::default().with_user_specs(temp.path());
    assert!(catalog.command("unbounded").is_none());
    assert_eq!(catalog.command("git").expect("Git spec").name, "git");
    assert!(catalog.command("tool-63").is_some());
    assert!(catalog.command("tool-64").is_none());
    assert!(catalog.command("tool-65").is_none());
}
