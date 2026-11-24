use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tauri::{AppHandle, Emitter};

use crate::artwork::ArtworkService;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::launcher::ActiveMap;
use crate::models::ScanReport;
use crate::monitor::Monitor;
use crate::platforms;

#[derive(Clone)]
pub struct Paths {
    pub data: PathBuf,
    pub db: PathBuf,
    pub artwork: PathBuf,
    pub logs: PathBuf,
    pub backups: PathBuf,
    pub prefixes: PathBuf,
}

impl Paths {
    pub fn new(data: PathBuf) -> Self {
        Paths {
            db: data.join("vanta.db"),
            artwork: data.join("artwork"),
            logs: data.join("logs"),
            backups: data.join("backups"),
            prefixes: data.join("prefixes"),
            data,
        }
    }
}

pub struct AppState {
    pub db: Arc<Mutex<Db>>,
    pub paths: Paths,
    pub active: ActiveMap,
    pub monitor: Arc<Mutex<Monitor>>,
    pub artwork: Arc<ArtworkService>,
    pub scanning: Arc<AtomicBool>,
    pub startup_notice: Mutex<Option<String>>,
}

/// Cheap clones of the shared handles, for background tasks.
#[derive(Clone)]
pub struct Handles {
    pub db: Arc<Mutex<Db>>,
    pub artwork: Arc<ArtworkService>,
    pub scanning: Arc<AtomicBool>,
}

impl AppState {
    pub fn db(&self) -> AppResult<MutexGuard<'_, Db>> {
        // A poisoned lock means a previous holder panicked mid-operation; the
        // connection itself is still valid (SQLite transactions roll back).
        Ok(self.db.lock().unwrap_or_else(|p| p.into_inner()))
    }

    pub fn clone_handles(&self) -> Handles {
        Handles {
            db: self.db.clone(),
            artwork: self.artwork.clone(),
            scanning: self.scanning.clone(),
        }
    }

    pub fn new(paths: Paths, db: Db, notice: Option<String>) -> Self {
        AppState {
            db: Arc::new(Mutex::new(db)),
            artwork: Arc::new(ArtworkService::new(paths.artwork.clone())),
            paths,
            active: Arc::new(Mutex::new(HashMap::new())),
            monitor: Arc::new(Mutex::new(Monitor::new())),
            scanning: Arc::new(AtomicBool::new(false)),
            startup_notice: Mutex::new(notice),
        }
    }
}

struct ScanGuard(Arc<AtomicBool>);
impl Drop for ScanGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Scan enabled platforms (or one platform). Each adapter runs in isolation:
/// a failing integration is recorded and reported but never stops the others.
pub async fn run_scan(
    app: AppHandle,
    h: Handles,
    only: Option<String>,
) -> AppResult<Vec<ScanReport>> {
    if h.scanning.swap(true, Ordering::SeqCst) {
        return Err(AppError::Invalid(
            "a library scan is already running".into(),
        ));
    }
    let _guard = ScanGuard(h.scanning.clone());
    let _ = app.emit("scan-state", true);
    let mut reports = vec![];
    for adapter_id in platforms::all_adapters().iter().map(|a| a.id()) {
        if only.as_deref().is_some_and(|o| o != adapter_id) || adapter_id == "local" {
            continue;
        }
        let enabled =
            h.db.lock()
                .unwrap_or_else(|p| p.into_inner())
                .platform_enabled(adapter_id)?;
        if !enabled {
            continue;
        }
        let result = tauri::async_runtime::spawn_blocking(move || {
            let adapter = platforms::adapter_for(adapter_id).ok_or("adapter missing")?;
            if !adapter.detect().detected {
                return Ok(None);
            }
            adapter.discover().map(Some)
        })
        .await
        .unwrap_or_else(|e| Err(format!("integration crashed: {e}")));
        let db = h.db.lock().unwrap_or_else(|p| p.into_inner());
        match result {
            Ok(Some(found)) => {
                let mut db = db;
                let report = db.apply_scan(adapter_id, &found)?;
                db.set_platform_sync(adapter_id, None)?;
                tracing::info!(
                    platform = adapter_id,
                    found = report.found,
                    added = report.added,
                    "scan complete"
                );
                reports.push(report);
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(platform = adapter_id, error = %e, "platform scan failed");
                db.set_platform_sync(adapter_id, Some(&e))?;
                reports.push(ScanReport {
                    platform: adapter_id.into(),
                    found: 0,
                    added: 0,
                    updated: 0,
                    marked_uninstalled: 0,
                    error: Some(e),
                });
            }
        }
    }
    let _ = app.emit("scan-state", false);
    let _ = app.emit("library-updated", ());
    // Artwork in the background; the library is usable immediately.
    let app2 = app.clone();
    let h2 = h.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = refresh_artwork(app2, h2).await {
            tracing::warn!(error = %e, "artwork refresh failed");
        }
    });
    Ok(reports)
}

pub async fn refresh_artwork(app: AppHandle, h: Handles) -> AppResult<usize> {
    let (ids, allow_download) = {
        let db = h.db.lock().unwrap_or_else(|p| p.into_inner());
        (
            db.games_needing_artwork()?,
            db.load_settings()?.download_artwork,
        )
    };
    let mut changed = 0;
    for (i, id) in ids.iter().enumerate() {
        match h.artwork.ensure(&h.db, id, allow_download).await {
            Ok(true) => changed += 1,
            Ok(false) => {}
            Err(e) => tracing::debug!(game = %id, error = %e, "artwork error"),
        }
        if changed > 0 && i % 8 == 7 {
            let _ = app.emit("library-updated", ());
        }
    }
    if changed > 0 {
        let _ = app.emit("library-updated", ());
    }
    Ok(changed)
}
