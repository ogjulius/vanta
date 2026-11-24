//! Epic Games Store via the Legendary CLI (standalone, or the copy bundled
//! with Heroic). Authentication is performed by `legendary auth` / Heroic;
//! VANTA only reads the display name from Legendary's `user.json` and never
//! reads, stores or logs tokens.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::*;
use crate::models::ArtworkHints;

pub struct EpicAdapter {
    /// (config dir, legendary binary if known, label)
    configs: Vec<(PathBuf, Option<PathBuf>, &'static str)>,
}

fn heroic_bundled_legendary() -> Option<PathBuf> {
    [
        "/opt/Heroic/resources/app.asar.unpacked/build/bin/x64/linux/legendary",
        "/opt/Heroic/resources/app.asar.unpacked/build/bin/linux/legendary",
        "/usr/lib/heroic/resources/app.asar.unpacked/build/bin/x64/linux/legendary",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| is_executable(p))
}

impl EpicAdapter {
    pub fn system() -> Self {
        let mut configs = vec![];
        let standalone = std::env::var_os("LEGENDARY_CONFIG_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| config_dir().join("legendary"));
        let legendary_bin = which("legendary");
        if standalone.is_dir() || legendary_bin.is_some() {
            configs.push((standalone, legendary_bin, "Legendary"));
        }
        for root in heroic_roots() {
            let cfg = root.join("legendaryConfig/legendary");
            if cfg.is_dir() {
                configs.push((
                    cfg,
                    heroic_bundled_legendary().or_else(|| which("legendary")),
                    "Heroic",
                ));
            }
        }
        EpicAdapter { configs }
    }

    pub fn with_configs(configs: Vec<(PathBuf, Option<PathBuf>, &'static str)>) -> Self {
        EpicAdapter { configs }
    }

    fn display_name(cfg: &Path) -> Option<String> {
        // Only the displayName field is extracted; the rest of user.json holds tokens.
        let v = read_json(&cfg.join("user.json")).ok()?;
        str_field(&v, &["displayName"]).map(str::to_string)
    }
}

fn key_image(meta: &serde_json::Value, types: &[&str]) -> Option<String> {
    let imgs = meta.pointer("/metadata/keyImages")?.as_array()?;
    types.iter().find_map(|t| {
        imgs.iter()
            .find(|i| i.get("type").and_then(|x| x.as_str()) == Some(*t))
            .and_then(|i| i.get("url"))
            .and_then(|u| u.as_str())
            .map(str::to_string)
    })
}

fn is_dlc_or_app(meta: &serde_json::Value) -> bool {
    if meta.pointer("/metadata/mainGameItem").is_some() {
        return true;
    }
    if let Some(cats) = meta
        .pointer("/metadata/categories")
        .and_then(|c| c.as_array())
    {
        let paths: Vec<&str> = cats
            .iter()
            .filter_map(|c| c.get("path").and_then(|p| p.as_str()))
            .collect();
        if !paths.is_empty() && !paths.contains(&"games") {
            return true;
        }
    }
    false
}

/// Parse a Legendary config dir into discovered games.
pub fn parse_legendary_dir(cfg: &Path) -> Result<Vec<DiscoveredGame>, String> {
    let mut games: HashMap<String, DiscoveredGame> = HashMap::new();
    let installed_path = cfg.join("installed.json");
    if installed_path.is_file() {
        let installed = read_json(&installed_path)?;
        if let Some(map) = installed.as_object() {
            for (app, v) in map {
                if v.get("is_dlc").and_then(|x| x.as_bool()) == Some(true) {
                    continue;
                }
                let install_path = str_field(v, &["install_path"]).map(str::to_string);
                let exe = match (install_path.as_deref(), str_field(v, &["executable"])) {
                    (Some(base), Some(exe)) => {
                        Some(Path::new(base).join(exe).to_string_lossy().into_owned())
                    }
                    _ => None,
                };
                games.insert(
                    app.clone(),
                    DiscoveredGame {
                        platform: "epic".into(),
                        external_id: app.clone(),
                        name: str_field(v, &["title"]).unwrap_or(app).to_string(),
                        installed: install_path
                            .as_deref()
                            .map(|p| Path::new(p).is_dir())
                            .unwrap_or(false),
                        install_dir: install_path,
                        executable: exe,
                        size_bytes: v.get("install_size").and_then(|x| x.as_i64()),
                        platform_data: serde_json::json!({
                            "appName": app,
                            "platform": str_field(v, &["platform"]).unwrap_or("Windows"),
                        }),
                        ..Default::default()
                    },
                );
            }
        }
    }
    // Owned games come from Legendary's metadata cache (populated by `legendary list`).
    if let Ok(rd) = std::fs::read_dir(cfg.join("metadata")) {
        for e in rd.flatten() {
            let Ok(meta) = read_json(&e.path()) else {
                continue;
            };
            let Some(app) = str_field(&meta, &["app_name"]).map(str::to_string) else {
                continue;
            };
            if is_dlc_or_app(&meta) {
                continue;
            }
            let title = str_field(&meta, &["app_title"]).unwrap_or(&app).to_string();
            let g = games.entry(app.clone()).or_insert_with(|| DiscoveredGame {
                platform: "epic".into(),
                external_id: app.clone(),
                name: title,
                installed: false,
                platform_data: serde_json::json!({ "appName": app }),
                ..Default::default()
            });
            g.artwork = ArtworkHints {
                cover_url: key_image(&meta, &["DieselGameBoxTall", "Thumbnail"]),
                hero_url: key_image(&meta, &["DieselGameBox", "featuredMedia"]),
                logo_url: key_image(&meta, &["DieselGameBoxLogo"]),
                ..Default::default()
            };
        }
    }
    Ok(games.into_values().collect())
}

impl PlatformAdapter for EpicAdapter {
    fn id(&self) -> &'static str {
        "epic"
    }
    fn name(&self) -> &'static str {
        "Epic Games"
    }

    fn detect(&self) -> Detection {
        let help = "Install Legendary (`sudo pacman -S legendary` or `pipx install legendary-gl`) and run `legendary auth` in a terminal, \
                    then `legendary list` to cache your library. Heroic Games Launcher installations are detected automatically. \
                    VANTA never asks for your Epic password or tokens."
            .to_string();
        if self.configs.is_empty() {
            return Detection {
                help: Some(help),
                ..Default::default()
            };
        }
        let mut details = vec![];
        let mut account = None;
        let mut can_launch = false;
        for (cfg, bin, label) in &self.configs {
            details.push(format!("{label} config: {}", cfg.display()));
            match bin {
                Some(b) => {
                    can_launch = true;
                    details.push(format!("  legendary: {}", b.display()));
                }
                None => details.push("  legendary binary not found".into()),
            }
            if account.is_none() {
                account = Self::display_name(cfg);
            }
        }
        if !can_launch && heroic_command().is_some() {
            details.push("Launching through Heroic".into());
        }
        Detection {
            detected: true,
            connected: account.is_some(),
            account,
            details,
            help: Some(help),
        }
    }

    fn discover(&self) -> Result<Vec<DiscoveredGame>, String> {
        if self.configs.is_empty() {
            return Err("Legendary / Heroic not found".into());
        }
        let mut all: HashMap<String, DiscoveredGame> = HashMap::new();
        let mut errors = vec![];
        for (cfg, _, label) in &self.configs {
            match parse_legendary_dir(cfg) {
                Ok(list) => {
                    for mut g in list {
                        g.platform_data["source"] = serde_json::json!(label);
                        g.platform_data["configDir"] = serde_json::json!(cfg.to_string_lossy());
                        match all.get(&g.external_id) {
                            Some(existing) if existing.installed && !g.installed => {}
                            _ => {
                                all.insert(g.external_id.clone(), g);
                            }
                        }
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        if all.is_empty() && !errors.is_empty() {
            return Err(errors.join("; "));
        }
        Ok(all.into_values().collect())
    }

    fn launch_plan(&self, game: &Game, extra_args: &[String]) -> Result<LaunchPlan, String> {
        let app = &game.external_id;
        if app.is_empty()
            || app.starts_with('-')
            || !app
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c))
        {
            return Err(format!("invalid Epic app name '{app}'"));
        }
        if !game.installed {
            return Err("This Epic game is not installed. Install it with `legendary install` or Heroic first.".into());
        }
        let cfg_dir = game
            .platform_data
            .get("configDir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from);
        let entry = self
            .configs
            .iter()
            .find(|(c, b, _)| b.is_some() && Some(c) == cfg_dir.as_ref())
            .or_else(|| self.configs.iter().find(|(_, b, _)| b.is_some()));
        let tracking = match game.install_dir.as_deref() {
            Some(d) if Path::new(d).is_dir() => Tracking::PathMatch(PathBuf::from(d)),
            _ => Tracking::Child,
        };
        if let Some((cfg, Some(bin), _)) = entry {
            let mut args = vec!["launch".to_string(), app.clone()];
            args.extend(extra_args.iter().cloned());
            return Ok(LaunchPlan {
                program: bin.clone(),
                args,
                env: vec![(
                    "LEGENDARY_CONFIG_PATH".into(),
                    cfg.to_string_lossy().into_owned(),
                )],
                cwd: None,
                tracking,
                supports_profile_env: true,
            });
        }
        if let Some((bin, mut args)) = heroic_command() {
            args.push("--no-gui".into());
            args.push(format!("heroic://launch?appName={app}&runner=legendary"));
            return Ok(LaunchPlan {
                program: bin,
                args,
                env: vec![],
                cwd: None,
                tracking,
                supports_profile_env: false,
            });
        }
        Err(
            "Neither the Legendary CLI nor Heroic was found, so this Epic game cannot be launched."
                .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legendary_config() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path();
        let game_dir = cfg.join("games/Fortnite");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(
            cfg.join("installed.json"),
            serde_json::json!({
                "Fn": {"app_name": "Fn", "title": "Fortnite", "install_path": game_dir, "executable": "bin/game.exe", "is_dlc": false, "install_size": 10},
                "Dlc": {"app_name": "Dlc", "title": "Some DLC", "install_path": "/x", "is_dlc": true}
            })
            .to_string(),
        )
        .unwrap();
        std::fs::create_dir_all(cfg.join("metadata")).unwrap();
        std::fs::write(
            cfg.join("metadata/Fn.json"),
            serde_json::json!({"app_name": "Fn", "app_title": "Fortnite", "metadata": {"keyImages": [{"type": "DieselGameBoxTall", "url": "https://img/tall.jpg"}], "categories": [{"path": "games"}]}}).to_string(),
        )
        .unwrap();
        std::fs::write(
            cfg.join("metadata/Owned.json"),
            serde_json::json!({"app_name": "Owned", "app_title": "Owned Game", "metadata": {"categories": [{"path": "games"}]}}).to_string(),
        )
        .unwrap();
        std::fs::write(
            cfg.join("metadata/UE.json"),
            serde_json::json!({"app_name": "UE", "app_title": "Unreal Engine", "metadata": {"categories": [{"path": "applications"}]}}).to_string(),
        )
        .unwrap();
        std::fs::write(
            cfg.join("user.json"),
            r#"{"displayName":"Player","access_token":"SECRET"}"#,
        )
        .unwrap();

        let mut games = parse_legendary_dir(cfg).unwrap();
        games.sort_by(|a, b| a.external_id.cmp(&b.external_id));
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].name, "Fortnite");
        assert!(games[0].installed);
        assert!(games[0]
            .executable
            .as_ref()
            .unwrap()
            .ends_with("bin/game.exe"));
        assert_eq!(
            games[0].artwork.cover_url.as_deref(),
            Some("https://img/tall.jpg")
        );
        assert!(!games[1].installed);

        let a = EpicAdapter::with_configs(vec![(cfg.to_path_buf(), None, "Legendary")]);
        let d = a.detect();
        assert!(d.detected && d.connected);
        assert_eq!(d.account.as_deref(), Some("Player"));
        assert!(
            !format!("{d:?}").contains("SECRET"),
            "tokens must never leak into status"
        );
    }

    #[test]
    fn not_installed_reports_cleanly() {
        let a = EpicAdapter::with_configs(vec![]);
        assert!(!a.detect().detected);
        assert!(a.discover().is_err());
    }
}
