//! Platform adapters. Every store integration implements [`PlatformAdapter`].
//!
//! Conceptual mapping of the adapter interface:
//! * `detect()`            → [`PlatformAdapter::detect`]
//! * `connect()/disconnect()` → enabling/disabling the adapter (persisted in the
//!   `platforms` table). Account sign-in is always delegated to the platform's
//!   own supported tooling; VANTA never handles passwords or tokens.
//! * `sync_library()` + `discover_installed_games()` → [`PlatformAdapter::discover`]
//! * `launch_game()`       → [`PlatformAdapter::launch_plan`] executed by the launcher
//! * `get_platform_status()` → `detect()` combined with database counters.
//!
//! Adapters are pure with respect to the filesystem paths they are given, so
//! they can be unit-tested against fixture directories.

pub mod desktop;
pub mod epic;
pub mod gog;
pub mod itch;
pub mod local;
pub mod runners;
pub mod steam;

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::models::{DiscoveredGame, Game};

#[derive(Debug, Clone, Default, Serialize)]
pub struct Detection {
    pub detected: bool,
    pub connected: bool,
    pub account: Option<String>,
    pub details: Vec<String>,
    pub help: Option<String>,
}

/// How the launcher should determine when a game session ends.
#[derive(Debug, Clone, PartialEq)]
pub enum Tracking {
    /// The spawned process *is* the game (or waits for it).
    Child,
    /// Launch is handed to the Steam client; find the game by its reaper `AppId=`.
    SteamApp(String),
    /// Launch is handed to another launcher; find processes running from this directory.
    PathMatch(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaunchPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub tracking: Tracking,
    /// Whether profile env vars / wrapper / runner settings can apply.
    pub supports_profile_env: bool,
}

pub trait PlatformAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn detect(&self) -> Detection;
    /// Return every game this platform knows about locally (installed or owned).
    fn discover(&self) -> Result<Vec<DiscoveredGame>, String>;
    /// Build the platform-native launch command for a game.
    fn launch_plan(&self, game: &Game, extra_args: &[String]) -> Result<LaunchPlan, String>;
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| home().join(".config"))
}

/// Resolve a program name through `$PATH` without invoking a shell.
pub fn which(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let p = PathBuf::from(name);
        return is_executable(&p).then_some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| is_executable(p))
}

pub fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Read and parse a JSON file, returning a descriptive error.
pub fn read_json(path: &Path) -> Result<serde_json::Value, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("invalid JSON in {}: {e}", path.display()))
}

pub fn str_field<'a>(v: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .filter(|s| !s.is_empty())
}

/// Heroic Games Launcher config roots (native and Flatpak).
pub fn heroic_roots() -> Vec<PathBuf> {
    [
        config_dir().join("heroic"),
        home().join(".var/app/com.heroicgameslauncher.hgl/config/heroic"),
    ]
    .into_iter()
    .filter(|p| p.is_dir())
    .collect()
}

/// How to invoke Heroic, if installed.
pub fn heroic_command() -> Option<(PathBuf, Vec<String>)> {
    if let Some(p) = which("heroic") {
        return Some((p, vec![]));
    }
    for p in ["/opt/Heroic/heroic", "/usr/lib/heroic/heroic"] {
        let p = PathBuf::from(p);
        if is_executable(&p) {
            return Some((p, vec![]));
        }
    }
    if home().join(".var/app/com.heroicgameslauncher.hgl").is_dir() {
        if let Some(fp) = which("flatpak") {
            return Some((fp, vec!["run".into(), "com.heroicgameslauncher.hgl".into()]));
        }
    }
    None
}

pub fn all_adapters() -> Vec<Box<dyn PlatformAdapter>> {
    vec![
        Box::new(steam::SteamAdapter::system()),
        Box::new(epic::EpicAdapter::system()),
        Box::new(gog::GogAdapter::system()),
        Box::new(itch::ItchAdapter::system()),
        Box::new(local::LocalAdapter),
    ]
}

pub fn adapter_for(platform: &str) -> Option<Box<dyn PlatformAdapter>> {
    all_adapters().into_iter().find(|a| a.id() == platform)
}

/// Build a plan that runs a resolved executable directly.
pub fn direct_plan(exe: &Path, cwd: Option<&Path>, args: &[String]) -> Result<LaunchPlan, String> {
    if !exe.is_absolute() {
        return Err(format!(
            "executable path must be absolute: {}",
            exe.display()
        ));
    }
    if !exe.is_file() {
        return Err(format!("executable not found: {}", exe.display()));
    }
    Ok(LaunchPlan {
        program: exe.to_path_buf(),
        args: args.to_vec(),
        env: vec![],
        cwd: cwd
            .map(Path::to_path_buf)
            .or_else(|| exe.parent().map(Path::to_path_buf)),
        tracking: Tracking::Child,
        supports_profile_env: true,
    })
}
