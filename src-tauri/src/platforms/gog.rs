//! GOG support. GOG has no official Linux client or public library API, so
//! VANTA discovers games locally:
//! * Heroic Games Launcher's GOG store data (`gog_store/installed.json`,
//!   `store_cache/gog_library.json`), launched via Heroic;
//! * standalone GOG Linux installers (`~/GOG Games/*/gameinfo` + `start.sh`),
//!   launched directly.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::*;
use crate::models::ArtworkHints;

pub struct GogAdapter {
    heroic_roots: Vec<PathBuf>,
    standalone_dirs: Vec<PathBuf>,
}

impl GogAdapter {
    pub fn system() -> Self {
        GogAdapter {
            heroic_roots: heroic_roots(),
            standalone_dirs: vec![home().join("GOG Games"), home().join("Games/GOG Games")],
        }
    }

    pub fn with_paths(heroic_roots: Vec<PathBuf>, standalone_dirs: Vec<PathBuf>) -> Self {
        GogAdapter {
            heroic_roots,
            standalone_dirs,
        }
    }
}

/// Heroic library caches wrap their arrays in different keys across versions.
pub fn heroic_library_entries(path: &Path) -> Vec<serde_json::Value> {
    let Ok(v) = read_json(path) else {
        return vec![];
    };
    for key in ["games", "library"] {
        if let Some(arr) = v.get(key).and_then(|a| a.as_array()) {
            return arr.clone();
        }
    }
    v.as_array().cloned().unwrap_or_default()
}

pub fn parse_heroic_gog(root: &Path) -> Vec<DiscoveredGame> {
    let mut games: HashMap<String, DiscoveredGame> = HashMap::new();
    for file in ["store_cache/gog_library.json", "gog_store/library.json"] {
        for e in heroic_library_entries(&root.join(file)) {
            let Some(app) = str_field(&e, &["app_name", "appName"]).map(str::to_string) else {
                continue;
            };
            if e.get("is_dlc").and_then(|x| x.as_bool()) == Some(true) {
                continue;
            }
            let title = str_field(&e, &["title"]).unwrap_or(&app).to_string();
            games.entry(app.clone()).or_insert_with(|| DiscoveredGame {
                platform: "gog".into(),
                external_id: app.clone(),
                name: title,
                installed: false,
                artwork: ArtworkHints {
                    cover_url: str_field(&e, &["art_square", "art_cover"]).map(str::to_string),
                    hero_url: str_field(&e, &["art_background", "art_cover"]).map(str::to_string),
                    logo_url: str_field(&e, &["art_logo"]).map(str::to_string),
                    ..Default::default()
                },
                platform_data: serde_json::json!({ "appName": app, "source": "Heroic" }),
                ..Default::default()
            });
        }
    }
    let installed = read_json(&root.join("gog_store/installed.json")).ok();
    let list = installed
        .as_ref()
        .and_then(|v| {
            v.get("installed")
                .and_then(|a| a.as_array())
                .cloned()
                .or_else(|| v.as_array().cloned())
        })
        .unwrap_or_default();
    for e in list {
        let Some(app) = str_field(&e, &["appName", "app_name"]).map(str::to_string) else {
            continue;
        };
        if e.get("is_dlc").and_then(|x| x.as_bool()) == Some(true) {
            continue;
        }
        let path = str_field(&e, &["install_path"]).map(str::to_string);
        let platform = str_field(&e, &["platform"])
            .unwrap_or("windows")
            .to_string();
        let g = games.entry(app.clone()).or_insert_with(|| DiscoveredGame {
            platform: "gog".into(),
            external_id: app.clone(),
            name: path
                .as_deref()
                .and_then(|p| Path::new(p).file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| app.clone()),
            platform_data: serde_json::json!({ "appName": app, "source": "Heroic" }),
            ..Default::default()
        });
        g.installed = path
            .as_deref()
            .map(|p| Path::new(p).is_dir())
            .unwrap_or(false);
        g.install_dir = path;
        g.size_bytes = e.get("install_size").and_then(|x| x.as_i64());
        g.platform_data["gamePlatform"] = serde_json::json!(platform);
    }
    games.into_values().collect()
}

/// Standalone GOG installers write a `gameinfo` file whose first line is the title.
pub fn parse_standalone(dir: &Path) -> Vec<DiscoveredGame> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    rd.flatten()
        .filter_map(|e| {
            let p = e.path();
            let info = std::fs::read_to_string(p.join("gameinfo")).ok()?;
            let mut lines = info.lines().map(str::trim);
            let name = lines.next().filter(|l| !l.is_empty())?.to_string();
            let start = p.join("start.sh");
            let folder = p.file_name()?.to_string_lossy().into_owned();
            Some(DiscoveredGame {
                platform: "gog".into(),
                external_id: format!("local-{}", folder.to_lowercase().replace(|c: char| !c.is_ascii_alphanumeric(), "-")),
                name,
                installed: start.is_file(),
                install_dir: Some(p.to_string_lossy().into_owned()),
                executable: start.is_file().then(|| start.to_string_lossy().into_owned()),
                platform_data: serde_json::json!({ "source": "standalone", "gamePlatform": "linux" }),
                ..Default::default()
            })
        })
        .collect()
}

impl PlatformAdapter for GogAdapter {
    fn id(&self) -> &'static str {
        "gog"
    }
    fn name(&self) -> &'static str {
        "GOG"
    }

    fn detect(&self) -> Detection {
        let help = "GOG offers no Linux client or public library API. Install Heroic Games Launcher and sign in to GOG there — \
                    VANTA reads Heroic's local GOG library. Standalone GOG Linux installers in ~/GOG Games are found automatically."
            .to_string();
        let mut details = vec![];
        let mut connected = false;
        let mut account = None;
        for root in &self.heroic_roots {
            details.push(format!("Heroic config: {}", root.display()));
            let user = read_json(&root.join("gog_store/config.json")).ok();
            if let Some(u) = user.as_ref().and_then(|u| u.get("userData")) {
                connected = true;
                account = str_field(u, &["username"]).map(str::to_string);
            } else if root.join("gog_store/auth.json").is_file()
                || root.join("gog_store/installed.json").is_file()
            {
                connected = true;
            }
        }
        match heroic_command() {
            Some((p, a)) => details.push(format!("Heroic: {} {}", p.display(), a.join(" "))),
            None if !self.heroic_roots.is_empty() => details
                .push("Heroic executable not found — Windows GOG games cannot be launched".into()),
            None => {}
        }
        let standalone: Vec<&PathBuf> =
            self.standalone_dirs.iter().filter(|d| d.is_dir()).collect();
        for d in &standalone {
            details.push(format!("Standalone GOG folder: {}", d.display()));
        }
        Detection {
            detected: !self.heroic_roots.is_empty() || !standalone.is_empty(),
            connected,
            account,
            details,
            help: Some(help),
        }
    }

    fn discover(&self) -> Result<Vec<DiscoveredGame>, String> {
        if self.heroic_roots.is_empty() && !self.standalone_dirs.iter().any(|d| d.is_dir()) {
            return Err("No Heroic installation or GOG Games folder found".into());
        }
        let mut out: HashMap<String, DiscoveredGame> = HashMap::new();
        for root in &self.heroic_roots {
            for g in parse_heroic_gog(root) {
                out.insert(g.external_id.clone(), g);
            }
        }
        for d in &self.standalone_dirs {
            for g in parse_standalone(d) {
                out.entry(g.external_id.clone()).or_insert(g);
            }
        }
        Ok(out.into_values().collect())
    }

    fn launch_plan(&self, game: &Game, extra_args: &[String]) -> Result<LaunchPlan, String> {
        if !game.installed {
            return Err("This GOG game is not installed. Install it through Heroic first.".into());
        }
        let install = game.install_dir.as_deref().map(PathBuf::from);
        // Native Linux GOG builds ship a start.sh that can be run directly.
        if let Some(dir) = &install {
            let start = dir.join("start.sh");
            if start.is_file() {
                return direct_plan(&start, Some(dir), extra_args);
            }
        }
        let app = &game.external_id;
        if !app
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
        {
            return Err(format!("invalid GOG app id '{app}'"));
        }
        let (bin, mut args) = heroic_command()
            .ok_or("This GOG game needs Heroic Games Launcher to run (Wine configuration lives there), but Heroic was not found.")?;
        args.push("--no-gui".into());
        args.push(format!("heroic://launch?appName={app}&runner=gog"));
        Ok(LaunchPlan {
            program: bin,
            args,
            env: vec![],
            cwd: None,
            tracking: install
                .filter(|d| d.is_dir())
                .map(Tracking::PathMatch)
                .unwrap_or(Tracking::Child),
            supports_profile_env: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_heroic_and_standalone() {
        let dir = tempfile::tempdir().unwrap();
        let heroic = dir.path().join("heroic");
        let gdir = dir.path().join("Games/Witcher");
        std::fs::create_dir_all(&gdir).unwrap();
        std::fs::create_dir_all(heroic.join("store_cache")).unwrap();
        std::fs::create_dir_all(heroic.join("gog_store")).unwrap();
        std::fs::write(
            heroic.join("store_cache/gog_library.json"),
            serde_json::json!({"games": [
                {"app_name": "1207664643", "title": "The Witcher", "art_square": "https://a/sq.jpg"},
                {"app_name": "999", "title": "Owned Only"}
            ]})
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            heroic.join("gog_store/installed.json"),
            serde_json::json!({"installed": [{"appName": "1207664643", "install_path": gdir, "platform": "windows"}]}).to_string(),
        )
        .unwrap();
        let standalone = dir.path().join("GOG Games");
        std::fs::create_dir_all(standalone.join("Stardew Valley")).unwrap();
        std::fs::write(
            standalone.join("Stardew Valley/gameinfo"),
            "Stardew Valley\n1.6\n",
        )
        .unwrap();
        std::fs::write(standalone.join("Stardew Valley/start.sh"), "#!/bin/sh").unwrap();

        let a = GogAdapter::with_paths(vec![heroic], vec![standalone]);
        assert!(a.detect().detected);
        let mut games = a.discover().unwrap();
        games.sort_by(|x, y| x.name.cmp(&y.name));
        assert_eq!(games.len(), 3);
        assert_eq!(games[0].name, "Owned Only");
        assert!(!games[0].installed);
        assert_eq!(games[1].name, "Stardew Valley");
        assert!(games[1].executable.as_ref().unwrap().ends_with("start.sh"));
        assert_eq!(games[2].name, "The Witcher");
        assert!(games[2].installed);
        assert_eq!(
            games[2].artwork.cover_url.as_deref(),
            Some("https://a/sq.jpg")
        );
    }

    #[test]
    fn absent_gog_is_reported() {
        let a = GogAdapter::with_paths(vec![], vec![PathBuf::from("/nope")]);
        assert!(!a.detect().detected);
        assert!(a.discover().is_err());
    }
}
