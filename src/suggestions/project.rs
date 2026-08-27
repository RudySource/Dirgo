use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

use crate::{DirgoError, Result};

const MAX_MANIFEST_BYTES: u64 = 512 * 1024;
const MAX_PROJECT_COMMANDS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCommand {
    pub replacement: String,
    pub display: String,
    pub description: String,
    pub stable_id: String,
}

pub fn load_project_command_snapshot(root: &Path) -> Result<ProjectCommandSnapshot> {
    let mut commands = Vec::new();
    if let Some(package_json) = confined_manifest(root, &root.join("package.json"))
        && let Ok(package_commands) = load_package_commands(root, &package_json)
    {
        commands.extend(package_commands);
    }
    if let Some(cargo_toml) = confined_manifest(root, &root.join("Cargo.toml"))
        && let Ok(cargo_commands) = load_cargo_commands(root, &cargo_toml)
    {
        commands.extend(cargo_commands);
    }
    if let Some(makefile) = confined_manifest(root, &root.join("Makefile"))
        && let Ok(make_commands) = load_make_commands(&makefile)
    {
        commands.extend(make_commands);
    }
    for name in ["justfile", "Justfile"] {
        if let Some(path) = confined_manifest(root, &root.join(name))
            && let Ok(just_commands) = load_just_commands(&path)
        {
            commands.extend(just_commands);
            break;
        }
    }
    for name in [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yaml",
        "docker-compose.yml",
    ] {
        if let Some(path) = confined_manifest(root, &root.join(name))
            && let Ok(compose_commands) = load_compose_commands(&path)
        {
            commands.extend(compose_commands);
            break;
        }
    }
    commands.sort_by(|left, right| {
        left.replacement
            .cmp(&right.replacement)
            .then_with(|| left.stable_id.cmp(&right.stable_id))
    });
    commands.dedup_by(|left, right| left.replacement == right.replacement);
    commands.truncate(MAX_PROJECT_COMMANDS);
    Ok(ProjectCommandSnapshot::new(root.to_path_buf(), commands))
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedProjectCommands {
    version: u8,
    fingerprint: u64,
    snapshot: ProjectCommandSnapshot,
}

pub fn load_cached_project_command_snapshot(
    cache_dir: &Path,
    cwd: &Path,
) -> Option<ProjectCommandSnapshot> {
    let (root, _) = crate::index::find_project_root(cwd)?;
    let path = project_cache_path(cache_dir, &root);
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.file_type().is_file() || metadata.len() > 2 * 1024 * 1024 {
        return None;
    }
    let entry: CachedProjectCommands = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (entry.version == 1 && entry.snapshot.root() == root).then_some(entry.snapshot)
}

pub fn refresh_project_command_cache(cache_dir: &Path, cwd: &Path) -> Result<()> {
    let Some((root, _)) = crate::index::find_project_root(cwd) else {
        return Ok(());
    };
    let fingerprint = project_fingerprint(&root);
    let cache_path = project_cache_path(cache_dir, &root);
    if let Ok(bytes) = fs::read(&cache_path)
        && let Ok(entry) = serde_json::from_slice::<CachedProjectCommands>(&bytes)
        && entry.version == 1
        && entry.fingerprint == fingerprint
        && entry.snapshot.root() == root
    {
        if let Some(parent) = cache_path.parent() {
            prune_project_cache(parent, &cache_path);
        }
        return Ok(());
    }
    let entry = CachedProjectCommands {
        version: 1,
        fingerprint,
        snapshot: load_project_command_snapshot(&root)?,
    };
    let parent = cache_path
        .parent()
        .ok_or_else(|| DirgoError::User("invalid project command cache path".into()))?;
    fs::create_dir_all(parent).map_err(|error| DirgoError::io(parent, error))?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| DirgoError::io(parent, error))?;
    serde_json::to_writer(&mut temporary, &entry)
        .map_err(|error| DirgoError::User(format!("could not encode project cache: {error}")))?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|error| DirgoError::io(temporary.path(), error))?;
    persist_cache_file(temporary, &cache_path)?;
    prune_project_cache(parent, &cache_path);
    Ok(())
}

pub fn claim_project_command_refresh(cache_dir: &Path, cwd: &Path) -> bool {
    const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

    let Some((root, _)) = crate::index::find_project_root(cwd) else {
        return false;
    };
    let marker = project_cache_path(cache_dir, &root).with_extension("checked");
    if let Ok(metadata) = fs::symlink_metadata(&marker) {
        if metadata.file_type().is_file()
            && metadata
                .modified()
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|elapsed| elapsed < REFRESH_INTERVAL)
        {
            return false;
        }
        if fs::remove_file(&marker).is_err() {
            return false;
        }
    }
    let Some(parent) = marker.parent() else {
        return false;
    };
    if fs::create_dir_all(parent).is_err() {
        return false;
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker)
        .and_then(|mut file| file.write_all(b"refresh"))
        .is_ok()
}

fn persist_cache_file(temporary: tempfile::NamedTempFile, destination: &Path) -> Result<()> {
    let temporary = temporary.into_temp_path();
    super::settings::replace_file(&temporary, destination)
}

fn prune_project_cache(directory: &Path, keep: &Path) {
    const MAX_CACHED_PROJECTS: usize = 64;

    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut snapshots = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter(|path| path != keep)
        .map(|path| {
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (modified, path)
        })
        .collect::<Vec<_>>();
    snapshots.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let remove_count = snapshots.len().saturating_sub(MAX_CACHED_PROJECTS - 1);
    for (_, path) in snapshots.into_iter().take(remove_count) {
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("checked"));
    }
}

fn project_cache_path(cache_dir: &Path, root: &Path) -> PathBuf {
    let mut hash = 0xcbf29ce484222325_u64;
    hash_bytes(&mut hash, root.to_string_lossy().as_bytes());
    cache_dir
        .join("project-commands")
        .join(format!("{hash:016x}.json"))
}

fn project_fingerprint(root: &Path) -> u64 {
    let mut paths = vec![
        root.join("package.json"),
        root.join("pnpm-lock.yaml"),
        root.join("yarn.lock"),
        root.join("bun.lock"),
        root.join("bun.lockb"),
        root.join("package-lock.json"),
        root.join("npm-shrinkwrap.json"),
        root.join("Cargo.toml"),
        root.join("Makefile"),
        root.join("justfile"),
        root.join("Justfile"),
        root.join("compose.yaml"),
        root.join("compose.yml"),
        root.join("docker-compose.yaml"),
        root.join("docker-compose.yml"),
    ];
    if let Some(cargo_toml) = confined_manifest(root, &root.join("Cargo.toml"))
        && let Ok(bytes) = read_bounded_manifest(&cargo_toml)
        && let Ok(value) = toml::from_slice::<TomlValue>(&bytes)
    {
        paths.extend(
            value
                .get("workspace")
                .and_then(|workspace| workspace.get("members"))
                .and_then(TomlValue::as_array)
                .into_iter()
                .flatten()
                .filter_map(TomlValue::as_str)
                .flat_map(|member| expand_workspace_member(root, member))
                .take(64)
                .map(|member| member.join("Cargo.toml")),
        );
    }
    paths.sort();
    paths.dedup();
    let mut hash = 0xcbf29ce484222325_u64;
    for path in paths
        .into_iter()
        .filter_map(|path| confined_manifest(root, &path))
    {
        hash_bytes(&mut hash, path.to_string_lossy().as_bytes());
        match read_bounded_manifest(&path) {
            Ok(bytes) => hash_bytes(&mut hash, &bytes),
            Err(_) => {
                let length = fs::metadata(&path).map_or(0, |metadata| metadata.len());
                hash_bytes(&mut hash, &length.to_le_bytes());
            }
        }
    }
    hash
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}

fn load_cargo_commands(root: &Path, path: &Path) -> Result<Vec<ProjectCommand>> {
    const MAX_WORKSPACE_MEMBERS: usize = 64;

    let bytes = read_bounded_manifest(path)?;
    let value: TomlValue = toml::from_slice(&bytes)
        .map_err(|error| DirgoError::User(format!("invalid {}: {error}", path.display())))?;
    let mut commands = cargo_package_commands(&value, None);
    let members = value
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(TomlValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(TomlValue::as_str)
        .flat_map(|member| expand_workspace_member(root, member))
        .take(MAX_WORKSPACE_MEMBERS);
    for member in members {
        let manifest = member.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let Ok(bytes) = read_bounded_manifest(&manifest) else {
            continue;
        };
        let Ok(value) = toml::from_slice::<TomlValue>(&bytes) else {
            continue;
        };
        let Some(package) = cargo_package_name(&value) else {
            continue;
        };
        commands.extend(cargo_package_commands(&value, Some(package)));
    }
    Ok(commands)
}

fn load_make_commands(path: &Path) -> Result<Vec<ProjectCommand>> {
    let contents = String::from_utf8(read_bounded_manifest(path)?)
        .map_err(|_| DirgoError::User(format!("{} is not valid UTF-8", path.display())))?;
    Ok(contents
        .lines()
        .filter_map(|line| {
            if line.starts_with(char::is_whitespace) || line.starts_with('#') {
                return None;
            }
            let (target, _) = line.split_once(':')?;
            (is_portable_task_name(target)
                && !target.starts_with('.')
                && !target.contains('%')
                && !target.contains('$'))
            .then_some(target)
        })
        .take(MAX_PROJECT_COMMANDS)
        .map(|target| {
            ProjectCommand::new(
                format!("make {target}"),
                target,
                "Make target",
                format!("make:{target}"),
            )
        })
        .collect())
}

fn load_just_commands(path: &Path) -> Result<Vec<ProjectCommand>> {
    let contents = String::from_utf8(read_bounded_manifest(path)?)
        .map_err(|_| DirgoError::User(format!("{} is not valid UTF-8", path.display())))?;
    Ok(contents
        .lines()
        .filter_map(|line| {
            if line.starts_with(char::is_whitespace)
                || line.starts_with('#')
                || line.starts_with("alias ")
                || line.starts_with('[')
            {
                return None;
            }
            let (signature, body) = line.split_once(':')?;
            if body.trim_start().starts_with('=') {
                return None;
            }
            let name = signature.split_whitespace().next()?;
            (is_portable_task_name(name) && !name.starts_with('_')).then_some(name)
        })
        .take(MAX_PROJECT_COMMANDS)
        .map(|recipe| {
            ProjectCommand::new(
                format!("just {recipe}"),
                recipe,
                "Just recipe",
                format!("just:{recipe}"),
            )
        })
        .collect())
}

fn load_compose_commands(path: &Path) -> Result<Vec<ProjectCommand>> {
    let contents = String::from_utf8(read_bounded_manifest(path)?)
        .map_err(|_| DirgoError::User(format!("{} is not valid UTF-8", path.display())))?;
    let mut in_services = false;
    let mut services = Vec::new();
    for line in contents.lines() {
        let content = line.split('#').next().unwrap_or_default();
        if content.trim() == "services:" && !content.starts_with(char::is_whitespace) {
            in_services = true;
            continue;
        }
        if !in_services || content.trim().is_empty() {
            continue;
        }
        if !content.starts_with(char::is_whitespace) {
            break;
        }
        if !content.starts_with("  ") || content.starts_with("    ") || content.starts_with('\t') {
            continue;
        }
        let Some(service) = content.trim().strip_suffix(':') else {
            continue;
        };
        if is_portable_task_name(service) {
            services.push(ProjectCommand::new(
                format!("docker compose up {service}"),
                service,
                "Compose service",
                format!("compose-service:{service}"),
            ));
        }
        if services.len() == MAX_PROJECT_COMMANDS {
            break;
        }
    }
    Ok(services)
}

fn cargo_package_commands(value: &TomlValue, package: Option<&str>) -> Vec<ProjectCommand> {
    let mut commands = Vec::new();
    let package_args = package.map_or_else(String::new, |name| format!(" -p {name}"));
    if let Some(package) = package {
        commands.push(ProjectCommand::new(
            format!("cargo test{package_args}"),
            package,
            "Cargo workspace package",
            format!("cargo-package:{package}:test"),
        ));
    }
    for name in cargo_named_targets(value, "bin") {
        commands.push(ProjectCommand::new(
            format!("cargo run{package_args} --bin {name}"),
            &name,
            "Cargo binary",
            format!("cargo-bin:{}:{name}", package.unwrap_or("root")),
        ));
    }
    for name in cargo_named_targets(value, "example") {
        commands.push(ProjectCommand::new(
            format!("cargo run{package_args} --example {name}"),
            &name,
            "Cargo example",
            format!("cargo-example:{}:{name}", package.unwrap_or("root")),
        ));
    }
    if let Some(features) = value.get("features").and_then(TomlValue::as_table) {
        for name in features
            .keys()
            .filter(|name| is_portable_task_name(name))
            .take(MAX_PROJECT_COMMANDS)
        {
            commands.push(ProjectCommand::new(
                format!("cargo build{package_args} --features {name}"),
                name,
                "Cargo feature",
                format!("cargo-feature:{}:{name}", package.unwrap_or("root")),
            ));
        }
    }
    commands
}

fn cargo_package_name(value: &TomlValue) -> Option<&str> {
    value
        .get("package")?
        .get("name")?
        .as_str()
        .filter(|name| is_portable_task_name(name))
}

fn cargo_named_targets(value: &TomlValue, kind: &str) -> Vec<String> {
    value
        .get(kind)
        .and_then(TomlValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(|target| target.get("name").and_then(TomlValue::as_str))
        .filter(|name| is_portable_task_name(name))
        .take(MAX_PROJECT_COMMANDS)
        .map(str::to_owned)
        .collect()
}

fn expand_workspace_member(root: &Path, member: &str) -> Vec<PathBuf> {
    if member.is_empty() || Path::new(member).is_absolute() || member.contains("..") {
        return Vec::new();
    }
    let Ok(canonical_root) = root.canonicalize() else {
        return Vec::new();
    };
    let Some(parent) = member.strip_suffix("/*") else {
        let candidate = root.join(member);
        return candidate
            .canonicalize()
            .ok()
            .filter(|path| path.starts_with(&canonical_root))
            .into_iter()
            .collect();
    };
    let directory = root.join(parent);
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for entry in entries.filter_map(|entry| entry.ok()) {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Some(path) = entry
            .path()
            .canonicalize()
            .ok()
            .filter(|path| path.starts_with(&canonical_root))
        else {
            continue;
        };
        paths.push(path);
        if paths.len() > 64 {
            return Vec::new();
        }
    }
    paths.sort();
    paths
}

fn load_package_commands(root: &Path, path: &Path) -> Result<Vec<ProjectCommand>> {
    let bytes = read_bounded_manifest(path)?;
    let value: JsonValue = serde_json::from_slice(&bytes)
        .map_err(|error| DirgoError::User(format!("invalid {}: {error}", path.display())))?;
    let manager = package_manager(root, &value);
    let package = value
        .get("name")
        .and_then(JsonValue::as_str)
        .and_then(safe_label);
    let description = package.map_or_else(
        || "package.json script".to_owned(),
        |name| format!("package.json script · {name}"),
    );
    let Some(scripts) = value.get("scripts").and_then(JsonValue::as_object) else {
        return Ok(Vec::new());
    };
    Ok(scripts
        .keys()
        .filter(|name| is_portable_task_name(name))
        .take(MAX_PROJECT_COMMANDS)
        .map(|name| {
            ProjectCommand::new(
                format!("{manager} run {name}"),
                name,
                &description,
                format!("package-json:{manager}:{name}"),
            )
        })
        .collect())
}

fn read_bounded_manifest(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(|error| DirgoError::io(path, error))?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(DirgoError::User(format!(
            "{} exceeds the 512 KiB project manifest limit",
            path.display()
        )));
    }
    fs::read(path).map_err(|error| DirgoError::io(path, error))
}

fn confined_manifest(root: &Path, path: &Path) -> Option<PathBuf> {
    let canonical_root = root.canonicalize().ok()?;
    let canonical_path = path.canonicalize().ok()?;
    canonical_path
        .starts_with(canonical_root)
        .then_some(canonical_path)
}

fn package_manager(root: &Path, value: &JsonValue) -> &'static str {
    if let Some(manager) = value.get("packageManager").and_then(JsonValue::as_str) {
        let name = manager.split('@').next().unwrap_or(manager);
        if matches!(name, "npm" | "pnpm" | "yarn" | "bun") {
            return match name {
                "pnpm" => "pnpm",
                "yarn" => "yarn",
                "bun" => "bun",
                _ => "npm",
            };
        }
    }
    for (file, manager) in [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("bun.lock", "bun"),
        ("bun.lockb", "bun"),
        ("package-lock.json", "npm"),
        ("npm-shrinkwrap.json", "npm"),
    ] {
        if confined_manifest(root, &root.join(file)).is_some() {
            return manager;
        }
    }
    "npm"
}

fn is_portable_task_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.'))
}

fn safe_label(value: &str) -> Option<&str> {
    (!value.is_empty() && value.len() <= 80 && !value.chars().any(char::is_control))
        .then_some(value)
}

impl ProjectCommand {
    pub fn new(
        replacement: impl Into<String>,
        display: impl Into<String>,
        description: impl Into<String>,
        stable_id: impl Into<String>,
    ) -> Self {
        Self {
            replacement: replacement.into(),
            display: display.into(),
            description: description.into(),
            stable_id: stable_id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCommandSnapshot {
    root: PathBuf,
    commands: Vec<ProjectCommand>,
}

impl ProjectCommandSnapshot {
    pub fn new(root: PathBuf, commands: Vec<ProjectCommand>) -> Self {
        Self { root, commands }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn commands(&self) -> &[ProjectCommand] {
        &self.commands
    }

    pub fn contains(&self, cwd: &Path) -> bool {
        cwd.starts_with(&self.root)
    }
}
