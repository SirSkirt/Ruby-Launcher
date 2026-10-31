use futures_util::StreamExt;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use sha2::{Sha256, Sha512};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;

mod ruby_edition;

const BUILD_ID: &str = "b2026.23";
const MICROSOFT_CLIENT_ID: &str = "3fcb3980-88fb-46bb-b044-4aff88ff9454";
const SECURE_CREDENTIAL_SERVICE: &str = "com.qtechstudios.rubylauncher";
const SECURE_REFRESH_TOKEN_USER: &str = "microsoft-refresh-token";
const SETTINGS_FILE: &str = "launcher-settings.json";
const INSTANCES_FILE: &str = "instances.json";
const LOG_FILE: &str = "ruby-launcher.log";
const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const ASSET_BASE_URL: &str = "https://resources.download.minecraft.net";
const DEFAULT_LIBRARY_BASE_URL: &str = "https://libraries.minecraft.net/";
const ADOPTIUM_API_BASE: &str = "https://api.adoptium.net/v3/assets/feature_releases";
const MICROSOFT_DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MICROSOFT_TOKEN_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBOX_USER_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XBOX_XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT_LOGIN_URL: &str =
    "https://api.minecraftservices.com/authentication/login_with_xbox";
const MINECRAFT_ENTITLEMENTS_URL: &str =
    "https://api.minecraftservices.com/entitlements/mcstore";
const MINECRAFT_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v2";
const FABRIC_META_BASE: &str = "https://meta.fabricmc.net/v2";
const QUILT_META_BASE: &str = "https://meta.quiltmc.org/v3";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LauncherInfo {
    name: &'static str,
    version: &'static str,
    build: &'static str,
    platform: &'static str,
    native_backend: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LauncherSettings {
    memory_mb: u32,
    java_runtime: String,
    renderer: String,
    keep_screen_on: bool,
    show_logs: bool,
    allow_cellular: bool,
    selected_instance_id: Option<String>,
    controller_deadzone: f32,
    controller_vibration: bool,
    controller_support_enabled: bool,
    launcher_fullscreen: bool,
    controller_prompt_on_start: bool,
    hide_launcher_while_playing: bool,
    /// "controller" (fullscreen, console-style) or "desktop" (windowed, mouse and keyboard).
    ui_mode: String,
    /// The Ruby Edition channel Home launches: "stable" or "nightly".
    ruby_channel: String,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            memory_mb: 2048,
            java_runtime: "automatic".to_string(),
            renderer: "automatic".to_string(),
            keep_screen_on: true,
            show_logs: true,
            allow_cellular: false,
            selected_instance_id: None,
            controller_deadzone: 0.22,
            controller_vibration: true,
            controller_support_enabled: true,
            launcher_fullscreen: true,
            controller_prompt_on_start: true,
            hide_launcher_while_playing: true,
            ui_mode: "controller".to_string(),
            ruby_channel: "stable".to_string(),
        }
    }
}

fn default_profile_memory_mb() -> u32 { 2048 }
fn default_initial_memory_mb() -> u32 { 512 }
fn default_profile_java_runtime() -> String { "automatic".to_string() }
fn default_profile_renderer() -> String { "automatic".to_string() }
fn default_jvm_preset() -> String { "balanced".to_string() }
fn default_true() -> bool { true }
fn default_resolution_width() -> u32 { 1280 }
fn default_resolution_height() -> u32 { 720 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstanceProfile {
    id: String,
    name: String,
    minecraft_version: String,
    loader: String,
    status: String,
    game_dir: String,
    created_at: String,
    #[serde(default = "default_profile_memory_mb")]
    memory_mb: u32,
    #[serde(default = "default_initial_memory_mb")]
    initial_memory_mb: u32,
    #[serde(default = "default_profile_java_runtime")]
    java_runtime: String,
    #[serde(default = "default_profile_renderer")]
    renderer: String,
    #[serde(default = "default_jvm_preset")]
    jvm_preset: String,
    #[serde(default = "default_true")]
    controller_support_enabled: bool,
    #[serde(default = "default_true")]
    game_fullscreen: bool,
    #[serde(default = "default_resolution_width")]
    resolution_width: u32,
    #[serde(default = "default_resolution_height")]
    resolution_height: u32,
    #[serde(default)]
    source: String,
    #[serde(default)]
    loader_version: Option<String>,
    #[serde(default)]
    launch_version: Option<String>,
    #[serde(default)]
    modpack_project_id: Option<String>,
    #[serde(default)]
    modpack_version_id: Option<String>,
    #[serde(default)]
    modpack_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInstanceRequest {
    name: String,
    minecraft_version: String,
    loader: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInstanceRequest {
    instance_id: String,
    name: String,
    memory_mb: u32,
    initial_memory_mb: u32,
    java_runtime: String,
    renderer: String,
    jvm_preset: String,
    controller_support_enabled: bool,
    game_fullscreen: bool,
    resolution_width: u32,
    resolution_height: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareResult {
    instance_id: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountSummary {
    id: String,
    name: String,
    entitlement_count: usize,
    session_only: bool,
}

#[derive(Debug, Clone)]
struct AccountSession {
    summary: AccountSummary,
    minecraft_access_token: String,
    xuid: Option<String>,
    microsoft_refresh_token: Option<String>,
    #[allow(dead_code)]
    minecraft_expires_at: u64,
    #[allow(dead_code)]
    microsoft_expires_at: u64,
}

#[derive(Debug, Clone)]
struct PendingDeviceAuth {
    device_code: String,
    expires_at: u64,
    interval: u64,
}

#[derive(Default)]
struct AuthStore {
    pending: Option<PendingDeviceAuth>,
    account: Option<AccountSession>,
}

struct AppState {
    client: Client,
    auth: Mutex<AuthStore>,
    secure_store_available: bool,
    running_instances: Arc<Mutex<HashSet<String>>>,
}

impl AppState {
    fn new(secure_store_available: bool) -> Self {
        let client = Client::builder()
            .user_agent(format!("QTechStudios/RubyLauncher/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(90))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .expect("could not create HTTP client");
        Self {
            client,
            auth: Mutex::new(AuthStore::default()),
            secure_store_available,
            running_instances: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

#[derive(Debug, Deserialize)]
struct MicrosoftDeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCodeStartResult {
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthPollResult {
    status: String,
    message: String,
    retry_after: u64,
    account: Option<AccountSummary>,
}

#[derive(Debug, Deserialize)]
struct MicrosoftTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct XboxTokenResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XboxDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct XboxDisplayClaims {
    xui: Vec<XboxUserClaim>,
}

#[derive(Debug, Deserialize)]
struct XboxUserClaim {
    uhs: String,
    #[serde(default)]
    xid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MinecraftLoginResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct MinecraftEntitlements {
    #[serde(default)]
    items: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct MinecraftProfile {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionManifest {
    latest: LatestVersions,
    versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct LatestVersions {
    release: String,
    snapshot: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionEntry {
    id: String,
    #[serde(rename = "type")]
    version_type: String,
    url: String,
    sha1: String,
    release_time: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionCatalog {
    latest_release: String,
    latest_snapshot: String,
    versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionMetadata {
    id: String,
    main_class: String,
    asset_index: AssetIndexDescriptor,
    downloads: VersionDownloads,
    #[serde(default)]
    libraries: Vec<LibraryEntry>,
    java_version: Option<JavaVersion>,
    logging: Option<LoggingConfig>,
    arguments: Option<VersionArguments>,
    #[serde(rename = "minecraftArguments")]
    minecraft_arguments: Option<String>,
    #[serde(default, rename = "type")]
    version_type: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JavaVersion {
    major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct VersionDownloads {
    client: DownloadDescriptor,
    #[serde(default)]
    client_mappings: Option<DownloadDescriptor>,
    #[serde(default)]
    server: Option<DownloadDescriptor>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetIndexDescriptor {
    id: String,
    url: String,
    sha1: String,
    size: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
    #[serde(default, rename = "virtual")]
    virtual_assets: bool,
    #[serde(default)]
    map_to_resources: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct DownloadDescriptor {
    url: String,
    sha1: String,
    size: u64,
    path: Option<String>,
    id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LibraryEntry {
    name: String,
    url: Option<String>,
    downloads: Option<LibraryDownloads>,
    natives: Option<HashMap<String, String>>,
    extract: Option<ExtractRules>,
    rules: Option<Vec<RuleEntry>>,
}

#[derive(Debug, Clone, Deserialize)]
struct LibraryDownloads {
    artifact: Option<DownloadDescriptor>,
    classifiers: Option<HashMap<String, DownloadDescriptor>>,
}

#[derive(Debug, Clone, Deserialize)]
struct ExtractRules {
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RuleEntry {
    action: String,
    os: Option<OsRule>,
    features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize)]
struct OsRule {
    name: Option<String>,
    arch: Option<String>,
    version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LoggingConfig {
    client: LoggingClient,
}

#[derive(Debug, Clone, Deserialize)]
struct LoggingClient {
    file: DownloadDescriptor,
    argument: Option<String>,
}


#[derive(Debug, Clone, Deserialize)]
struct VersionArguments {
    #[serde(default)]
    game: Vec<ArgumentValue>,
    #[serde(default)]
    jvm: Vec<ArgumentValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ArgumentValue {
    Plain(String),
    Conditional {
        #[serde(default)]
        rules: Vec<RuleEntry>,
        value: ArgumentContents,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ArgumentContents {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedRuntimeManifest {
    java_major_version: u32,
    java_path: String,
    source: String,
    installed_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchResult {
    instance_id: String,
    pid: u32,
    java_major_version: u32,
    message: String,
}

#[derive(Debug, Clone)]
struct NativeArchive {
    path: PathBuf,
    excludes: Vec<String>,
}

#[derive(Debug, Clone)]
struct DownloadTask {
    url: String,
    destination: PathBuf,
    sha1: Option<String>,
    size: u64,
    label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallProgress {
    stage: String,
    message: String,
    downloaded_bytes: u64,
    total_bytes: u64,
    completed_files: usize,
    total_files: usize,
    percent: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallResult {
    instance_id: String,
    version: String,
    java_major_version: u32,
    downloaded_bytes: u64,
    files: usize,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
struct ModrinthSearchHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    author: String,
    downloads: u64,
    icon_url: Option<String>,
    latest_version: Option<String>,
    #[serde(default)]
    versions: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthSearchHit>,
    total_hits: u64,
    offset: u64,
    limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
struct ModrinthVersionFile {
    #[serde(default)]
    hashes: HashMap<String, String>,
    url: String,
    filename: String,
    primary: bool,
    size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
struct ModrinthVersionSummary {
    id: String,
    project_id: String,
    name: String,
    version_number: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    version_type: String,
    date_published: String,
    downloads: u64,
    #[serde(default, skip_serializing)]
    files: Vec<ModrinthVersionFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallModrinthRequest {
    project_id: String,
    version_id: String,
    name: String,
    controller_support_enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MrpackIndex {
    format_version: u32,
    game: String,
    version_id: String,
    name: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    files: Vec<MrpackFile>,
    dependencies: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct MrpackFile {
    path: String,
    #[serde(default)]
    hashes: HashMap<String, String>,
    #[serde(default)]
    env: Option<MrpackEnvironment>,
    #[serde(default)]
    downloads: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MrpackEnvironment {
    #[serde(default)]
    client: String,
    #[serde(default)]
    server: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledVersionManifest {
    version: String,
    main_class: String,
    java_major_version: u32,
    installed_at: String,
    files: usize,
    downloaded_bytes: u64,
}

fn platform_name() -> &'static str {
    #[cfg(target_os = "android")]
    {
        return "Android";
    }
    #[cfg(target_os = "windows")]
    {
        return "Windows development host";
    }
    #[cfg(target_os = "linux")]
    {
        return "Linux development host";
    }
    #[cfg(target_os = "macos")]
    {
        return "macOS development host";
    }
    #[allow(unreachable_code)]
    "Unknown platform"
}

fn unix_timestamp_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn unix_timestamp() -> String {
    unix_timestamp_u64().to_string()
}

fn launcher_root(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Could not resolve the app data directory: {error}"))?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("Could not create the app data directory: {error}"))?;
    Ok(root)
}

fn path_for(app: &AppHandle, file_name: &str) -> Result<PathBuf, String> {
    Ok(launcher_root(app)?.join(file_name))
}

fn read_json<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    serde_json::from_str(&contents)
        .map_err(|error| format!("Could not parse {}: {error}", path.display()))
}

fn write_json<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }

    let serialized = serde_json::to_string_pretty(value)
        .map_err(|error| format!("Could not serialize data: {error}"))?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serialized)
        .map_err(|error| format!("Could not write {}: {error}", temporary.display()))?;
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("Could not replace {}: {error}", path.display()))?;
    }
    fs::rename(&temporary, path)
        .map_err(|error| format!("Could not replace {}: {error}", path.display()))
}

fn append_log(app: &AppHandle, message: &str) {
    let Ok(log_path) = path_for(app, LOG_FILE) else {
        return;
    };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) else {
        return;
    };
    let _ = writeln!(file, "[{}] {}", unix_timestamp(), message);
}

fn load_instances_internal(app: &AppHandle) -> Result<Vec<InstanceProfile>, String> {
    let path = path_for(app, INSTANCES_FILE)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    read_json(&path)
}

fn save_instances_internal(app: &AppHandle, instances: &[InstanceProfile]) -> Result<(), String> {
    let path = path_for(app, INSTANCES_FILE)?;
    write_json(&path, &instances)
}

fn update_instance_status(
    app: &AppHandle,
    instance_id: &str,
    status: &str,
) -> Result<InstanceProfile, String> {
    let mut instances = load_instances_internal(app)?;
    let instance = instances
        .iter_mut()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| "Instance not found.".to_string())?;
    instance.status = status.to_string();
    let updated = instance.clone();
    save_instances_internal(app, &instances)?;
    let root = launcher_root(app)?.join("instances").join(instance_id);
    write_json(&root.join("instance.json"), &updated)?;
    Ok(updated)
}

fn sanitize_id(name: &str) -> String {
    let mut result = String::new();
    let mut previous_dash = false;

    for character in name.trim().to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            result.push(character);
            previous_dash = false;
        } else if !previous_dash && !result.is_empty() {
            result.push('-');
            previous_dash = true;
        }

        if result.len() >= 40 {
            break;
        }
    }

    while result.ends_with('-') {
        result.pop();
    }

    if result.is_empty() {
        "instance".to_string()
    } else {
        result
    }
}

fn unique_instance_id(name: &str, instances: &[InstanceProfile]) -> String {
    let base = sanitize_id(name);
    if !instances.iter().any(|instance| instance.id == base) {
        return base;
    }

    for suffix in 2..10_000 {
        let candidate = format!("{base}-{suffix}");
        if !instances.iter().any(|instance| instance.id == candidate) {
            return candidate;
        }
    }

    format!("{base}-{}", unix_timestamp())
}

fn validate_loader(loader: &str) -> Result<(), String> {
    match loader {
        "none" | "fabric" | "quilt" | "forge" | "neoforge" => Ok(()),
        _ => Err(format!("Unsupported loader profile: {loader}")),
    }
}

fn create_instance_directories(instance_root: &Path) -> Result<(), String> {
    let minecraft_dir = instance_root.join(".minecraft");
    for directory in [
        minecraft_dir.clone(),
        minecraft_dir.join("mods"),
        minecraft_dir.join("resourcepacks"),
        minecraft_dir.join("shaderpacks"),
        minecraft_dir.join("saves"),
        minecraft_dir.join("screenshots"),
        minecraft_dir.join("logs"),
        instance_root.join("runtime"),
        instance_root.join("libraries"),
        instance_root.join("assets"),
        instance_root.join("assets").join("indexes"),
        instance_root.join("assets").join("objects"),
        instance_root.join("assets").join("log_configs"),
        instance_root.join("natives"),
        instance_root.join("versions"),
    ] {
        fs::create_dir_all(&directory)
            .map_err(|error| format!("Could not create {}: {error}", directory.display()))?;
    }
    Ok(())
}

fn initialize_secure_store() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let store = windows_native_keyring_store::Store::new()
            .map_err(|error| format!("Windows Credential Manager is unavailable: {error}"))?;
        keyring_core::set_default_store(store);
        return Ok(());
    }

    #[cfg(target_os = "android")]
    {
        let store = android_native_keyring_store::Store::new()
            .map_err(|error| format!("Android secure credential storage is unavailable: {error}"))?;
        keyring_core::set_default_store(store);
        return Ok(());
    }

    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    {
        Err("Secure Remember Me storage is currently available on Windows and Android builds."
            .to_string())
    }
}

fn secure_refresh_token_entry() -> Result<keyring_core::Entry, String> {
    keyring_core::Entry::new(SECURE_CREDENTIAL_SERVICE, SECURE_REFRESH_TOKEN_USER)
        .map_err(|error| format!("Could not access secure credential storage: {error}"))
}

fn load_saved_refresh_token(secure_store_available: bool) -> Result<Option<String>, String> {
    if !secure_store_available {
        return Ok(None);
    }
    let entry = secure_refresh_token_entry()?;
    match entry.get_password() {
        Ok(token) if !token.trim().is_empty() => Ok(Some(token)),
        Ok(_) => Ok(None),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("Could not read the saved Microsoft session: {error}")),
    }
}

fn save_refresh_token(secure_store_available: bool, refresh_token: &str) -> Result<(), String> {
    if !secure_store_available {
        return Err(
            "Secure Remember Me storage is unavailable on this build. The account remains signed in for this session only."
                .to_string(),
        );
    }
    if refresh_token.trim().is_empty() {
        return Err("Microsoft did not return a refresh token, so this account cannot be remembered."
            .to_string());
    }
    secure_refresh_token_entry()?
        .set_password(refresh_token)
        .map_err(|error| format!("Could not save the Microsoft session securely: {error}"))
}

fn delete_saved_refresh_token(secure_store_available: bool) -> Result<(), String> {
    if !secure_store_available {
        return Ok(());
    }
    let entry = secure_refresh_token_entry()?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("Could not remove the saved Microsoft session: {error}")),
    }
}

async fn response_text_or_error(response: reqwest::Response, context: &str) -> Result<String, String> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("{context}: could not read response: {error}"))?;
    if !status.is_success() {
        return Err(format!("{context} failed ({status}): {text}"));
    }
    Ok(text)
}

async fn fetch_json<T>(client: &Client, url: &str, context: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("{context}: {error}"))?;
    let text = response_text_or_error(response, context).await?;
    serde_json::from_str(&text).map_err(|error| format!("{context}: invalid JSON: {error}"))
}

fn sha1_hex_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sha1_hex_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Could not open {} for verification: {error}", path.display()))?;
    let mut hasher = Sha1::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not verify {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    let partial = path.with_extension("part");
    fs::write(&partial, bytes)
        .map_err(|error| format!("Could not write {}: {error}", partial.display()))?;
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("Could not replace {}: {error}", path.display()))?;
    }
    fs::rename(&partial, path)
        .map_err(|error| format!("Could not finish {}: {error}", path.display()))
}

fn minecraft_os_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        return "windows";
    }
    #[cfg(target_os = "macos")]
    {
        return "osx";
    }
    #[cfg(target_os = "linux")]
    {
        return "linux";
    }
    #[cfg(target_os = "android")]
    {
        return "linux";
    }
    #[allow(unreachable_code)]
    "unknown"
}

fn minecraft_arch_token() -> &'static str {
    if cfg!(target_pointer_width = "64") {
        "64"
    } else {
        "32"
    }
}

fn rule_matches(rule: &RuleEntry) -> bool {
    if rule
        .features
        .as_ref()
        .is_some_and(|features| !features.is_empty())
    {
        return false;
    }

    let Some(os) = &rule.os else {
        return true;
    };

    if let Some(name) = &os.name {
        if name != minecraft_os_name() {
            return false;
        }
    }

    if let Some(arch) = &os.arch {
        let current_arch = std::env::consts::ARCH;
        let Ok(pattern) = Regex::new(arch) else {
            return false;
        };
        if !pattern.is_match(current_arch) {
            return false;
        }
    }

    // OS-version-specific launcher rules are uncommon for library downloads. Ruby
    // does not guess the host version string; an unresolvable version rule does not match.
    if os.version.is_some() {
        return false;
    }

    true
}

fn library_allowed(rules: Option<&Vec<RuleEntry>>) -> bool {
    let Some(rules) = rules else {
        return true;
    };
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn maven_path(name: &str) -> Result<String, String> {
    let clean_name = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = clean_name.split(':').collect();
    if parts.len() < 3 {
        return Err(format!("Unsupported Maven library coordinate: {name}"));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).map(|value| format!("-{value}")).unwrap_or_default();
    Ok(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}{classifier}.jar"
    ))
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

fn add_task(tasks: &mut Vec<DownloadTask>, seen: &mut HashSet<PathBuf>, task: DownloadTask) {
    if seen.insert(task.destination.clone()) {
        tasks.push(task);
    }
}

fn emit_progress(
    app: &AppHandle,
    stage: &str,
    message: impl Into<String>,
    downloaded_bytes: u64,
    total_bytes: u64,
    completed_files: usize,
    total_files: usize,
) {
    let percent = if total_bytes > 0 {
        (downloaded_bytes as f64 / total_bytes as f64 * 100.0).clamp(0.0, 100.0)
    } else if total_files > 0 {
        (completed_files as f64 / total_files as f64 * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            stage: stage.to_string(),
            message: message.into(),
            downloaded_bytes,
            total_bytes,
            completed_files,
            total_files,
            percent,
        },
    );
}

async fn ensure_download(
    client: Client,
    task: DownloadTask,
    app: AppHandle,
    downloaded: Arc<AtomicU64>,
    completed_files: Arc<AtomicUsize>,
    total_bytes: u64,
    total_files: usize,
    verify_existing: bool,
) -> Result<(), String> {
    if task.destination.exists() {
        let valid = if verify_existing {
            match &task.sha1 {
                Some(expected) => sha1_hex_file(&task.destination)
                    .map(|actual| actual.eq_ignore_ascii_case(expected))
                    .unwrap_or(false),
                None => fs::metadata(&task.destination)
                    .map(|metadata| task.size == 0 || metadata.len() == task.size)
                    .unwrap_or(false),
            }
        } else {
            fs::metadata(&task.destination)
                .map(|metadata| task.size == 0 || metadata.len() == task.size)
                .unwrap_or(false)
        };
        if valid {
            let bytes = if task.size > 0 {
                task.size
            } else {
                fs::metadata(&task.destination)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0)
            };
            let current = downloaded.fetch_add(bytes, Ordering::Relaxed) + bytes;
            let files = completed_files.fetch_add(1, Ordering::Relaxed) + 1;
            emit_progress(
                &app,
                "downloading",
                format!("Found {}", task.label),
                current,
                total_bytes,
                files,
                total_files,
            );
            return Ok(());
        }
    }

    if let Some(parent) = task.destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }

    emit_progress(
        &app,
        "downloading",
        format!("Downloading {}", task.label),
        downloaded.load(Ordering::Relaxed),
        total_bytes,
        completed_files.load(Ordering::Relaxed),
        total_files,
    );

    let response = client
        .get(&task.url)
        .send()
        .await
        .map_err(|error| format!("Could not download {}: {error}", task.label))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Could not download {} ({status}): {body}",
            task.label
        ));
    }

    let partial = task.destination.with_extension("part");
    let mut file = tokio::fs::File::create(&partial)
        .await
        .map_err(|error| format!("Could not create {}: {error}", partial.display()))?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha1::new();
    let mut actual_size = 0_u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("Download interrupted for {}: {error}", task.label))?;
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("Could not write {}: {error}", partial.display()))?;
        hasher.update(&chunk);
        actual_size += chunk.len() as u64;
        downloaded.fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
    file.flush()
        .await
        .map_err(|error| format!("Could not finish {}: {error}", partial.display()))?;
    drop(file);

    let actual_sha1 = format!("{:x}", hasher.finalize());
    if let Some(expected) = &task.sha1 {
        if !actual_sha1.eq_ignore_ascii_case(expected) {
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(format!(
                "SHA-1 verification failed for {}. Expected {}, received {}.",
                task.label, expected, actual_sha1
            ));
        }
    }
    if task.size > 0 && actual_size != task.size {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(format!(
            "Size verification failed for {}. Expected {} bytes, received {}.",
            task.label, task.size, actual_size
        ));
    }

    if task.destination.exists() {
        tokio::fs::remove_file(&task.destination)
            .await
            .map_err(|error| format!("Could not replace {}: {error}", task.destination.display()))?;
    }
    tokio::fs::rename(&partial, &task.destination)
        .await
        .map_err(|error| format!("Could not finish {}: {error}", task.destination.display()))?;

    let files = completed_files.fetch_add(1, Ordering::Relaxed) + 1;
    emit_progress(
        &app,
        "downloading",
        format!("Installed {}", task.label),
        downloaded.load(Ordering::Relaxed),
        total_bytes,
        files,
        total_files,
    );
    Ok(())
}

fn safe_asset_logical_path(logical_name: &str) -> Result<PathBuf, String> {
    let path = Path::new(logical_name);
    if path.components().all(|component| matches!(component, Component::Normal(_))) {
        Ok(path.to_path_buf())
    } else {
        Err(format!("Asset index contains an unsafe logical path: {logical_name}"))
    }
}

fn copy_verified_asset(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| format!("Could not replace {}: {error}", destination.display()))?;
    }
    fs::hard_link(source, destination).or_else(|_| fs::copy(source, destination).map(|_| ()))
        .map_err(|error| {
            format!(
                "Could not materialize verified asset {} at {}: {error}",
                source.display(),
                destination.display()
            )
        })
}

fn materialize_legacy_assets(
    asset_index: &AssetIndex,
    asset_index_id: &str,
    instance_root: &Path,
) -> Result<(), String> {
    if !asset_index.virtual_assets && !asset_index.map_to_resources {
        return Ok(());
    }

    for (logical_name, asset) in &asset_index.objects {
        let prefix = asset
            .hash
            .get(0..2)
            .ok_or_else(|| format!("Asset '{logical_name}' has an invalid content hash."))?;
        let source = instance_root
            .join("assets")
            .join("objects")
            .join(prefix)
            .join(&asset.hash);
        let relative = safe_asset_logical_path(logical_name)?;

        if asset_index.virtual_assets {
            copy_verified_asset(
                &source,
                &instance_root
                    .join("assets")
                    .join("virtual")
                    .join(asset_index_id)
                    .join(&relative),
            )?;
        }
        if asset_index.map_to_resources {
            copy_verified_asset(
                &source,
                &instance_root.join(".minecraft").join("resources").join(&relative),
            )?;
        }
    }
    Ok(())
}

fn extract_natives(archives: &[NativeArchive], target: &Path) -> Result<(), String> {
    if target.exists() {
        fs::remove_dir_all(target)
            .map_err(|error| format!("Could not reset {}: {error}", target.display()))?;
    }
    fs::create_dir_all(target)
        .map_err(|error| format!("Could not create {}: {error}", target.display()))?;

    for archive in archives {
        let file = File::open(&archive.path)
            .map_err(|error| format!("Could not open {}: {error}", archive.path.display()))?;
        let mut zip = zip::ZipArchive::new(file)
            .map_err(|error| format!("Could not read {}: {error}", archive.path.display()))?;

        for index in 0..zip.len() {
            let mut entry = zip
                .by_index(index)
                .map_err(|error| format!("Could not read native archive entry: {error}"))?;
            let raw_name = entry.name().replace('\\', "/");
            if raw_name.ends_with('/')
                || raw_name.starts_with("META-INF/")
                || archive.excludes.iter().any(|prefix| raw_name.starts_with(prefix))
            {
                continue;
            }
            let Some(enclosed) = entry.enclosed_name().map(Path::to_path_buf) else {
                continue;
            };
            let output = target.join(enclosed);
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
            }
            let mut output_file = File::create(&output)
                .map_err(|error| format!("Could not create {}: {error}", output.display()))?;
            std::io::copy(&mut entry, &mut output_file)
                .map_err(|error| format!("Could not extract {}: {error}", output.display()))?;
        }
    }
    Ok(())
}

fn parse_service_error(status: reqwest::StatusCode, body: &str, service: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        let message = value
            .get("error_description")
            .or_else(|| value.get("errorMessage"))
            .or_else(|| value.get("message"))
            .or_else(|| value.get("error"))
            .and_then(Value::as_str)
            .unwrap_or(body);
        return format!("{service} failed ({status}): {message}");
    }
    format!("{service} failed ({status}): {body}")
}

async fn post_json_checked<T>(
    client: &Client,
    url: &str,
    body: Value,
    headers: &[(&str, &str)],
    service: &str,
) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let mut request = client.post(url).json(&body);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("{service}: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("{service}: could not read response: {error}"))?;
    if !status.is_success() {
        return Err(parse_service_error(status, &text, service));
    }
    serde_json::from_str(&text).map_err(|error| format!("{service}: invalid JSON: {error}"))
}

async fn refresh_microsoft_access_token(
    client: &Client,
    refresh_token: &str,
) -> Result<MicrosoftTokenResponse, String> {
    let response = client
        .post(MICROSOFT_TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", MICROSOFT_CLIENT_ID),
            ("refresh_token", refresh_token),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await
        .map_err(|error| format!("Could not refresh the Microsoft session: {error}"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(parse_service_error(
            status,
            &text,
            "Microsoft session refresh",
        ));
    }
    serde_json::from_str(&text)
        .map_err(|error| format!("Microsoft returned an invalid refresh response: {error}"))
}

async fn exchange_minecraft_account(
    client: &Client,
    microsoft_token: &MicrosoftTokenResponse,
) -> Result<AccountSession, String> {
    let xbox_user: XboxTokenResponse = post_json_checked(
        client,
        XBOX_USER_AUTH_URL,
        json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={}", microsoft_token.access_token)
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }),
        &[("x-xbl-contract-version", "1")],
        "Xbox user authentication",
    )
    .await?;

    let xsts: XboxTokenResponse = post_json_checked(
        client,
        XBOX_XSTS_URL,
        json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbox_user.token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }),
        &[("x-xbl-contract-version", "1")],
        "Xbox XSTS authorization",
    )
    .await?;

    let xbox_claim = xsts
        .display_claims
        .xui
        .first()
        .ok_or_else(|| "Xbox XSTS response did not include a user claim.".to_string())?;
    let user_hash = xbox_claim.uhs.clone();
    let xuid = xbox_claim.xid.clone();

    let minecraft_login: MinecraftLoginResponse = post_json_checked(
        client,
        MINECRAFT_LOGIN_URL,
        json!({
            "identityToken": format!("XBL3.0 x={user_hash};{}", xsts.token)
        }),
        &[],
        "Minecraft Services authentication",
    )
    .await
    .map_err(|error| {
        if error.contains("Invalid app registration") {
            format!(
                "{error}\n\nYour Microsoft client ID has not been approved for Minecraft Services. See https://aka.ms/AppRegInfo."
            )
        } else {
            error
        }
    })?;

    let entitlement_response = client
        .get(MINECRAFT_ENTITLEMENTS_URL)
        .bearer_auth(&minecraft_login.access_token)
        .send()
        .await
        .map_err(|error| format!("Minecraft entitlement check: {error}"))?;
    let entitlement_status = entitlement_response.status();
    let entitlement_text = entitlement_response.text().await.unwrap_or_default();
    if !entitlement_status.is_success() {
        return Err(parse_service_error(
            entitlement_status,
            &entitlement_text,
            "Minecraft entitlement check",
        ));
    }
    let entitlements: MinecraftEntitlements = serde_json::from_str(&entitlement_text)
        .map_err(|error| format!("Minecraft entitlement check returned invalid JSON: {error}"))?;
    if entitlements.items.is_empty() {
        return Err(
            "This Microsoft account does not appear to own Minecraft: Java Edition or have an active entitlement."
                .to_string(),
        );
    }

    let profile_response = client
        .get(MINECRAFT_PROFILE_URL)
        .bearer_auth(&minecraft_login.access_token)
        .send()
        .await
        .map_err(|error| format!("Minecraft profile lookup: {error}"))?;
    let profile_status = profile_response.status();
    let profile_text = profile_response.text().await.unwrap_or_default();
    if !profile_status.is_success() {
        return Err(parse_service_error(
            profile_status,
            &profile_text,
            "Minecraft profile lookup",
        ));
    }
    let profile: MinecraftProfile = serde_json::from_str(&profile_text)
        .map_err(|error| format!("Minecraft profile lookup returned invalid JSON: {error}"))?;

    Ok(AccountSession {
        summary: AccountSummary {
            id: profile.id,
            name: profile.name,
            entitlement_count: entitlements.items.len(),
            session_only: true,
        },
        minecraft_access_token: minecraft_login.access_token,
        xuid,
        microsoft_refresh_token: microsoft_token.refresh_token.clone(),
        minecraft_expires_at: unix_timestamp_u64() + minecraft_login.expires_in,
        microsoft_expires_at: unix_timestamp_u64() + microsoft_token.expires_in,
    })
}

#[tauri::command]
fn launcher_info() -> LauncherInfo {
    LauncherInfo {
        name: "Ruby Launcher",
        version: env!("CARGO_PKG_VERSION"),
        build: BUILD_ID,
        platform: platform_name(),
        native_backend: true,
    }
}

fn apply_launcher_window_settings(app: &AppHandle, settings: &LauncherSettings) -> Result<(), String> {
    // Desktop mode is always a normal window; controller mode follows the fullscreen switch.
    let fullscreen = settings.ui_mode != "desktop" && settings.launcher_fullscreen;
    if let Some(window) = app.get_webview_window("main") {
        let was_fullscreen = window.is_fullscreen().unwrap_or(false);
        window
            .set_decorations(!fullscreen)
            .map_err(|error| format!("Could not change launcher window decorations: {error}"))?;
        window
            .set_fullscreen(fullscreen)
            .map_err(|error| format!("Could not change launcher fullscreen mode: {error}"))?;
        if was_fullscreen && !fullscreen {
            // Leaving fullscreen into a window the size of the screen feels like nothing happened.
            let _ = window.set_size(tauri::LogicalSize::new(1280.0, 800.0));
            let _ = window.center();
        }
    }
    Ok(())
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<LauncherSettings, String> {
    let path = path_for(&app, SETTINGS_FILE)?;
    if !path.exists() {
        let settings = LauncherSettings::default();
        write_json(&path, &settings)?;
        apply_launcher_window_settings(&app, &settings)?;
        append_log(&app, "Created default launcher settings.");
        return Ok(settings);
    }
    let settings: LauncherSettings = read_json(&path)?;
    apply_launcher_window_settings(&app, &settings)?;
    Ok(settings)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: LauncherSettings) -> Result<LauncherSettings, String> {
    if !(768..=16_384).contains(&settings.memory_mb) {
        return Err("Memory allocation must be between 768 MB and 16384 MB.".to_string());
    }
    if !(0.05..=0.75).contains(&settings.controller_deadzone) {
        return Err("Controller deadzone must be between 5% and 75%.".to_string());
    }
    if !["controller", "desktop"].contains(&settings.ui_mode.as_str()) {
        return Err("Interface mode must be Controller or Desktop.".to_string());
    }
    if !["stable", "nightly"].contains(&settings.ruby_channel.as_str()) {
        return Err("Ruby Edition channel must be Stable or Nightly.".to_string());
    }
    let path = path_for(&app, SETTINGS_FILE)?;
    write_json(&path, &settings)?;
    apply_launcher_window_settings(&app, &settings)?;
    append_log(&app, "Launcher settings saved.");
    Ok(settings)
}

#[tauri::command]
fn list_instances(app: AppHandle) -> Result<Vec<InstanceProfile>, String> {
    load_instances_internal(&app)
}

fn validate_profile_runtime(profile: &UpdateInstanceRequest) -> Result<(), String> {
    if profile.name.trim().is_empty() || profile.name.trim().len() > 48 {
        return Err("Profile name must contain between 1 and 48 characters.".to_string());
    }
    if !(768..=16_384).contains(&profile.memory_mb) {
        return Err("Profile memory must be between 768 MB and 16384 MB.".to_string());
    }
    if !(256..=profile.memory_mb).contains(&profile.initial_memory_mb) {
        return Err("Initial memory must be at least 256 MB and no higher than the maximum memory.".to_string());
    }
    if !matches!(profile.java_runtime.as_str(), "automatic" | "java8" | "java17" | "java21" | "java25") {
        return Err("Unsupported Java runtime preference.".to_string());
    }
    if !matches!(profile.renderer.as_str(), "automatic" | "zink" | "angle" | "gl4es") {
        return Err("Unsupported renderer preference.".to_string());
    }
    if !matches!(profile.jvm_preset.as_str(), "standard" | "balanced" | "low-memory" | "performance") {
        return Err("Unsupported JVM preset.".to_string());
    }
    if !(640..=7680).contains(&profile.resolution_width) || !(480..=4320).contains(&profile.resolution_height) {
        return Err("Game resolution is outside Ruby Launcher's supported range.".to_string());
    }
    Ok(())
}

#[tauri::command]
fn update_instance(app: AppHandle, request: UpdateInstanceRequest) -> Result<InstanceProfile, String> {
    validate_profile_runtime(&request)?;
    let mut instances = load_instances_internal(&app)?;
    let instance = instances
        .iter_mut()
        .find(|instance| instance.id == request.instance_id)
        .ok_or_else(|| "Instance not found.".to_string())?;

    instance.name = request.name.trim().to_string();
    instance.memory_mb = request.memory_mb;
    instance.initial_memory_mb = request.initial_memory_mb;
    instance.java_runtime = request.java_runtime;
    instance.renderer = request.renderer;
    instance.jvm_preset = request.jvm_preset;
    instance.controller_support_enabled = request.controller_support_enabled;
    instance.game_fullscreen = request.game_fullscreen;
    instance.resolution_width = request.resolution_width;
    instance.resolution_height = request.resolution_height;

    let updated = instance.clone();
    let instance_root = launcher_root(&app)?.join("instances").join(&updated.id);
    write_json(&instance_root.join("instance.json"), &updated)?;
    save_instances_internal(&app, &instances)?;
    append_log(&app, &format!("Updated profile settings for '{}'.", updated.name));
    Ok(updated)
}

#[tauri::command]
fn create_instance(
    app: AppHandle,
    request: CreateInstanceRequest,
) -> Result<InstanceProfile, String> {
    let name = request.name.trim();
    let minecraft_version = request.minecraft_version.trim();

    if name.is_empty() || name.len() > 48 {
        return Err("Instance name must contain between 1 and 48 characters.".to_string());
    }
    if minecraft_version.is_empty() || minecraft_version.len() > 40 {
        return Err("Minecraft version ID must contain between 1 and 40 characters.".to_string());
    }
    validate_loader(&request.loader)?;

    let mut instances = load_instances_internal(&app)?;
    let settings = load_settings(app.clone()).unwrap_or_default();
    let id = unique_instance_id(name, &instances);
    let instance_root = launcher_root(&app)?.join("instances").join(&id);
    create_instance_directories(&instance_root)?;

    let profile = InstanceProfile {
        id: id.clone(),
        name: name.to_string(),
        minecraft_version: minecraft_version.to_string(),
        loader: request.loader,
        status: "Not installed".to_string(),
        game_dir: instance_root
            .join(".minecraft")
            .to_string_lossy()
            .into_owned(),
        created_at: unix_timestamp(),
        memory_mb: settings.memory_mb.max(768),
        initial_memory_mb: 512,
        java_runtime: settings.java_runtime,
        renderer: settings.renderer,
        jvm_preset: "balanced".to_string(),
        controller_support_enabled: settings.controller_support_enabled,
        game_fullscreen: true,
        resolution_width: 1280,
        resolution_height: 720,
        source: "manual".to_string(),
        loader_version: None,
        launch_version: None,
        modpack_project_id: None,
        modpack_version_id: None,
        modpack_name: None,
    };

    write_json(&instance_root.join("instance.json"), &profile)?;
    instances.push(profile.clone());
    save_instances_internal(&app, &instances)?;
    append_log(
        &app,
        &format!(
            "Created instance '{}' for Minecraft {} with loader profile '{}'.",
            profile.name, profile.minecraft_version, profile.loader
        ),
    );
    Ok(profile)
}

#[tauri::command]
fn delete_instance(app: AppHandle, instance_id: String) -> Result<bool, String> {
    let mut instances = load_instances_internal(&app)?;
    let Some(index) = instances.iter().position(|instance| instance.id == instance_id) else {
        return Err("Instance not found.".to_string());
    };

    let removed = instances.remove(index);
    let instance_root = launcher_root(&app)?.join("instances").join(&removed.id);
    if instance_root.exists() {
        fs::remove_dir_all(&instance_root)
            .map_err(|error| format!("Could not remove {}: {error}", instance_root.display()))?;
    }
    save_instances_internal(&app, &instances)?;
    append_log(&app, &format!("Removed instance '{}'.", removed.name));
    Ok(true)
}

#[tauri::command]
fn prepare_instance(app: AppHandle, instance_id: String) -> Result<PrepareResult, String> {
    let instances = load_instances_internal(&app)?;
    let instance = instances
        .iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| "Instance not found.".to_string())?;

    let instance_root = launcher_root(&app)?.join("instances").join(&instance.id);
    create_instance_directories(&instance_root)?;
    write_json(&instance_root.join("instance.json"), instance)?;
    append_log(&app, &format!("Prepared directories for '{}'.", instance.name));

    Ok(PrepareResult {
        instance_id: instance.id.clone(),
        message: "Instance directories are ready.".to_string(),
    })
}

#[tauri::command]
async fn fetch_version_catalog(state: State<'_, AppState>) -> Result<VersionCatalog, String> {
    let manifest: VersionManifest =
        fetch_json(&state.client, VERSION_MANIFEST_URL, "Minecraft version catalog").await?;
    let versions = manifest
        .versions
        .into_iter()
        .filter(|version| version.version_type == "release" || version.version_type == "snapshot")
        .take(80)
        .collect();
    Ok(VersionCatalog {
        latest_release: manifest.latest.release,
        latest_snapshot: manifest.latest.snapshot,
        versions,
    })
}


#[derive(Debug, Clone)]
struct ModpackDownloadTask {
    urls: Vec<String>,
    destination: PathBuf,
    sha1: Option<String>,
    sha512: Option<String>,
    label: String,
}

fn safe_pack_path(raw: &str) -> Result<PathBuf, String> {
    let normalized = raw.replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.is_empty()
        || path.is_absolute()
        || !path.components().all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(format!("Modpack contains an unsafe path: {raw}"));
    }
    Ok(path.to_path_buf())
}

fn sha512_hex_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Could not open {} for SHA-512 verification: {error}", path.display()))?;
    let mut hasher = Sha512::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not read {} for SHA-512 verification: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn pack_file_valid(task: &ModpackDownloadTask) -> bool {
    if !task.destination.is_file() {
        return false;
    }
    if let Some(expected) = &task.sha512 {
        return sha512_hex_file(&task.destination)
            .map(|actual| actual.eq_ignore_ascii_case(expected))
            .unwrap_or(false);
    }
    if let Some(expected) = &task.sha1 {
        return sha1_hex_file(&task.destination)
            .map(|actual| actual.eq_ignore_ascii_case(expected))
            .unwrap_or(false);
    }
    false
}

async fn ensure_modpack_download(
    client: Client,
    task: ModpackDownloadTask,
    app: AppHandle,
    completed: Arc<AtomicUsize>,
    total_files: usize,
) -> Result<(), String> {
    if pack_file_valid(&task) {
        let files = completed.fetch_add(1, Ordering::Relaxed) + 1;
        emit_progress(
            &app,
            "modpack-files",
            format!("Found {}", task.label),
            0,
            0,
            files,
            total_files,
        );
        return Ok(());
    }
    if task.urls.is_empty() {
        return Err(format!("{} does not provide a download URL.", task.label));
    }
    if let Some(parent) = task.destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }

    let mut last_error = String::new();
    for url in &task.urls {
        emit_progress(
            &app,
            "modpack-files",
            format!("Downloading {}", task.label),
            0,
            0,
            completed.load(Ordering::Relaxed),
            total_files,
        );
        let response = match client.get(url).send().await {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                last_error = format!("HTTP {} from {url}", response.status());
                continue;
            }
            Err(error) => {
                last_error = error.to_string();
                continue;
            }
        };

        let partial = task.destination.with_extension("ruby-part");
        let mut output = tokio::fs::File::create(&partial)
            .await
            .map_err(|error| format!("Could not create {}: {error}", partial.display()))?;
        let mut sha1 = Sha1::new();
        let mut sha512 = Sha512::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| format!("Download interrupted for {}: {error}", task.label))?;
            output
                .write_all(&chunk)
                .await
                .map_err(|error| format!("Could not write {}: {error}", partial.display()))?;
            sha1.update(&chunk);
            sha512.update(&chunk);
        }
        output
            .flush()
            .await
            .map_err(|error| format!("Could not finish {}: {error}", partial.display()))?;
        drop(output);

        let actual_sha1 = format!("{:x}", sha1.finalize());
        let actual_sha512 = format!("{:x}", sha512.finalize());
        let valid_sha1 = task
            .sha1
            .as_ref()
            .map(|expected| actual_sha1.eq_ignore_ascii_case(expected))
            .unwrap_or(true);
        let valid_sha512 = task
            .sha512
            .as_ref()
            .map(|expected| actual_sha512.eq_ignore_ascii_case(expected))
            .unwrap_or(true);
        if !valid_sha1 || !valid_sha512 {
            let _ = tokio::fs::remove_file(&partial).await;
            last_error = "download failed the modpack hash verification".to_string();
            continue;
        }
        if task.destination.exists() {
            tokio::fs::remove_file(&task.destination)
                .await
                .map_err(|error| format!("Could not replace {}: {error}", task.destination.display()))?;
        }
        tokio::fs::rename(&partial, &task.destination)
            .await
            .map_err(|error| format!("Could not finish {}: {error}", task.destination.display()))?;
        let files = completed.fetch_add(1, Ordering::Relaxed) + 1;
        emit_progress(
            &app,
            "modpack-files",
            format!("Installed {}", task.label),
            0,
            0,
            files,
            total_files,
        );
        return Ok(());
    }
    Err(format!("Could not download {}: {last_error}", task.label))
}

fn read_mrpack_index(archive_bytes: &[u8]) -> Result<MrpackIndex, String> {
    let cursor = Cursor::new(archive_bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|error| format!("The Modrinth pack file is not a valid archive: {error}"))?;
    let mut index_file = archive
        .by_name("modrinth.index.json")
        .map_err(|_| "The Modrinth pack is missing modrinth.index.json.".to_string())?;
    let mut json = String::new();
    index_file
        .read_to_string(&mut json)
        .map_err(|error| format!("Could not read modrinth.index.json: {error}"))?;
    let index: MrpackIndex = serde_json::from_str(&json)
        .map_err(|error| format!("modrinth.index.json is invalid: {error}"))?;
    if index.format_version != 1 || index.game != "minecraft" {
        return Err(format!(
            "Ruby Launcher supports Minecraft Modrinth pack format 1; this pack reports format {} for '{}'.",
            index.format_version, index.game
        ));
    }
    Ok(index)
}

fn extract_mrpack_overrides(archive_bytes: &[u8], game_directory: &Path) -> Result<usize, String> {
    let mut extracted = 0;
    for prefix in ["overrides/", "client-overrides/"] {
        let cursor = Cursor::new(archive_bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|error| format!("Could not reopen Modrinth pack archive: {error}"))?;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| format!("Could not read Modrinth override entry: {error}"))?;
            let raw = entry.name().replace('\\', "/");
            if entry.is_dir() || !raw.starts_with(prefix) {
                continue;
            }
            let relative_raw = &raw[prefix.len()..];
            if relative_raw.is_empty() {
                continue;
            }
            let relative = safe_pack_path(relative_raw)?;
            let destination = game_directory.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
            }
            let mut output = File::create(&destination)
                .map_err(|error| format!("Could not create {}: {error}", destination.display()))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|error| format!("Could not extract {}: {error}", destination.display()))?;
            extracted += 1;
        }
    }
    Ok(extracted)
}

fn loader_from_pack(index: &MrpackIndex) -> Result<(String, Option<String>), String> {
    if let Some(version) = index.dependencies.get("fabric-loader") {
        return Ok(("fabric".to_string(), Some(version.clone())));
    }
    if let Some(version) = index.dependencies.get("quilt-loader") {
        return Ok(("quilt".to_string(), Some(version.clone())));
    }
    if index.dependencies.contains_key("forge") {
        return Err("This pack uses Forge. Ruby's first Modrinth installer supports Fabric, Quilt, and vanilla packs; Forge installer processing is not connected yet.".to_string());
    }
    if index.dependencies.contains_key("neoforge") {
        return Err("This pack uses NeoForge. Ruby's first Modrinth installer supports Fabric, Quilt, and vanilla packs; NeoForge installer processing is not connected yet.".to_string());
    }
    Ok(("none".to_string(), None))
}

async fn download_loader_libraries(
    app: &AppHandle,
    state: &AppState,
    instance_root: &Path,
    libraries: &[LibraryEntry],
) -> Result<(), String> {
    let mut tasks = Vec::new();
    let mut seen = HashSet::new();
    for library in libraries {
        if !library_allowed(library.rules.as_ref()) {
            continue;
        }
        if let Some(downloads) = &library.downloads {
            if let Some(artifact) = &downloads.artifact {
                let path = artifact.path.clone().unwrap_or(maven_path(&library.name)?);
                add_task(
                    &mut tasks,
                    &mut seen,
                    DownloadTask {
                        url: artifact.url.clone(),
                        destination: instance_root.join("libraries").join(path),
                        sha1: Some(artifact.sha1.clone()),
                        size: artifact.size,
                        label: format!("loader library {}", library.name),
                    },
                );
            }
        } else {
            let path = maven_path(&library.name)?;
            let base = library.url.as_deref().unwrap_or(DEFAULT_LIBRARY_BASE_URL);
            add_task(
                &mut tasks,
                &mut seen,
                DownloadTask {
                    url: join_url(base, &path),
                    destination: instance_root.join("libraries").join(path),
                    sha1: None,
                    size: 0,
                    label: format!("loader library {}", library.name),
                },
            );
        }
    }
    let total_files = tasks.len();
    if total_files == 0 {
        return Ok(());
    }
    let total_bytes = tasks.iter().map(|task| task.size).sum();
    let downloaded = Arc::new(AtomicU64::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    let results = futures_util::stream::iter(tasks.into_iter().map(|task| {
        ensure_download(
            state.client.clone(),
            task,
            app.clone(),
            downloaded.clone(),
            completed.clone(),
            total_bytes,
            total_files,
            false,
        )
    }))
    .buffer_unordered(6)
    .collect::<Vec<_>>()
    .await;
    for result in results {
        result?;
    }
    Ok(())
}

fn merge_argument_arrays(base: &mut Value, loader: &Value, key: &str) {
    let loader_values = loader
        .get("arguments")
        .and_then(|arguments| arguments.get(key))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if loader_values.is_empty() {
        return;
    }
    if base.get("arguments").and_then(Value::as_object).is_none() {
        base["arguments"] = json!({"game": [], "jvm": []});
    }
    if base["arguments"].get(key).and_then(Value::as_array).is_none() {
        base["arguments"][key] = Value::Array(Vec::new());
    }
    if let Some(target) = base["arguments"][key].as_array_mut() {
        target.extend(loader_values);
    }
}

async fn install_loader_profile(
    app: &AppHandle,
    state: &AppState,
    instance: &InstanceProfile,
    loader: &str,
    loader_version: &str,
) -> Result<String, String> {
    if loader == "none" {
        return Ok(instance.minecraft_version.clone());
    }
    let endpoint = match loader {
        "fabric" => format!(
            "{FABRIC_META_BASE}/versions/loader/{}/{}/profile/json",
            instance.minecraft_version, loader_version
        ),
        "quilt" => format!(
            "{QUILT_META_BASE}/versions/loader/{}/{}/profile/json",
            instance.minecraft_version, loader_version
        ),
        other => return Err(format!("Unsupported Modrinth loader: {other}")),
    };
    emit_progress(
        app,
        "loader",
        format!("Preparing {loader} loader {loader_version}…"),
        0,
        0,
        0,
        1,
    );
    let loader_profile: Value = fetch_json(&state.client, &endpoint, "mod loader profile").await?;
    let launch_id = loader_profile
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "The loader profile did not include a launch ID.".to_string())?
        .to_string();
    let instance_root = launcher_root(app)?.join("instances").join(&instance.id);
    let base_dir = instance_root.join("versions").join(&instance.minecraft_version);
    let base_json_path = base_dir.join(format!("{}.json", instance.minecraft_version));
    let mut merged: Value = read_json(&base_json_path)?;

    merged["id"] = Value::String(launch_id.clone());
    if let Some(main_class) = loader_profile.get("mainClass") {
        merged["mainClass"] = main_class.clone();
    }
    merge_argument_arrays(&mut merged, &loader_profile, "jvm");
    merge_argument_arrays(&mut merged, &loader_profile, "game");
    if let Some(loader_legacy) = loader_profile.get("minecraftArguments").and_then(Value::as_str) {
        let base_legacy = merged
            .get("minecraftArguments")
            .and_then(Value::as_str)
            .unwrap_or_default();
        merged["minecraftArguments"] = Value::String(format!("{base_legacy} {loader_legacy}").trim().to_string());
    }

    let loader_library_values = loader_profile
        .get("libraries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if merged.get("libraries").and_then(Value::as_array).is_none() {
        merged["libraries"] = Value::Array(Vec::new());
    }
    if let Some(base_libraries) = merged["libraries"].as_array_mut() {
        base_libraries.extend(loader_library_values.clone());
    }
    let loader_libraries: Vec<LibraryEntry> = serde_json::from_value(Value::Array(loader_library_values))
        .map_err(|error| format!("The loader library list is invalid: {error}"))?;
    download_loader_libraries(app, state, &instance_root, &loader_libraries).await?;

    let launch_dir = instance_root.join("versions").join(&launch_id);
    fs::create_dir_all(&launch_dir)
        .map_err(|error| format!("Could not create {}: {error}", launch_dir.display()))?;
    write_json(&launch_dir.join(format!("{launch_id}.json")), &merged)?;
    let base_jar = base_dir.join(format!("{}.jar", instance.minecraft_version));
    fs::copy(&base_jar, launch_dir.join(format!("{launch_id}.jar")))
        .map_err(|error| format!("Could not prepare the loader client JAR: {error}"))?;
    let base_mappings = base_dir.join(format!("{}-client-mappings.txt", instance.minecraft_version));
    if base_mappings.exists() {
        fs::copy(
            &base_mappings,
            launch_dir.join(format!("{launch_id}-client-mappings.txt")),
        )
        .map_err(|error| format!("Could not prepare controller mappings for the loader profile: {error}"))?;
    }
    emit_progress(
        app,
        "loader",
        format!("{loader} loader {loader_version} is ready."),
        0,
        0,
        1,
        1,
    );
    Ok(launch_id)
}

#[tauri::command]
async fn search_modrinth_modpacks(
    state: State<'_, AppState>,
    query: String,
    index: String,
    offset: u64,
    limit: u64,
) -> Result<ModrinthSearchResponse, String> {
    let sort = match index.as_str() {
        "downloads" | "updated" | "newest" | "relevance" => index,
        _ => "relevance".to_string(),
    };
    let facets = serde_json::to_string(&vec![vec!["project_type:modpack"]])
        .map_err(|error| format!("Could not create Modrinth search filters: {error}"))?;
    let response = state
        .client
        .get(format!("{MODRINTH_API_BASE}/search"))
        .query(&[
            ("query", query),
            ("facets", facets),
            ("index", sort),
            ("offset", offset.min(10_000).to_string()),
            ("limit", limit.clamp(1, 50).to_string()),
        ])
        .send()
        .await
        .map_err(|error| format!("Modrinth search failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Could not read Modrinth search results: {error}"))?;
    if !status.is_success() {
        return Err(parse_service_error(status, &body, "Modrinth search"));
    }
    serde_json::from_str(&body).map_err(|error| format!("Modrinth search returned invalid JSON: {error}"))
}

#[tauri::command]
async fn list_modrinth_modpack_versions(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<ModrinthVersionSummary>, String> {
    let endpoint = format!(
        "{MODRINTH_API_BASE}/project/{}/version?include_changelog=false",
        project_id.trim()
    );
    let mut versions: Vec<ModrinthVersionSummary> =
        fetch_json(&state.client, &endpoint, "Modrinth pack versions").await?;
    versions.retain(|version| {
        version
            .files
            .iter()
            .any(|file| file.filename.to_ascii_lowercase().ends_with(".mrpack"))
    });
    versions.truncate(40);
    Ok(versions)
}

#[tauri::command]
async fn install_modrinth_modpack(
    app: AppHandle,
    state: State<'_, AppState>,
    request: InstallModrinthRequest,
) -> Result<InstanceProfile, String> {
    let version_endpoint = format!("{MODRINTH_API_BASE}/version/{}", request.version_id.trim());
    let version: ModrinthVersionSummary =
        fetch_json(&state.client, &version_endpoint, "Modrinth pack version").await?;
    if version.project_id != request.project_id {
        return Err("The selected Modrinth version does not belong to the selected project.".to_string());
    }
    let pack_file = version
        .files
        .iter()
        .find(|file| file.primary && file.filename.to_ascii_lowercase().ends_with(".mrpack"))
        .or_else(|| version.files.iter().find(|file| file.filename.to_ascii_lowercase().ends_with(".mrpack")))
        .ok_or_else(|| "The selected Modrinth version does not provide an .mrpack file.".to_string())?;

    emit_progress(&app, "modpack", "Downloading the Modrinth pack manifest…", 0, pack_file.size, 0, 1);
    let response = state
        .client
        .get(&pack_file.url)
        .send()
        .await
        .map_err(|error| format!("Could not download the Modrinth pack: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Could not download the Modrinth pack ({}).", response.status()));
    }
    let pack_bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Could not read the Modrinth pack: {error}"))?
        .to_vec();
    if let Some(expected) = pack_file.hashes.get("sha512") {
        let mut hasher = Sha512::new();
        hasher.update(&pack_bytes);
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            return Err("The Modrinth pack failed SHA-512 verification.".to_string());
        }
    } else if let Some(expected) = pack_file.hashes.get("sha1") {
        if !sha1_hex_bytes(&pack_bytes).eq_ignore_ascii_case(expected) {
            return Err("The Modrinth pack failed SHA-1 verification.".to_string());
        }
    } else {
        return Err("The selected Modrinth pack did not include a SHA-512 or SHA-1 hash.".to_string());
    }
    let index = read_mrpack_index(&pack_bytes)?;
    let minecraft_version = index
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or_else(|| "The Modrinth pack does not declare a Minecraft version.".to_string())?;
    let (loader, loader_version) = loader_from_pack(&index)?;

    let settings = load_settings(app.clone()).unwrap_or_default();
    let mut instances = load_instances_internal(&app)?;
    let requested_name = if request.name.trim().is_empty() {
        index.name.as_str()
    } else {
        request.name.trim()
    };
    let profile_name: String = requested_name.chars().take(48).collect();
    let id = unique_instance_id(&profile_name, &instances);
    let instance_root = launcher_root(&app)?.join("instances").join(&id);
    create_instance_directories(&instance_root)?;
    let profile = InstanceProfile {
        id: id.clone(),
        name: profile_name,
        minecraft_version: minecraft_version.clone(),
        loader: loader.clone(),
        status: "Installing modpack".to_string(),
        game_dir: instance_root.join(".minecraft").to_string_lossy().into_owned(),
        created_at: unix_timestamp(),
        memory_mb: settings.memory_mb.max(768),
        initial_memory_mb: 512,
        java_runtime: settings.java_runtime.clone(),
        renderer: settings.renderer.clone(),
        jvm_preset: "balanced".to_string(),
        controller_support_enabled: request.controller_support_enabled,
        game_fullscreen: true,
        resolution_width: 1280,
        resolution_height: 720,
        source: "modrinth".to_string(),
        loader_version: loader_version.clone(),
        launch_version: None,
        modpack_project_id: Some(request.project_id.clone()),
        modpack_version_id: Some(request.version_id.clone()),
        modpack_name: Some(index.name.clone()),
    };
    write_json(&instance_root.join("instance.json"), &profile)?;
    instances.push(profile.clone());
    save_instances_internal(&app, &instances)?;

    let install_result = async {
        install_vanilla_internal(app.clone(), state.inner(), id.clone(), false).await?;
        let launch_version = install_loader_profile(
            &app,
            state.inner(),
            &profile,
            &loader,
            loader_version.as_deref().unwrap_or_default(),
        )
        .await?;

        let game_directory = instance_root.join(".minecraft");
        let mut tasks = Vec::new();
        let mut seen_paths = HashSet::new();
        for file in &index.files {
            if file
                .env
                .as_ref()
                .map(|env| env.client.as_str() == "unsupported")
                .unwrap_or(false)
            {
                continue;
            }
            let relative = safe_pack_path(&file.path)?;
            if !seen_paths.insert(relative.clone()) {
                return Err(format!("The Modrinth pack lists '{}' more than once.", file.path));
            }
            let sha1 = file.hashes.get("sha1").cloned();
            let sha512 = file.hashes.get("sha512").cloned();
            if sha1.is_none() && sha512.is_none() {
                return Err(format!("{} does not include a SHA-512 or SHA-1 hash.", file.path));
            }
            tasks.push(ModpackDownloadTask {
                urls: file.downloads.clone(),
                destination: game_directory.join(relative),
                sha1,
                sha512,
                label: file.path.clone(),
            });
        }
        let total_files = tasks.len();
        let completed = Arc::new(AtomicUsize::new(0));
        emit_progress(
            &app,
            "modpack-files",
            format!("Installing {total_files} modpack files…"),
            0,
            0,
            0,
            total_files,
        );
        let results = futures_util::stream::iter(tasks.into_iter().map(|task| {
            ensure_modpack_download(
                state.client.clone(),
                task,
                app.clone(),
                completed.clone(),
                total_files,
            )
        }))
        .buffer_unordered(8)
        .collect::<Vec<_>>()
        .await;
        for result in results {
            result?;
        }
        emit_progress(&app, "overrides", "Applying modpack configuration and resource overrides…", 0, 0, 0, 1);
        let override_count = extract_mrpack_overrides(&pack_bytes, &game_directory)?;
        write_json(
            &instance_root.join("modrinth-pack.json"),
            &json!({
                "projectId": request.project_id,
                "versionId": request.version_id,
                "packName": index.name,
                "packVersionId": index.version_id,
                "summary": index.summary,
                "minecraftVersion": minecraft_version,
                "loader": loader,
                "loaderVersion": loader_version,
                "fileCount": total_files,
                "overrideCount": override_count,
            }),
        )?;

        let mut updated_instances = load_instances_internal(&app)?;
        let updated = updated_instances
            .iter_mut()
            .find(|instance| instance.id == id)
            .ok_or_else(|| "The installed profile disappeared while saving it.".to_string())?;
        updated.launch_version = Some(launch_version);
        updated.status = "Installed".to_string();
        let final_profile = updated.clone();
        write_json(&instance_root.join("instance.json"), &final_profile)?;
        save_instances_internal(&app, &updated_instances)?;
        emit_progress(
            &app,
            "complete",
            format!("{} is ready.", final_profile.name),
            0,
            0,
            1,
            1,
        );
        Ok::<InstanceProfile, String>(final_profile)
    }
    .await;

    if let Err(error) = &install_result {
        append_log(&app, &format!("Modrinth installation failed for '{}': {error}", profile.name));
        let mut remaining = load_instances_internal(&app).unwrap_or_default();
        remaining.retain(|instance| instance.id != id);
        let _ = save_instances_internal(&app, &remaining);
        let _ = fs::remove_dir_all(&instance_root);
    }
    install_result
}

async fn install_vanilla_internal(
    app: AppHandle,
    state: &AppState,
    instance_id: String,
    verify_existing: bool,
) -> Result<InstallResult, String> {
    let instances = load_instances_internal(&app)?;
    let instance = instances
        .iter()
        .find(|instance| instance.id == instance_id)
        .cloned()
        .ok_or_else(|| "Instance not found.".to_string())?;
    if cfg!(target_os = "android") {
        return Err(
            "Ruby Launcher desktop installs and launches Java Edition profiles. The Android mobile JVM and graphics compatibility layer are a later milestone."
                .to_string(),
        );
    }

    let instance_root = launcher_root(&app)?.join("instances").join(&instance.id);
    create_instance_directories(&instance_root)?;
    update_instance_status(
        &app,
        &instance.id,
        if verify_existing { "Repairing" } else { "Checking files" },
    )?;
    append_log(
        &app,
        &format!(
            "Beginning {} for '{}' ({})",
            if verify_existing { "deep vanilla repair" } else { "quick launch file check" },
            instance.name,
            instance.minecraft_version
        ),
    );
    emit_progress(
        &app,
        "metadata",
        if verify_existing { "Loading Mojang metadata for deep verification…" } else { "Checking local Minecraft metadata…" },
        0,
        0,
        0,
        0,
    );

    let install_result = install_vanilla_files(
        app.clone(),
        state,
        &instance_root,
        &instance.minecraft_version,
        &instance.id,
        verify_existing,
    )
    .await;

    match install_result {
        Ok(result) => {
            update_instance_status(&app, &instance.id, "Installed")?;
            append_log(&app, &result.message);
            Ok(result)
        }
        Err(error) => {
            let _ = update_instance_status(&app, &instance.id, "Install failed");
            append_log(&app, &format!("Installation failed: {error}"));
            emit_progress(&app, "failed", &error, 0, 0, 0, 0);
            Err(error)
        }
    }
}



/// Downloads and verifies one vanilla Minecraft version into `instance_root` using the official
/// launcher's layout (`versions/`, `libraries/`, `assets/`). Shared by Java Edition profiles and
/// Ruby Edition's asset preparation.
async fn install_vanilla_files(
    app: AppHandle,
    state: &AppState,
    instance_root: &Path,
    minecraft_version: &str,
    result_id: &str,
    verify_existing: bool,
) -> Result<InstallResult, String> {
    let version_dir = instance_root
        .join("versions")
        .join(&minecraft_version);
    let version_json_path = version_dir.join(format!("{}.json", minecraft_version));

    let metadata: VersionMetadata = if !verify_existing && version_json_path.exists() {
        let bytes = fs::read(&version_json_path).map_err(|error| {
            format!("Could not read local version metadata {}: {error}", version_json_path.display())
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("Local version metadata is invalid: {error}"))?
    } else {
        let manifest: VersionManifest = fetch_json(
            &state.client,
            VERSION_MANIFEST_URL,
            "Minecraft version catalog",
        )
        .await?;
        let version_entry = manifest
            .versions
            .into_iter()
            .find(|version| version.id == minecraft_version)
            .ok_or_else(|| {
                format!(
                    "Minecraft version '{}' was not found in Mojang's current version catalog.",
                    minecraft_version
                )
            })?;

        emit_progress(&app, "metadata", "Loading version metadata…", 0, 0, 0, 0);
        let version_response = state
            .client
            .get(&version_entry.url)
            .send()
            .await
            .map_err(|error| format!("Could not download version metadata: {error}"))?;
        let version_status = version_response.status();
        if !version_status.is_success() {
            let body = version_response.text().await.unwrap_or_default();
            return Err(format!(
                "Could not download version metadata ({version_status}): {body}"
            ));
        }
        let version_bytes = version_response
            .bytes()
            .await
            .map_err(|error| format!("Could not read version metadata: {error}"))?;
        if !sha1_hex_bytes(&version_bytes).eq_ignore_ascii_case(&version_entry.sha1) {
            return Err("Version metadata failed SHA-1 verification.".to_string());
        }
        let parsed: VersionMetadata = serde_json::from_slice(&version_bytes)
            .map_err(|error| format!("Version metadata is invalid: {error}"))?;
        if parsed.id != version_entry.id {
            return Err(format!(
                "Version metadata ID mismatch. Expected '{}', received '{}'.",
                version_entry.id, parsed.id
            ));
        }
        write_bytes_atomic(&version_json_path, &version_bytes)?;
        parsed
    };

    if metadata.id != minecraft_version {
        return Err(format!(
            "The local version metadata belongs to '{}' instead of '{}'. Run Repair to replace it.",
            metadata.id, minecraft_version
        ));
    }

    let asset_index_path = instance_root
        .join("assets")
        .join("indexes")
        .join(format!("{}.json", metadata.asset_index.id));
    let asset_index: AssetIndex = if !verify_existing && asset_index_path.exists() {
        let bytes = fs::read(&asset_index_path).map_err(|error| {
            format!("Could not read local asset index {}: {error}", asset_index_path.display())
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("Local asset index is invalid: {error}"))?
    } else {
        emit_progress(&app, "metadata", "Loading asset index…", 0, 0, 0, 0);
        let asset_response = state
            .client
            .get(&metadata.asset_index.url)
            .send()
            .await
            .map_err(|error| format!("Could not download asset index: {error}"))?;
        let asset_status = asset_response.status();
        if !asset_status.is_success() {
            let body = asset_response.text().await.unwrap_or_default();
            return Err(format!("Could not download asset index ({asset_status}): {body}"));
        }
        let asset_bytes = asset_response
            .bytes()
            .await
            .map_err(|error| format!("Could not read asset index: {error}"))?;
        if !sha1_hex_bytes(&asset_bytes).eq_ignore_ascii_case(&metadata.asset_index.sha1) {
            return Err("Asset index failed SHA-1 verification.".to_string());
        }
        if asset_bytes.len() as u64 != metadata.asset_index.size {
            return Err("Asset index failed size verification.".to_string());
        }
        let parsed: AssetIndex = serde_json::from_slice(&asset_bytes)
            .map_err(|error| format!("Asset index is invalid: {error}"))?;
        write_bytes_atomic(&asset_index_path, &asset_bytes)?;
        parsed
    };

    let mut tasks = Vec::new();
    let mut seen = HashSet::new();
    let mut native_archives = Vec::new();

    add_task(
        &mut tasks,
        &mut seen,
        DownloadTask {
            url: metadata.downloads.client.url.clone(),
            destination: version_dir.join(format!("{}.jar", metadata.id)),
            sha1: Some(metadata.downloads.client.sha1.clone()),
            size: metadata.downloads.client.size,
            label: format!("Minecraft {} client", metadata.id),
        },
    );

    if let Some(mappings) = &metadata.downloads.client_mappings {
        add_task(
            &mut tasks,
            &mut seen,
            DownloadTask {
                url: mappings.url.clone(),
                destination: version_dir.join(format!("{}-client-mappings.txt", metadata.id)),
                sha1: Some(mappings.sha1.clone()),
                size: mappings.size,
                label: format!("Minecraft {} client mappings", metadata.id),
            },
        );
    }

    for (logical_name, asset) in &asset_index.objects {
        let prefix = asset.hash.get(0..2).ok_or_else(|| {
            format!("Asset '{logical_name}' has an invalid content hash.")
        })?;
        add_task(
            &mut tasks,
            &mut seen,
            DownloadTask {
                url: format!("{ASSET_BASE_URL}/{prefix}/{}", asset.hash),
                destination: instance_root
                    .join("assets")
                    .join("objects")
                    .join(prefix)
                    .join(&asset.hash),
                sha1: Some(asset.hash.clone()),
                size: asset.size,
                label: format!("asset {logical_name}"),
            },
        );
    }

    for library in &metadata.libraries {
        if !library_allowed(library.rules.as_ref()) {
            continue;
        }

        if let Some(downloads) = &library.downloads {
            if let Some(artifact) = &downloads.artifact {
                let path = artifact
                    .path
                    .clone()
                    .unwrap_or(maven_path(&library.name)?);
                add_task(
                    &mut tasks,
                    &mut seen,
                    DownloadTask {
                        url: artifact.url.clone(),
                        destination: instance_root.join("libraries").join(path),
                        sha1: Some(artifact.sha1.clone()),
                        size: artifact.size,
                        label: format!("library {}", library.name),
                    },
                );
            }
        } else {
            let path = maven_path(&library.name)?;
            let base = library.url.as_deref().unwrap_or(DEFAULT_LIBRARY_BASE_URL);
            add_task(
                &mut tasks,
                &mut seen,
                DownloadTask {
                    url: join_url(base, &path),
                    destination: instance_root.join("libraries").join(path),
                    sha1: None,
                    size: 0,
                    label: format!("library {}", library.name),
                },
            );
        }

        if let (Some(natives), Some(downloads)) = (&library.natives, &library.downloads) {
            if let Some(template) = natives.get(minecraft_os_name()) {
                let classifier = template.replace("${arch}", minecraft_arch_token());
                if let Some(descriptor) = downloads
                    .classifiers
                    .as_ref()
                    .and_then(|classifiers| classifiers.get(&classifier))
                {
                    let path = descriptor
                        .path
                        .clone()
                        .unwrap_or_else(|| format!("natives/{}/{}.jar", metadata.id, classifier));
                    let destination = instance_root.join("libraries").join(path);
                    add_task(
                        &mut tasks,
                        &mut seen,
                        DownloadTask {
                            url: descriptor.url.clone(),
                            destination: destination.clone(),
                            sha1: Some(descriptor.sha1.clone()),
                            size: descriptor.size,
                            label: format!("native library {}", library.name),
                        },
                    );
                    native_archives.push(NativeArchive {
                        path: destination,
                        excludes: library
                            .extract
                            .as_ref()
                            .map(|extract| extract.exclude.clone())
                            .unwrap_or_default(),
                    });
                }
            }
        }
    }

    if let Some(logging) = &metadata.logging {
        let file = &logging.client.file;
        let destination = instance_root
            .join("assets")
            .join("log_configs")
            .join(
                file.id
                    .clone()
                    .or_else(|| file.path.clone())
                    .unwrap_or_else(|| "client.xml".to_string()),
            );
        add_task(
            &mut tasks,
            &mut seen,
            DownloadTask {
                url: file.url.clone(),
                destination,
                sha1: Some(file.sha1.clone()),
                size: file.size,
                label: "logging configuration".to_string(),
            },
        );
    }

    let total_bytes: u64 = tasks.iter().map(|task| task.size).sum();
    let total_files = tasks.len();
    let downloaded = Arc::new(AtomicU64::new(0));
    let completed_files = Arc::new(AtomicUsize::new(0));
    emit_progress(
        &app,
        "downloading",
        format!("Checking {} required files…", total_files),
        0,
        total_bytes,
        0,
        total_files,
    );

    let results = futures_util::stream::iter(tasks.into_iter().map(|task| {
        ensure_download(
            state.client.clone(),
            task,
            app.clone(),
            downloaded.clone(),
            completed_files.clone(),
            total_bytes,
            total_files,
            verify_existing,
        )
    }))
    .buffer_unordered(8)
    .collect::<Vec<_>>()
    .await;

    for result in results {
        result?;
    }

    if asset_index.virtual_assets || asset_index.map_to_resources {
        emit_progress(
            &app,
            "assets",
            "Preparing legacy asset layout…",
            downloaded.load(Ordering::Relaxed),
            total_bytes,
            completed_files.load(Ordering::Relaxed),
            total_files,
        );
        materialize_legacy_assets(&asset_index, &metadata.asset_index.id, &instance_root)?;
    }

    emit_progress(
        &app,
        "extracting",
        "Extracting native libraries…",
        downloaded.load(Ordering::Relaxed),
        total_bytes,
        completed_files.load(Ordering::Relaxed),
        total_files,
    );
    extract_natives(&native_archives, &instance_root.join("natives"))?;

    let java_major_version = metadata
        .java_version
        .as_ref()
        .map(|java| java.major_version)
        .unwrap_or(8);
    let installed_manifest = InstalledVersionManifest {
        version: metadata.id.clone(),
        main_class: metadata.main_class.clone(),
        java_major_version,
        installed_at: unix_timestamp(),
        files: total_files,
        downloaded_bytes: downloaded.load(Ordering::Relaxed),
    };
    write_json(&instance_root.join("installed-version.json"), &installed_manifest)?;

    emit_progress(
        &app,
        "complete",
        format!("Minecraft {} files are ready.", metadata.id),
        total_bytes,
        total_bytes,
        total_files,
        total_files,
    );

    Ok::<InstallResult, String>(InstallResult {
        instance_id: result_id.to_string(),
        version: metadata.id,
        java_major_version,
        downloaded_bytes: installed_manifest.downloaded_bytes,
        files: total_files,
        message: format!(
            "Minecraft {} files are ready. Java {} is required.",
            installed_manifest.version, java_major_version
        ),
    })
}

#[tauri::command]
async fn install_vanilla(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<InstallResult, String> {
    install_vanilla_internal(app, state.inner(), instance_id, true).await
}

#[tauri::command]
async fn start_microsoft_sign_in(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DeviceCodeStartResult, String> {
    let response = state
        .client
        .post(MICROSOFT_DEVICE_CODE_URL)
        .form(&[
            ("client_id", MICROSOFT_CLIENT_ID),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await
        .map_err(|error| format!("Could not start Microsoft sign-in: {error}"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(parse_service_error(
            status,
            &text,
            "Microsoft device-code sign-in",
        ));
    }
    let device: MicrosoftDeviceCodeResponse = serde_json::from_str(&text)
        .map_err(|error| format!("Microsoft returned an invalid device-code response: {error}"))?;
    let interval = device.interval.unwrap_or(5).max(5);
    {
        let mut auth = state
            .auth
            .lock()
            .map_err(|_| "Authentication state is unavailable.".to_string())?;
        auth.pending = Some(PendingDeviceAuth {
            device_code: device.device_code.clone(),
            expires_at: unix_timestamp_u64() + device.expires_in,
            interval,
        });
    }
    append_log(&app, "Microsoft device-code sign-in started with Ruby Launcher's approved application ID.");
    Ok(DeviceCodeStartResult {
        user_code: device.user_code,
        verification_uri: device.verification_uri,
        expires_in: device.expires_in,
        interval,
        message: device.message,
    })
}

#[tauri::command]
async fn poll_microsoft_sign_in(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AuthPollResult, String> {
    let pending = {
        let auth = state
            .auth
            .lock()
            .map_err(|_| "Authentication state is unavailable.".to_string())?;
        auth.pending.clone()
    }
    .ok_or_else(|| "No Microsoft sign-in is waiting for completion.".to_string())?;

    if unix_timestamp_u64() >= pending.expires_at {
        let mut auth = state
            .auth
            .lock()
            .map_err(|_| "Authentication state is unavailable.".to_string())?;
        auth.pending = None;
        return Ok(AuthPollResult {
            status: "expired".to_string(),
            message: "The Microsoft sign-in code expired. Start again.".to_string(),
            retry_after: 0,
            account: None,
        });
    }

    let response = state
        .client
        .post(MICROSOFT_TOKEN_URL)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", MICROSOFT_CLIENT_ID),
            ("device_code", pending.device_code.as_str()),
        ])
        .send()
        .await
        .map_err(|error| format!("Could not check Microsoft sign-in: {error}"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        let error_value: Value = serde_json::from_str(&text).unwrap_or_default();
        let code = error_value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown_error");
        let description = error_value
            .get("error_description")
            .and_then(Value::as_str)
            .unwrap_or("Microsoft sign-in is still waiting.");
        return match code {
            "authorization_pending" => Ok(AuthPollResult {
                status: "pending".to_string(),
                message: "Waiting for you to finish signing in…".to_string(),
                retry_after: pending.interval,
                account: None,
            }),
            "slow_down" => Ok(AuthPollResult {
                status: "pending".to_string(),
                message: "Microsoft asked Ruby Launcher to check less often.".to_string(),
                retry_after: pending.interval + 5,
                account: None,
            }),
            "expired_token" => Ok(AuthPollResult {
                status: "expired".to_string(),
                message: "The Microsoft sign-in code expired. Start again.".to_string(),
                retry_after: 0,
                account: None,
            }),
            "authorization_declined" => Ok(AuthPollResult {
                status: "declined".to_string(),
                message: "Microsoft sign-in was declined.".to_string(),
                retry_after: 0,
                account: None,
            }),
            _ => Err(format!("Microsoft sign-in failed: {description}")),
        };
    }

    let microsoft_token: MicrosoftTokenResponse = serde_json::from_str(&text)
        .map_err(|error| format!("Microsoft returned an invalid token response: {error}"))?;
    let account = exchange_minecraft_account(&state.client, &microsoft_token).await?;
    let summary = account.summary.clone();
    {
        let mut auth = state
            .auth
            .lock()
            .map_err(|_| "Authentication state is unavailable.".to_string())?;
        auth.pending = None;
        auth.account = Some(account);
    }
    append_log(
        &app,
        &format!(
            "Signed in as Minecraft profile '{}'. Waiting for the user's Remember Me choice.",
            summary.name
        ),
    );
    Ok(AuthPollResult {
        status: "signedIn".to_string(),
        message: format!("Signed in as {}.", summary.name),
        retry_after: 0,
        account: Some(summary),
    })
}

#[tauri::command]
fn set_remember_me(
    app: AppHandle,
    state: State<'_, AppState>,
    remember: bool,
) -> Result<AccountSummary, String> {
    let refresh_token = {
        let auth = state
            .auth
            .lock()
            .map_err(|_| "Authentication state is unavailable.".to_string())?;
        let account = auth
            .account
            .as_ref()
            .ok_or_else(|| "No Microsoft account is signed in.".to_string())?;
        account.microsoft_refresh_token.clone()
    };

    if remember {
        let token = refresh_token.ok_or_else(|| {
            "Microsoft did not return a refresh token, so Ruby Launcher cannot keep this account signed in."
                .to_string()
        })?;
        save_refresh_token(state.secure_store_available, &token)?;
    } else {
        delete_saved_refresh_token(state.secure_store_available)?;
    }

    let summary = {
        let mut auth = state
            .auth
            .lock()
            .map_err(|_| "Authentication state is unavailable.".to_string())?;
        let account = auth
            .account
            .as_mut()
            .ok_or_else(|| "No Microsoft account is signed in.".to_string())?;
        account.summary.session_only = !remember;
        account.summary.clone()
    };

    append_log(
        &app,
        if remember {
            "Remember Me enabled. The Microsoft refresh token was stored in the operating system's secure credential store."
        } else {
            "Remember Me declined. The Microsoft account will remain signed in only until Ruby Launcher closes."
        },
    );
    Ok(summary)
}

#[tauri::command]
async fn restore_saved_account(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<AccountSummary>, String> {
    let Some(saved_refresh_token) = load_saved_refresh_token(state.secure_store_available)? else {
        return Ok(None);
    };

    append_log(&app, "Refreshing the saved Microsoft account session.");
    let mut microsoft_token = refresh_microsoft_access_token(&state.client, &saved_refresh_token)
        .await
        .map_err(|error| {
            format!(
                "Ruby Launcher could not restore the saved Microsoft account. You can sign in again without losing installed instances. {error}"
            )
        })?;
    if microsoft_token.refresh_token.is_none() {
        microsoft_token.refresh_token = Some(saved_refresh_token);
    }

    let mut account = exchange_minecraft_account(&state.client, &microsoft_token).await?;
    account.summary.session_only = false;
    if let Some(refresh_token) = account.microsoft_refresh_token.as_deref() {
        save_refresh_token(state.secure_store_available, refresh_token)?;
    }
    let summary = account.summary.clone();
    {
        let mut auth = state
            .auth
            .lock()
            .map_err(|_| "Authentication state is unavailable.".to_string())?;
        auth.account = Some(account);
        auth.pending = None;
    }
    append_log(
        &app,
        &format!("Restored saved Minecraft profile '{}'.", summary.name),
    );
    Ok(Some(summary))
}

#[tauri::command]
fn cancel_microsoft_sign_in(state: State<'_, AppState>) -> Result<(), String> {
    let mut auth = state
        .auth
        .lock()
        .map_err(|_| "Authentication state is unavailable.".to_string())?;
    auth.pending = None;
    Ok(())
}

#[tauri::command]
fn get_account(state: State<'_, AppState>) -> Result<Option<AccountSummary>, String> {
    let auth = state
        .auth
        .lock()
        .map_err(|_| "Authentication state is unavailable.".to_string())?;
    Ok(auth.account.as_ref().map(|account| account.summary.clone()))
}

#[tauri::command]
fn sign_out(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut auth = state
            .auth
            .lock()
            .map_err(|_| "Authentication state is unavailable.".to_string())?;
        auth.account = None;
        auth.pending = None;
    }
    delete_saved_refresh_token(state.secure_store_available)?;
    append_log(&app, "Microsoft account signed out and any saved session was removed.");
    Ok(())
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let allowed = [
        "https://microsoft.com/",
        "https://www.microsoft.com/",
        "https://login.microsoftonline.com/",
    ];
    if !allowed.iter().any(|prefix| url.starts_with(prefix)) {
        return Err("Ruby Launcher refused to open an untrusted external URL.".to_string());
    }
    open::that(&url).map_err(|error| format!("Could not open the system browser: {error}"))
}

fn requested_java_major(settings: &LauncherSettings, required: u32) -> Result<u32, String> {
    match settings.java_runtime.as_str() {
        "automatic" => Ok(required),
        "java8" => Ok(8),
        "java17" => Ok(17),
        "java21" => Ok(21),
        "java25" => Ok(25),
        value => Err(format!("Unsupported Java runtime preference: {value}")),
    }
}

fn adoptium_architecture() -> Result<&'static str, String> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("x64"),
        "x86" => Ok("x32"),
        "aarch64" => Ok("aarch64"),
        other => Err(format!(
            "Ruby Launcher does not yet have a managed Java runtime mapping for architecture '{other}'."
        )),
    }
}

fn find_java_executable(root: &Path) -> Option<PathBuf> {
    let target = if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    };
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_java_executable(&path) {
                return Some(found);
            }
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(target))
            && path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("bin"))
        {
            return Some(path);
        }
    }
    None
}

fn extract_runtime_zip(archive_path: &Path, target: &Path) -> Result<PathBuf, String> {
    if target.exists() {
        fs::remove_dir_all(target)
            .map_err(|error| format!("Could not replace {}: {error}", target.display()))?;
    }
    fs::create_dir_all(target)
        .map_err(|error| format!("Could not create {}: {error}", target.display()))?;

    let file = File::open(archive_path)
        .map_err(|error| format!("Could not open Java runtime archive: {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Java runtime archive is invalid: {error}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not read Java runtime archive: {error}"))?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| "Java runtime archive contains an unsafe path.".to_string())?
            .to_path_buf();
        let output = target.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)
                .map_err(|error| format!("Could not create {}: {error}", output.display()))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
        }
        let mut destination = File::create(&output)
            .map_err(|error| format!("Could not create {}: {error}", output.display()))?;
        std::io::copy(&mut entry, &mut destination)
            .map_err(|error| format!("Could not extract {}: {error}", output.display()))?;

        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output, fs::Permissions::from_mode(mode)).map_err(|error| {
                format!("Could not set permissions on {}: {error}", output.display())
            })?;
        }
    }

    find_java_executable(target)
        .ok_or_else(|| "The downloaded Java runtime did not contain a Java executable.".to_string())
}

async fn ensure_java_runtime(
    app: &AppHandle,
    state: &AppState,
    required_major: u32,
    settings: &LauncherSettings,
) -> Result<(PathBuf, u32), String> {
    if cfg!(target_os = "android") {
        return Err(
            "The desktop Java runtime cannot be used on Android. Ruby Launcher's mobile JVM layer is a later milestone."
                .to_string(),
        );
    }

    let java_major = requested_java_major(settings, required_major)?;
    if settings.java_runtime != "automatic" && java_major != required_major {
        append_log(
            app,
            &format!(
                "Java {} was manually selected for a version whose metadata requests Java {}.",
                java_major, required_major
            ),
        );
    }

    #[cfg(not(target_os = "windows"))]
    {
        let status = Command::new("java").arg("-version").status();
        return match status {
            Ok(result) if result.success() => Ok((PathBuf::from("java"), java_major)),
            _ => Err(format!(
                "Managed Java downloads are currently implemented for Windows. Install Java {java_major} and make the 'java' command available on PATH."
            )),
        };
    }

    #[cfg(target_os = "windows")]
    {
        let architecture = adoptium_architecture()?;
        let runtime_root = launcher_root(app)?
            .join("runtimes")
            .join(format!("temurin-{java_major}-windows-{architecture}"));
        let manifest_path = runtime_root.join("ruby-runtime.json");
        if manifest_path.exists() {
            if let Ok(manifest) = read_json::<ManagedRuntimeManifest>(&manifest_path) {
                let java_path = PathBuf::from(&manifest.java_path);
                if manifest.java_major_version == java_major && java_path.exists() {
                    emit_progress(
                        app,
                        "runtime",
                        format!("Found managed Java {java_major}."),
                        1,
                        1,
                        1,
                        1,
                    );
                    return Ok((java_path, java_major));
                }
            }
        }

        emit_progress(
            app,
            "runtime",
            format!("Finding Eclipse Temurin Java {java_major}…"),
            0,
            0,
            0,
            1,
        );
        let endpoint = format!(
            "{ADOPTIUM_API_BASE}/{java_major}/ga?architecture={architecture}&heap_size=normal&image_type=jre&jvm_impl=hotspot&os=windows&page=0&page_size=1&project=jdk&sort_method=DEFAULT&sort_order=DESC&vendor=eclipse"
        );
        let releases: Value = fetch_json(&state.client, &endpoint, "Java runtime catalog").await?;
        let package = releases
            .as_array()
            .and_then(|items| items.first())
            .and_then(|release| release.get("binaries"))
            .and_then(Value::as_array)
            .and_then(|binaries| binaries.first())
            .and_then(|binary| binary.get("package"))
            .ok_or_else(|| format!("No Windows {architecture} Java {java_major} JRE was returned by Adoptium."))?;
        let url = package
            .get("link")
            .and_then(Value::as_str)
            .ok_or_else(|| "Adoptium did not provide a Java runtime download URL.".to_string())?;
        let expected_sha256 = package
            .get("checksum")
            .and_then(Value::as_str)
            .ok_or_else(|| "Adoptium did not provide a Java runtime checksum.".to_string())?;
        let expected_size = package.get("size").and_then(Value::as_u64).unwrap_or(0);

        let parent = runtime_root
            .parent()
            .ok_or_else(|| "Could not determine the Java runtime directory.".to_string())?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
        let archive_path = parent.join(format!("temurin-{java_major}-{architecture}.zip.part"));
        let response = state
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| format!("Could not download Java {java_major}: {error}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Could not download Java {java_major} ({status}): {body}"));
        }

        let total = if expected_size > 0 {
            expected_size
        } else {
            response.content_length().unwrap_or(0)
        };
        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&archive_path)
            .await
            .map_err(|error| format!("Could not create {}: {error}", archive_path.display()))?;
        let mut hasher = Sha256::new();
        let mut downloaded = 0_u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| format!("Java runtime download was interrupted: {error}"))?;
            file.write_all(&chunk)
                .await
                .map_err(|error| format!("Could not write Java runtime: {error}"))?;
            hasher.update(&chunk);
            downloaded += chunk.len() as u64;
            emit_progress(
                app,
                "runtime",
                format!("Downloading Java {java_major}…"),
                downloaded,
                total,
                0,
                1,
            );
        }
        file.flush()
            .await
            .map_err(|error| format!("Could not finish Java runtime download: {error}"))?;
        drop(file);

        let actual_sha256 = format!("{:x}", hasher.finalize());
        if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
            let _ = tokio::fs::remove_file(&archive_path).await;
            return Err(format!(
                "Java runtime checksum verification failed. Expected {expected_sha256}, received {actual_sha256}."
            ));
        }
        if expected_size > 0 && downloaded != expected_size {
            let _ = tokio::fs::remove_file(&archive_path).await;
            return Err(format!(
                "Java runtime size verification failed. Expected {expected_size} bytes, received {downloaded}."
            ));
        }

        emit_progress(
            app,
            "runtime",
            format!("Extracting Java {java_major}…"),
            downloaded,
            total,
            0,
            1,
        );
        let archive_for_extract = archive_path.clone();
        let target_for_extract = runtime_root.clone();
        let java_path = tokio::task::spawn_blocking(move || {
            extract_runtime_zip(&archive_for_extract, &target_for_extract)
        })
        .await
        .map_err(|error| format!("Java runtime extraction task failed: {error}"))??;
        let _ = tokio::fs::remove_file(&archive_path).await;

        let manifest = ManagedRuntimeManifest {
            java_major_version: java_major,
            java_path: java_path.to_string_lossy().into_owned(),
            source: "Eclipse Temurin via Adoptium".to_string(),
            installed_at: unix_timestamp(),
        };
        write_json(&manifest_path, &manifest)?;
        append_log(
            app,
            &format!("Installed managed Eclipse Temurin Java {java_major}."),
        );
        emit_progress(
            app,
            "runtime",
            format!("Java {java_major} is ready."),
            total.max(1),
            total.max(1),
            1,
            1,
        );
        Ok((java_path, java_major))
    }
}

async fn ensure_launch_account(app: &AppHandle, state: &AppState) -> Result<AccountSession, String> {
    let current = {
        let auth = state
            .auth
            .lock()
            .map_err(|_| "Authentication state is unavailable.".to_string())?;
        auth.account.clone()
    }
    .ok_or_else(|| "Sign in with the Microsoft account that owns Minecraft first.".to_string())?;

    if current.minecraft_expires_at > unix_timestamp_u64() + 120 {
        return Ok(current);
    }

    let refresh_token = current.microsoft_refresh_token.clone().ok_or_else(|| {
        "The Minecraft session expired and Microsoft did not provide a refresh token. Sign in again."
            .to_string()
    })?;
    emit_progress(app, "account", "Refreshing your Minecraft session…", 0, 0, 0, 0);
    let mut microsoft_token = refresh_microsoft_access_token(&state.client, &refresh_token).await?;
    if microsoft_token.refresh_token.is_none() {
        microsoft_token.refresh_token = Some(refresh_token);
    }
    let session_only = current.summary.session_only;
    let mut refreshed = exchange_minecraft_account(&state.client, &microsoft_token).await?;
    refreshed.summary.session_only = session_only;
    if !session_only {
        if let Some(token) = refreshed.microsoft_refresh_token.as_deref() {
            save_refresh_token(state.secure_store_available, token)?;
        }
    }
    {
        let mut auth = state
            .auth
            .lock()
            .map_err(|_| "Authentication state is unavailable.".to_string())?;
        auth.account = Some(refreshed.clone());
    }
    append_log(app, "Refreshed the Minecraft account before launch.");
    Ok(refreshed)
}

fn launch_rule_matches(rule: &RuleEntry, features: &HashMap<String, bool>) -> bool {
    if let Some(required_features) = &rule.features {
        for (name, expected) in required_features {
            if features.get(name).copied().unwrap_or(false) != *expected {
                return false;
            }
        }
    }

    let Some(os) = &rule.os else {
        return true;
    };
    if let Some(name) = &os.name {
        if name != minecraft_os_name() {
            return false;
        }
    }
    if let Some(arch) = &os.arch {
        let Ok(pattern) = Regex::new(arch) else {
            return false;
        };
        if !pattern.is_match(std::env::consts::ARCH) {
            return false;
        }
    }
    if os.version.is_some() {
        return false;
    }
    true
}

fn argument_allowed(rules: &[RuleEntry], features: &HashMap<String, bool>) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if launch_rule_matches(rule, features) {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn collect_arguments(values: &[ArgumentValue], features: &HashMap<String, bool>) -> Vec<String> {
    let mut result = Vec::new();
    for value in values {
        match value {
            ArgumentValue::Plain(value) => result.push(value.clone()),
            ArgumentValue::Conditional { rules, value } if argument_allowed(rules, features) => {
                match value {
                    ArgumentContents::One(value) => result.push(value.clone()),
                    ArgumentContents::Many(values) => result.extend(values.clone()),
                }
            }
            ArgumentValue::Conditional { .. } => {}
        }
    }
    result
}

fn split_legacy_arguments(input: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in input.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            } else {
                current.push(character);
            }
            continue;
        }
        if character == '"' || character == '\'' {
            quote = Some(character);
        } else if character.is_whitespace() {
            if !current.is_empty() {
                values.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        values.push(current);
    }
    values
}

fn replace_argument_placeholders(
    argument: &str,
    replacements: &HashMap<&str, String>,
) -> Result<String, String> {
    let mut result = argument.to_string();
    for (name, value) in replacements {
        result = result.replace(&format!("${{{name}}}"), value);
    }
    if result.contains("${") {
        return Err(format!("Minecraft metadata contains an unsupported launch placeholder in '{argument}'."));
    }
    Ok(result)
}

fn classpath_for_version(instance_root: &Path, metadata: &VersionMetadata) -> Result<String, String> {
    let mut paths = Vec::<PathBuf>::new();
    let mut seen = HashSet::<PathBuf>::new();
    for library in &metadata.libraries {
        if !library_allowed(library.rules.as_ref()) {
            continue;
        }
        let path = if let Some(downloads) = &library.downloads {
            let Some(artifact) = &downloads.artifact else {
                continue;
            };
            artifact.path.clone().unwrap_or(maven_path(&library.name)?)
        } else {
            maven_path(&library.name)?
        };
        let full = instance_root.join("libraries").join(path);
        if full.exists() && seen.insert(full.clone()) {
            paths.push(full);
        }
    }
    let client = instance_root
        .join("versions")
        .join(&metadata.id)
        .join(format!("{}.jar", metadata.id));
    if !client.exists() {
        return Err(format!("Minecraft client JAR is missing: {}", client.display()));
    }
    paths.push(client);
    let separator = if cfg!(target_os = "windows") { ";" } else { ":" };
    Ok(paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(separator))
}

fn pipe_game_output<R: Read + Send + 'static>(
    app: AppHandle,
    instance_name: String,
    stream_name: &'static str,
    reader: R,
) {
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            let message = format!("[Minecraft/{instance_name}/{stream_name}] {line}");
            append_log(&app, &message);
            let _ = app.emit("game-log", message);
        }
    });
}


fn java_compatible_path(path: &Path) -> String {
    let value = path.to_string_lossy();

    #[cfg(target_os = "windows")]
    {
        // Tauri may return installed resource paths in Windows extended-length form
        // (\\?\C:\...). Java 21's instrumentation agent loader cannot add those
        // paths to the system class path, so pass the equivalent normal Win32 path.
        if let Some(stripped) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{}", stripped);
        }
        if let Some(stripped) = value.strip_prefix(r"\\?\") {
            return stripped.to_string();
        }
    }

    value.into_owned()
}

fn ruby_module_loader_jar(app: &AppHandle) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("resources").join("ruby-module-loader.jar"));
        candidates.push(resource_dir.join("ruby-module-loader.jar"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("ruby-module-loader.jar"));
    if let Ok(current) = std::env::current_dir() {
        candidates.push(current.join("src-tauri").join("resources").join("ruby-module-loader.jar"));
        candidates.push(current.join("resources").join("ruby-module-loader.jar"));
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn bundled_controller_module(app: &AppHandle) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("resources").join("modules").join("ruby-controller.rmod"));
        candidates.push(resource_dir.join("modules").join("ruby-controller.rmod"));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("modules")
            .join("ruby-controller.rmod"),
    );
    if let Ok(current) = std::env::current_dir() {
        candidates.push(
            current
                .join("src-tauri")
                .join("resources")
                .join("modules")
                .join("ruby-controller.rmod"),
        );
        candidates.push(current.join("resources").join("modules").join("ruby-controller.rmod"));
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn ruby_mappings_path(instance_root: &Path, metadata: &VersionMetadata) -> Option<PathBuf> {
    let path = instance_root
        .join("versions")
        .join(&metadata.id)
        .join(format!("{}-client-mappings.txt", metadata.id));
    path.is_file().then_some(path)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn minimize_launcher_to_tray(app: &AppHandle) {
    let ui_app = app.clone();
    let log_app = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        if let Some(window) = ui_app.get_webview_window("main") {
            if let Err(error) = window.minimize() {
                append_log(
                    &ui_app,
                    &format!("Could not minimize Ruby Launcher after game launch: {error}"),
                );
                return;
            }
            if let Err(error) = window.set_skip_taskbar(true) {
                append_log(
                    &ui_app,
                    &format!("Ruby Launcher minimized, but could not remove its taskbar button: {error}"),
                );
            }
            append_log(&ui_app, "Ruby Launcher minimized to the system tray while Minecraft runs.");
        }
    }) {
        append_log(
            &log_app,
            &format!("Could not schedule Ruby Launcher tray minimization: {error}"),
        );
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn minimize_launcher_to_tray(_app: &AppHandle) {}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn restore_launcher_window(app: &AppHandle) {
    let ui_app = app.clone();
    let log_app = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        if let Some(window) = ui_app.get_webview_window("main") {
            if let Err(error) = window.set_skip_taskbar(false) {
                append_log(
                    &ui_app,
                    &format!("Could not restore Ruby Launcher's taskbar button: {error}"),
                );
            }
            if let Err(error) = window.unminimize() {
                append_log(&ui_app, &format!("Could not restore Ruby Launcher from the tray: {error}"));
            }
            if let Err(error) = window.show() {
                append_log(&ui_app, &format!("Could not show Ruby Launcher after Minecraft closed: {error}"));
            }
            if let Err(error) = window.set_focus() {
                append_log(&ui_app, &format!("Could not focus Ruby Launcher after Minecraft closed: {error}"));
            }
            if let Err(error) = window.emit("launcher-restored", ()) {
                append_log(&ui_app, &format!("Could not notify the launcher UI that it was restored: {error}"));
            }
            append_log(&ui_app, "Ruby Launcher restored from the system tray and input recovery was requested.");
        }
    }) {
        append_log(
            &log_app,
            &format!("Could not schedule Ruby Launcher restoration: {error}"),
        );
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn restore_launcher_window(_app: &AppHandle) {}

fn start_minecraft_process(
    app: &AppHandle,
    state: &AppState,
    instance: &InstanceProfile,
    settings: &LauncherSettings,
    account: &AccountSession,
    metadata: &VersionMetadata,
    java_path: &Path,
    java_major_version: u32,
) -> Result<LaunchResult, String> {
    {
        let mut running = state
            .running_instances
            .lock()
            .map_err(|_| "Running-game state is unavailable.".to_string())?;
        if !running.insert(instance.id.clone()) {
            return Err(format!("{} is already running.", instance.name));
        }
    }

    let result = (|| {
        let instance_root = launcher_root(app)?.join("instances").join(&instance.id);
        let game_directory = instance_root.join(".minecraft");
        let assets_root = instance_root.join("assets");
        let natives_directory = instance_root.join("natives");
        let libraries_directory = instance_root.join("libraries");
        let classpath = classpath_for_version(&instance_root, metadata)?;
        let classpath_separator = if cfg!(target_os = "windows") { ";" } else { ":" };
        let version_type = if metadata.version_type.is_empty() {
            "release"
        } else {
            metadata.version_type.as_str()
        };
        let game_assets = if instance_root
            .join("assets")
            .join("virtual")
            .join(&metadata.asset_index.id)
            .exists()
        {
            instance_root
                .join("assets")
                .join("virtual")
                .join(&metadata.asset_index.id)
        } else {
            assets_root.clone()
        };

        let mut replacements = HashMap::<&str, String>::new();
        replacements.insert("auth_player_name", account.summary.name.clone());
        replacements.insert("version_name", metadata.id.clone());
        replacements.insert("game_directory", game_directory.to_string_lossy().into_owned());
        replacements.insert("assets_root", assets_root.to_string_lossy().into_owned());
        replacements.insert("game_assets", game_assets.to_string_lossy().into_owned());
        replacements.insert("assets_index_name", metadata.asset_index.id.clone());
        replacements.insert("auth_uuid", account.summary.id.clone());
        replacements.insert("auth_access_token", account.minecraft_access_token.clone());
        replacements.insert(
            "auth_session",
            format!("token:{}:{}", account.minecraft_access_token, account.summary.id),
        );
        replacements.insert("clientid", MICROSOFT_CLIENT_ID.to_string());
        replacements.insert("auth_xuid", account.xuid.clone().unwrap_or_default());
        replacements.insert("user_type", "msa".to_string());
        replacements.insert("version_type", version_type.to_string());
        replacements.insert("user_properties", "{}".to_string());
        replacements.insert("natives_directory", natives_directory.to_string_lossy().into_owned());
        replacements.insert("launcher_name", "Ruby Launcher".to_string());
        replacements.insert("launcher_version", BUILD_ID.to_string());
        replacements.insert("classpath", classpath.clone());
        replacements.insert("classpath_separator", classpath_separator.to_string());
        replacements.insert("library_directory", libraries_directory.to_string_lossy().into_owned());
        replacements.insert(
            "primary_jar",
            instance_root
                .join("versions")
                .join(&metadata.id)
                .join(format!("{}.jar", metadata.id))
                .to_string_lossy()
                .into_owned(),
        );
        replacements.insert("resolution_width", instance.resolution_width.to_string());
        replacements.insert("resolution_height", instance.resolution_height.to_string());
        replacements.insert("quickPlayPath", String::new());

        let features = HashMap::<String, bool>::from([
            ("is_demo_user".to_string(), false),
            ("has_custom_resolution".to_string(), !instance.game_fullscreen),
            ("has_quick_plays_support".to_string(), false),
            ("is_quick_play_singleplayer".to_string(), false),
            ("is_quick_play_multiplayer".to_string(), false),
            ("is_quick_play_realms".to_string(), false),
        ]);

        let mut jvm_arguments = vec![
            format!("-Xms{}M", instance.initial_memory_mb.min(instance.memory_mb)),
            format!("-Xmx{}M", instance.memory_mb),
            format!("-Djava.library.path={}", natives_directory.to_string_lossy()),
            "-Dminecraft.launcher.brand=RubyLauncher".to_string(),
            format!("-Dminecraft.launcher.version={BUILD_ID}"),
        ];
        match instance.jvm_preset.as_str() {
            "low-memory" => jvm_arguments.extend([
                "-XX:+UseG1GC".to_string(),
                "-XX:MaxGCPauseMillis=100".to_string(),
            ]),
            "balanced" => jvm_arguments.extend([
                "-XX:+UseG1GC".to_string(),
                "-XX:MaxGCPauseMillis=75".to_string(),
                "-XX:+ParallelRefProcEnabled".to_string(),
            ]),
            "performance" => jvm_arguments.extend([
                "-XX:+UseG1GC".to_string(),
                "-XX:MaxGCPauseMillis=50".to_string(),
                "-XX:+ParallelRefProcEnabled".to_string(),
                "-XX:+AlwaysPreTouch".to_string(),
            ]),
            _ => {}
        }

        let controller_enabled = settings.controller_support_enabled && instance.controller_support_enabled;
        match (
            controller_enabled,
            ruby_module_loader_jar(app),
            ruby_mappings_path(&instance_root, metadata),
        ) {
            (true, Some(loader_jar), Some(mappings)) => {
                let loader_path = java_compatible_path(&loader_jar);
                let mappings_path = java_compatible_path(&mappings);
                let game_directory_path = java_compatible_path(&game_directory);
                let module_path = bundled_controller_module(app)
                    .map(|path| java_compatible_path(&path))
                    .unwrap_or_default();
                jvm_arguments.extend([
                    format!("-javaagent:{loader_path}"),
                    "-Druby.loader.enabled=true".to_string(),
                    format!("-Druby.loader.minecraftVersion={}", instance.minecraft_version),
                    format!("-Druby.loader.launchVersion={}", metadata.id),
                    format!("-Druby.loader.platform={}", instance.loader),
                    format!("-Druby.loader.mappings={mappings_path}"),
                    format!("-Druby.loader.gameDirectory={game_directory_path}"),
                    format!("-Druby.loader.modules={module_path}"),
                    "-Druby.controller.enabled=true".to_string(),
                    format!("-Druby.controller.deadzone={}", settings.controller_deadzone),
                    "-Druby.controller.lookSensitivity=1.0".to_string(),
                ]);
                append_log(
                    app,
                    &format!(
                        "Ruby Module Loader enabled for Minecraft {} from {}{}.",
                        metadata.id,
                        loader_path,
                        if module_path.is_empty() {
                            " without bundled modules"
                        } else {
                            " with Controller Support"
                        }
                    ),
                );
            }
            (false, _, _) => append_log(
                app,
                "In-game Controller Support is disabled for this launch.",
            ),
            (true, None, _) => append_log(
                app,
                "Ruby Module Loader resource was not found; Minecraft will start without Ruby modules.",
            ),
            (true, _, None) => append_log(
                app,
                &format!(
                    "Minecraft {} has no downloaded official client mappings; starting without Ruby Module Loader.",
                    metadata.id
                ),
            ),
        }
        if let Some(arguments) = &metadata.arguments {
            for argument in collect_arguments(&arguments.jvm, &features) {
                jvm_arguments.push(replace_argument_placeholders(&argument, &replacements)?);
            }
        } else {
            jvm_arguments.push("-cp".to_string());
            jvm_arguments.push(classpath.clone());
        }

        if let Some(logging) = &metadata.logging {
            if let Some(argument) = &logging.client.argument {
                let file = &logging.client.file;
                let file_name = file
                    .id
                    .clone()
                    .or_else(|| file.path.clone())
                    .unwrap_or_else(|| "client.xml".to_string());
                let logging_path = instance_root.join("assets").join("log_configs").join(file_name);
                jvm_arguments.push(argument.replace("${path}", logging_path.to_string_lossy().as_ref()));
            }
        }

        let mut game_arguments = if let Some(arguments) = &metadata.arguments {
            collect_arguments(&arguments.game, &features)
        } else if let Some(arguments) = &metadata.minecraft_arguments {
            split_legacy_arguments(arguments)
        } else {
            return Err("Minecraft version metadata does not contain launch arguments.".to_string());
        };
        game_arguments = game_arguments
            .into_iter()
            .map(|argument| replace_argument_placeholders(&argument, &replacements))
            .collect::<Result<Vec<_>, _>>()?;
        if instance.game_fullscreen && !game_arguments.iter().any(|argument| argument == "--fullscreen") {
            game_arguments.push("--fullscreen".to_string());
        }

        let mut command = Command::new(java_path);
        command
            .args(&jvm_arguments)
            .arg(&metadata.main_class)
            .args(&game_arguments)
            .current_dir(&game_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn().map_err(|error| {
            format!(
                "Could not start Minecraft with {}: {error}",
                java_path.display()
            )
        })?;
        let pid = child.id();
        if settings.hide_launcher_while_playing {
            minimize_launcher_to_tray(app);
        }
        if let Some(stdout) = child.stdout.take() {
            pipe_game_output(app.clone(), instance.name.clone(), "stdout", stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            pipe_game_output(app.clone(), instance.name.clone(), "stderr", stderr);
        }

        if let Err(error) = update_instance_status(app, &instance.id, "Running") {
            append_log(app, &format!("Could not update the running status: {error}"));
        }
        append_log(
            app,
            &format!(
                "Started '{}' with managed Java {} as process {}.",
                instance.name, java_major_version, pid
            ),
        );

        let app_for_wait = app.clone();
        let running_instances = state.running_instances.clone();
        let instance_id = instance.id.clone();
        let instance_name = instance.name.clone();
        let restore_launcher = settings.hide_launcher_while_playing;
        std::thread::spawn(move || {
            let exit = child.wait();
            let no_games_running = if let Ok(mut running) = running_instances.lock() {
                running.remove(&instance_id);
                running.is_empty()
            } else {
                true
            };
            let _ = update_instance_status(&app_for_wait, &instance_id, "Installed");
            if restore_launcher && no_games_running {
                restore_launcher_window(&app_for_wait);
            }
            let _ = app_for_wait.emit("game-exited", instance_id.clone());
            match exit {
                Ok(status) => append_log(
                    &app_for_wait,
                    &format!("Minecraft instance '{instance_name}' exited with status {status}."),
                ),
                Err(error) => append_log(
                    &app_for_wait,
                    &format!("Could not read the exit status for '{instance_name}': {error}"),
                ),
            }
        });

        Ok(LaunchResult {
            instance_id: instance.id.clone(),
            pid,
            java_major_version,
            message: format!("Minecraft {} is starting.", metadata.id),
        })
    })();

    if result.is_err() {
        if let Ok(mut running) = state.running_instances.lock() {
            running.remove(&instance.id);
        }
    }
    result
}

#[tauri::command]
async fn play_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<LaunchResult, String> {
    if cfg!(target_os = "android") {
        return Err(
            "The Android launcher interface is supported, but the mobile Java runtime and graphics compatibility layer are not connected yet."
                .to_string(),
        );
    }

    let instances = load_instances_internal(&app)?;
    let instance = instances
        .iter()
        .find(|instance| instance.id == instance_id)
        .cloned()
        .ok_or_else(|| "Instance not found.".to_string())?;
    if instance.loader != "none" && instance.launch_version.is_none() {
        return Err(format!(
            "{} is marked as a {} profile, but no loader launch profile is installed. Create it through Modrinth or use a vanilla profile until manual loader installation is added.",
            instance.name, instance.loader
        ));
    }
    let account = ensure_launch_account(&app, state.inner()).await?;
    let install = install_vanilla_internal(
        app.clone(),
        state.inner(),
        instance.id.clone(),
        false,
    )
    .await?;
    let mut settings = load_settings(app.clone())?;
    settings.memory_mb = instance.memory_mb;
    settings.java_runtime = instance.java_runtime.clone();
    settings.renderer = instance.renderer.clone();
    let (java_path, java_major_version) = ensure_java_runtime(
        &app,
        state.inner(),
        install.java_major_version,
        &settings,
    )
    .await?;

    let instance_root = launcher_root(&app)?.join("instances").join(&instance.id);
    let metadata_id = instance
        .launch_version
        .as_deref()
        .unwrap_or(&instance.minecraft_version);
    let metadata_path = instance_root
        .join("versions")
        .join(metadata_id)
        .join(format!("{metadata_id}.json"));
    let metadata: VersionMetadata = read_json(&metadata_path)?;
    emit_progress(
        &app,
        "launching",
        format!("Starting Minecraft {}…", metadata.id),
        1,
        1,
        1,
        1,
    );
    let result = start_minecraft_process(
        &app,
        state.inner(),
        &instance,
        &settings,
        &account,
        &metadata,
        &java_path,
        java_major_version,
    )?;
    emit_progress(
        &app,
        "complete",
        result.message.clone(),
        1,
        1,
        1,
        1,
    );
    Ok(result)
}

#[tauri::command]
fn exit_launcher(app: AppHandle) -> Result<(), String> {
    append_log(&app, "Exit requested from the launcher menu.");
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn read_launcher_log(app: AppHandle) -> Result<String, String> {
    let path = path_for(&app, LOG_FILE)?;
    if !path.exists() {
        return Ok(format!(
            "Ruby Launcher {BUILD_ID} started.\nNo launcher actions have been logged yet."
        ));
    }

    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let lines: Vec<&str> = contents.lines().rev().take(300).collect();
    Ok(lines.into_iter().rev().collect::<Vec<_>>().join("\n"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let secure_store_status = initialize_secure_store();
    let secure_store_available = secure_store_status.is_ok();
    if let Err(error) = &secure_store_status {
        eprintln!("Ruby Launcher secure storage warning: {error}");
    }

    tauri::Builder::default()
        .setup(|app| {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                use tauri::{
                    menu::{Menu, MenuItem},
                    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
                };

                let open_item = MenuItem::with_id(
                    app,
                    "open-ruby-launcher",
                    "Open Ruby Launcher",
                    true,
                    None::<&str>,
                )?;
                let exit_item = MenuItem::with_id(
                    app,
                    "exit-ruby-launcher",
                    "Exit Ruby Launcher",
                    true,
                    None::<&str>,
                )?;
                let menu = Menu::with_items(app, &[&open_item, &exit_item])?;

                let mut tray = TrayIconBuilder::with_id("ruby-launcher-tray")
                    .tooltip("Ruby Launcher")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "open-ruby-launcher" => restore_launcher_window(app),
                        "exit-ruby-launcher" => {
                            append_log(app, "Exit requested from the system tray.");
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            restore_launcher_window(tray.app_handle());
                        }
                    });

                if let Some(icon) = app.default_window_icon().cloned() {
                    tray = tray.icon(icon);
                }
                tray.build(app)?;
            }
            Ok(())
        })
        .manage(AppState::new(secure_store_available))
        .invoke_handler(tauri::generate_handler![
            launcher_info,
            load_settings,
            save_settings,
            list_instances,
            create_instance,
            update_instance,
            delete_instance,
            prepare_instance,
            fetch_version_catalog,
            search_modrinth_modpacks,
            list_modrinth_modpack_versions,
            install_modrinth_modpack,
            install_vanilla,
            start_microsoft_sign_in,
            poll_microsoft_sign_in,
            set_remember_me,
            restore_saved_account,
            cancel_microsoft_sign_in,
            get_account,
            sign_out,
            open_external_url,
            play_instance,
            exit_launcher,
            read_launcher_log,
            ruby_edition::ruby_edition_status,
            ruby_edition::ruby_edition_refresh,
            ruby_edition::ruby_edition_install,
            ruby_edition::ruby_edition_cancel,
            ruby_edition::ruby_edition_play,
            ruby_edition::ruby_edition_uninstall,
            ruby_edition::ruby_edition_open_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ruby Launcher");
}
