use std::{fs::OpenOptions, io::Write, path::Path};

use serde::Serialize;

use crate::{DirgoError, Result, terminal};

use super::{SavedWorkflowV1, WorkflowTransitionV1};

#[derive(Serialize)]
struct ExportRow<T> {
    format: &'static str,
    version: u8,
    kind: &'static str,
    workflow: T,
}

pub fn export_workflows(
    transitions: &[WorkflowTransitionV1],
    saved: &[SavedWorkflowV1],
    output: &Path,
    include_paths: bool,
    force: bool,
) -> Result<()> {
    if std::fs::symlink_metadata(output).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(DirgoError::User(
            "refusing to export through a symlink target".into(),
        ));
    }
    if output.exists() && !force {
        return Err(DirgoError::User(format!(
            "{} already exists; pass --force to replace it",
            terminal::safe_path(output)
        )));
    }
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| DirgoError::io(parent, error))?;
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(DirgoError::NonUtf8Path)?;
    let temp = parent.join(format!(
        ".{name}.dirgo-{}-{}.tmp",
        std::process::id(),
        crate::model::unix_now()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| DirgoError::io(&temp, error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| DirgoError::io(&temp, error))?;
    }
    for transition in transitions {
        let mut row = transition.clone();
        if !include_paths && row.scope_key.starts_with("project:") {
            row.scope_key = "project".into();
        }
        write_row(&mut file, &temp, "learned", &row)?;
    }
    for workflow in saved {
        let mut row = workflow.clone();
        if !include_paths && row.scope_key.starts_with("project:") {
            row.scope_key = "project".into();
        }
        write_row(&mut file, &temp, "saved", &row)?;
    }
    file.sync_all()
        .map_err(|error| DirgoError::io(&temp, error))?;
    drop(file);
    crate::suggestions::settings::replace_file(&temp, output)
}

fn write_row<T: Serialize>(
    file: &mut std::fs::File,
    temp: &Path,
    kind: &'static str,
    workflow: &T,
) -> Result<()> {
    let row = ExportRow {
        format: "dirgo-workflows",
        version: 1,
        kind,
        workflow,
    };
    writeln!(file, "{}", serde_json::to_string(&row)?).map_err(|error| DirgoError::io(temp, error))
}
