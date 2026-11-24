use serde::{Deserialize, Serialize};

pub const PLATFORMS: &[&str] = &["steam", "epic", "gog", "itch", "local"];

pub fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A game as presented to the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub platform: String,
    pub external_id: String,
    pub name: String,
    pub installed: bool,
    pub install_dir: Option<String>,
    pub executable: Option<String>,
    pub cover: Option<String>,
    pub hero: Option<String>,
    pub logo: Option<String>,
    /// Playtime reported by the platform itself (e.g. Steam), in minutes.
    pub platform_playtime_min: i64,
    /// Playtime measured by VANTA sessions, in seconds.
    pub tracked_seconds: i64,
    pub session_count: i64,
    pub last_played: Option<i64>,
    pub favorite: bool,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub source: String,
    pub size_bytes: Option<i64>,
    pub added_at: i64,
    pub platform_data: serde_json::Value,
}

impl Game {
    /// Playtime estimate that never double counts: Steam's own counter already
    /// includes sessions VANTA started, so take the larger of the two.
    pub fn effective_playtime_seconds(&self) -> i64 {
        (self.platform_playtime_min * 60).max(self.tracked_seconds)
    }
}

/// Artwork hints produced by a platform adapter during discovery.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ArtworkHints {
    pub cover_local: Option<String>,
    pub hero_local: Option<String>,
    pub logo_local: Option<String>,
    pub cover_url: Option<String>,
    pub hero_url: Option<String>,
    pub logo_url: Option<String>,
}

/// A game found by a platform adapter. Upserted into the library by sync.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredGame {
    pub platform: String,
    pub external_id: String,
    pub name: String,
    pub installed: bool,
    pub install_dir: Option<String>,
    pub executable: Option<String>,
    pub platform_playtime_min: Option<i64>,
    pub last_played: Option<i64>,
    pub size_bytes: Option<i64>,
    pub artwork: ArtworkHints,
    pub platform_data: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GameQuery {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    /// "all" | "installed" | "not_installed"
    #[serde(default)]
    pub install_status: Option<String>,
    #[serde(default)]
    pub favorites_only: bool,
    #[serde(default)]
    pub played_only: bool,
    #[serde(default)]
    pub tag: Option<String>,
    /// "name" | "recent" | "playtime" | "added"
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GameUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub favorite: Option<bool>,
    pub executable: Option<String>,
    pub install_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManualGameInput {
    pub name: String,
    pub executable: String,
    #[serde(default)]
    pub install_dir: Option<String>,
    #[serde(default)]
    pub launch_args: Option<String>,
    /// "native" | "wine" | "proton"
    #[serde(default)]
    pub runner: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub cover: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameProfile {
    pub game_id: String,
    #[serde(default)]
    pub launch_args: String,
    #[serde(default)]
    pub env: Vec<EnvVar>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub executable_override: Option<String>,
    /// "default" | "native" | "wine" | "proton"
    #[serde(default = "default_runner")]
    pub runner: String,
    #[serde(default)]
    pub wine_prefix: Option<String>,
    #[serde(default)]
    pub wine_binary: Option<String>,
    #[serde(default)]
    pub proton_path: Option<String>,
    /// Optional wrapper command, e.g. `gamemoderun` or `mangohud`. Tokenized, never run via a shell.
    #[serde(default)]
    pub wrapper: String,
}

fn default_runner() -> String {
    "default".into()
}

impl GameProfile {
    pub fn default_for(game_id: &str) -> Self {
        GameProfile {
            game_id: game_id.to_string(),
            launch_args: String::new(),
            env: vec![],
            working_dir: None,
            executable_override: None,
            runner: default_runner(),
            wine_prefix: None,
            wine_binary: None,
            proton_path: None,
            wrapper: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: i64,
    pub game_id: String,
    pub game_name: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub duration_s: i64,
    pub exit_code: Option<i32>,
    /// "running" | "completed" | "failed" | "interrupted"
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayPlaytime {
    pub date: String,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GamePlaytime {
    pub game_id: String,
    pub name: String,
    pub platform: String,
    pub cover: Option<String>,
    pub tracked_seconds: i64,
    pub platform_playtime_min: i64,
    pub sessions: i64,
    pub last_played: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Statistics {
    pub total_tracked_seconds: i64,
    pub total_platform_minutes: i64,
    pub total_sessions: i64,
    pub failed_launches: i64,
    pub games_played: i64,
    pub library_size: i64,
    pub installed_count: i64,
    pub per_game: Vec<GamePlaytime>,
    pub longest_sessions: Vec<Session>,
    pub history: Vec<Session>,
    pub last_7_days: Vec<DayPlaytime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformStatus {
    pub id: String,
    pub name: String,
    /// User toggle: adapter participates in scans and launching.
    pub enabled: bool,
    /// The platform's client/tooling was found on this machine.
    pub detected: bool,
    /// Authenticated / usable account state, when the tooling exposes it.
    pub connected: bool,
    pub account: Option<String>,
    pub game_count: i64,
    pub installed_count: i64,
    pub last_sync: Option<i64>,
    pub last_error: Option<String>,
    pub details: Vec<String>,
    pub help: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub platform: String,
    pub found: usize,
    pub added: usize,
    pub updated: usize,
    pub marked_uninstalled: usize,
    pub error: Option<String>,
}
