//! SQLite persistence. All writes that touch more than one row run inside a
//! transaction; the database runs in WAL mode with `synchronous=FULL`, so an
//! unexpected exit never leaves a half-written library behind.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::settings::Settings;

const MIGRATIONS: &[&str] = &[
    // v1 — initial schema
    r#"
    CREATE TABLE platforms (
        id          TEXT PRIMARY KEY,
        enabled     INTEGER NOT NULL DEFAULT 1,
        last_sync   INTEGER,
        last_error  TEXT
    );
    CREATE TABLE games (
        id                    TEXT PRIMARY KEY,
        platform              TEXT NOT NULL,
        external_id           TEXT NOT NULL,
        name                  TEXT NOT NULL,
        installed             INTEGER NOT NULL DEFAULT 0,
        install_dir           TEXT,
        executable            TEXT,
        cover_path            TEXT,
        hero_path             TEXT,
        logo_path             TEXT,
        custom_cover          INTEGER NOT NULL DEFAULT 0,
        platform_playtime_min INTEGER NOT NULL DEFAULT 0,
        last_played           INTEGER,
        favorite              INTEGER NOT NULL DEFAULT 0,
        description           TEXT,
        source                TEXT NOT NULL DEFAULT 'auto',
        excluded              INTEGER NOT NULL DEFAULT 0,
        size_bytes            INTEGER,
        platform_data         TEXT NOT NULL DEFAULT '{}',
        artwork_hints         TEXT NOT NULL DEFAULT '{}',
        added_at              INTEGER NOT NULL,
        updated_at            INTEGER NOT NULL,
        UNIQUE(platform, external_id)
    );
    CREATE INDEX idx_games_platform ON games(platform);
    CREATE INDEX idx_games_last_played ON games(last_played);
    CREATE TABLE profiles (
        game_id             TEXT PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
        launch_args         TEXT NOT NULL DEFAULT '',
        env                 TEXT NOT NULL DEFAULT '[]',
        working_dir         TEXT,
        executable_override TEXT,
        runner              TEXT NOT NULL DEFAULT 'default',
        wine_prefix         TEXT,
        wine_binary         TEXT,
        proton_path         TEXT,
        wrapper             TEXT NOT NULL DEFAULT '',
        updated_at          INTEGER NOT NULL
    );
    CREATE TABLE sessions (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        game_id     TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
        started_at  INTEGER NOT NULL,
        ended_at    INTEGER,
        duration_s  INTEGER NOT NULL DEFAULT 0,
        exit_code   INTEGER,
        status      TEXT NOT NULL,
        error       TEXT
    );
    CREATE INDEX idx_sessions_game ON sessions(game_id);
    CREATE INDEX idx_sessions_started ON sessions(started_at);
    CREATE TABLE tags (
        id   INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE
    );
    CREATE TABLE game_tags (
        game_id TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
        tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
        PRIMARY KEY (game_id, tag_id)
    );
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    CREATE TABLE artwork_cache (
        game_id    TEXT NOT NULL,
        kind       TEXT NOT NULL,
        source     TEXT,
        path       TEXT,
        status     TEXT NOT NULL,
        fetched_at INTEGER NOT NULL,
        PRIMARY KEY (game_id, kind)
    );
    "#,
];

pub struct Db {
    conn: Connection,
}

pub fn game_id(platform: &str, external_id: &str) -> String {
    format!("{platform}:{external_id}")
}

const GAME_COLUMNS: &str = "g.id, g.platform, g.external_id, g.name, g.installed, g.install_dir, g.executable,
    g.cover_path, g.hero_path, g.logo_path, g.platform_playtime_min, COALESCE(s.tracked, 0), COALESCE(s.cnt, 0),
    g.last_played, g.favorite, g.description, g.source, g.size_bytes, g.added_at, g.platform_data,
    (SELECT group_concat(t.name, char(31)) FROM game_tags gt JOIN tags t ON t.id = gt.tag_id WHERE gt.game_id = g.id)";

const GAME_FROM: &str = "FROM games g LEFT JOIN (
        SELECT game_id, SUM(duration_s) AS tracked, COUNT(*) AS cnt FROM sessions
        WHERE status IN ('completed', 'running') GROUP BY game_id
    ) s ON s.game_id = g.id";

fn row_to_game(r: &Row) -> rusqlite::Result<Game> {
    let tags: Option<String> = r.get(20)?;
    let pdata: String = r.get(19)?;
    let mut tags: Vec<String> = tags
        .map(|t| t.split('\u{1f}').map(str::to_string).collect())
        .unwrap_or_default();
    tags.sort_by_key(|t| t.to_lowercase());
    Ok(Game {
        id: r.get(0)?,
        platform: r.get(1)?,
        external_id: r.get(2)?,
        name: r.get(3)?,
        installed: r.get::<_, i64>(4)? != 0,
        install_dir: r.get(5)?,
        executable: r.get(6)?,
        cover: r.get(7)?,
        hero: r.get(8)?,
        logo: r.get(9)?,
        platform_playtime_min: r.get(10)?,
        tracked_seconds: r.get(11)?,
        session_count: r.get(12)?,
        last_played: r.get(13)?,
        favorite: r.get::<_, i64>(14)? != 0,
        description: r.get(15)?,
        source: r.get(16)?,
        size_bytes: r.get(17)?,
        added_at: r.get(18)?,
        platform_data: serde_json::from_str(&pdata).unwrap_or(serde_json::Value::Null),
        tags,
    })
}

fn row_to_session(r: &Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: r.get(0)?,
        game_id: r.get(1)?,
        game_name: r.get(2)?,
        started_at: r.get(3)?,
        ended_at: r.get(4)?,
        duration_s: r.get(5)?,
        exit_code: r.get(6)?,
        status: r.get(7)?,
        error: r.get(8)?,
    })
}

const SESSION_SELECT: &str = "SELECT s.id, s.game_id, g.name, s.started_at, s.ended_at, s.duration_s, s.exit_code, s.status, s.error
    FROM sessions s LEFT JOIN games g ON g.id = s.game_id";

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> AppResult<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON;",
        )?;
        let mut db = Db { conn };
        db.migrate()?;
        db.recover_interrupted_sessions()?;
        Ok(db)
    }

    pub fn schema_version(&self) -> AppResult<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    fn migrate(&mut self) -> AppResult<()> {
        let current = self.schema_version()? as usize;
        if current > MIGRATIONS.len() {
            return Err(AppError::Invalid(format!(
                "database schema v{current} is newer than this VANTA build supports (v{})",
                MIGRATIONS.len()
            )));
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
            tracing::info!(version = i + 1, "applied database migration");
        }
        let tx = self.conn.transaction()?;
        for p in PLATFORMS {
            tx.execute(
                "INSERT OR IGNORE INTO platforms (id, enabled) VALUES (?1, 1)",
                [p],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Sessions still marked running belong to a previous process that exited
    /// unexpectedly. Their duration is unknown, so record them honestly.
    fn recover_interrupted_sessions(&self) -> AppResult<usize> {
        Ok(self.conn.execute(
            "UPDATE sessions SET status = 'interrupted', ended_at = started_at, duration_s = 0,
             error = 'VANTA exited while this session was running' WHERE status = 'running'",
            [],
        )?)
    }

    // ---------------------------------------------------------------- settings

    pub fn load_settings(&self) -> AppResult<Settings> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| {
                r.get(0)
            })
            .optional()?;
        let Some(raw) = raw else {
            return Ok(Settings::default());
        };
        let (settings, repaired) = Settings::from_stored(&raw);
        if repaired {
            tracing::warn!("stored settings were invalid; recovered with defaults");
            self.conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES ('app.corrupt', ?1)",
                [&raw],
            )?;
            self.save_settings(&settings)?;
        }
        Ok(settings)
    }

    pub fn save_settings(&self, s: &Settings) -> AppResult<Settings> {
        let s = s.clone().validated()?;
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('app', ?1)",
            [serde_json::to_string(&s)?],
        )?;
        Ok(s)
    }

    // --------------------------------------------------------------- platforms

    pub fn platform_enabled(&self, id: &str) -> AppResult<bool> {
        Ok(self
            .conn
            .query_row("SELECT enabled FROM platforms WHERE id = ?1", [id], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
            .map(|v| v != 0)
            .unwrap_or(false))
    }

    pub fn set_platform_enabled(&self, id: &str, enabled: bool) -> AppResult<()> {
        if !PLATFORMS.contains(&id) {
            return Err(AppError::Invalid(format!("unknown platform '{id}'")));
        }
        self.conn.execute(
            "UPDATE platforms SET enabled = ?2 WHERE id = ?1",
            params![id, enabled as i64],
        )?;
        Ok(())
    }

    pub fn platform_sync_info(&self, id: &str) -> AppResult<(Option<i64>, Option<String>)> {
        Ok(self
            .conn
            .query_row(
                "SELECT last_sync, last_error FROM platforms WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or((None, None)))
    }

    pub fn set_platform_sync(&self, id: &str, error: Option<&str>) -> AppResult<()> {
        self.conn.execute(
            "UPDATE platforms SET last_sync = ?2, last_error = ?3 WHERE id = ?1",
            params![id, now_ts(), error],
        )?;
        Ok(())
    }

    pub fn platform_counts(&self, id: &str) -> AppResult<(i64, i64)> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(installed), 0) FROM games WHERE platform = ?1 AND excluded = 0",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    }

    // ------------------------------------------------------------------- games

    /// Upsert the full result of one platform scan atomically.
    ///
    /// * New games are inserted.
    /// * Existing games get refreshed platform fields; user data (favorite, tags,
    ///   description, custom cover, profiles) is never touched.
    /// * Games the user removed (`excluded`) are not resurrected.
    /// * Previously-installed games of this platform that were not found are
    ///   marked as not installed (never deleted).
    pub fn apply_scan(
        &mut self,
        platform: &str,
        found: &[DiscoveredGame],
    ) -> AppResult<ScanReport> {
        let now = now_ts();
        let tx = self.conn.transaction()?;
        let mut added = 0;
        let mut updated = 0;
        let mut seen = HashSet::new();
        for g in found {
            if g.platform != platform || g.external_id.trim().is_empty() || g.name.trim().is_empty()
            {
                continue;
            }
            let id = game_id(platform, &g.external_id);
            if !seen.insert(id.clone()) {
                continue; // duplicate within one scan
            }
            let hints = serde_json::to_string(&g.artwork)?;
            let pdata = serde_json::to_string(&g.platform_data)?;
            let exists: bool = tx
                .query_row("SELECT 1 FROM games WHERE id = ?1", [&id], |_| Ok(()))
                .optional()?
                .is_some();
            if exists {
                tx.execute(
                    "UPDATE games SET name = ?2, installed = ?3, install_dir = ?4,
                        executable = CASE WHEN source = 'manual' THEN executable ELSE ?5 END,
                        platform_playtime_min = MAX(platform_playtime_min, COALESCE(?6, 0)),
                        last_played = CASE WHEN ?7 IS NULL THEN last_played
                                           WHEN last_played IS NULL OR ?7 > last_played THEN ?7
                                           ELSE last_played END,
                        size_bytes = COALESCE(?8, size_bytes), platform_data = ?9, artwork_hints = ?10,
                        updated_at = ?11
                     WHERE id = ?1",
                    params![
                        id, g.name.trim(), g.installed as i64, g.install_dir, g.executable,
                        g.platform_playtime_min, g.last_played, g.size_bytes, pdata, hints, now
                    ],
                )?;
                updated += 1;
            } else {
                tx.execute(
                    "INSERT INTO games (id, platform, external_id, name, installed, install_dir, executable,
                        platform_playtime_min, last_played, size_bytes, platform_data, artwork_hints,
                        source, added_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, COALESCE(?8, 0), ?9, ?10, ?11, ?12, 'auto', ?13, ?13)",
                    params![
                        id, platform, g.external_id, g.name.trim(), g.installed as i64, g.install_dir,
                        g.executable, g.platform_playtime_min, g.last_played, g.size_bytes, pdata, hints, now
                    ],
                )?;
                added += 1;
            }
        }
        // Mark auto-discovered games that disappeared as uninstalled.
        let mut stmt = tx.prepare(
            "SELECT id FROM games WHERE platform = ?1 AND source = 'auto' AND installed = 1",
        )?;
        let installed: Vec<String> = stmt
            .query_map([platform], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        drop(stmt);
        let mut marked = 0;
        for id in installed {
            if !seen.contains(&id) {
                tx.execute(
                    "UPDATE games SET installed = 0, updated_at = ?2 WHERE id = ?1",
                    params![id, now],
                )?;
                marked += 1;
            }
        }
        tx.commit()?;
        Ok(ScanReport {
            platform: platform.to_string(),
            found: seen.len(),
            added,
            updated,
            marked_uninstalled: marked,
            error: None,
        })
    }

    pub fn add_manual_game(&mut self, input: &ManualGameInput) -> AppResult<Game> {
        let name = input.name.trim();
        if name.is_empty() || name.len() > 256 {
            return Err(AppError::Invalid(
                "game name must be between 1 and 256 characters".into(),
            ));
        }
        let exe = input.executable.trim();
        if exe.is_empty() {
            return Err(AppError::Invalid("an executable is required".into()));
        }
        let exe_path = Path::new(exe);
        if !exe_path.is_absolute() {
            return Err(AppError::Invalid(
                "the executable path must be absolute".into(),
            ));
        }
        if !exe_path.is_file() {
            return Err(AppError::Invalid(format!("executable not found: {exe}")));
        }
        let platform = input.platform.clone().unwrap_or_else(|| "local".into());
        if !PLATFORMS.contains(&platform.as_str()) {
            return Err(AppError::Invalid(format!("unknown platform '{platform}'")));
        }
        let runner = input.runner.clone().unwrap_or_else(|| "native".into());
        if !["native", "wine", "proton"].contains(&runner.as_str()) {
            return Err(AppError::Invalid(format!("unknown runner '{runner}'")));
        }
        // Duplicate prevention: same executable already imported manually.
        let dup: Option<String> = self
            .conn
            .query_row(
                "SELECT name FROM games WHERE source = 'manual' AND executable = ?1 AND excluded = 0",
                [exe],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(existing) = dup {
            return Err(AppError::Invalid(format!(
                "this executable is already in your library as '{existing}'"
            )));
        }
        let install_dir = non_empty(input.install_dir.clone())
            .or_else(|| exe_path.parent().map(|p| p.to_string_lossy().into_owned()));
        let now = now_ts();
        // Unique, stable external id derived from time + a counter to avoid collisions.
        let ext = {
            let base = format!("m{now:x}");
            let mut n = 0;
            loop {
                let candidate = if n == 0 {
                    base.clone()
                } else {
                    format!("{base}-{n}")
                };
                let taken: bool = self
                    .conn
                    .query_row(
                        "SELECT 1 FROM games WHERE platform = ?1 AND external_id = ?2",
                        params![platform, candidate],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some();
                if !taken {
                    break candidate;
                }
                n += 1;
            }
        };
        let id = game_id(&platform, &ext);
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO games (id, platform, external_id, name, installed, install_dir, executable, source, added_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, 'manual', ?7, ?7)",
            params![id, platform, ext, name, install_dir, exe, now],
        )?;
        let mut profile = GameProfile::default_for(&id);
        profile.runner = runner;
        profile.launch_args = input.launch_args.clone().unwrap_or_default();
        Self::write_profile(&tx, &profile)?;
        tx.commit()?;
        self.get_game(&id)
    }

    pub fn get_game(&self, id: &str) -> AppResult<Game> {
        let sql = format!("SELECT {GAME_COLUMNS} {GAME_FROM} WHERE g.id = ?1");
        self.conn
            .query_row(&sql, [id], row_to_game)
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("game '{id}' not found")))
    }

    pub fn query_games(&self, q: &GameQuery) -> AppResult<Vec<Game>> {
        // Disabled ("disconnected") integrations keep their data but are hidden.
        let mut clauses = vec![
            "g.excluded = 0".to_string(),
            "g.platform IN (SELECT id FROM platforms WHERE enabled = 1)".to_string(),
        ];
        let mut args: Vec<rusqlite::types::Value> = vec![];
        if let Some(search) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let escaped = search
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            args.push(format!("%{escaped}%").into());
            clauses.push(format!("g.name LIKE ?{} ESCAPE '\\'", args.len()));
        }
        let platforms: Vec<&String> = q
            .platforms
            .iter()
            .filter(|p| PLATFORMS.contains(&p.as_str()))
            .collect();
        if !platforms.is_empty() {
            let start = args.len();
            for p in &platforms {
                args.push((*p).clone().into());
            }
            let ph: Vec<String> = (start + 1..=args.len()).map(|i| format!("?{i}")).collect();
            clauses.push(format!("g.platform IN ({})", ph.join(",")));
        }
        match q.install_status.as_deref() {
            Some("installed") => clauses.push("g.installed = 1".into()),
            Some("not_installed") => clauses.push("g.installed = 0".into()),
            _ => {}
        }
        if q.favorites_only {
            clauses.push("g.favorite = 1".into());
        }
        if q.played_only {
            clauses.push("g.last_played IS NOT NULL".into());
        }
        if let Some(tag) = q.tag.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            args.push(tag.to_string().into());
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM game_tags gt JOIN tags t ON t.id = gt.tag_id WHERE gt.game_id = g.id AND t.name = ?{} COLLATE NOCASE)",
                args.len()
            ));
        }
        let order = match q.sort.as_deref() {
            Some("recent") => "g.last_played IS NULL, g.last_played DESC, g.name COLLATE NOCASE",
            Some("playtime") => "MAX(g.platform_playtime_min * 60, COALESCE(s.tracked, 0)) DESC, g.name COLLATE NOCASE",
            Some("added") => "g.added_at DESC, g.name COLLATE NOCASE",
            _ => "g.name COLLATE NOCASE",
        };
        let mut sql = format!(
            "SELECT {GAME_COLUMNS} {GAME_FROM} WHERE {} ORDER BY {order}",
            clauses.join(" AND ")
        );
        if let Some(limit) = q.limit.filter(|l| *l > 0) {
            sql.push_str(&format!(" LIMIT {}", limit.min(10_000)));
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), row_to_game)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn update_game(&self, id: &str, u: &GameUpdate) -> AppResult<Game> {
        let game = self.get_game(id)?;
        if let Some(name) = &u.name {
            let name = name.trim();
            if name.is_empty() || name.len() > 256 {
                return Err(AppError::Invalid(
                    "game name must be between 1 and 256 characters".into(),
                ));
            }
            self.conn.execute(
                "UPDATE games SET name = ?2 WHERE id = ?1",
                params![id, name],
            )?;
        }
        if let Some(desc) = &u.description {
            if desc.len() > 20_000 {
                return Err(AppError::Invalid("description is too long".into()));
            }
            self.conn.execute(
                "UPDATE games SET description = ?2 WHERE id = ?1",
                params![id, non_empty(Some(desc.clone()))],
            )?;
        }
        if let Some(f) = u.favorite {
            self.conn.execute(
                "UPDATE games SET favorite = ?2 WHERE id = ?1",
                params![id, f as i64],
            )?;
        }
        if game.source == "manual" {
            if let Some(exe) = &u.executable {
                let p = Path::new(exe.trim());
                if !p.is_absolute() || !p.is_file() {
                    return Err(AppError::Invalid(format!("executable not found: {exe}")));
                }
                self.conn.execute(
                    "UPDATE games SET executable = ?2 WHERE id = ?1",
                    params![id, exe.trim()],
                )?;
            }
            if let Some(dir) = &u.install_dir {
                self.conn.execute(
                    "UPDATE games SET install_dir = ?2 WHERE id = ?1",
                    params![id, non_empty(Some(dir.clone()))],
                )?;
            }
        }
        self.conn.execute(
            "UPDATE games SET updated_at = ?2 WHERE id = ?1",
            params![id, now_ts()],
        )?;
        self.get_game(id)
    }

    /// Remove a game from VANTA. Never touches files on disk.
    /// Auto-discovered games are tombstoned so the next scan doesn't re-add them.
    pub fn remove_game(&mut self, id: &str) -> AppResult<()> {
        let game = self.get_game(id)?;
        if game.source == "manual" {
            self.conn.execute("DELETE FROM games WHERE id = ?1", [id])?;
        } else {
            self.conn.execute(
                "UPDATE games SET excluded = 1, favorite = 0 WHERE id = ?1",
                [id],
            )?;
        }
        Ok(())
    }

    pub fn excluded_games(&self) -> AppResult<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, platform FROM games WHERE excluded = 1 ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn restore_game(&self, id: &str) -> AppResult<()> {
        let n = self
            .conn
            .execute("UPDATE games SET excluded = 0 WHERE id = ?1", [id])?;
        if n == 0 {
            return Err(AppError::NotFound(format!("game '{id}' not found")));
        }
        Ok(())
    }

    pub fn artwork_hints(&self, id: &str) -> AppResult<ArtworkHints> {
        let raw: String =
            self.conn
                .query_row("SELECT artwork_hints FROM games WHERE id = ?1", [id], |r| {
                    r.get(0)
                })?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    pub fn games_needing_artwork(&self) -> AppResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM games WHERE excluded = 0 AND (cover_path IS NULL OR hero_path IS NULL OR logo_path IS NULL)",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_artwork(
        &self,
        id: &str,
        kind: &str,
        path: Option<&str>,
        source: Option<&str>,
        custom: bool,
    ) -> AppResult<()> {
        let col = match kind {
            "cover" => "cover_path",
            "hero" => "hero_path",
            "logo" => "logo_path",
            _ => return Err(AppError::Invalid(format!("unknown artwork kind '{kind}'"))),
        };
        if kind == "cover" && custom {
            self.conn.execute(
                "UPDATE games SET cover_path = ?2, custom_cover = 1 WHERE id = ?1",
                params![id, path],
            )?;
        } else if kind == "cover" {
            self.conn.execute(
                "UPDATE games SET cover_path = ?2 WHERE id = ?1 AND custom_cover = 0",
                params![id, path],
            )?;
        } else {
            self.conn.execute(
                &format!("UPDATE games SET {col} = ?2 WHERE id = ?1"),
                params![id, path],
            )?;
        }
        self.conn.execute(
            "INSERT OR REPLACE INTO artwork_cache (game_id, kind, source, path, status, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, kind, source, path, if path.is_some() { "ok" } else { "missing" }, now_ts()],
        )?;
        Ok(())
    }

    /// When an artwork kind was last attempted and failed, to avoid re-downloading.
    pub fn artwork_missing_since(&self, id: &str, kind: &str) -> AppResult<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT fetched_at FROM artwork_cache WHERE game_id = ?1 AND kind = ?2 AND status = 'missing'",
                params![id, kind],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn clear_artwork_records(&self) -> AppResult<()> {
        self.conn.execute_batch(
            "DELETE FROM artwork_cache; UPDATE games SET hero_path = NULL, logo_path = NULL;
             UPDATE games SET cover_path = NULL WHERE custom_cover = 0;",
        )?;
        Ok(())
    }

    // -------------------------------------------------------------------- tags

    pub fn set_tags(&mut self, game_id: &str, tags: &[String]) -> AppResult<Vec<String>> {
        self.get_game(game_id)?;
        let mut clean: Vec<String> = vec![];
        for t in tags {
            let t = t.trim();
            if t.is_empty() {
                continue;
            }
            if t.len() > 40 {
                return Err(AppError::Invalid(format!(
                    "tag '{t}' is longer than 40 characters"
                )));
            }
            if !clean.iter().any(|c| c.eq_ignore_ascii_case(t)) {
                clean.push(t.to_string());
            }
        }
        if clean.len() > 32 {
            return Err(AppError::Invalid("a game can have at most 32 tags".into()));
        }
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM game_tags WHERE game_id = ?1", [game_id])?;
        for t in &clean {
            tx.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [t])?;
            tx.execute(
                "INSERT OR IGNORE INTO game_tags (game_id, tag_id) SELECT ?1, id FROM tags WHERE name = ?2 COLLATE NOCASE",
                params![game_id, t],
            )?;
        }
        tx.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM game_tags)",
            [],
        )?;
        tx.commit()?;
        Ok(self.get_game(game_id)?.tags)
    }

    pub fn all_tags(&self) -> AppResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM tags ORDER BY name COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // ---------------------------------------------------------------- profiles

    pub fn get_profile(&self, game_id: &str) -> AppResult<GameProfile> {
        self.get_game(game_id)?;
        let p = self
            .conn
            .query_row(
                "SELECT launch_args, env, working_dir, executable_override, runner, wine_prefix, wine_binary, proton_path, wrapper
                 FROM profiles WHERE game_id = ?1",
                [game_id],
                |r| {
                    let env: String = r.get(1)?;
                    Ok(GameProfile {
                        game_id: game_id.to_string(),
                        launch_args: r.get(0)?,
                        env: serde_json::from_str(&env).unwrap_or_default(),
                        working_dir: r.get(2)?,
                        executable_override: r.get(3)?,
                        runner: r.get(4)?,
                        wine_prefix: r.get(5)?,
                        wine_binary: r.get(6)?,
                        proton_path: r.get(7)?,
                        wrapper: r.get(8)?,
                    })
                },
            )
            .optional()?;
        Ok(p.unwrap_or_else(|| GameProfile::default_for(game_id)))
    }

    fn write_profile(conn: &Connection, p: &GameProfile) -> AppResult<()> {
        conn.execute(
            "INSERT OR REPLACE INTO profiles (game_id, launch_args, env, working_dir, executable_override, runner,
                wine_prefix, wine_binary, proton_path, wrapper, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                p.game_id, p.launch_args, serde_json::to_string(&p.env)?, non_empty(p.working_dir.clone()),
                non_empty(p.executable_override.clone()), p.runner, non_empty(p.wine_prefix.clone()),
                non_empty(p.wine_binary.clone()), non_empty(p.proton_path.clone()), p.wrapper.trim(), now_ts()
            ],
        )?;
        Ok(())
    }

    /// Persist a profile. Validation lives in `launcher::validate_profile`.
    pub fn save_profile(&self, p: &GameProfile) -> AppResult<GameProfile> {
        self.get_game(&p.game_id)?;
        Self::write_profile(&self.conn, p)?;
        self.get_profile(&p.game_id)
    }

    pub fn reset_profile(&self, game_id: &str) -> AppResult<GameProfile> {
        self.conn
            .execute("DELETE FROM profiles WHERE game_id = ?1", [game_id])?;
        self.get_profile(game_id)
    }

    // ---------------------------------------------------------------- sessions

    pub fn start_session(&self, game_id: &str, started_at: i64) -> AppResult<i64> {
        self.conn.execute(
            "INSERT INTO sessions (game_id, started_at, status) VALUES (?1, ?2, 'running')",
            params![game_id, started_at],
        )?;
        let id = self.conn.last_insert_rowid();
        self.conn.execute(
            "UPDATE games SET last_played = MAX(COALESCE(last_played, 0), ?2) WHERE id = ?1",
            params![game_id, started_at],
        )?;
        Ok(id)
    }

    pub fn end_session(
        &self,
        id: i64,
        ended_at: i64,
        exit_code: Option<i32>,
        error: Option<&str>,
    ) -> AppResult<Session> {
        let status = if error.is_some() {
            "failed"
        } else {
            "completed"
        };
        self.conn.execute(
            "UPDATE sessions SET ended_at = ?2, duration_s = MAX(0, ?2 - started_at), exit_code = ?3, status = ?4, error = ?5
             WHERE id = ?1",
            params![id, ended_at, exit_code, status, error],
        )?;
        self.get_session(id)
    }

    pub fn record_failed_launch(&self, game_id: &str, error: &str) -> AppResult<Session> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO sessions (game_id, started_at, ended_at, duration_s, status, error) VALUES (?1, ?2, ?2, 0, 'failed', ?3)",
            params![game_id, now, error],
        )?;
        self.get_session(self.conn.last_insert_rowid())
    }

    pub fn get_session(&self, id: i64) -> AppResult<Session> {
        Ok(self.conn.query_row(
            &format!("{SESSION_SELECT} WHERE s.id = ?1"),
            [id],
            row_to_session,
        )?)
    }

    pub fn game_sessions(&self, game_id: &str, limit: i64) -> AppResult<Vec<Session>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SESSION_SELECT} WHERE s.game_id = ?1 ORDER BY s.started_at DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![game_id, limit], row_to_session)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn statistics(&self) -> AppResult<Statistics> {
        let c = &self.conn;
        let (total_tracked, total_sessions): (i64, i64) = c.query_row(
            "SELECT COALESCE(SUM(duration_s), 0), COUNT(*) FROM sessions WHERE status = 'completed'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let failed: i64 = c.query_row(
            "SELECT COUNT(*) FROM sessions WHERE status = 'failed'",
            [],
            |r| r.get(0),
        )?;
        let (lib, installed, platform_min): (i64, i64, i64) = c.query_row(
            "SELECT COUNT(*), COALESCE(SUM(installed), 0), COALESCE(SUM(platform_playtime_min), 0) FROM games WHERE excluded = 0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let mut stmt = c.prepare(
            "SELECT g.id, g.name, g.platform, g.cover_path, COALESCE(s.tracked, 0), g.platform_playtime_min, COALESCE(s.cnt, 0), g.last_played
             FROM games g LEFT JOIN (
                SELECT game_id, SUM(duration_s) AS tracked, COUNT(*) AS cnt FROM sessions WHERE status = 'completed' GROUP BY game_id
             ) s ON s.game_id = g.id
             WHERE g.excluded = 0 AND (COALESCE(s.tracked, 0) > 0 OR g.platform_playtime_min > 0)
             ORDER BY MAX(g.platform_playtime_min * 60, COALESCE(s.tracked, 0)) DESC LIMIT 50",
        )?;
        let per_game: Vec<GamePlaytime> = stmt
            .query_map([], |r| {
                Ok(GamePlaytime {
                    game_id: r.get(0)?,
                    name: r.get(1)?,
                    platform: r.get(2)?,
                    cover: r.get(3)?,
                    tracked_seconds: r.get(4)?,
                    platform_playtime_min: r.get(5)?,
                    sessions: r.get(6)?,
                    last_played: r.get(7)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        let games_played = per_game.len() as i64;
        let collect = |sql: String| -> AppResult<Vec<Session>> {
            let mut st = c.prepare(&sql)?;
            let rows = st.query_map([], row_to_session)?;
            Ok(rows.collect::<Result<_, _>>()?)
        };
        let longest = collect(format!(
            "{SESSION_SELECT} WHERE s.status = 'completed' ORDER BY s.duration_s DESC LIMIT 5"
        ))?;
        let history = collect(format!(
            "{SESSION_SELECT} ORDER BY s.started_at DESC, s.id DESC LIMIT 40"
        ))?;
        // Last 7 local days, oldest first, including empty days.
        let mut stmt = c.prepare(
            "WITH RECURSIVE days(d, n) AS (
                SELECT date('now', 'localtime', '-6 days'), 0
                UNION ALL SELECT date(d, '+1 day'), n + 1 FROM days WHERE n < 6
             )
             SELECT days.d, COALESCE((
                SELECT SUM(duration_s) FROM sessions
                WHERE status = 'completed' AND date(started_at, 'unixepoch', 'localtime') = days.d
             ), 0) FROM days ORDER BY days.d",
        )?;
        let last_7_days = stmt
            .query_map([], |r| {
                Ok(DayPlaytime {
                    date: r.get(0)?,
                    seconds: r.get(1)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(Statistics {
            total_tracked_seconds: total_tracked,
            total_platform_minutes: platform_min,
            total_sessions,
            failed_launches: failed,
            games_played,
            library_size: lib,
            installed_count: installed,
            per_game,
            longest_sessions: longest,
            history,
            last_7_days,
        })
    }

    // ------------------------------------------------------------ data safety

    /// Write a consistent snapshot of the live database to `dest`.
    pub fn backup_to(&self, dest: &Path) -> AppResult<()> {
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }
        self.conn
            .execute("VACUUM INTO ?1", [dest.to_string_lossy()])?;
        Ok(())
    }

    /// Validate that `src` is a VANTA database, then replace the live database
    /// with it. The current database is snapshotted to `safety` first.
    pub fn restore_from(&mut self, src: &Path, safety: &Path) -> AppResult<()> {
        {
            let probe =
                Connection::open_with_flags(src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .map_err(|e| {
                        AppError::Invalid(format!("not a readable SQLite database: {e}"))
                    })?;
            let version: i64 = probe.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            let has_games: bool = probe
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'games'",
                    [],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !has_games || version < 1 {
                return Err(AppError::Invalid("this file is not a VANTA backup".into()));
            }
            if version as usize > MIGRATIONS.len() {
                return Err(AppError::Invalid(
                    "this backup was created by a newer VANTA version".into(),
                ));
            }
            let ok: String = probe.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
            if ok != "ok" {
                return Err(AppError::Invalid(format!(
                    "backup failed integrity check: {ok}"
                )));
            }
        }
        self.backup_to(safety)?;
        self.conn.restore(
            rusqlite::MAIN_DB,
            src,
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        self.migrate()?;
        self.recover_interrupted_sessions()?;
        Ok(())
    }

    /// Export user configuration (library customizations, profiles, tags, settings) as JSON.
    pub fn export_config(&self) -> AppResult<serde_json::Value> {
        let games = self.query_games(&GameQuery {
            sort: Some("name".into()),
            ..Default::default()
        })?;
        let mut profiles = vec![];
        for g in &games {
            profiles.push(self.get_profile(&g.id)?);
        }
        Ok(serde_json::json!({
            "app": "VANTA",
            "format": 1,
            "exportedAt": now_ts(),
            "settings": self.load_settings()?,
            "games": games,
            "profiles": profiles,
            "tags": self.all_tags()?,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discovered(platform: &str, ext: &str, name: &str) -> DiscoveredGame {
        DiscoveredGame {
            platform: platform.into(),
            external_id: ext.into(),
            name: name.into(),
            installed: true,
            install_dir: Some(format!("/games/{ext}")),
            ..Default::default()
        }
    }

    fn temp_exe(dir: &Path, name: &str) -> String {
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn initializes_and_migrates() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        for p in PLATFORMS {
            assert!(db.platform_enabled(p).unwrap());
        }
    }

    #[test]
    fn migration_is_idempotent_on_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.db");
        {
            let mut db = Db::open(&path).unwrap();
            db.apply_scan("steam", &[discovered("steam", "1", "A")])
                .unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        assert_eq!(db.query_games(&GameQuery::default()).unwrap().len(), 1);
    }

    #[test]
    fn rejects_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.db");
        {
            let c = Connection::open(&path).unwrap();
            c.pragma_update(None, "user_version", 999).unwrap();
        }
        assert!(Db::open(&path).is_err());
    }

    #[test]
    fn insert_update_and_duplicate_prevention() {
        let mut db = Db::open_in_memory().unwrap();
        let r = db
            .apply_scan(
                "steam",
                &[
                    discovered("steam", "730", "CS2"),
                    discovered("steam", "730", "CS2 dup"),
                ],
            )
            .unwrap();
        assert_eq!((r.added, r.updated, r.found), (1, 0, 1));
        db.update_game(
            "steam:730",
            &GameUpdate {
                name: None,
                description: Some("mine".into()),
                favorite: Some(true),
                executable: None,
                install_dir: None,
            },
        )
        .unwrap();
        let mut g = discovered("steam", "730", "Counter-Strike 2");
        g.platform_playtime_min = Some(120);
        g.last_played = Some(1000);
        let r = db.apply_scan("steam", &[g]).unwrap();
        assert_eq!((r.added, r.updated), (0, 1));
        let game = db.get_game("steam:730").unwrap();
        assert_eq!(game.name, "Counter-Strike 2");
        assert_eq!(game.platform_playtime_min, 120);
        assert!(game.favorite, "user data must survive rescans");
        assert_eq!(game.description.as_deref(), Some("mine"));
        assert_eq!(db.query_games(&GameQuery::default()).unwrap().len(), 1);
    }

    #[test]
    fn missing_games_marked_uninstalled_not_deleted() {
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan(
            "steam",
            &[discovered("steam", "1", "A"), discovered("steam", "2", "B")],
        )
        .unwrap();
        let r = db
            .apply_scan("steam", &[discovered("steam", "1", "A")])
            .unwrap();
        assert_eq!(r.marked_uninstalled, 1);
        assert!(!db.get_game("steam:2").unwrap().installed);
    }

    #[test]
    fn removal_tombstones_auto_and_deletes_manual() {
        let dir = tempfile::tempdir().unwrap();
        let exe = temp_exe(dir.path(), "game.sh");
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan("steam", &[discovered("steam", "1", "A")])
            .unwrap();
        db.remove_game("steam:1").unwrap();
        assert!(db.query_games(&GameQuery::default()).unwrap().is_empty());
        db.apply_scan("steam", &[discovered("steam", "1", "A")])
            .unwrap();
        assert!(
            db.query_games(&GameQuery::default()).unwrap().is_empty(),
            "removed games must not come back"
        );
        db.restore_game("steam:1").unwrap();
        assert_eq!(db.query_games(&GameQuery::default()).unwrap().len(), 1);

        let g = db
            .add_manual_game(&ManualGameInput {
                name: "Mine".into(),
                executable: exe.clone(),
                install_dir: None,
                launch_args: None,
                runner: None,
                platform: None,
                cover: None,
            })
            .unwrap();
        db.remove_game(&g.id).unwrap();
        assert!(db.get_game(&g.id).is_err());
        assert!(Path::new(&exe).exists(), "removal must never delete files");
    }

    #[test]
    fn manual_import_validation_and_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let exe = temp_exe(dir.path(), "run.sh");
        let mut db = Db::open_in_memory().unwrap();
        let input = |exe: &str| ManualGameInput {
            name: "Game".into(),
            executable: exe.into(),
            install_dir: None,
            launch_args: Some("-w".into()),
            runner: Some("native".into()),
            platform: None,
            cover: None,
        };
        let g = db.add_manual_game(&input(&exe)).unwrap();
        assert_eq!(g.platform, "local");
        assert_eq!(g.install_dir.as_deref(), Some(dir.path().to_str().unwrap()));
        assert_eq!(db.get_profile(&g.id).unwrap().launch_args, "-w");
        assert!(
            db.add_manual_game(&input(&exe)).is_err(),
            "duplicate executable"
        );
        assert!(
            db.add_manual_game(&input("/definitely/missing/exe"))
                .is_err(),
            "missing executable"
        );
        assert!(
            db.add_manual_game(&input("relative/path")).is_err(),
            "relative path"
        );
    }

    #[test]
    fn search_filter_sort() {
        let mut db = Db::open_in_memory().unwrap();
        let mut a = discovered("steam", "1", "Alpha Strike");
        a.last_played = Some(50);
        a.platform_playtime_min = Some(10);
        let mut b = discovered("steam", "2", "beta_100%");
        b.installed = false;
        b.platform_playtime_min = Some(500);
        db.apply_scan("steam", &[a, b]).unwrap();
        let mut c = discovered("epic", "x", "Gamma");
        c.last_played = Some(100);
        db.apply_scan("epic", &[c]).unwrap();

        let q = |q: GameQuery| {
            db.query_games(&q)
                .unwrap()
                .into_iter()
                .map(|g| g.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            q(GameQuery {
                search: Some("STRIKE".into()),
                ..Default::default()
            }),
            vec!["Alpha Strike"]
        );
        assert_eq!(
            q(GameQuery {
                search: Some("100%".into()),
                ..Default::default()
            }),
            vec!["beta_100%"]
        );
        assert_eq!(
            q(GameQuery {
                search: Some("%".into()),
                ..Default::default()
            }),
            vec!["beta_100%"],
            "LIKE wildcards are escaped"
        );
        assert_eq!(
            q(GameQuery {
                platforms: vec!["epic".into()],
                ..Default::default()
            }),
            vec!["Gamma"]
        );
        assert_eq!(
            q(GameQuery {
                install_status: Some("not_installed".into()),
                ..Default::default()
            }),
            vec!["beta_100%"]
        );
        assert_eq!(
            q(GameQuery {
                sort: Some("name".into()),
                ..Default::default()
            }),
            vec!["Alpha Strike", "beta_100%", "Gamma"]
        );
        assert_eq!(
            q(GameQuery {
                sort: Some("recent".into()),
                ..Default::default()
            }),
            vec!["Gamma", "Alpha Strike", "beta_100%"]
        );
        assert_eq!(
            q(GameQuery {
                sort: Some("playtime".into()),
                ..Default::default()
            })[0],
            "beta_100%"
        );
        assert_eq!(
            q(GameQuery {
                played_only: true,
                ..Default::default()
            })
            .len(),
            2
        );

        db.update_game(
            "epic:x",
            &GameUpdate {
                name: None,
                description: None,
                favorite: Some(true),
                executable: None,
                install_dir: None,
            },
        )
        .unwrap();
        assert_eq!(
            q(GameQuery {
                favorites_only: true,
                ..Default::default()
            }),
            vec!["Gamma"]
        );
    }

    #[test]
    fn tags_roundtrip_and_filter() {
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan(
            "steam",
            &[discovered("steam", "1", "A"), discovered("steam", "2", "B")],
        )
        .unwrap();
        let tags = db
            .set_tags(
                "steam:1",
                &["RPG".into(), "rpg".into(), " Co-op ".into(), "".into()],
            )
            .unwrap();
        assert_eq!(tags, vec!["Co-op", "RPG"]);
        let hits = db
            .query_games(&GameQuery {
                tag: Some("rpg".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        db.set_tags("steam:1", &[]).unwrap();
        assert!(
            db.all_tags().unwrap().is_empty(),
            "orphan tags are cleaned up"
        );
        assert!(db.set_tags("steam:1", &["x".repeat(41)]).is_err());
    }

    #[test]
    fn profile_persistence_and_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.db");
        {
            let mut db = Db::open(&path).unwrap();
            db.apply_scan("steam", &[discovered("steam", "1", "A")])
                .unwrap();
            let mut p = GameProfile::default_for("steam:1");
            p.launch_args = "-novid -high".into();
            p.env = vec![EnvVar {
                key: "DXVK_HUD".into(),
                value: "fps".into(),
            }];
            p.wrapper = "gamemoderun".into();
            db.save_profile(&p).unwrap();
        }
        let db = Db::open(&path).unwrap();
        let p = db.get_profile("steam:1").unwrap();
        assert_eq!(p.launch_args, "-novid -high");
        assert_eq!(p.env[0].key, "DXVK_HUD");
        assert_eq!(p.wrapper, "gamemoderun");
        let p = db.reset_profile("steam:1").unwrap();
        assert_eq!(p, GameProfile::default_for("steam:1"));
    }

    #[test]
    fn session_tracking_and_statistics() {
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan("steam", &[discovered("steam", "1", "A")])
            .unwrap();
        let now = now_ts();
        let s = db.start_session("steam:1", now - 3600).unwrap();
        let ended = db.end_session(s, now, Some(0), None).unwrap();
        assert_eq!(ended.duration_s, 3600);
        assert_eq!(ended.status, "completed");
        db.record_failed_launch("steam:1", "boom").unwrap();
        let g = db.get_game("steam:1").unwrap();
        assert_eq!(g.tracked_seconds, 3600);
        assert_eq!(g.session_count, 1);
        assert_eq!(g.last_played, Some(now - 3600));
        let st = db.statistics().unwrap();
        assert_eq!(st.total_tracked_seconds, 3600);
        assert_eq!(st.total_sessions, 1);
        assert_eq!(st.failed_launches, 1);
        assert_eq!(st.last_7_days.len(), 7);
        assert_eq!(st.last_7_days.iter().map(|d| d.seconds).sum::<i64>(), 3600);
        assert_eq!(st.longest_sessions[0].duration_s, 3600);
        assert_eq!(st.history.len(), 2);
    }

    #[test]
    fn interrupted_sessions_recovered_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.db");
        {
            let mut db = Db::open(&path).unwrap();
            db.apply_scan("steam", &[discovered("steam", "1", "A")])
                .unwrap();
            db.start_session("steam:1", now_ts()).unwrap();
        }
        let db = Db::open(&path).unwrap();
        let s = db.game_sessions("steam:1", 10).unwrap();
        assert_eq!(s[0].status, "interrupted");
        assert_eq!(
            db.get_game("steam:1").unwrap().tracked_seconds,
            0,
            "unknown durations are not invented"
        );
    }

    #[test]
    fn backup_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Db::open(&dir.path().join("live.db")).unwrap();
        db.apply_scan("steam", &[discovered("steam", "1", "A")])
            .unwrap();
        let backup = dir.path().join("backup.db");
        db.backup_to(&backup).unwrap();
        db.apply_scan(
            "steam",
            &[discovered("steam", "1", "A"), discovered("steam", "2", "B")],
        )
        .unwrap();
        assert_eq!(db.query_games(&GameQuery::default()).unwrap().len(), 2);
        db.restore_from(&backup, &dir.path().join("safety.db"))
            .unwrap();
        assert_eq!(db.query_games(&GameQuery::default()).unwrap().len(), 1);
        assert!(dir.path().join("safety.db").exists());
        // Garbage files are rejected without touching the live DB.
        let junk = dir.path().join("junk.db");
        std::fs::write(&junk, b"not a database at all").unwrap();
        assert!(db
            .restore_from(&junk, &dir.path().join("safety2.db"))
            .is_err());
        assert_eq!(db.query_games(&GameQuery::default()).unwrap().len(), 1);
    }

    #[test]
    fn settings_persist_and_recover() {
        let db = Db::open_in_memory().unwrap();
        let mut s = db.load_settings().unwrap();
        s.theme = "graphite".into();
        db.save_settings(&s).unwrap();
        assert_eq!(db.load_settings().unwrap().theme, "graphite");
        db.conn
            .execute(
                "UPDATE settings SET value = 'garbage' WHERE key = 'app'",
                [],
            )
            .unwrap();
        assert_eq!(db.load_settings().unwrap(), Settings::default());
        let bad = Settings {
            theme: "x".into(),
            ..Default::default()
        };
        assert!(db.save_settings(&bad).is_err());
    }

    #[test]
    fn export_contains_profiles() {
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan("steam", &[discovered("steam", "1", "A")])
            .unwrap();
        let v = db.export_config().unwrap();
        assert_eq!(v["games"].as_array().unwrap().len(), 1);
        assert_eq!(v["profiles"][0]["gameId"], "steam:1");
    }
}
