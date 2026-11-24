//! itch.io support via the itch app's local butler database
//! (`~/.config/itch/db/butler.db`), opened strictly read-only.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use super::*;
use crate::models::ArtworkHints;

pub struct ItchAdapter {
    db_paths: Vec<PathBuf>,
}

impl ItchAdapter {
    pub fn system() -> Self {
        ItchAdapter {
            db_paths: vec![
                config_dir().join("itch/db/butler.db"),
                home().join(".var/app/io.itch.itch/config/itch/db/butler.db"),
            ],
        }
    }

    pub fn with_db(path: PathBuf) -> Self {
        ItchAdapter {
            db_paths: vec![path],
        }
    }

    fn db(&self) -> Option<&PathBuf> {
        self.db_paths.iter().find(|p| p.is_file())
    }
}

/// Pick the best launch candidate from a butler "verdict" JSON.
pub fn pick_candidate(verdict: &serde_json::Value) -> Option<String> {
    let cands = verdict.get("candidates")?.as_array()?;
    let score = |c: &serde_json::Value| match c.get("flavor").and_then(|f| f.as_str()) {
        Some("linux") => 0,
        Some("script") => 1,
        Some("jar") => 2,
        Some("html") => 4,
        Some("windows") => 3,
        _ => 5,
    };
    cands
        .iter()
        .min_by_key(|c| score(c))
        .and_then(|c| c.get("path").and_then(|p| p.as_str()))
        .map(str::to_string)
}

pub fn read_butler_db(path: &Path) -> Result<Vec<DiscoveredGame>, String> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("cannot open itch database: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT c.id, g.id, g.title, g.cover_url, g.still_cover_url, c.install_folder_name, l.path,
                    c.verdict, COALESCE(c.seconds_run, 0), c.installed_size, c.custom_install_folder
             FROM caves c JOIN games g ON g.id = c.game_id
             LEFT JOIN install_locations l ON l.id = c.install_location_id",
        )
        .map_err(|e| format!("unsupported itch database schema: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, i64>(8)?,
                r.get::<_, Option<i64>>(9)?,
                r.get::<_, Option<String>>(10)?,
            ))
        })
        .map_err(|e| format!("failed to read itch caves: {e}"))?;
    let mut out = vec![];
    for row in rows.flatten() {
        let (cave_id, game_id, title, cover, still, folder, loc, verdict, secs, size, custom) = row;
        let install_dir = custom
            .filter(|c| !c.is_empty())
            .map(PathBuf::from)
            .or_else(|| match (loc, folder) {
                (Some(l), Some(f)) => Some(Path::new(&l).join(f)),
                _ => None,
            });
        let exe = verdict
            .as_deref()
            .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
            .and_then(|v| pick_candidate(&v))
            .zip(install_dir.as_ref())
            .map(|(c, d)| d.join(c).to_string_lossy().into_owned());
        out.push(DiscoveredGame {
            platform: "itch".into(),
            external_id: game_id.to_string(),
            name: title,
            installed: install_dir.as_ref().map(|d| d.is_dir()).unwrap_or(false),
            install_dir: install_dir.map(|d| d.to_string_lossy().into_owned()),
            executable: exe,
            platform_playtime_min: Some(secs / 60),
            size_bytes: size,
            artwork: ArtworkHints {
                cover_url: still.or(cover.clone()),
                hero_url: cover,
                ..Default::default()
            },
            platform_data: serde_json::json!({ "caveId": cave_id }),
            ..Default::default()
        });
    }
    Ok(out)
}

impl PlatformAdapter for ItchAdapter {
    fn id(&self) -> &'static str {
        "itch"
    }
    fn name(&self) -> &'static str {
        "itch.io"
    }

    fn detect(&self) -> Detection {
        let help =
            "Install the itch app and install games through it — VANTA reads its local database. \
                    DRM-free itch downloads can also be added with “Add game”."
                .to_string();
        match self.db() {
            Some(p) => Detection {
                detected: true,
                connected: true,
                account: None,
                details: vec![format!("itch database: {}", p.display())],
                help: Some(help),
            },
            None => Detection {
                help: Some(help),
                ..Default::default()
            },
        }
    }

    fn discover(&self) -> Result<Vec<DiscoveredGame>, String> {
        let db = self.db().ok_or("itch app database not found")?;
        read_butler_db(db)
    }

    fn launch_plan(&self, game: &Game, extra_args: &[String]) -> Result<LaunchPlan, String> {
        let exe = game
            .executable
            .as_deref()
            .ok_or("No launchable executable was found for this itch.io game.")?;
        direct_plan(
            Path::new(exe),
            game.install_dir.as_deref().map(Path::new),
            extra_args,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_butler_schema() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("butler.db");
        let gdir = dir.path().join("apps/cool-game");
        std::fs::create_dir_all(&gdir).unwrap();
        let c = Connection::open(&db).unwrap();
        c.execute_batch(&format!(
            "CREATE TABLE games (id INTEGER PRIMARY KEY, title TEXT, cover_url TEXT, still_cover_url TEXT);
             CREATE TABLE install_locations (id TEXT PRIMARY KEY, path TEXT);
             CREATE TABLE caves (id TEXT PRIMARY KEY, game_id INTEGER, install_folder_name TEXT, install_location_id TEXT,
                                 verdict TEXT, seconds_run INTEGER, installed_size INTEGER, custom_install_folder TEXT);
             INSERT INTO games VALUES (7, 'Cool Game', 'https://c/cover.png', NULL);
             INSERT INTO install_locations VALUES ('loc', '{}');
             INSERT INTO caves VALUES ('cave1', 7, 'cool-game', 'loc',
                '{{\"candidates\":[{{\"path\":\"Game.exe\",\"flavor\":\"windows\"}},{{\"path\":\"game.x86_64\",\"flavor\":\"linux\"}}]}}', 7200, 99, NULL);",
            dir.path().join("apps").display()
        ))
        .unwrap();
        drop(c);
        let games = ItchAdapter::with_db(db).discover().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Cool Game");
        assert!(games[0].installed);
        assert!(
            games[0]
                .executable
                .as_ref()
                .unwrap()
                .ends_with("game.x86_64"),
            "linux build preferred"
        );
        assert_eq!(games[0].platform_playtime_min, Some(120));
    }

    #[test]
    fn incompatible_schema_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("butler.db");
        Connection::open(&db)
            .unwrap()
            .execute_batch("CREATE TABLE x (y INT);")
            .unwrap();
        assert!(ItchAdapter::with_db(db).discover().is_err());
    }
}
