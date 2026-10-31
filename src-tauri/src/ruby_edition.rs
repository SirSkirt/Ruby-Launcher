//! Minecraft: Ruby Edition — release discovery, per-channel installs and launching.
//!
//! Ruby Edition publishes one GitHub release per successful build of `main` (Stable) or
//! `nightly` (Nightly), tagged `ruby-edition-<branch>-<run>-<sha8>`, each with a zip per platform
//! and a `SHA256SUMS` file. Every channel lives in its own folder so installing or updating one
//! never touches the other:
//!
//! ```text
//! <app data>/games/ruby-edition/
//!   releases.json            last release check (works offline)
//!   shared/minecraft/        the player's vanilla client + server jar, used only as prepare input
//!   stable/  bin/ game/ home/ downloads/ install.json stats.json
//!   nightly/ bin/ game/ home/ downloads/ install.json stats.json
//! ```
//!
//! `bin/` is replaced atomically on install/update; `game/` (worlds, options) and `home/` (logs,
//! crash reports) are never touched by updates.

use super::*;
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;

const RELEASES_URL: &str = "https://api.github.com/repos/SirSkirt/ruby-edtion/releases?per_page=60";
const RELEASES_PAGE: &str = "https://github.com/SirSkirt/ruby-edtion/releases";
const RELEASE_CACHE_FILE: &str = "releases.json";
const INSTALL_FILE: &str = "install.json";
const STATS_FILE: &str = "stats.json";
const PROGRESS_EVENT: &str = "ruby-edition-progress";

/// (channel id, display name, git branch)
const CHANNELS: [(&str, &str, &str); 2] = [("stable", "Stable", "main"), ("nightly", "Nightly", "nightly")];

fn channel_meta(channel: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    CHANNELS
        .iter()
        .copied()
        .find(|(id, _, _)| *id == channel)
        .ok_or_else(|| format!("Unknown Ruby Edition channel '{channel}'."))
}

fn running_key(channel: &str) -> String {
    format!("ruby-edition:{channel}")
}

/// The release asset label for this machine, matching the packaging workflow's matrix.
fn platform_asset_label() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("windows-x64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        _ => None,
    }
}

fn exe_name(base: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

// ---------------------------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RubyRelease {
    tag: String,
    run: u64,
    commit: String,
    title: String,
    published_at: String,
    html_url: String,
    asset_name: String,
    asset_url: String,
    asset_size: u64,
    checksums_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ReleaseCache {
    checked_at: u64,
    stable: Option<RubyRelease>,
    nightly: Option<RubyRelease>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RubyInstall {
    tag: String,
    run: u64,
    commit: String,
    published_at: String,
    installed_at: u64,
    minecraft_version: String,
    /// Relative to the channel folder.
    resources_dir: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PlayStats {
    last_played: u64,
    total_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RubyChannelStatus {
    channel: &'static str,
    name: &'static str,
    branch: &'static str,
    latest: Option<RubyRelease>,
    installed: Option<RubyInstall>,
    update_available: bool,
    installing: bool,
    running: bool,
    last_played: u64,
    total_seconds: u64,
    install_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RubyEditionStatus {
    channels: Vec<RubyChannelStatus>,
    checked_at: u64,
    check_error: Option<String>,
    platform_label: Option<&'static str>,
    releases_page: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RubyProgress {
    channel: String,
    /// download | verify | extract | prepare | done | failed | cancelled
    step: &'static str,
    step_index: u8,
    message: String,
    downloaded_bytes: u64,
    total_bytes: u64,
    bytes_per_second: u64,
    /// Overall install progress, 0–100.
    percent: f64,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

// ---------------------------------------------------------------------------------------------
// Paths and small helpers
// ---------------------------------------------------------------------------------------------

fn edition_root(app: &AppHandle) -> Result<PathBuf, String> {
    let root = launcher_root(app)?.join("games").join("ruby-edition");
    fs::create_dir_all(&root).map_err(|error| format!("Could not create {}: {error}", root.display()))?;
    Ok(root)
}

fn channel_dir(app: &AppHandle, channel: &str) -> Result<PathBuf, String> {
    channel_meta(channel)?;
    Ok(edition_root(app)?.join(channel))
}

fn read_or_default<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> T {
    if path.exists() {
        read_json(path).unwrap_or_default()
    } else {
        T::default()
    }
}

fn read_install(dir: &Path) -> Option<RubyInstall> {
    let path = dir.join(INSTALL_FILE);
    if !path.exists() {
        return None;
    }
    let install: RubyInstall = read_json(&path).ok()?;
    // An install whose game files went missing is treated as not installed.
    dir.join("bin").join(exe_name("horizon-client")).is_file().then_some(install)
}

fn launcher_settings(app: &AppHandle) -> LauncherSettings {
    path_for(app, SETTINGS_FILE)
        .ok()
        .filter(|path| path.exists())
        .and_then(|path| read_json(&path).ok())
        .unwrap_or_default()
}

fn active_installs() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static INSTALLS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    INSTALLS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn is_installing(channel: &str) -> bool {
    active_installs()
        .lock()
        .map(|installs| installs.contains_key(channel))
        .unwrap_or(false)
}

fn is_running(state: &AppState, channel: &str) -> bool {
    state
        .running_instances
        .lock()
        .map(|running| running.contains(&running_key(channel)))
        .unwrap_or(false)
}

fn emit(app: &AppHandle, progress: RubyProgress) {
    let _ = app.emit(PROGRESS_EVENT, progress);
}

fn step_progress(channel: &str, step: &'static str, step_index: u8, message: impl Into<String>, percent: f64) -> RubyProgress {
    RubyProgress {
        channel: channel.to_string(),
        step,
        step_index,
        message: message.into(),
        downloaded_bytes: 0,
        total_bytes: 0,
        bytes_per_second: 0,
        percent,
    }
}

/// Parses `ruby-edition-<branch>-<run>-<sha8>`.
fn parse_tag(tag: &str, branch: &str) -> Option<(u64, String)> {
    let rest = tag.strip_prefix(&format!("ruby-edition-{branch}-"))?;
    let (run, sha) = rest.split_once('-')?;
    Some((run.parse().ok()?, sha.to_string()))
}

fn latest_for_branch(releases: &[GhRelease], branch: &str, label: &str) -> Option<RubyRelease> {
    let asset_name = format!("ruby-edition-{label}.zip");
    releases
        .iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let (run, commit) = parse_tag(&release.tag_name, branch)?;
            let asset = release.assets.iter().find(|asset| asset.name == asset_name)?;
            let checksums = release.assets.iter().find(|asset| asset.name == "SHA256SUMS");
            Some(RubyRelease {
                tag: release.tag_name.clone(),
                run,
                commit,
                title: release.name.clone().unwrap_or_else(|| release.tag_name.clone()),
                published_at: release.published_at.clone().unwrap_or_default(),
                html_url: release.html_url.clone(),
                asset_name: asset.name.clone(),
                asset_url: asset.browser_download_url.clone(),
                asset_size: asset.size,
                checksums_url: checksums.map(|asset| asset.browser_download_url.clone()),
            })
        })
        .max_by_key(|release| release.run)
}

fn build_status(app: &AppHandle, state: &AppState) -> Result<RubyEditionStatus, String> {
    let root = edition_root(app)?;
    let cache: ReleaseCache = read_or_default(&root.join(RELEASE_CACHE_FILE));
    let mut channels = Vec::new();
    for (id, name, branch) in CHANNELS {
        let dir = root.join(id);
        let latest = if id == "stable" { cache.stable.clone() } else { cache.nightly.clone() };
        let installed = read_install(&dir);
        let stats: PlayStats = read_or_default(&dir.join(STATS_FILE));
        let update_available = match (&latest, &installed) {
            (Some(latest), Some(installed)) => latest.run > installed.run,
            _ => false,
        };
        channels.push(RubyChannelStatus {
            channel: id,
            name,
            branch,
            latest,
            installed,
            update_available,
            installing: is_installing(id),
            running: is_running(state, id),
            last_played: stats.last_played,
            total_seconds: stats.total_seconds,
            install_dir: dir.to_string_lossy().into_owned(),
        });
    }
    Ok(RubyEditionStatus {
        channels,
        checked_at: cache.checked_at,
        check_error: cache.error,
        platform_label: platform_asset_label(),
        releases_page: RELEASES_PAGE,
    })
}

// ---------------------------------------------------------------------------------------------
// Commands: status
// ---------------------------------------------------------------------------------------------

/// Local state only (no network): installs, cached release info, running/installing flags.
#[tauri::command]
pub(crate) fn ruby_edition_status(app: AppHandle, state: State<'_, AppState>) -> Result<RubyEditionStatus, String> {
    build_status(&app, state.inner())
}

/// Asks GitHub for the newest Stable and Nightly builds. Failures are stored and reported, but
/// the previously known releases are kept so the launcher still works offline.
#[tauri::command]
pub(crate) async fn ruby_edition_refresh(app: AppHandle, state: State<'_, AppState>) -> Result<RubyEditionStatus, String> {
    let root = edition_root(&app)?;
    let cache_path = root.join(RELEASE_CACHE_FILE);
    let mut cache: ReleaseCache = read_or_default(&cache_path);
    let result: Result<Vec<GhRelease>, String> = async {
        let response = state
            .client
            .get(RELEASES_URL)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|error| format!("Could not reach GitHub: {error}"))?;
        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err("GitHub's hourly limit for release checks was reached. Try again later.".to_string());
        }
        if !status.is_success() {
            return Err(format!("GitHub returned {status} for the Ruby Edition release list."));
        }
        response
            .json::<Vec<GhRelease>>()
            .await
            .map_err(|error| format!("Could not read the Ruby Edition release list: {error}"))
    }
    .await;

    match (result, platform_asset_label()) {
        (Ok(releases), Some(label)) => {
            cache.stable = latest_for_branch(&releases, "main", label);
            cache.nightly = latest_for_branch(&releases, "nightly", label);
            cache.error = None;
            append_log(
                &app,
                &format!(
                    "Ruby Edition release check: stable={}, nightly={}.",
                    cache.stable.as_ref().map(|r| r.tag.as_str()).unwrap_or("none"),
                    cache.nightly.as_ref().map(|r| r.tag.as_str()).unwrap_or("none"),
                ),
            );
        }
        (Ok(_), None) => {
            cache.stable = None;
            cache.nightly = None;
            cache.error = Some(format!(
                "Ruby Edition builds are published for Windows x64 and Linux x86-64; this device is {} {}.",
                std::env::consts::OS,
                std::env::consts::ARCH
            ));
        }
        (Err(error), _) => {
            append_log(&app, &format!("Ruby Edition release check failed: {error}"));
            cache.error = Some(error);
        }
    }
    cache.checked_at = unix_timestamp_u64();
    write_json(&cache_path, &cache)?;
    build_status(&app, state.inner())
}

// ---------------------------------------------------------------------------------------------
// Commands: install / update
// ---------------------------------------------------------------------------------------------

struct InstallGuard(String);

impl Drop for InstallGuard {
    fn drop(&mut self) {
        if let Ok(mut installs) = active_installs().lock() {
            installs.remove(&self.0);
        }
    }
}

fn check_cancel(flag: &AtomicBool) -> Result<(), String> {
    if flag.load(Ordering::Relaxed) {
        Err("cancelled".to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub(crate) async fn ruby_edition_install(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: String,
) -> Result<RubyEditionStatus, String> {
    let (channel, name, _) = channel_meta(&channel)?;
    if cfg!(target_os = "android") {
        return Err("Ruby Edition runs on desktop for now. Android support is a later milestone.".to_string());
    }
    if is_running(state.inner(), channel) {
        return Err(format!("Close Ruby Edition {name} before updating it."));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut installs = active_installs()
            .lock()
            .map_err(|_| "Install state is unavailable.".to_string())?;
        if installs.contains_key(channel) {
            return Err(format!("Ruby Edition {name} is already being installed."));
        }
        installs.insert(channel.to_string(), cancel.clone());
    }
    let _guard = InstallGuard(channel.to_string());

    // Mojang's files are only fetched for a signed-in owner (Ruby Edition's launch guide asks the same).
    let has_account = state
        .auth
        .lock()
        .map(|auth| auth.account.is_some())
        .unwrap_or(false);
    if !has_account {
        return Err("Sign in with the Microsoft account that owns Minecraft: Java Edition first. Ruby Edition prepares its files from your own copy.".to_string());
    }

    let release = {
        let cache: ReleaseCache = read_or_default(&edition_root(&app)?.join(RELEASE_CACHE_FILE));
        if channel == "stable" { cache.stable } else { cache.nightly }
    }
    .ok_or_else(|| format!("No Ruby Edition {name} build has been published for this platform yet."))?;

    append_log(&app, &format!("Installing Ruby Edition {name} {}.", release.tag));
    let result = install_release(&app, state.inner(), channel, &release, &cancel).await;
    match &result {
        Ok(()) => {
            emit(&app, step_progress(channel, "done", 4, format!("Ruby Edition {name} build #{} is ready.", release.run), 100.0));
            append_log(&app, &format!("Ruby Edition {name} {} installed.", release.tag));
        }
        Err(error) if error == "cancelled" => {
            emit(&app, step_progress(channel, "cancelled", 0, "Install cancelled.", 0.0));
            append_log(&app, &format!("Ruby Edition {name} install cancelled."));
        }
        Err(error) => {
            emit(&app, step_progress(channel, "failed", 0, error.clone(), 0.0));
            append_log(&app, &format!("Ruby Edition {name} install failed: {error}"));
        }
    }
    drop(_guard);
    match result {
        Ok(()) => build_status(&app, state.inner()),
        Err(error) if error == "cancelled" => Err("Install cancelled.".to_string()),
        Err(error) => Err(error),
    }
}

#[tauri::command]
pub(crate) fn ruby_edition_cancel(channel: String) -> Result<(), String> {
    if let Some(flag) = active_installs()
        .lock()
        .map_err(|_| "Install state is unavailable.".to_string())?
        .get(&channel)
    {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

async fn install_release(
    app: &AppHandle,
    state: &AppState,
    channel: &str,
    release: &RubyRelease,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let dir = channel_dir(app, channel)?;
    let downloads = dir.join("downloads");
    fs::create_dir_all(&downloads).map_err(|error| format!("Could not create {}: {error}", downloads.display()))?;

    // 1. Download (0–55%), hashing as we go.
    let archive = downloads.join(&release.asset_name);
    let digest = download_with_sha256(app, state, channel, release, &archive, cancel).await?;
    check_cancel(cancel)?;

    // 2. Verify against the release's SHA256SUMS (55–58%).
    emit(app, step_progress(channel, "verify", 1, "Checking the download against the release's SHA-256…", 55.0));
    let sums_url = release
        .checksums_url
        .as_ref()
        .ok_or("This release has no SHA256SUMS file, so the download can't be verified. Ruby Launcher won't install it.")?;
    let sums = state
        .client
        .get(sums_url)
        .send()
        .await
        .map_err(|error| format!("Could not download SHA256SUMS: {error}"))?
        .text()
        .await
        .map_err(|error| format!("Could not read SHA256SUMS: {error}"))?;
    let expected = sums
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let file = parts.next()?.trim_start_matches('*');
            (file == release.asset_name).then(|| hash.to_ascii_lowercase())
        })
        .next()
        .ok_or_else(|| format!("SHA256SUMS does not list {}.", release.asset_name))?;
    if expected != digest {
        let _ = fs::remove_file(&archive);
        return Err(format!(
            "{} failed SHA-256 verification (expected {expected}, got {digest}). The file was deleted; try again.",
            release.asset_name
        ));
    }
    check_cancel(cancel)?;

    // 3. Extract into a staging folder (58–62%). The current install keeps working until the swap.
    emit(app, step_progress(channel, "extract", 2, "Extracting Ruby Edition…", 58.0));
    let staging = dir.join("staging");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| format!("Could not clear {}: {error}", staging.display()))?;
    }
    let archive_for_extract = archive.clone();
    let staging_for_extract = staging.clone();
    tauri::async_runtime::spawn_blocking(move || extract_zip(&archive_for_extract, &staging_for_extract))
        .await
        .map_err(|error| format!("Extraction stopped unexpectedly: {error}"))??;
    let package = if staging.join("ruby-edition").is_dir() { staging.join("ruby-edition") } else { staging.clone() };
    for binary in ["horizon-client", "horizon-prepare"] {
        let path = package.join(exe_name(binary));
        if !path.is_file() {
            return Err(format!("The downloaded package is missing {}.", exe_name(binary)));
        }
        make_executable(&path)?;
    }
    let _ = make_executable(&package.join(exe_name("horizon-ctl")));
    let lock: Value = read_json(&package.join("research-lock.json"))?;
    let minecraft_version = lock["minecraft"]["version"]
        .as_str()
        .ok_or("research-lock.json does not name a Minecraft version.")?
        .to_string();
    let java_major = lock["minecraft"]["java_major"].as_u64().unwrap_or(0) as u32;
    check_cancel(cancel)?;

    // 4. Prepare resources from the player's own Minecraft files (62–100%).
    let resources = prepare_resources(app, state, channel, &package, &minecraft_version, java_major, cancel).await?;
    check_cancel(cancel)?;

    // Swap the new build in. Worlds (game/) and logs (home/) are untouched.
    emit(app, step_progress(channel, "prepare", 3, "Finishing up…", 99.0));
    let bin = dir.join("bin");
    let old = dir.join("bin.old");
    if old.exists() {
        let _ = fs::remove_dir_all(&old);
    }
    if bin.exists() {
        fs::rename(&bin, &old).map_err(|error| format!("Could not replace the previous build: {error}"))?;
    }
    if let Err(error) = fs::rename(&package, &bin) {
        if old.exists() {
            let _ = fs::rename(&old, &bin);
        }
        return Err(format!("Could not move the new build into place: {error}"));
    }
    let _ = fs::remove_dir_all(&old);
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_file(&archive);
    for folder in ["game", "home"] {
        fs::create_dir_all(dir.join(folder)).map_err(|error| format!("Could not create the {folder} folder: {error}"))?;
    }
    let relative_resources = resources
        .strip_prefix(&package)
        .map(|rest| Path::new("bin").join(rest))
        .map_err(|_| "Prepared resources ended up outside the package folder.".to_string())?;
    write_json(
        &dir.join(INSTALL_FILE),
        &RubyInstall {
            tag: release.tag.clone(),
            run: release.run,
            commit: release.commit.clone(),
            published_at: release.published_at.clone(),
            installed_at: unix_timestamp_u64(),
            minecraft_version,
            resources_dir: relative_resources.to_string_lossy().into_owned(),
        },
    )
}

async fn download_with_sha256(
    app: &AppHandle,
    state: &AppState,
    channel: &str,
    release: &RubyRelease,
    destination: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<String, String> {
    let label = release.asset_name.clone();
    emit(app, RubyProgress {
        total_bytes: release.asset_size,
        ..step_progress(channel, "download", 0, format!("Downloading {label}…"), 0.0)
    });
    let response = state
        .client
        .get(&release.asset_url)
        .timeout(Duration::from_secs(60 * 30))
        .send()
        .await
        .map_err(|error| format!("Could not download {label}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Could not download {label} ({}).", response.status()));
    }
    let total = response.content_length().unwrap_or(release.asset_size).max(1);
    let partial = destination.with_extension("part");
    let mut file = tokio::fs::File::create(&partial)
        .await
        .map_err(|error| format!("Could not create {}: {error}", partial.display()))?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha256::new();
    let mut received = 0_u64;
    let started = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now() - Duration::from_secs(1);
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = tokio::fs::remove_file(&partial).await;
            return Err("cancelled".to_string());
        }
        let chunk = chunk.map_err(|error| format!("The download of {label} was interrupted: {error}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("Could not write {}: {error}", partial.display()))?;
        hasher.update(&chunk);
        received += chunk.len() as u64;
        if last_emit.elapsed() >= Duration::from_millis(150) {
            last_emit = std::time::Instant::now();
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            emit(app, RubyProgress {
                channel: channel.to_string(),
                step: "download",
                step_index: 0,
                message: label.clone(),
                downloaded_bytes: received,
                total_bytes: total,
                bytes_per_second: (received as f64 / elapsed) as u64,
                percent: (received as f64 / total as f64 * 55.0).min(55.0),
            });
        }
    }
    file.flush().await.map_err(|error| format!("Could not finish {}: {error}", partial.display()))?;
    drop(file);
    if release.asset_size > 0 && received != release.asset_size {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(format!(
            "{label} is {received} bytes but GitHub said {}. The download was discarded; try again.",
            release.asset_size
        ));
    }
    if destination.exists() {
        let _ = tokio::fs::remove_file(destination).await;
    }
    tokio::fs::rename(&partial, destination)
        .await
        .map_err(|error| format!("Could not finish {}: {error}", destination.display()))?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn extract_zip(archive_path: &Path, target: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|error| format!("Could not open {}: {error}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| format!("The downloaded package is not a valid zip: {error}"))?;
    fs::create_dir_all(target).map_err(|error| format!("Could not create {}: {error}", target.display()))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| format!("Could not read the package: {error}"))?;
        let relative = entry
            .enclosed_name()
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("The package contains an unsafe path: {}", entry.name()))?;
        let output = target.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output).map_err(|error| format!("Could not create {}: {error}", output.display()))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
        }
        let mut out = File::create(&output).map_err(|error| format!("Could not create {}: {error}", output.display()))?;
        std::io::copy(&mut entry, &mut out).map_err(|error| format!("Could not extract {}: {error}", output.display()))?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&output, fs::Permissions::from_mode(mode & 0o777));
        }
    }
    Ok(())
}

fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path.is_file() {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .map_err(|error| format!("Could not mark {} as executable: {error}", path.display()))?;
        }
    }
    let _ = path;
    Ok(())
}

/// Gets the player's own Minecraft client + server jar through the normal vanilla installer,
/// a Java runtime, then runs `horizon-prepare`. Returns the prepared resources folder.
async fn prepare_resources(
    app: &AppHandle,
    state: &AppState,
    channel: &str,
    package: &Path,
    minecraft_version: &str,
    lock_java_major: u32,
    cancel: &Arc<AtomicBool>,
) -> Result<PathBuf, String> {
    let shared = edition_root(app)?.join("shared").join("minecraft");
    create_instance_directories(&shared)?;

    emit(app, step_progress(channel, "prepare", 3, format!("Getting Minecraft {minecraft_version} files from Mojang…"), 62.0));
    let vanilla = install_vanilla_files(app.clone(), state, &shared, minecraft_version, "ruby-edition", false).await?;
    check_cancel(cancel)?;

    // The server jar is a separate download in the same version metadata.
    emit(app, step_progress(channel, "prepare", 3, format!("Getting the Minecraft {minecraft_version} server jar…"), 80.0));
    let version_json = shared.join("versions").join(minecraft_version).join(format!("{minecraft_version}.json"));
    let metadata: VersionMetadata = read_json(&version_json)?;
    let server = metadata
        .downloads
        .server
        .clone()
        .ok_or_else(|| format!("Mojang's metadata for {minecraft_version} has no server download."))?;
    let server_jar = shared.join("server").join(format!("minecraft-server-{minecraft_version}.jar"));
    ensure_download(
        state.client.clone(),
        DownloadTask {
            url: server.url.clone(),
            destination: server_jar.clone(),
            sha1: Some(server.sha1.clone()),
            size: server.size,
            label: format!("Minecraft {minecraft_version} server"),
        },
        app.clone(),
        Arc::new(AtomicU64::new(0)),
        Arc::new(AtomicUsize::new(0)),
        server.size,
        1,
        true,
    )
    .await?;
    check_cancel(cancel)?;

    emit(app, step_progress(channel, "prepare", 3, "Checking Java…", 82.0));
    let mut java_settings = launcher_settings(app);
    java_settings.java_runtime = "automatic".to_string();
    let required = lock_java_major.max(vanilla.java_major_version);
    let (java_path, _) = ensure_java_runtime(app, state, required, &java_settings).await?;
    let java_home = java_path
        .is_absolute()
        .then(|| java_path.parent().and_then(Path::parent).map(Path::to_path_buf))
        .flatten();
    check_cancel(cancel)?;

    emit(app, step_progress(channel, "prepare", 3, "Preparing Ruby Edition's resources (this can take a few minutes)…", 84.0));
    let prepare = package.join(exe_name("horizon-prepare"));
    let mut args = vec![
        "--prepare".to_string(),
        "--server-jar".to_string(),
        server_jar.to_string_lossy().into_owned(),
        "--minecraft-dir".to_string(),
        shared.to_string_lossy().into_owned(),
        "--root".to_string(),
        package.to_string_lossy().into_owned(),
    ];
    run_tool(app, channel, &prepare, &args, java_home.as_deref(), cancel, 84.0, 96.0).await?;

    // Sounds are optional: without them the game is silent but playable.
    emit(app, step_progress(channel, "prepare", 3, "Copying game sounds…", 96.0));
    args = vec![
        "sounds".to_string(),
        "--minecraft-dir".to_string(),
        shared.to_string_lossy().into_owned(),
        "--root".to_string(),
        package.to_string_lossy().into_owned(),
    ];
    if let Err(error) = run_tool(app, channel, &prepare, &args, java_home.as_deref(), cancel, 96.0, 98.0).await {
        if error == "cancelled" {
            return Err(error);
        }
        append_log(app, &format!("Ruby Edition sounds were not copied (the game will be silent): {error}"));
    }

    let resources = package.join(".local").join("horizon-client").join(minecraft_version);
    if !resources.is_dir() {
        return Err(format!(
            "horizon-prepare finished but {} was not created. Check the launcher log for its output.",
            resources.display()
        ));
    }
    Ok(resources)
}

/// Runs one of Ruby Edition's helper programs, streaming its output to the log and the progress
/// line, and stopping it if the install is cancelled.
#[allow(clippy::too_many_arguments)]
async fn run_tool(
    app: &AppHandle,
    channel: &str,
    program: &Path,
    args: &[String],
    java_home: Option<&Path>,
    cancel: &Arc<AtomicBool>,
    from_percent: f64,
    to_percent: f64,
) -> Result<(), String> {
    let app = app.clone();
    let channel = channel.to_string();
    let program = program.to_path_buf();
    let args = args.to_vec();
    let java_home = java_home.map(Path::to_path_buf);
    let cancel = cancel.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = Command::new(&program);
        command
            .args(&args)
            .current_dir(program.parent().unwrap_or(Path::new(".")))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(home) = &java_home {
            command.env("JAVA_HOME", home);
            let mut paths = vec![home.join("bin")];
            if let Some(existing) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&existing));
            }
            if let Ok(joined) = std::env::join_paths(paths) {
                command.env("PATH", joined);
            }
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let name = program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        append_log(&app, &format!("Running {name} {}", args.join(" ")));
        let mut child = command.spawn().map_err(|error| format!("Could not start {name}: {error}"))?;

        let lines = Arc::new(AtomicUsize::new(0));
        let stdout_thread = child.stdout.take().map(|stdout| {
            let app = app.clone();
            let channel = channel.clone();
            let lines = lines.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    append_log(&app, &format!("[ruby-edition/{channel}] {line}"));
                    let seen = lines.fetch_add(1, Ordering::Relaxed) as f64;
                    // No percentage from the tool yet: creep towards the end of this step.
                    let percent = from_percent + (to_percent - from_percent) * (1.0 - (-seen / 60.0).exp());
                    emit(&app, step_progress(&channel, "prepare", 3, line, percent));
                }
            })
        });
        let tail = Arc::new(Mutex::new(VecDeque::<String>::new()));
        let stderr_thread = child.stderr.take().map(|stderr| {
            let app = app.clone();
            let channel = channel.clone();
            let tail = tail.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    append_log(&app, &format!("[ruby-edition/{channel}/stderr] {line}"));
                    if let Ok(mut tail) = tail.lock() {
                        tail.push_back(line);
                        if tail.len() > 6 {
                            tail.pop_front();
                        }
                    }
                }
            })
        });

        let status = loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                return Err("cancelled".to_string());
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                Err(error) => return Err(format!("Could not watch {name}: {error}")),
            }
        };
        if let Some(thread) = stdout_thread {
            let _ = thread.join();
        }
        if let Some(thread) = stderr_thread {
            let _ = thread.join();
        }
        if status.success() {
            Ok(())
        } else {
            let detail = tail.lock().map(|t| t.iter().cloned().collect::<Vec<_>>().join(" ")).unwrap_or_default();
            Err(if detail.is_empty() {
                format!("{name} failed ({status}). See the launcher log for details.")
            } else {
                format!("{name} failed: {detail}")
            })
        }
    })
    .await
    .map_err(|error| format!("The preparation helper stopped unexpectedly: {error}"))?
}

// ---------------------------------------------------------------------------------------------
// Commands: launch / manage
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RubyLaunchResult {
    channel: String,
    pid: u32,
    message: String,
}

#[tauri::command]
pub(crate) async fn ruby_edition_play(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: String,
) -> Result<RubyLaunchResult, String> {
    let (channel, name, _) = channel_meta(&channel)?;
    let dir = channel_dir(&app, channel)?;
    let install = read_install(&dir).ok_or_else(|| format!("Ruby Edition {name} isn't installed yet."))?;
    if is_installing(channel) {
        return Err(format!("Ruby Edition {name} is still installing."));
    }
    let key = running_key(channel);
    {
        let mut running = state
            .running_instances
            .lock()
            .map_err(|_| "Running-game state is unavailable.".to_string())?;
        if !running.insert(key.clone()) {
            return Err(format!("Ruby Edition {name} is already running."));
        }
    }
    let result = start_game(&app, state.inner(), channel, name, &dir, &install).await;
    if result.is_err() {
        if let Ok(mut running) = state.running_instances.lock() {
            running.remove(&key);
        }
    }
    result
}

async fn start_game(
    app: &AppHandle,
    state: &AppState,
    channel: &'static str,
    name: &'static str,
    dir: &Path,
    install: &RubyInstall,
) -> Result<RubyLaunchResult, String> {
    let account = ensure_launch_account(app, state).await?;
    let session = json!({
        "name": account.summary.name,
        "uuid": account.summary.id,
        "access_token": account.minecraft_access_token,
        "user_type": "msa",
        "xuid": account.xuid,
    })
    .to_string();

    let settings = launcher_settings(app);
    let bin = dir.join("bin");
    let game_dir = dir.join("game");
    let home_dir = dir.join("home");
    for folder in [&game_dir, &home_dir] {
        fs::create_dir_all(folder).map_err(|error| format!("Could not create {}: {error}", folder.display()))?;
    }
    let mut command = Command::new(bin.join(exe_name("horizon-client")));
    command
        .current_dir(&bin)
        .arg("--resources")
        .arg(dir.join(&install.resources_dir))
        .arg("--game-dir")
        .arg(&game_dir)
        .arg("--home")
        .arg(&home_dir)
        .arg("--session-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Controller mode is a couch setup, so the game opens fullscreen there; desktop mode opens a window.
    if settings.ui_mode != "desktop" {
        command.arg("--fullscreen");
    } else {
        command.args(["--width", "1280", "--height", "720"]);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start Ruby Edition {name}: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        // The session goes over stdin so the token never touches the disk or the command line.
        stdin
            .write_all(session.as_bytes())
            .map_err(|error| format!("Could not hand your session to Ruby Edition: {error}"))?;
    }
    let pid = child.id();
    let label = format!("Ruby Edition {name}");
    if let Some(stdout) = child.stdout.take() {
        pipe_game_output(app.clone(), label.clone(), "stdout", stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        pipe_game_output(app.clone(), label.clone(), "stderr", stderr);
    }
    if settings.hide_launcher_while_playing {
        minimize_launcher_to_tray(app);
    }
    append_log(app, &format!("Started {label} build #{} as process {pid}.", install.run));

    let app_for_wait = app.clone();
    let running = state.running_instances.clone();
    let stats_path = dir.join(STATS_FILE);
    let restore = settings.hide_launcher_while_playing;
    let started = unix_timestamp_u64();
    let message = format!("{label} is starting.");
    std::thread::spawn(move || {
        let exit = child.wait();
        let key = running_key(channel);
        let none_left = running
            .lock()
            .map(|mut running| {
                running.remove(&key);
                running.is_empty()
            })
            .unwrap_or(true);
        let mut stats: PlayStats = read_or_default(&stats_path);
        let ended = unix_timestamp_u64();
        stats.last_played = ended;
        stats.total_seconds += ended.saturating_sub(started);
        let _ = write_json(&stats_path, &stats);
        if restore && none_left {
            restore_launcher_window(&app_for_wait);
        }
        let _ = app_for_wait.emit("game-exited", key);
        match exit {
            Ok(status) if status.success() => append_log(&app_for_wait, &format!("{label} closed.")),
            Ok(status) => append_log(
                &app_for_wait,
                &format!("{label} exited with {status}. If it crashed, the crash report is in the channel's home/crash-reports folder."),
            ),
            Err(error) => append_log(&app_for_wait, &format!("Could not read {label}'s exit status: {error}")),
        }
    });

    Ok(RubyLaunchResult {
        channel: channel.to_string(),
        pid,
        message,
    })
}

/// Removes a channel's game build. Worlds and settings are kept unless `remove_game_data` is set.
#[tauri::command]
pub(crate) fn ruby_edition_uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: String,
    remove_game_data: bool,
) -> Result<RubyEditionStatus, String> {
    let (channel, name, _) = channel_meta(&channel)?;
    if is_running(state.inner(), channel) || is_installing(channel) {
        return Err(format!("Ruby Edition {name} is busy. Close it or wait for the install to finish."));
    }
    let dir = channel_dir(&app, channel)?;
    let mut targets = vec!["bin", "bin.old", "staging", "downloads"];
    if remove_game_data {
        targets.extend(["game", "home"]);
    }
    for target in targets {
        let path = dir.join(target);
        if path.exists() {
            fs::remove_dir_all(&path).map_err(|error| format!("Could not remove {}: {error}", path.display()))?;
        }
    }
    let _ = fs::remove_file(dir.join(INSTALL_FILE));
    if remove_game_data {
        let _ = fs::remove_file(dir.join(STATS_FILE));
    }
    append_log(
        &app,
        &format!(
            "Uninstalled Ruby Edition {name}{}.",
            if remove_game_data { " including worlds and settings" } else { " (worlds and settings kept)" }
        ),
    );
    build_status(&app, state.inner())
}

/// Opens a channel folder: "game" (worlds), "home" (logs and crash reports) or the channel root.
#[tauri::command]
pub(crate) fn ruby_edition_open_folder(app: AppHandle, channel: String, folder: Option<String>) -> Result<(), String> {
    let dir = channel_dir(&app, &channel)?;
    let target = match folder.as_deref() {
        Some("game") => dir.join("game"),
        Some("home") => dir.join("home"),
        _ => dir,
    };
    fs::create_dir_all(&target).map_err(|error| format!("Could not create {}: {error}", target.display()))?;
    open::that(&target).map_err(|error| format!("Could not open {}: {error}", target.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, assets: &[&str]) -> GhRelease {
        GhRelease {
            tag_name: tag.to_string(),
            name: None,
            draft: false,
            published_at: Some("2026-10-31T00:00:00Z".to_string()),
            html_url: String::new(),
            assets: assets
                .iter()
                .map(|name| GhAsset { name: name.to_string(), size: 10, browser_download_url: format!("https://x/{name}") })
                .collect(),
        }
    }

    #[test]
    fn tags_parse_run_and_commit() {
        assert_eq!(parse_tag("ruby-edition-nightly-57-9b0d44a1", "nightly"), Some((57, "9b0d44a1".to_string())));
        assert_eq!(parse_tag("ruby-edition-main-42-3fa91c2e", "main"), Some((42, "3fa91c2e".to_string())));
        assert_eq!(parse_tag("ruby-edition-main-42-3fa91c2e", "nightly"), None);
        assert_eq!(parse_tag("v1.0", "main"), None);
    }

    #[test]
    fn newest_build_per_channel_with_this_platforms_asset() {
        let zip = "ruby-edition-windows-x64.zip";
        let releases = vec![
            release("ruby-edition-nightly-57-aaaaaaaa", &[zip, "SHA256SUMS"]),
            release("ruby-edition-main-9-bbbbbbbb", &[zip, "SHA256SUMS"]),
            release("ruby-edition-nightly-60-cccccccc", &["ruby-edition-linux-x86_64.zip"]),
            release("ruby-edition-main-12-dddddddd", &[zip]),
        ];
        let nightly = latest_for_branch(&releases, "nightly", "windows-x64").unwrap();
        assert_eq!(nightly.run, 57);
        assert!(nightly.checksums_url.is_some());
        let stable = latest_for_branch(&releases, "main", "windows-x64").unwrap();
        assert_eq!(stable.run, 12);
        assert!(stable.checksums_url.is_none());
        assert!(latest_for_branch(&releases, "main", "linux-x86_64").is_none());
    }
}
