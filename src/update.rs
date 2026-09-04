use std::{
    env, fmt, fs,
    io::{self, IsTerminal, Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use fs2::FileExt;
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
const ATTEMPT_LEASE_SECONDS: u64 = 5 * 60;
const FETCH_BACKOFF_SECONDS: u64 = 15 * 60;
const SPAWN_BACKOFF_SECONDS: u64 = 60;
const UPDATE_STATE_MAX_BYTES: u64 = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StableVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl StableVersion {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for StableVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheFreshness {
    Fresh,
    Stale,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionRelation {
    UpdateAvailable { latest: StableVersion },
    Current { latest: StableVersion },
    AheadOfLatest { latest: StableVersion },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshDisposition {
    NotDue,
    Started,
    AlreadyRunning,
    BackingOff { retry_at: u64 },
    Disabled,
    StartFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateView {
    pub relation: VersionRelation,
    pub freshness: CacheFreshness,
    pub last_success_at: Option<u64>,
    pub refresh: RefreshDisposition,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
enum AttemptState {
    Running { started_at: u64 },
    BackingOff { retry_at: u64, category: String },
    Completed { completed_at: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationSetting {
    Enabled,
    Disabled,
    Invalid,
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
    let dumb_terminal = env::var("TERM").is_ok_and(|term| term.eq_ignore_ascii_case("dumb"));
    let color = env::var_os("NO_COLOR").is_none() && !dumb_terminal;
    let unicode = env::var_os("DGO_NO_UNICODE").is_none() && !dumb_terminal;
    let timestamp = now();
    let mut view = local_view_at(&paths, timestamp);
    view.refresh = schedule_refresh_at(&paths, timestamp, spawn_background_check);
    print!("{}", render_version_status(&view, color, unicode));
    Ok(0)
}

pub fn local_view(paths: &AppPaths) -> UpdateView {
    local_view_at(paths, now())
}

pub fn local_view_at(paths: &AppPaths, timestamp: u64) -> UpdateView {
    let setting = notification_setting(paths);
    let Some(cache) = read_cache(paths) else {
        return UpdateView {
            relation: VersionRelation::Unknown,
            freshness: CacheFreshness::Missing,
            last_success_at: None,
            refresh: match setting {
                NotificationSetting::Disabled => RefreshDisposition::Disabled,
                NotificationSetting::Enabled => RefreshDisposition::NotDue,
                NotificationSetting::Invalid => RefreshDisposition::StartFailed,
            },
        };
    };
    let latest = parse_version(&cache.latest_version).expect("validated update cache");
    let current = parse_version(env!("CARGO_PKG_VERSION")).expect("valid package version");
    let relation = match current.cmp(&latest) {
        std::cmp::Ordering::Less => VersionRelation::UpdateAvailable { latest },
        std::cmp::Ordering::Equal => VersionRelation::Current { latest },
        std::cmp::Ordering::Greater => VersionRelation::AheadOfLatest { latest },
    };
    let freshness =
        if timestamp >= cache.checked_at && timestamp - cache.checked_at < CHECK_INTERVAL_SECONDS {
            CacheFreshness::Fresh
        } else {
            CacheFreshness::Stale
        };
    UpdateView {
        relation,
        freshness,
        last_success_at: Some(cache.checked_at),
        refresh: match setting {
            NotificationSetting::Disabled => RefreshDisposition::Disabled,
            NotificationSetting::Enabled => RefreshDisposition::NotDue,
            NotificationSetting::Invalid => RefreshDisposition::StartFailed,
        },
    }
}

pub fn render_version_status(view: &UpdateView, color: bool, unicode: bool) -> String {
    let marker = if unicode { "●" } else { "*" };
    let muted = if color { "\u{1b}[38;5;245m" } else { "" };
    let green = if color { "\u{1b}[38;5;42m" } else { "" };
    let reset = if color { "\u{1b}[0m" } else { "" };
    if matches!(view.refresh, RefreshDisposition::Disabled) {
        return format!(
            "\n{muted}{marker}  Update checks are off\n   Enable with `dgo update-notifications on`{reset}\n"
        );
    }
    let checking = matches!(
        view.refresh,
        RefreshDisposition::Started | RefreshDisposition::AlreadyRunning
    );
    match &view.relation {
        VersionRelation::UpdateAvailable { latest } => {
            let detail = match view.freshness {
                CacheFreshness::Fresh => format!(
                    "{}  {}  {latest}",
                    env!("CARGO_PKG_VERSION"),
                    if unicode { "→" } else { "->" }
                ),
                CacheFreshness::Stale if checking => "Cached result · checking again".into(),
                CacheFreshness::Stale => "Cached result · will retry later".into(),
                CacheFreshness::Missing => "Update status unavailable".into(),
            };
            format!(
                "\n{green}{marker}  Update {latest} available{reset}\n{muted}   {detail}\n   Run `dgo --update`{reset}\n"
            )
        }
        VersionRelation::Current { latest } if view.freshness == CacheFreshness::Fresh => {
            format!(
                "\n{green}{marker}  Dirgo is up to date{reset}\n{muted}   Stable {latest} confirmed.{reset}\n"
            )
        }
        VersionRelation::Current { latest } | VersionRelation::AheadOfLatest { latest }
            if checking =>
        {
            format!(
                "\n{muted}{marker}  Checking for updates\n   Last known stable: {latest}{reset}\n"
            )
        }
        VersionRelation::AheadOfLatest { latest } => format!(
            "\n{muted}{marker}  Running ahead of latest stable\n   Last known stable: {latest}{reset}\n"
        ),
        VersionRelation::Current { latest } => format!(
            "\n{muted}{marker}  Update status is cached\n   Last known stable: {latest} · will retry later{reset}\n"
        ),
        VersionRelation::Unknown if checking => format!(
            "\n{muted}{marker}  Checking for updates\n   Running quietly in the background.{reset}\n"
        ),
        VersionRelation::Unknown => format!(
            "\n{muted}{marker}  Update status unavailable\n   Dirgo will retry on a later command.{reset}\n"
        ),
    }
}

pub fn notify_and_refresh_in_background(paths: &AppPaths) {
    let timestamp = now();
    let view = local_view_at(paths, timestamp);
    if matches!(view.refresh, RefreshDisposition::Disabled) {
        return;
    }

    if let VersionRelation::UpdateAvailable { latest } = view.relation {
        let dumb_terminal = env::var("TERM").is_ok_and(|term| term.eq_ignore_ascii_case("dumb"));
        let color =
            io::stderr().is_terminal() && env::var_os("NO_COLOR").is_none() && !dumb_terminal;
        let unicode = env::var_os("DGO_NO_UNICODE").is_none() && !dumb_terminal;
        eprint!(
            "{}",
            render_update_notice(&latest.to_string(), color, unicode)
        );
    }

    let disposition = schedule_refresh_at(paths, timestamp, spawn_background_check);
    tracing::debug!(?disposition, "background update refresh disposition");
}

pub fn refresh_cache(paths: &AppPaths) -> Result<i32> {
    refresh_cache_at(paths, now(), fetch_latest_version)
}

fn refresh_cache_at(
    paths: &AppPaths,
    timestamp: u64,
    fetcher: impl FnOnce() -> Result<String>,
) -> Result<i32> {
    match fetcher().and_then(|latest| publish_cache(paths, latest, timestamp)) {
        Ok(()) => {
            complete_attempt(
                paths,
                AttemptState::Completed {
                    completed_at: timestamp,
                },
            )?;
            Ok(0)
        }
        Err(error) => {
            let _ = complete_attempt(
                paths,
                AttemptState::BackingOff {
                    retry_at: timestamp.saturating_add(FETCH_BACKOFF_SECONDS),
                    category: "fetch-failed".into(),
                },
            );
            Err(error)
        }
    }
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
    let version = release.tag_name.strip_prefix('v').ok_or_else(|| {
        DirgoError::User(format!(
            "GitHub returned an invalid release version {:?}",
            release.tag_name
        ))
    })?;
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
    if !metadata.file_type().is_file() || metadata.len() > UPDATE_STATE_MAX_BYTES {
        return None;
    }
    let raw = fs::read(&paths.update_cache_file).ok()?;
    let cache: UpdateCache = serde_json::from_slice(&raw).ok()?;
    parse_version(&cache.latest_version)?;
    (!cache.latest_version.chars().any(char::is_control)).then_some(cache)
}

fn notification_setting(paths: &AppPaths) -> NotificationSetting {
    if env::var_os("DGO_DISABLE_UPDATE_CHECK").is_some() {
        return NotificationSetting::Disabled;
    }
    match fs::symlink_metadata(&paths.update_notice_disabled_file) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => NotificationSetting::Enabled,
        Err(_) => NotificationSetting::Invalid,
        Ok(metadata)
            if metadata.file_type().is_file()
                && metadata.len() <= 64
                && fs::read(&paths.update_notice_disabled_file)
                    .is_ok_and(|bytes| bytes == b"disabled\n") =>
        {
            NotificationSetting::Disabled
        }
        Ok(_) => NotificationSetting::Invalid,
    }
}

fn schedule_refresh_at(
    paths: &AppPaths,
    timestamp: u64,
    launcher: impl FnOnce() -> io::Result<()>,
) -> RefreshDisposition {
    match notification_setting(paths) {
        NotificationSetting::Disabled => return RefreshDisposition::Disabled,
        NotificationSetting::Invalid => return RefreshDisposition::StartFailed,
        NotificationSetting::Enabled => {}
    }
    if read_cache(paths).is_some_and(|cache| {
        timestamp >= cache.checked_at && timestamp - cache.checked_at < CHECK_INTERVAL_SECONDS
    }) {
        return RefreshDisposition::NotDue;
    }
    if let Err(error) = paths.ensure_dirs() {
        tracing::debug!(%error, "could not prepare update-check state");
        return RefreshDisposition::StartFailed;
    }
    let Ok(mut state_file) = open_attempt_state(paths) else {
        return RefreshDisposition::StartFailed;
    };
    if let Err(error) = state_file.try_lock_exclusive() {
        return if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() {
            RefreshDisposition::AlreadyRunning
        } else {
            tracing::debug!(%error, "could not lock update-check state");
            RefreshDisposition::StartFailed
        };
    }
    match read_attempt_state(&mut state_file) {
        Some(AttemptState::Running { started_at })
            if timestamp >= started_at && timestamp - started_at < ATTEMPT_LEASE_SECONDS =>
        {
            return RefreshDisposition::AlreadyRunning;
        }
        Some(AttemptState::BackingOff { retry_at, .. })
            if retry_at > timestamp && retry_at - timestamp <= FETCH_BACKOFF_SECONDS =>
        {
            return RefreshDisposition::BackingOff { retry_at };
        }
        _ => {}
    }
    if write_attempt_state(
        &mut state_file,
        &AttemptState::Running {
            started_at: timestamp,
        },
    )
    .is_err()
    {
        return RefreshDisposition::StartFailed;
    }
    match launcher() {
        Ok(()) => RefreshDisposition::Started,
        Err(error) => {
            tracing::debug!(%error, "could not start background update check");
            let _ = write_attempt_state(
                &mut state_file,
                &AttemptState::BackingOff {
                    retry_at: timestamp.saturating_add(SPAWN_BACKOFF_SECONDS),
                    category: "spawn-failed".into(),
                },
            );
            RefreshDisposition::StartFailed
        }
    }
}

fn open_attempt_state(paths: &AppPaths) -> Result<fs::File> {
    reject_symlink(&paths.update_check_file)?;
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options
        .open(&paths.update_check_file)
        .map_err(|error| DirgoError::io(&paths.update_check_file, error))?;
    if file
        .metadata()
        .is_ok_and(|metadata| metadata.len() > UPDATE_STATE_MAX_BYTES)
    {
        return Err(DirgoError::User("update attempt state is too large".into()));
    }
    Ok(file)
}

fn read_attempt_state(file: &mut fs::File) -> Option<AttemptState> {
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut bytes = Vec::new();
    file.take(UPDATE_STATE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= UPDATE_STATE_MAX_BYTES)
        .then(|| serde_json::from_slice(&bytes).ok())
        .flatten()
}

fn write_attempt_state(file: &mut fs::File, state: &AttemptState) -> io::Result<()> {
    let bytes = serde_json::to_vec(state).map_err(io::Error::other)?;
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    file.write_all(&bytes)?;
    file.sync_data()
}

fn complete_attempt(paths: &AppPaths, state: AttemptState) -> Result<()> {
    paths.ensure_dirs()?;
    let mut file = open_attempt_state(paths)?;
    FileExt::lock_exclusive(&file)
        .map_err(|error| DirgoError::io(&paths.update_check_file, error))?;
    write_attempt_state(&mut file, &state)
        .map_err(|error| DirgoError::io(&paths.update_check_file, error))
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

fn parse_version(version: &str) -> Option<StableVersion> {
    let mut parts = version.strip_prefix('v').unwrap_or(version).split('.');
    let parse_part = |part: &str| {
        (!part.is_empty()
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && (part == "0" || !part.starts_with('0')))
        .then(|| part.parse().ok())
        .flatten()
    };
    let parsed = StableVersion::new(
        parse_part(parts.next()?)?,
        parse_part(parts.next()?)?,
        parse_part(parts.next()?)?,
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

    fn test_paths(temp: &tempfile::TempDir) -> AppPaths {
        AppPaths {
            config_file: temp.path().join("config.toml"),
            cache_dir: temp.path().join("cache"),
            state_dir: temp.path().join("state"),
            index_file: temp.path().join("cache/index.redb"),
            state_file: temp.path().join("state/state.redb"),
            suggestions_state_file: temp.path().join("state/suggestions.redb"),
            update_cache_file: temp.path().join("cache/update.json"),
            update_check_file: temp.path().join("cache/update-check"),
            update_notice_disabled_file: temp.path().join("state/disabled"),
        }
    }

    #[test]
    fn compares_release_versions_numerically() {
        assert!(is_newer("0.3.10", "0.3.9"));
        assert!(is_newer("v1.0.0", "0.9.99"));
        assert!(!is_newer("0.3.1", "0.3.1"));
        assert!(!is_newer("invalid", "0.3.1"));
        assert!(parse_version("1.2.3-alpha").is_none());
        assert!(parse_version("1.2.3.4").is_none());
        assert!(parse_version("01.2.3").is_none());
        assert!(parse_version("vv1.2.3").is_none());
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

    #[test]
    fn scheduler_claims_once_and_observes_the_active_lease() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&temp);
        let thread_paths = paths.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first = std::thread::spawn(move || {
            schedule_refresh_at(&thread_paths, 1_000, || {
                started_tx.send(()).expect("signal launch");
                release_rx.recv().expect("release launch");
                Ok(())
            })
        });
        started_rx.recv().expect("first launch started");
        assert_eq!(
            schedule_refresh_at(&paths, 1_000, || panic!("second launcher must not run")),
            RefreshDisposition::AlreadyRunning
        );
        release_tx.send(()).expect("finish first launch");
        assert_eq!(
            first.join().expect("first scheduler"),
            RefreshDisposition::Started
        );
        assert_eq!(
            schedule_refresh_at(&paths, 1_001, || panic!("active lease must not launch")),
            RefreshDisposition::AlreadyRunning
        );
    }

    #[test]
    fn scheduler_recovers_from_malformed_expired_and_future_attempt_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&temp);
        paths.ensure_dirs().expect("state dirs");

        fs::write(&paths.update_check_file, "not json").expect("malformed state");
        assert_eq!(
            schedule_refresh_at(&paths, 2_000, || Ok(())),
            RefreshDisposition::Started
        );

        fs::write(
            &paths.update_check_file,
            serde_json::to_vec(&AttemptState::Running { started_at: 1 }).expect("state"),
        )
        .expect("expired state");
        assert_eq!(
            schedule_refresh_at(&paths, 2_000, || Ok(())),
            RefreshDisposition::Started
        );

        fs::write(
            &paths.update_check_file,
            serde_json::to_vec(&AttemptState::Running {
                started_at: u64::MAX,
            })
            .expect("state"),
        )
        .expect("future state");
        assert_eq!(
            schedule_refresh_at(&paths, 2_000, || Ok(())),
            RefreshDisposition::Started
        );
    }

    #[test]
    fn spawn_and_fetch_failures_use_short_bounded_backoff() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&temp);

        assert_eq!(
            schedule_refresh_at(&paths, 10_000, || Err(io::Error::other("fixture failure"))),
            RefreshDisposition::StartFailed
        );
        assert_eq!(
            schedule_refresh_at(&paths, 10_001, || Ok(())),
            RefreshDisposition::BackingOff { retry_at: 10_060 }
        );
        assert_eq!(
            schedule_refresh_at(&paths, 10_061, || Ok(())),
            RefreshDisposition::Started
        );

        let error = refresh_cache_at(&paths, 20_000, || {
            Err(DirgoError::User("offline fixture".into()))
        })
        .expect_err("fetch failure");
        assert!(error.to_string().contains("offline fixture"));
        assert_eq!(
            schedule_refresh_at(&paths, 20_001, || Ok(())),
            RefreshDisposition::BackingOff { retry_at: 20_900 }
        );
        assert_eq!(
            schedule_refresh_at(&paths, 20_901, || Ok(())),
            RefreshDisposition::Started
        );
    }

    #[test]
    fn successful_child_publishes_fresh_cache_and_completes_attempt() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&temp);
        assert_eq!(
            schedule_refresh_at(&paths, 30_000, || Ok(())),
            RefreshDisposition::Started
        );

        refresh_cache_at(&paths, 30_001, || Ok("9.9.9".into())).expect("refresh success");
        let view = local_view_at(&paths, 30_002);
        assert_eq!(view.freshness, CacheFreshness::Fresh);
        assert!(matches!(
            view.relation,
            VersionRelation::UpdateAvailable { .. }
        ));
        assert_eq!(
            schedule_refresh_at(&paths, 30_002, || panic!("fresh cache must not launch")),
            RefreshDisposition::NotDue
        );
        let mut state = fs::File::open(&paths.update_check_file).expect("attempt state");
        assert!(matches!(
            read_attempt_state(&mut state),
            Some(AttemptState::Completed {
                completed_at: 30_001
            })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn scheduler_refuses_directory_and_symlink_state_paths() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let temp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&temp);
        paths.ensure_dirs().expect("state dirs");
        fs::create_dir(&paths.update_check_file).expect("directory state");
        assert_eq!(
            schedule_refresh_at(&paths, 1_000, || panic!("unsafe state must not launch")),
            RefreshDisposition::StartFailed
        );

        fs::remove_dir(&paths.update_check_file).expect("remove directory state");
        let victim = temp.path().join("victim");
        fs::write(&victim, "keep me").expect("victim");
        symlink(&victim, &paths.update_check_file).expect("state symlink");
        assert_eq!(
            schedule_refresh_at(&paths, 1_000, || panic!("symlink state must not launch")),
            RefreshDisposition::StartFailed
        );
        assert_eq!(
            fs::read_to_string(&victim).expect("victim bytes"),
            "keep me"
        );

        fs::remove_file(&paths.update_check_file).expect("remove symlink");
        assert_eq!(
            schedule_refresh_at(&paths, 1_000, || Ok(())),
            RefreshDisposition::Started
        );
        let mode = fs::metadata(&paths.update_check_file)
            .expect("state metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn cache_publication_is_private_atomic_and_refuses_symlinks() {
        use std::os::unix::{fs::PermissionsExt, fs::symlink};

        let temp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&temp);
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
