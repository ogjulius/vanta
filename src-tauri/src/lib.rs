pub mod artwork;
pub mod commands;
pub mod db;
pub mod error;
pub mod launcher;
pub mod models;
pub mod monitor;
pub mod platforms;
pub mod settings;
pub mod state;
pub mod vdf;

use std::path::Path;

use tauri::Manager;

use crate::db::Db;
use crate::state::{AppState, Paths};

fn init_logging(logs: &Path) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let _ = std::fs::create_dir_all(logs);
    let file_path = logs.join("vanta.log");
    // Keep the log bounded: start fresh when it grows past 5 MB.
    if std::fs::metadata(&file_path)
        .map(|m| m.len() > 5 * 1024 * 1024)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&file_path, logs.join("vanta.log.old"));
    }
    let filter = EnvFilter::try_from_env("VANTA_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,vanta_lib=info"));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .ok();
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false));
    match file {
        Some(f) => {
            let _ = registry
                .with(
                    fmt::layer()
                        .with_ansi(false)
                        .with_writer(std::sync::Mutex::new(f)),
                )
                .try_init();
        }
        None => {
            let _ = registry.try_init();
        }
    }
}

/// Open the database; if it is unreadable, keep the damaged file aside and
/// start fresh so the app still opens (the user can restore a backup).
fn open_database(paths: &Paths) -> Result<(Db, Option<String>), String> {
    match Db::open(&paths.db) {
        Ok(db) => Ok((db, None)),
        Err(e) => {
            tracing::error!(error = %e, "failed to open database");
            if matches!(e, crate::error::AppError::Invalid(_)) {
                // Newer schema: never touch it.
                return Err(e.to_string());
            }
            let aside = paths
                .db
                .with_extension(format!("db.broken-{}", models::now_ts()));
            std::fs::rename(&paths.db, &aside)
                .map_err(|re| format!("{e}; also failed to move it aside: {re}"))?;
            for ext in ["db-wal", "db-shm"] {
                let _ = std::fs::rename(paths.db.with_extension(ext), aside.with_extension(ext));
            }
            let db = Db::open(&paths.db).map_err(|e| e.to_string())?;
            Ok((db, Some(format!(
                "Your library database could not be opened ({e}). It was preserved at {} and a fresh database was created. You can restore a backup from Settings → Data.",
                aside.display()
            ))))
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data = app.path().app_local_data_dir()?;
            let paths = Paths::new(data);
            init_logging(&paths.logs);
            tracing::info!(data = %paths.data.display(), "starting VANTA {}", app.package_info().version);
            let (db, notice) = open_database(&paths).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            let settings = db.load_settings().unwrap_or_default();
            app.manage(AppState::new(paths, db, notice));

            if settings.start_minimized {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.minimize();
                }
            }
            if settings.auto_scan {
                let handle = app.handle().clone();
                let h = app.state::<AppState>().clone_handles();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = state::run_scan(handle, h, None).await {
                        tracing::warn!(error = %e, "startup scan failed");
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::list_games,
            commands::get_game,
            commands::update_game,
            commands::remove_game,
            commands::list_removed_games,
            commands::restore_game,
            commands::add_manual_game,
            commands::set_tags,
            commands::list_tags,
            commands::set_custom_cover,
            commands::game_sessions,
            commands::get_profile,
            commands::save_profile,
            commands::reset_profile,
            commands::preview_launch,
            commands::launch_game,
            commands::stop_game,
            commands::active_sessions,
            commands::platform_statuses,
            commands::set_platform_enabled,
            commands::scan_libraries,
            commands::is_scanning,
            commands::desktop_entries,
            commands::detect_runners,
            commands::steam_downloads,
            commands::system_metrics,
            commands::get_statistics,
            commands::get_settings,
            commands::save_settings,
            commands::data_info,
            commands::backup_database,
            commands::restore_database,
            commands::export_config,
            commands::clear_artwork_cache,
            commands::refresh_artwork,
            commands::open_folder,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("VANTA failed to start: {e}");
            std::process::exit(1);
        });
}
