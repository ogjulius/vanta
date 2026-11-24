//! Tauri command surface. Every command returns `Result<_, AppError>` so the
//! frontend always gets a structured, human-readable error.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::error::{AppError, AppResult};
use crate::launcher::{self, ActiveSession, LaunchContext, SessionEvent};
use crate::models::*;
use crate::monitor::{self, SystemMetrics};
use crate::platforms::{self, desktop::DesktopGame, runners::Runner, steam::SteamDownload};
use crate::settings::Settings;
use crate::state::AppState;

// ------------------------------------------------------------------ bootstrap

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub version: String,
    pub settings: Settings,
    pub user_name: String,
    pub notice: Option<String>,
}

#[tauri::command]
pub fn bootstrap(app: AppHandle, state: State<'_, AppState>) -> AppResult<Bootstrap> {
    let settings = state.db()?.load_settings()?;
    let user_name = std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "player".into());
    let user_name = user_name[..1].to_uppercase() + &user_name[1..];
    Ok(Bootstrap {
        version: app.package_info().version.to_string(),
        settings,
        user_name,
        notice: state
            .startup_notice
            .lock()
            .map(|n| n.clone())
            .unwrap_or(None),
    })
}

// ---------------------------------------------------------------------- games

#[tauri::command]
pub fn list_games(state: State<'_, AppState>, query: GameQuery) -> AppResult<Vec<Game>> {
    state.db()?.query_games(&query)
}

#[tauri::command]
pub fn get_game(state: State<'_, AppState>, id: String) -> AppResult<Game> {
    state.db()?.get_game(&id)
}

#[tauri::command]
pub fn update_game(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    update: GameUpdate,
) -> AppResult<Game> {
    let g = state.db()?.update_game(&id, &update)?;
    let _ = app.emit("library-updated", ());
    Ok(g)
}

#[tauri::command]
pub fn remove_game(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    if state
        .active
        .lock()
        .map(|a| a.contains_key(&id))
        .unwrap_or(false)
    {
        return Err(AppError::Invalid(
            "stop the game before removing it from your library".into(),
        ));
    }
    state.db()?.remove_game(&id)?;
    let _ = app.emit("library-updated", ());
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedGame {
    pub id: String,
    pub name: String,
    pub platform: String,
}

#[tauri::command]
pub fn list_removed_games(state: State<'_, AppState>) -> AppResult<Vec<RemovedGame>> {
    Ok(state
        .db()?
        .excluded_games()?
        .into_iter()
        .map(|(id, name, platform)| RemovedGame { id, name, platform })
        .collect())
}

#[tauri::command]
pub fn restore_game(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db()?.restore_game(&id)?;
    let _ = app.emit("library-updated", ());
    Ok(())
}

#[tauri::command]
pub async fn add_manual_game(
    app: AppHandle,
    state: State<'_, AppState>,
    input: ManualGameInput,
) -> AppResult<Game> {
    let game = state.db()?.add_manual_game(&input)?;
    if let Some(cover) = input.cover.as_deref().filter(|c| !c.trim().is_empty()) {
        if let Err(e) = state
            .artwork
            .set_custom_cover(&*state.db()?, &game.id, Path::new(cover))
        {
            tracing::warn!(error = %e, "custom cover rejected during import");
        }
    }
    let _ = app.emit("library-updated", ());
    state.db()?.get_game(&game.id)
}

#[tauri::command]
pub fn set_tags(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    tags: Vec<String>,
) -> AppResult<Vec<String>> {
    let t = state.db()?.set_tags(&id, &tags)?;
    let _ = app.emit("library-updated", ());
    Ok(t)
}

#[tauri::command]
pub fn list_tags(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    state.db()?.all_tags()
}

#[tauri::command]
pub fn set_custom_cover(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> AppResult<Game> {
    let db = state.db()?;
    state.artwork.set_custom_cover(&db, &id, Path::new(&path))?;
    let _ = app.emit("library-updated", ());
    db.get_game(&id)
}

#[tauri::command]
pub fn game_sessions(state: State<'_, AppState>, id: String) -> AppResult<Vec<Session>> {
    state.db()?.game_sessions(&id, 20)
}

// ------------------------------------------------------------------- profiles

#[tauri::command]
pub fn get_profile(state: State<'_, AppState>, id: String) -> AppResult<GameProfile> {
    state.db()?.get_profile(&id)
}

#[tauri::command]
pub fn save_profile(state: State<'_, AppState>, profile: GameProfile) -> AppResult<GameProfile> {
    launcher::validate_profile(&profile)?;
    state.db()?.save_profile(&profile)
}

#[tauri::command]
pub fn reset_profile(state: State<'_, AppState>, id: String) -> AppResult<GameProfile> {
    state.db()?.reset_profile(&id)
}

/// Show the exact command a launch would run, for transparency in the UI.
#[tauri::command]
pub fn preview_launch(state: State<'_, AppState>, id: String) -> AppResult<Vec<String>> {
    let (game, profile) = {
        let db = state.db()?;
        (db.get_game(&id)?, db.get_profile(&id)?)
    };
    let p = launcher::prepare(&game, &profile, &state.paths.prefixes)?;
    let mut out: Vec<String> = p.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
    out.push(p.program.to_string_lossy().into_owned());
    out.extend(p.args);
    Ok(out)
}

// ------------------------------------------------------------------- launching

#[tauri::command]
pub async fn launch_game(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    let (game, profile, settings) = {
        let db = state.db()?;
        (db.get_game(&id)?, db.get_profile(&id)?, db.load_settings()?)
    };
    if !state.db()?.platform_enabled(&game.platform)? {
        return Err(AppError::Launch(format!(
            "the {} integration is disabled in Settings",
            game.platform
        )));
    }
    let emitter_app = app.clone();
    let ctx = LaunchContext {
        db: state.db.clone(),
        active: state.active.clone(),
        emit: Arc::new(move |e: SessionEvent| {
            let _ = emitter_app.emit("session", &e);
            if e.state == "ended" || e.state == "failed" {
                let _ = emitter_app.emit("library-updated", ());
            }
        }),
        logs_dir: state.paths.logs.clone(),
        poll_secs: settings.session_poll_secs,
    };
    let prepared = match launcher::prepare(&game, &profile, &state.paths.prefixes) {
        Ok(p) => p,
        Err(e) => {
            let msg = e.to_string();
            let session = state.db()?.record_failed_launch(&game.id, &msg).ok();
            let _ = app.emit(
                "session",
                SessionEvent {
                    game_id: game.id.clone(),
                    state: "failed".into(),
                    message: Some(msg),
                    session,
                },
            );
            return Err(e);
        }
    };
    launcher::launch(ctx, game, prepared).await
}

#[tauri::command]
pub fn stop_game(state: State<'_, AppState>, id: String) -> AppResult<()> {
    launcher::stop(&state.active, &id)
}

#[tauri::command]
pub fn active_sessions(state: State<'_, AppState>) -> AppResult<Vec<ActiveSession>> {
    Ok(state
        .active
        .lock()
        .map(|a| a.values().cloned().collect())
        .unwrap_or_default())
}

// ------------------------------------------------------------------ platforms

#[tauri::command]
pub async fn platform_statuses(state: State<'_, AppState>) -> AppResult<Vec<PlatformStatus>> {
    let detections = tauri::async_runtime::spawn_blocking(|| {
        platforms::all_adapters()
            .into_iter()
            .map(|a| (a.id(), a.name(), a.detect()))
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;
    let db = state.db()?;
    let mut out = vec![];
    for (id, name, d) in detections {
        let (count, installed) = db.platform_counts(id)?;
        let (last_sync, last_error) = db.platform_sync_info(id)?;
        out.push(PlatformStatus {
            id: id.into(),
            name: name.into(),
            enabled: db.platform_enabled(id)?,
            detected: d.detected,
            connected: d.connected,
            account: d.account,
            game_count: count,
            installed_count: installed,
            last_sync,
            last_error,
            details: d.details,
            help: d.help,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn set_platform_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> AppResult<()> {
    state.db()?.set_platform_enabled(&id, enabled)?;
    let _ = app.emit("library-updated", ());
    Ok(())
}

#[tauri::command]
pub async fn scan_libraries(
    app: AppHandle,
    state: State<'_, AppState>,
    platform: Option<String>,
) -> AppResult<Vec<ScanReport>> {
    crate::state::run_scan(app, state.inner().clone_handles(), platform).await
}

#[tauri::command]
pub fn is_scanning(state: State<'_, AppState>) -> bool {
    state.scanning.load(Ordering::SeqCst)
}

#[tauri::command]
pub async fn desktop_entries(state: State<'_, AppState>) -> AppResult<Vec<DesktopGame>> {
    let entries = tauri::async_runtime::spawn_blocking(platforms::desktop::discover)
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    let existing: Vec<String> = state
        .db()?
        .query_games(&GameQuery {
            platforms: vec!["local".into()],
            ..Default::default()
        })?
        .into_iter()
        .filter_map(|g| g.executable)
        .collect();
    Ok(entries
        .into_iter()
        .filter(|e| !existing.contains(&e.executable))
        .collect())
}

#[tauri::command]
pub async fn detect_runners() -> AppResult<Vec<Runner>> {
    tauri::async_runtime::spawn_blocking(platforms::runners::detect)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn steam_downloads() -> AppResult<Vec<SteamDownload>> {
    tauri::async_runtime::spawn_blocking(|| platforms::steam::SteamAdapter::system().downloads())
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

// ---------------------------------------------------------------- performance

#[tauri::command]
pub async fn system_metrics(state: State<'_, AppState>) -> AppResult<SystemMetrics> {
    let running: Vec<(String, String, u32)> = state
        .active
        .lock()
        .map(|a| {
            a.values()
                .filter_map(|s| s.pid.map(|p| (s.game_id.clone(), s.game_name.clone(), p)))
                .collect()
        })
        .unwrap_or_default();
    let monitor = state.monitor.clone();
    let mut metrics = tauri::async_runtime::spawn_blocking(move || {
        monitor
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .sample(&running)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;
    let (gpus, note) = monitor::gpus().await;
    metrics.gpus = gpus;
    metrics.gpu_note = note;
    Ok(metrics)
}

#[tauri::command]
pub fn get_statistics(state: State<'_, AppState>) -> AppResult<Statistics> {
    state.db()?.statistics()
}

// -------------------------------------------------------------------- settings

fn autostart_file() -> PathBuf {
    platforms::config_dir().join("autostart/vanta.desktop")
}

fn apply_autostart(enabled: bool) -> AppResult<()> {
    let file = autostart_file();
    if enabled {
        let exe = std::env::current_exe()?;
        let exe = exe.to_string_lossy();
        if exe.contains('"') || exe.contains('\n') {
            return Err(AppError::Invalid(
                "cannot register autostart for this executable path".into(),
            ));
        }
        if let Some(p) = file.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(
            &file,
            format!(
                "[Desktop Entry]\nType=Application\nName=VANTA\nComment=Unified game launcher\nExec=\"{exe}\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
            ),
        )?;
    } else if file.is_file() {
        // Only ever remove the file VANTA itself created.
        let content = std::fs::read_to_string(&file).unwrap_or_default();
        if content.contains("Name=VANTA") {
            std::fs::remove_file(&file)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    state.db()?.load_settings()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> AppResult<Settings> {
    let db = state.db()?;
    let previous = db.load_settings()?;
    let saved = db.save_settings(&settings)?;
    if previous.launch_on_startup != saved.launch_on_startup
        || (saved.launch_on_startup && !autostart_file().exists())
    {
        if let Err(e) = apply_autostart(saved.launch_on_startup) {
            let mut rollback = saved.clone();
            rollback.launch_on_startup = previous.launch_on_startup;
            db.save_settings(&rollback)?;
            return Err(AppError::Other(format!(
                "could not update autostart entry: {e}"
            )));
        }
    }
    Ok(saved)
}

// ------------------------------------------------------------------------ data

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataInfo {
    pub data_dir: String,
    pub database: String,
    pub database_bytes: u64,
    pub artwork_dir: String,
    pub artwork_bytes: u64,
    pub logs_dir: String,
    pub backups_dir: String,
    pub schema_version: i64,
}

#[tauri::command]
pub fn data_info(state: State<'_, AppState>) -> AppResult<DataInfo> {
    let p = &state.paths;
    Ok(DataInfo {
        data_dir: p.data.to_string_lossy().into(),
        database: p.db.to_string_lossy().into(),
        database_bytes: std::fs::metadata(&p.db).map(|m| m.len()).unwrap_or(0),
        artwork_dir: p.artwork.to_string_lossy().into(),
        artwork_bytes: state.artwork.cache_size(),
        logs_dir: p.logs.to_string_lossy().into(),
        backups_dir: p.backups.to_string_lossy().into(),
        schema_version: state.db()?.schema_version()?,
    })
}

fn check_dest(dest: &str) -> AppResult<PathBuf> {
    let p = PathBuf::from(dest.trim());
    if !p.is_absolute() {
        return Err(AppError::Invalid(
            "destination must be an absolute path".into(),
        ));
    }
    if p.is_dir() {
        return Err(AppError::Invalid("destination is a directory".into()));
    }
    Ok(p)
}

#[tauri::command]
pub fn backup_database(state: State<'_, AppState>, dest: String) -> AppResult<String> {
    let dest = check_dest(&dest)?;
    if dest == state.paths.db {
        return Err(AppError::Invalid(
            "cannot back up the database onto itself".into(),
        ));
    }
    state.db()?.backup_to(&dest)?;
    tracing::info!(dest = %dest.display(), "database backup written");
    Ok(dest.to_string_lossy().into())
}

#[tauri::command]
pub fn restore_database(
    app: AppHandle,
    state: State<'_, AppState>,
    src: String,
) -> AppResult<String> {
    if !state.active.lock().map(|a| a.is_empty()).unwrap_or(false) {
        return Err(AppError::Invalid(
            "close running games before restoring a backup".into(),
        ));
    }
    let src = PathBuf::from(src.trim());
    if !src.is_file() {
        return Err(AppError::Invalid("backup file not found".into()));
    }
    std::fs::create_dir_all(&state.paths.backups)?;
    let safety = state
        .paths
        .backups
        .join(format!("pre-restore-{}.db", now_ts()));
    state.db()?.restore_from(&src, &safety)?;
    tracing::info!(src = %src.display(), safety = %safety.display(), "database restored");
    let _ = app.emit("library-updated", ());
    Ok(safety.to_string_lossy().into())
}

#[tauri::command]
pub fn export_config(state: State<'_, AppState>, dest: String) -> AppResult<String> {
    let dest = check_dest(&dest)?;
    let json = serde_json::to_vec_pretty(&state.db()?.export_config()?)?;
    let tmp = dest.with_extension("tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(dest.to_string_lossy().into())
}

#[tauri::command]
pub fn clear_artwork_cache(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    state.artwork.clear(&*state.db()?)?;
    let _ = app.emit("library-updated", ());
    Ok(())
}

#[tauri::command]
pub async fn refresh_artwork(app: AppHandle, state: State<'_, AppState>) -> AppResult<usize> {
    crate::state::refresh_artwork(app, state.inner().clone_handles()).await
}

/// Open a directory in the user's file manager. Only directories are allowed,
/// so this can never be used to execute a file.
#[tauri::command]
pub fn open_folder(path: String) -> AppResult<()> {
    let p = PathBuf::from(path.trim());
    if !p.is_absolute() || !p.is_dir() {
        return Err(AppError::Invalid(format!(
            "folder not found: {}",
            p.display()
        )));
    }
    let opener =
        platforms::which("xdg-open").ok_or_else(|| AppError::Other("xdg-open not found".into()))?;
    std::process::Command::new(opener)
        .arg(&p)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}
