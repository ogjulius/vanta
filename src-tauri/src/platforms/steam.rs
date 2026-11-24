//! Steam integration based entirely on local client data:
//! `libraryfolders.vdf`, `appmanifest_*.acf`, `loginusers.vdf`,
//! `userdata/<id>/config/localconfig.vdf` and the client's artwork cache.
//! No credentials are ever read or requested.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::Serialize;

use super::*;
use crate::vdf::{self, Vdf};

const STEAMID64_BASE: u64 = 76_561_197_960_265_728;

pub struct SteamAdapter {
    roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamDownload {
    pub app_id: String,
    pub name: String,
    pub state: String,
    pub bytes_total: i64,
    pub bytes_done: i64,
}

#[derive(Debug, Clone)]
struct Manifest {
    app_id: String,
    name: String,
    install_dir: PathBuf,
    state_flags: u64,
    last_played: Option<i64>,
    size: Option<i64>,
    bytes_to_download: i64,
    bytes_downloaded: i64,
}

/// Steam ships runtimes and compatibility tools as "apps"; they are not games.
pub fn is_tool(app_id: &str, name: &str) -> bool {
    const TOOL_IDS: &[&str] = &[
        "228980", "1070560", "1391110", "1628350", "4183110", "1493710", "2180100", "250820",
    ];
    if TOOL_IDS.contains(&app_id) {
        return true;
    }
    let n = name.to_ascii_lowercase();
    n.starts_with("proton ")
        || n == "proton experimental"
        || n.starts_with("proton-")
        || n.starts_with("steam linux runtime")
        || n.starts_with("steamworks common")
        || n.starts_with("steamvr")
}

impl SteamAdapter {
    pub fn system() -> Self {
        let h = home();
        let candidates = [
            h.join(".local/share/Steam"),
            h.join(".steam/steam"),
            h.join(".steam/root"),
            h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
            h.join(".var/app/com.valvesoftware.Steam/data/Steam"),
        ];
        Self::with_roots(candidates.to_vec())
    }

    pub fn with_roots(candidates: Vec<PathBuf>) -> Self {
        let mut seen = HashSet::new();
        let roots = candidates
            .into_iter()
            .filter(|p| p.join("steamapps").is_dir())
            .filter(|p| seen.insert(std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())))
            .collect();
        SteamAdapter { roots }
    }

    pub fn root(&self) -> Option<&PathBuf> {
        self.roots.first()
    }

    pub fn is_flatpak(&self) -> bool {
        self.root()
            .map(|r| r.to_string_lossy().contains("com.valvesoftware.Steam"))
            .unwrap_or(false)
    }

    pub fn library_dirs(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = vec![];
        let mut seen = HashSet::new();
        for root in &self.roots {
            let mut add = |p: PathBuf| {
                let key = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
                if p.join("steamapps").is_dir() && seen.insert(key) {
                    out.push(p);
                }
            };
            add(root.clone());
            let lf = root.join("steamapps/libraryfolders.vdf");
            if let Ok(text) = std::fs::read_to_string(&lf) {
                match vdf::parse(&text) {
                    Ok(doc) => {
                        if let Some(map) = doc.get("libraryfolders").and_then(Vdf::as_map) {
                            for entry in map.values() {
                                // Modern format: { "path": "..." }; legacy: "1" "/path"
                                let p = entry.str_at("path").or_else(|| entry.as_str());
                                if let Some(p) = p {
                                    add(PathBuf::from(p));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(file = %lf.display(), error = %e, "failed to parse libraryfolders.vdf")
                    }
                }
            }
        }
        out
    }

    fn manifests(&self) -> Vec<Manifest> {
        let mut out = vec![];
        let mut seen = HashSet::new();
        for lib in self.library_dirs() {
            let apps = lib.join("steamapps");
            let Ok(rd) = std::fs::read_dir(&apps) else {
                continue;
            };
            for e in rd.flatten() {
                let fname = e.file_name().to_string_lossy().into_owned();
                if !(fname.starts_with("appmanifest_") && fname.ends_with(".acf")) {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(e.path()) else {
                    continue;
                };
                let doc = match vdf::parse(&text) {
                    Ok(d) => d,
                    Err(err) => {
                        tracing::warn!(file = %fname, error = %err, "skipping malformed Steam manifest");
                        continue;
                    }
                };
                let Some(st) = doc.get("AppState") else {
                    continue;
                };
                let (Some(app_id), Some(name)) = (st.str_at("appid"), st.str_at("name")) else {
                    continue;
                };
                if !seen.insert(app_id.to_string()) {
                    continue;
                }
                let num = |k: &str| st.str_at(k).and_then(|v| v.parse::<i64>().ok());
                out.push(Manifest {
                    app_id: app_id.to_string(),
                    name: name.to_string(),
                    install_dir: apps
                        .join("common")
                        .join(st.str_at("installdir").unwrap_or_default()),
                    state_flags: st
                        .str_at("StateFlags")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0),
                    last_played: num("LastPlayed").filter(|v| *v > 0),
                    size: num("SizeOnDisk").filter(|v| *v > 0),
                    bytes_to_download: num("BytesToDownload").unwrap_or(0),
                    bytes_downloaded: num("BytesDownloaded").unwrap_or(0),
                });
            }
        }
        out
    }

    /// Most recent locally signed-in user: (steamid64, persona name).
    fn current_user(&self) -> Option<(u64, String)> {
        let root = self.root()?;
        let text = std::fs::read_to_string(root.join("config/loginusers.vdf")).ok()?;
        let doc = vdf::parse(&text).ok()?;
        let users = doc.get("users")?.as_map()?;
        let pick = users
            .iter()
            .find(|(_, u)| u.str_at("MostRecent") == Some("1"))
            .or_else(|| users.iter().next())?;
        let id = pick.0.parse::<u64>().ok()?;
        let name = pick
            .1
            .str_at("PersonaName")
            .unwrap_or("Steam user")
            .to_string();
        Some((id, name))
    }

    /// Per-app playtime (minutes) and last played from the user's localconfig.vdf.
    fn playtimes(&self) -> HashMap<String, (i64, Option<i64>)> {
        let mut out = HashMap::new();
        let Some(root) = self.root() else { return out };
        let mut files = vec![];
        if let Some((id64, _)) = self.current_user() {
            if id64 > STEAMID64_BASE {
                files.push(root.join(format!(
                    "userdata/{}/config/localconfig.vdf",
                    id64 - STEAMID64_BASE
                )));
            }
        }
        if files.iter().all(|f| !f.is_file()) {
            if let Ok(rd) = std::fs::read_dir(root.join("userdata")) {
                files.extend(
                    rd.flatten()
                        .map(|e| e.path().join("config/localconfig.vdf")),
                );
            }
        }
        for f in files.into_iter().filter(|f| f.is_file()) {
            let Ok(text) = std::fs::read_to_string(&f) else {
                continue;
            };
            let Ok(doc) = vdf::parse(&text) else {
                tracing::warn!("could not parse Steam localconfig.vdf; playtime unavailable");
                continue;
            };
            let apps = doc
                .path(&["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"])
                .or_else(|| {
                    doc.path(&["UserLocalConfigStore", "Software", "Valve", "Steam", "Apps"])
                });
            if let Some(apps) = apps.and_then(Vdf::as_map) {
                for (id, v) in apps {
                    let pt = v
                        .str_at("Playtime")
                        .and_then(|x| x.parse().ok())
                        .unwrap_or(0);
                    let lp = v
                        .str_at("LastPlayed")
                        .and_then(|x| x.parse().ok())
                        .filter(|x: &i64| *x > 0);
                    let e = out.entry(id.clone()).or_insert((0, None));
                    e.0 = e.0.max(pt);
                    e.1 = e.1.max(lp);
                }
            }
            break;
        }
        out
    }

    fn local_art(&self, app_id: &str) -> ArtworkHintsLocal {
        let mut art = ArtworkHintsLocal::default();
        let Some(root) = self.root() else { return art };
        let cache = root.join("appcache/librarycache");
        let dir = cache.join(app_id);
        let find = |file: &str| -> Option<String> {
            let direct = dir.join(file);
            if direct.is_file() {
                return Some(direct.to_string_lossy().into_owned());
            }
            // Newer clients store some assets in hash-named subdirectories.
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let p = e.path().join(file);
                    if p.is_file() {
                        return Some(p.to_string_lossy().into_owned());
                    }
                }
            }
            // Legacy flat layout: <appid>_library_600x900.jpg
            let legacy = cache.join(format!("{app_id}_{file}"));
            legacy
                .is_file()
                .then(|| legacy.to_string_lossy().into_owned())
        };
        art.cover = find("library_600x900.jpg").or_else(|| find("library_capsule.jpg"));
        art.hero = find("library_hero.jpg").or_else(|| find("header.jpg"));
        art.logo = find("logo.png");
        art
    }

    /// Current downloads/updates as reported by the Steam client's manifests.
    pub fn downloads(&self) -> Vec<SteamDownload> {
        self.manifests()
            .into_iter()
            .filter(|m| !is_tool(&m.app_id, &m.name))
            .filter(|m| m.state_flags & 4 == 0 || m.state_flags & (2 | 1024 | 1048576) != 0)
            .filter(|m| m.bytes_to_download > 0 && m.bytes_downloaded < m.bytes_to_download)
            .map(|m| SteamDownload {
                state: if m.state_flags & 1024 != 0 {
                    "downloading".into()
                } else if m.state_flags & 4 == 0 {
                    "installing".into()
                } else {
                    "update queued".into()
                },
                app_id: m.app_id,
                name: m.name,
                bytes_total: m.bytes_to_download,
                bytes_done: m.bytes_downloaded,
            })
            .collect()
    }

    fn steam_command(&self) -> Option<(PathBuf, Vec<String>)> {
        if self.is_flatpak() {
            return which("flatpak")
                .map(|f| (f, vec!["run".into(), "com.valvesoftware.Steam".into()]));
        }
        which("steam").map(|s| (s, vec![])).or_else(|| {
            which("flatpak")
                .filter(|_| home().join(".var/app/com.valvesoftware.Steam").is_dir())
                .map(|f| (f, vec!["run".into(), "com.valvesoftware.Steam".into()]))
        })
    }
}

#[derive(Default)]
struct ArtworkHintsLocal {
    cover: Option<String>,
    hero: Option<String>,
    logo: Option<String>,
}

pub fn cdn_url(app_id: &str, file: &str) -> String {
    format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/{file}")
}

impl PlatformAdapter for SteamAdapter {
    fn id(&self) -> &'static str {
        "steam"
    }
    fn name(&self) -> &'static str {
        "Steam"
    }

    fn detect(&self) -> Detection {
        let Some(root) = self.root() else {
            return Detection {
                help: Some("Install Steam (e.g. `sudo pacman -S steam`) and sign in once. VANTA reads your local Steam library — it never asks for your Steam password.".into()),
                ..Default::default()
            };
        };
        let mut details = vec![format!("Steam root: {}", root.display())];
        let libs = self.library_dirs();
        details.push(format!("{} library folder(s)", libs.len()));
        for l in &libs {
            details.push(format!("  • {}", l.display()));
        }
        match self.steam_command() {
            Some((p, a)) => details.push(format!("Launcher: {} {}", p.display(), a.join(" "))),
            None => details.push(
                "Steam executable not found in PATH — games can be listed but not launched".into(),
            ),
        }
        let user = self.current_user();
        Detection {
            detected: true,
            connected: user.is_some(),
            account: user.map(|u| u.1),
            details,
            help: Some(
                "Sign in through the Steam client. VANTA reads local library data only.".into(),
            ),
        }
    }

    fn discover(&self) -> Result<Vec<DiscoveredGame>, String> {
        if self.roots.is_empty() {
            return Err("Steam installation not found".into());
        }
        let playtimes = self.playtimes();
        let games = self
            .manifests()
            .into_iter()
            .filter(|m| !is_tool(&m.app_id, &m.name))
            .map(|m| {
                let art = self.local_art(&m.app_id);
                let (pt, lp) = playtimes.get(&m.app_id).cloned().unwrap_or((0, None));
                DiscoveredGame {
                    platform: "steam".into(),
                    external_id: m.app_id.clone(),
                    name: m.name.clone(),
                    installed: m.state_flags & 4 != 0,
                    install_dir: Some(m.install_dir.to_string_lossy().into_owned()),
                    executable: None,
                    platform_playtime_min: Some(pt),
                    last_played: m.last_played.max(lp),
                    size_bytes: m.size,
                    artwork: crate::models::ArtworkHints {
                        cover_local: art.cover,
                        hero_local: art.hero,
                        logo_local: art.logo,
                        cover_url: Some(cdn_url(&m.app_id, "library_600x900_2x.jpg")),
                        hero_url: Some(cdn_url(&m.app_id, "library_hero.jpg")),
                        logo_url: Some(cdn_url(&m.app_id, "logo.png")),
                    },
                    platform_data: serde_json::json!({ "appId": m.app_id, "stateFlags": m.state_flags }),
                }
            })
            .collect();
        Ok(games)
    }

    fn launch_plan(&self, game: &Game, extra_args: &[String]) -> Result<LaunchPlan, String> {
        let app_id = &game.external_id;
        if app_id.is_empty() || !app_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("invalid Steam AppID '{app_id}'"));
        }
        let (program, mut args) = self
            .steam_command()
            .ok_or("Steam executable not found. Install Steam or make sure `steam` is in PATH.")?;
        if extra_args.is_empty() {
            args.push(format!("steam://rungameid/{app_id}"));
        } else {
            args.push("-applaunch".into());
            args.push(app_id.clone());
            args.extend(extra_args.iter().cloned());
        }
        Ok(LaunchPlan {
            program,
            args,
            env: vec![],
            cwd: None,
            tracking: Tracking::SteamApp(app_id.clone()),
            supports_profile_env: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, s: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    }

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Steam");
        let lib2 = dir.path().join("Games");
        write(
            &root.join("steamapps/libraryfolders.vdf"),
            &format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}",
                root.display(),
                lib2.display()
            ),
        );
        write(&root.join("steamapps/appmanifest_10.acf"), "\"AppState\" { \"appid\" \"10\" \"name\" \"Game One\" \"StateFlags\" \"4\" \"installdir\" \"One\" \"LastPlayed\" \"100\" \"SizeOnDisk\" \"42\" }");
        write(&root.join("steamapps/appmanifest_1493710.acf"), "\"AppState\" { \"appid\" \"1493710\" \"name\" \"Proton Experimental\" \"StateFlags\" \"4\" \"installdir\" \"P\" }");
        write(
            &root.join("steamapps/appmanifest_bad.acf"),
            "\"AppState\" {",
        );
        write(&lib2.join("steamapps/appmanifest_20.acf"), "\"AppState\" { \"appid\" \"20\" \"name\" \"Game Two\" \"StateFlags\" \"1026\" \"installdir\" \"Two\" \"BytesToDownload\" \"1000\" \"BytesDownloaded\" \"250\" }");
        write(&root.join("config/loginusers.vdf"), "\"users\" { \"76561197960265738\" { \"PersonaName\" \"Tester\" \"MostRecent\" \"1\" } }");
        write(
            &root.join("userdata/10/config/localconfig.vdf"),
            "\"UserLocalConfigStore\" { \"Software\" { \"Valve\" { \"Steam\" { \"apps\" { \"10\" { \"Playtime\" \"95\" \"LastPlayed\" \"500\" } } } } } }",
        );
        write(
            &root.join("appcache/librarycache/10/library_600x900.jpg"),
            "img",
        );
        write(&root.join("appcache/librarycache/10/abc/header.jpg"), "img");
        dir
    }

    #[test]
    fn discovers_games_across_libraries() {
        let dir = fixture();
        let a = SteamAdapter::with_roots(vec![dir.path().join("Steam"), dir.path().join("nope")]);
        assert_eq!(a.library_dirs().len(), 2);
        let mut games = a.discover().unwrap();
        games.sort_by(|x, y| x.external_id.cmp(&y.external_id));
        assert_eq!(games.len(), 2, "tools and malformed manifests are skipped");
        let one = &games[0];
        assert_eq!(one.name, "Game One");
        assert!(one.installed);
        assert_eq!(one.platform_playtime_min, Some(95));
        assert_eq!(one.last_played, Some(500));
        assert!(one
            .artwork
            .cover_local
            .as_ref()
            .unwrap()
            .ends_with("library_600x900.jpg"));
        assert!(one
            .artwork
            .hero_local
            .as_ref()
            .unwrap()
            .ends_with("header.jpg"));
        assert!(!games[1].installed, "downloading game is not installed yet");
        let det = a.detect();
        assert!(det.detected && det.connected);
        assert_eq!(det.account.as_deref(), Some("Tester"));
        let dl = a.downloads();
        assert_eq!(dl.len(), 1);
        assert_eq!(dl[0].bytes_done, 250);
    }

    #[test]
    fn no_steam_is_reported_not_faked() {
        let a = SteamAdapter::with_roots(vec![PathBuf::from("/definitely/not/here")]);
        let d = a.detect();
        assert!(!d.detected && !d.connected);
        assert!(a.discover().is_err());
    }

    #[test]
    fn tool_filter() {
        assert!(is_tool("1", "Proton 9.0"));
        assert!(is_tool("1", "Steam Linux Runtime 3.0 (sniper)"));
        assert!(is_tool("228980", "whatever"));
        assert!(!is_tool("730", "Counter-Strike 2"));
    }

    #[test]
    fn rejects_non_numeric_appid() {
        let a = SteamAdapter::with_roots(vec![]);
        let g = Game {
            id: "steam:x".into(),
            platform: "steam".into(),
            external_id: "730;rm".into(),
            name: "x".into(),
            installed: true,
            install_dir: None,
            executable: None,
            cover: None,
            hero: None,
            logo: None,
            platform_playtime_min: 0,
            tracked_seconds: 0,
            session_count: 0,
            last_played: None,
            favorite: false,
            description: None,
            tags: vec![],
            source: "auto".into(),
            size_bytes: None,
            added_at: 0,
            platform_data: serde_json::Value::Null,
        };
        assert!(a
            .launch_plan(&g, &[])
            .unwrap_err()
            .contains("invalid Steam AppID"));
    }
}
