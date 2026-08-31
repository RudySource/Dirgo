use std::{
    env, fs,
    io::{self, IsTerminal},
    path::Path,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{DirgoError, Result, paths::AppPaths};

const RELEASE_API: &str = "https://api.github.com/repos/RudySource/Dirgo/releases/latest";
#[cfg(unix)]
const UNIX_INSTALLER: &str =
    "https://github.com/RudySource/Dirgo/releases/latest/download/dirgo-installer.sh";
#[cfg(windows)]
const WINDOWS_INSTALLER: &str =
    "https://github.com/RudySource/Dirgo/releases/latest/download/dirgo-installer.ps1";
const CHECK_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    UpToDate,
    Available { latest: String },
    Unknown,
    Disabled,
    Stale { last_checked: u64 },
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    tag_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateCache {
    checked_at: u64,
    latest_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallSource {
    Homebrew,
    Cargo,
    Scoop,
    Direct,
}

pub fn set_notifications(paths: &AppPaths, enabled: bool) -> Result<i32> {
    paths.ensure_dirs()?;
    reject_symlink(&paths.update_notice_disabled_file)?;
    if enabled {
        match fs::remove_file(&paths.update_notice_disabled_file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(DirgoError::io(&paths.update_notice_disabled_file, error));
            }
        }
        println!("Dirgo update notifications enabled.");
    } else {
        crate::config_edit::atomic_write(&paths.update_notice_disabled_file, b"disabled\n")?;
        println!("Dirgo update notifications disabled.");
    }
    Ok(0)
}

pub fn print_version() -> Result<i32> {
    println!("dgo {}", env!("CARGO_PKG_VERSION"));
    if !io::stdout().is_terminal() {
        return Ok(0);
    }
    let paths = AppPaths::discover()?;
    let color = env::var_os("NO_COLOR").is_none();
    let unicode = env::var_os("DGO_NO_UNICODE").is_none();
    print!(
        "{}",
        render_version_status(&local_status(&paths), color, unicode)
    );
    Ok(0)
}

pub fn local_status(paths: &AppPaths) -> UpdateStatus {
    if env::var_os("DGO_DISABLE_UPDATE_CHECK").is_some()
        || regular_file(&paths.update_notice_disabled_file)
    {
        return UpdateStatus::Disabled;
    }
    let Some(cache) = read_cache(paths) else {
        return UpdateStatus::Unknown;
    };
    if now().saturating_sub(cache.checked_at) >= CHECK_INTERVAL_SECONDS {
        return UpdateStatus::Stale {
            last_checked: cache.checked_at,
        };
    }
    if is_newer(&cache.latest_version, env!("CARGO_PKG_VERSION")) {
        UpdateStatus::Available {
            latest: cache.latest_version,
        }
    } else {
        UpdateStatus::UpToDate
    }
}

pub fn render_version_status(status: &UpdateStatus, color: bool, unicode: bool) -> String {
    let marker = if unicode { "●" } else { "*" };
    let muted = if color { "\u{1b}[38;5;245m" } else { "" };
    let green = if color { "\u{1b}[38;5;42m" } else { "" };
    let reset = if color { "\u{1b}[0m" } else { "" };
    match status {
        UpdateStatus::Available { latest } => format!(
            "\n{green}{marker}  Update available{reset}\n{muted}   {}  {}  {latest}\n   Run `dgo --update`{reset}\n",
            env!("CARGO_PKG_VERSION"),
            if unicode { "→" } else { "->" }
        ),
        UpdateStatus::UpToDate => format!(
            "\n{green}{marker}  Dirgo is up to date{reset}\n{muted}   No action needed.{reset}\n"
        ),
        UpdateStatus::Disabled => format!(
            "\n{muted}{marker}  Update checks are off\n   Enable with `dgo update-notifications on`{reset}\n"
        ),
        UpdateStatus::Stale { .. } => format!(
            "\n{muted}{marker}  Update status is stale\n   Dirgo will refresh it in the background.{reset}\n"
        ),
        UpdateStatus::Unknown => format!(
            "\n{muted}{marker}  Update status unavailable\n   Dirgo will check quietly in the background.{reset}\n"
        ),
    }
}

pub fn notify_and_refresh_in_background(paths: &AppPaths) {
    if env::var_os("DGO_DISABLE_UPDATE_CHECK").is_some()
        || paths.update_notice_disabled_file.exists()
    {
        return;
    }

    if let Some(cache) = read_cache(paths)
        && is_newer(&cache.latest_version, env!("CARGO_PKG_VERSION"))
    {
        let color = io::stderr().is_terminal() && env::var_os("NO_COLOR").is_none();
        let unicode = env::var_os("DGO_NO_UNICODE").is_none();
        eprint!(
            "{}",
            render_update_notice(&cache.latest_version, color, unicode)
        );
    }

    if check_is_fresh(&paths.update_check_file) {
        return;
    }
    if let Err(error) = paths.ensure_dirs() {
        tracing::debug!(%error, "could not prepare update-check cache");
        return;
    }
    if let Err(error) = reject_symlink(&paths.update_check_file).and_then(|()| {
        crate::config_edit::atomic_write(&paths.update_check_file, now().to_string().as_bytes())
    }) {
        tracing::debug!(%error, "could not record update-check attempt");
        return;
    }
    if let Err(error) = spawn_background_check() {
        tracing::debug!(%error, "could not start background update check");
    }
}

pub fn refresh_cache(paths: &AppPaths) -> Result<i32> {
    let latest_version = fetch_latest_version()?;
    publish_cache(paths, latest_version, now())?;
    Ok(0)
}

fn publish_cache(paths: &AppPaths, latest_version: String, checked_at: u64) -> Result<()> {
    parse_version(&latest_version)
        .filter(|_| !latest_version.chars().any(char::is_control))
        .ok_or_else(|| DirgoError::User("update cache version is invalid".into()))?;
    paths.ensure_dirs()?;
    reject_symlink(&paths.update_cache_file)?;
    let bytes = serde_json::to_vec(&UpdateCache {
        checked_at,
        latest_version,
    })?;
    crate::config_edit::atomic_write(&paths.update_cache_file, &bytes)
}

pub fn run_update() -> Result<i32> {
    let latest = fetch_latest_version()?;
    let current = env!("CARGO_PKG_VERSION");
    if !is_newer(&latest, current) {
        println!("Dirgo {current} is already up to date.");
        return Ok(0);
    }

    let executable = env::current_exe().map_err(|error| DirgoError::io("dgo", error))?;
    println!("Updating Dirgo {current} to {latest}…");
    let status = match detect_install_source(&executable) {
        InstallSource::Homebrew => Command::new("brew")
            .args(["upgrade", "rudysource/tap/dirgo"])
            .status()
            .map(Some),
        InstallSource::Cargo => Command::new("cargo")
            .args(["install", "dirgo", "--version", &latest, "--locked"])
            .status()
            .map(Some),
        InstallSource::Scoop => Command::new("scoop")
            .args(["update", "dirgo"])
            .status()
            .map(Some),
        InstallSource::Direct => run_direct_installer(&executable),
    }
    .map_err(|error| DirgoError::io("updater", error))?;

    let Some(status) = status else {
        println!(
            "The verified update is downloading in the background and will replace dgo.exe after this process exits."
        );
        return Ok(0);
    };
    if !status.success() {
        return Err(DirgoError::User(format!(
            "update command exited with status {status}"
        )));
    }
    println!("Dirgo {latest} installed successfully.");
    Ok(0)
}

fn fetch_latest_version() -> Result<String> {
    let output = release_request().map_err(|error| DirgoError::io("release check", error))?;
    if !output.status.success() {
        return Err(DirgoError::User(format!(
            "could not check GitHub Releases (status {})",
            output.status
        )));
    }
    let release: ReleaseResponse = serde_json::from_slice(&output.stdout)?;
    let version = release.tag_name.trim_start_matches('v');
    parse_version(version).ok_or_else(|| {
        DirgoError::User(format!(
            "GitHub returned an invalid release version {:?}",
            release.tag_name
        ))
    })?;
    Ok(version.to_owned())
}

#[cfg(unix)]
fn release_request() -> std::io::Result<std::process::Output> {
    Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "-fsSL",
            "--connect-timeout",
            "2",
            "--max-time",
            "5",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            concat!("User-Agent: dirgo/", env!("CARGO_PKG_VERSION")),
            RELEASE_API,
        ])
        .output()
}

#[cfg(windows)]
fn release_request() -> std::io::Result<std::process::Output> {
    Command::new("powershell")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "(Invoke-RestMethod -Headers @{{Accept='application/vnd.github+json';'User-Agent'='dirgo/{}'}} -TimeoutSec 5 -Uri '{}') | ConvertTo-Json -Compress",
                env!("CARGO_PKG_VERSION"),
                RELEASE_API
            ),
        ])
        .output()
}

fn read_cache(paths: &AppPaths) -> Option<UpdateCache> {
    let metadata = fs::symlink_metadata(&paths.update_cache_file).ok()?;
    if !metadata.file_type().is_file() || metadata.len() > 4 * 1024 {
        return None;
    }
    let raw = fs::read(&paths.update_cache_file).ok()?;
    let cache: UpdateCache = serde_json::from_slice(&raw).ok()?;
    parse_version(&cache.latest_version)?;
    (!cache.latest_version.chars().any(char::is_control)).then_some(cache)
}

fn regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

fn check_is_fresh(path: &Path) -> bool {
    if !regular_file(path) {
        return false;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .is_some_and(|checked_at| now().saturating_sub(checked_at) < CHECK_INTERVAL_SECONDS)
}

fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(DirgoError::User(format!(
            "refusing symlink update-state path: {}",
            path.display()
        ))),
        Ok(metadata) if !metadata.file_type().is_file() => Err(DirgoError::User(format!(
            "update-state path is not a regular file: {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DirgoError::io(path, error)),
    }
}

fn render_update_notice(latest: &str, color: bool, unicode: bool) -> String {
    let marker = if unicode { "●" } else { "*" };
    let green = if color { "\u{1b}[38;5;42m" } else { "" };
    let muted = if color { "\u{1b}[38;5;245m" } else { "" };
    let reset = if color { "\u{1b}[0m" } else { "" };
    format!(
        "{green}{marker}  Dirgo {latest} is ready{reset}\n{muted}   You have {}  ·  `dgo --update`\n   Hide this with `dgo update-notifications off`{reset}\n",
        env!("CARGO_PKG_VERSION")
    )
}

fn spawn_background_check() -> std::io::Result<()> {
    let executable = env::current_exe()?;
    let mut command = Command::new(executable);
    command
        .arg("__check-update")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn().map(|_| ())
}

fn detect_install_source(executable: &Path) -> InstallSource {
    let normalized = executable
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    if normalized.contains("/cellar/")
        || normalized.starts_with("/opt/homebrew/")
        || normalized.starts_with("/usr/local/homebrew/")
    {
        InstallSource::Homebrew
    } else if normalized.contains("/.cargo/bin/") {
        InstallSource::Cargo
    } else if normalized.contains("/scoop/apps/") {
        InstallSource::Scoop
    } else {
        InstallSource::Direct
    }
}

#[cfg(unix)]
fn run_direct_installer(executable: &Path) -> std::io::Result<Option<std::process::ExitStatus>> {
    let temporary = tempfile::Builder::new().prefix("dirgo-update-").tempdir()?;
    let installer = temporary.path().join("dirgo-installer.sh");
    let download = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "-fsSL",
            UNIX_INSTALLER,
            "-o",
        ])
        .arg(&installer)
        .status()?;
    if !download.success() {
        return Ok(Some(download));
    }
    let install_dir = executable.parent().unwrap_or_else(|| Path::new("."));
    Command::new("sh")
        .arg(&installer)
        .args(["--yes", "--no-setup"])
        .env("DIRGO_INSTALL_DIR", install_dir)
        .status()
        .map(Some)
}

#[cfg(windows)]
fn run_direct_installer(executable: &Path) -> std::io::Result<Option<std::process::ExitStatus>> {
    use std::os::windows::process::CommandExt;

    let install_dir = executable.parent().unwrap_or_else(|| Path::new("."));
    let escaped = install_dir.to_string_lossy().replace('\'', "''");
    let parent_pid = std::process::id();
    let script = format!(
        "Wait-Process -Id {parent_pid} -ErrorAction SilentlyContinue; $p=Join-Path $env:TEMP ('dirgo-update-'+[guid]::NewGuid().ToString('N')+'.ps1'); Invoke-WebRequest -UseBasicParsing -Uri '{WINDOWS_INSTALLER}' -OutFile $p; $env:DIRGO_INSTALL_DIR='{escaped}'; try {{ & $p }} finally {{ Remove-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue }}"
    );
    let mut command = Command::new("powershell");
    command
        .args(["-NoLogo", "-NoProfile", "-Command", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x08000000)
        .spawn()
        .map(|_| None)
}

fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version
        .trim_start_matches('v')
        .split('-')
        .next()?
        .split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(parsed)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_versions_numerically() {
        assert!(is_newer("0.3.10", "0.3.9"));
        assert!(is_newer("v1.0.0", "0.9.99"));
        assert!(!is_newer("0.3.1", "0.3.1"));
        assert!(!is_newer("invalid", "0.3.1"));
    }

    #[test]
    fn recognizes_supported_installation_sources() {
        assert_eq!(
            detect_install_source(Path::new("/opt/homebrew/Cellar/dirgo/0.3.1/bin/dgo")),
            InstallSource::Homebrew
        );
        assert_eq!(
            detect_install_source(Path::new("/home/me/.cargo/bin/dgo")),
            InstallSource::Cargo
        );
        assert_eq!(
            detect_install_source(Path::new(
                "C:\\Users\\me\\scoop\\apps\\dirgo\\current\\dgo.exe"
            )),
            InstallSource::Scoop
        );
        assert_eq!(
            detect_install_source(Path::new("/home/me/.local/bin/dgo")),
            InstallSource::Direct
        );
    }

    #[cfg(unix)]
    #[test]
    fn cache_publication_is_private_atomic_and_refuses_symlinks() {
        use std::os::unix::{fs::PermissionsExt, fs::symlink};

        let temp = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths {
            config_file: temp.path().join("config.toml"),
            cache_dir: temp.path().join("cache"),
            state_dir: temp.path().join("state"),
            index_file: temp.path().join("cache/index.redb"),
            state_file: temp.path().join("state/state.redb"),
            suggestions_state_file: temp.path().join("state/suggestions.redb"),
            update_cache_file: temp.path().join("cache/update.json"),
            update_check_file: temp.path().join("cache/update-check"),
            update_notice_disabled_file: temp.path().join("state/disabled"),
        };
        publish_cache(&paths, "9.9.9".into(), 42).expect("publish cache");
        let mode = fs::metadata(&paths.update_cache_file)
            .expect("cache metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(read_cache(&paths).expect("cache").latest_version, "9.9.9");

        fs::remove_file(&paths.update_cache_file).expect("remove cache");
        let victim = temp.path().join("victim");
        fs::write(&victim, "keep me").expect("victim");
        symlink(&victim, &paths.update_cache_file).expect("cache symlink");
        assert!(publish_cache(&paths, "10.0.0".into(), 43).is_err());
        assert_eq!(fs::read_to_string(victim).expect("victim bytes"), "keep me");
    }
}
