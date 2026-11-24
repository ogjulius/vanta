//! Artwork cache. Images come from (in order): the platform's local cache
//! (e.g. Steam's `appcache/librarycache`), then the platform's public CDN.
//! Everything is copied into VANTA's own artwork directory so the UI only
//! ever loads files from one sandboxed location, works offline, and never
//! downloads the same image twice. Failed lookups are remembered for a week.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::launcher::sanitize_id;
use crate::models::now_ts;

const RETRY_MISSING_AFTER: i64 = 7 * 24 * 3600;
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
pub const KINDS: [&str; 3] = ["cover", "hero", "logo"];

fn ext_for(path_or_url: &str, content_type: Option<&str>) -> &'static str {
    match content_type {
        Some(ct) if ct.contains("png") => return "png",
        Some(ct) if ct.contains("webp") => return "webp",
        Some(ct) if ct.contains("jpeg") || ct.contains("jpg") => return "jpg",
        _ => {}
    }
    let lower = path_or_url.to_ascii_lowercase();
    let clean = lower.split(['?', '#']).next().unwrap_or("");
    if clean.ends_with(".png") {
        "png"
    } else if clean.ends_with(".webp") {
        "webp"
    } else {
        "jpg"
    }
}

fn is_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        || bytes.starts_with(b"\x89PNG")
        || (bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
}

pub fn game_dir(root: &Path, game_id: &str) -> PathBuf {
    root.join(sanitize_id(game_id))
}

/// Atomically write a file (temp file + rename) so a crash never leaves a torn image.
fn write_atomic(dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, dest)
}

pub struct ArtworkService {
    pub root: PathBuf,
    client: reqwest::Client,
}

impl ArtworkService {
    pub fn new(root: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("VANTA-launcher/0.1")
            .build()
            .unwrap_or_default();
        ArtworkService { root, client }
    }

    async fn download(&self, url: &str) -> Result<(Vec<u8>, Option<String>), String> {
        if !url.starts_with("https://") {
            return Err("only https artwork URLs are allowed".into());
        }
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        if let Some(len) = resp.content_length() {
            if len as usize > MAX_IMAGE_BYTES {
                return Err("image too large".into());
            }
        }
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() > MAX_IMAGE_BYTES || !is_image(&bytes) {
            return Err("response is not a supported image".into());
        }
        Ok((bytes.to_vec(), ct))
    }

    /// Make sure cover/hero/logo for one game are cached, if obtainable.
    /// Returns true if anything changed.
    pub async fn ensure(
        &self,
        db: &Arc<Mutex<Db>>,
        game_id: &str,
        allow_download: bool,
    ) -> AppResult<bool> {
        let (game, hints) = {
            let db = db.lock().unwrap_or_else(|p| p.into_inner());
            (db.get_game(game_id)?, db.artwork_hints(game_id)?)
        };
        let mut changed = false;
        for kind in KINDS {
            let current = match kind {
                "cover" => &game.cover,
                "hero" => &game.hero,
                _ => &game.logo,
            };
            if current
                .as_deref()
                .map(|p| Path::new(p).is_file())
                .unwrap_or(false)
            {
                continue;
            }
            let (local, url) = match kind {
                "cover" => (&hints.cover_local, &hints.cover_url),
                "hero" => (&hints.hero_local, &hints.hero_url),
                _ => (&hints.logo_local, &hints.logo_url),
            };
            let dir = game_dir(&self.root, game_id);
            let mut result: Option<(PathBuf, String)> = None;
            if let Some(src) = local.as_deref().filter(|p| Path::new(p).is_file()) {
                let dest = dir.join(format!("{kind}.{}", ext_for(src, None)));
                match std::fs::read(src) {
                    Ok(bytes) if is_image(&bytes) => {
                        write_atomic(&dest, &bytes)?;
                        result = Some((dest, src.to_string()));
                    }
                    Ok(_) => {
                        tracing::debug!(game = game_id, kind, "local artwork is not a valid image")
                    }
                    Err(e) => {
                        tracing::debug!(game = game_id, kind, error = %e, "cannot read local artwork")
                    }
                }
            }
            if result.is_none() && allow_download {
                if let Some(url) = url.as_deref() {
                    let recently_missing = {
                        let db = db.lock().unwrap_or_else(|p| p.into_inner());
                        db.artwork_missing_since(game_id, kind)?
                            .map(|t| now_ts() - t < RETRY_MISSING_AFTER)
                            .unwrap_or(false)
                    };
                    if !recently_missing {
                        match self.download(url).await {
                            Ok((bytes, ct)) => {
                                let dest =
                                    dir.join(format!("{kind}.{}", ext_for(url, ct.as_deref())));
                                write_atomic(&dest, &bytes)?;
                                result = Some((dest, url.to_string()));
                            }
                            Err(e) => {
                                tracing::debug!(game = game_id, kind, error = %e, "artwork download failed");
                                let db = db.lock().unwrap_or_else(|p| p.into_inner());
                                db.set_artwork(game_id, kind, None, Some(url), false)?;
                            }
                        }
                    }
                }
            }
            if let Some((path, source)) = result {
                let db = db.lock().unwrap_or_else(|p| p.into_inner());
                db.set_artwork(
                    game_id,
                    kind,
                    Some(&path.to_string_lossy()),
                    Some(&source),
                    false,
                )?;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Copy a user-chosen image in as the game's custom cover.
    pub fn set_custom_cover(&self, db: &Db, game_id: &str, src: &Path) -> AppResult<String> {
        db.get_game(game_id)?;
        let meta = std::fs::metadata(src)
            .map_err(|e| AppError::Invalid(format!("cannot read image: {e}")))?;
        if meta.len() as usize > MAX_IMAGE_BYTES {
            return Err(AppError::Invalid("image is larger than 20 MB".into()));
        }
        let bytes = std::fs::read(src)?;
        if !is_image(&bytes) {
            return Err(AppError::Invalid(
                "unsupported image format (use PNG, JPEG or WebP)".into(),
            ));
        }
        let dir = game_dir(&self.root, game_id);
        // Unique filename so the webview doesn't show a stale cached image.
        let dest = dir.join(format!(
            "cover-custom-{}.{}",
            now_ts(),
            ext_for(&src.to_string_lossy(), None)
        ));
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with("cover-custom-") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        write_atomic(&dest, &bytes)?;
        let path = dest.to_string_lossy().into_owned();
        db.set_artwork(game_id, "cover", Some(&path), Some("custom"), true)?;
        Ok(path)
    }

    pub fn cache_size(&self) -> u64 {
        fn walk(p: &Path) -> u64 {
            std::fs::read_dir(p)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| match e.metadata() {
                            Ok(m) if m.is_dir() => walk(&e.path()),
                            Ok(m) => m.len(),
                            Err(_) => 0,
                        })
                        .sum()
                })
                .unwrap_or(0)
        }
        walk(&self.root)
    }

    /// Remove cached (non-custom) artwork. Only ever touches VANTA's own artwork dir.
    pub fn clear(&self, db: &Db) -> AppResult<()> {
        db.clear_artwork_records()?;
        if let Ok(rd) = std::fs::read_dir(&self.root) {
            for game in rd.flatten() {
                let Ok(files) = std::fs::read_dir(game.path()) else {
                    continue;
                };
                for f in files.flatten() {
                    if !f.file_name().to_string_lossy().starts_with("cover-custom-") {
                        let _ = std::fs::remove_file(f.path());
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ArtworkHints, DiscoveredGame};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfakepng";

    #[tokio::test]
    async fn copies_local_artwork_and_handles_missing() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("library_600x900.png");
        std::fs::write(&src, PNG).unwrap();
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan(
            "steam",
            &[DiscoveredGame {
                platform: "steam".into(),
                external_id: "1".into(),
                name: "A".into(),
                artwork: ArtworkHints {
                    cover_local: Some(src.to_string_lossy().into()),
                    hero_local: Some("/missing.jpg".into()),
                    ..Default::default()
                },
                ..Default::default()
            }],
        )
        .unwrap();
        let db = Arc::new(Mutex::new(db));
        let svc = ArtworkService::new(dir.path().join("art"));
        assert!(svc.ensure(&db, "steam:1", false).await.unwrap());
        let g = db.lock().unwrap().get_game("steam:1").unwrap();
        let cover = g.cover.unwrap();
        assert!(cover.starts_with(dir.path().join("art").to_str().unwrap()));
        assert!(
            g.hero.is_none(),
            "missing artwork stays empty; UI uses fallback art"
        );
        // Second run is a no-op: nothing is copied or downloaded again.
        assert!(!svc.ensure(&db, "steam:1", false).await.unwrap());
        assert!(svc.cache_size() > 0);

        let custom = dir.path().join("mine.png");
        std::fs::write(&custom, PNG).unwrap();
        let path = svc
            .set_custom_cover(&db.lock().unwrap(), "steam:1", &custom)
            .unwrap();
        svc.clear(&db.lock().unwrap()).unwrap();
        assert_eq!(
            db.lock().unwrap().get_game("steam:1").unwrap().cover,
            Some(path),
            "custom covers survive cache clears"
        );
        let junk = dir.path().join("junk.png");
        std::fs::write(&junk, "nope").unwrap();
        assert!(svc
            .set_custom_cover(&db.lock().unwrap(), "steam:1", &junk)
            .is_err());
    }

    #[tokio::test]
    async fn unavailable_provider_does_not_fail() {
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan(
            "epic",
            &[DiscoveredGame {
                platform: "epic".into(),
                external_id: "x".into(),
                name: "X".into(),
                artwork: ArtworkHints {
                    cover_url: Some("http://insecure.invalid/a.jpg".into()),
                    ..Default::default()
                },
                ..Default::default()
            }],
        )
        .unwrap();
        let db = Arc::new(Mutex::new(db));
        let dir = tempfile::tempdir().unwrap();
        let svc = ArtworkService::new(dir.path().into());
        assert!(!svc.ensure(&db, "epic:x", true).await.unwrap());
        assert!(db
            .lock()
            .unwrap()
            .artwork_missing_since("epic:x", "cover")
            .unwrap()
            .is_some());
    }
}
