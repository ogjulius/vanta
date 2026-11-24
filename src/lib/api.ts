import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import type * as T from "./types";

/** Extract a readable message from any thrown value (including Rust AppError). */
export function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}

function call<R>(cmd: string, args?: Record<string, unknown>): Promise<R> {
  return invoke<R>(cmd, args);
}

const srcCache = new Map<string, string>();
/** Convert a local artwork path into a webview-loadable URL. */
export function assetUrl(path: string | null | undefined): string | null {
  if (!path) return null;
  let url = srcCache.get(path);
  if (!url) {
    url = convertFileSrc(path);
    srcCache.set(path, url);
  }
  return url;
}

export const api = {
  bootstrap: () => call<T.Bootstrap>("bootstrap"),
  listGames: (query: T.GameQuery) => call<T.Game[]>("list_games", { query }),
  getGame: (id: string) => call<T.Game>("get_game", { id }),
  updateGame: (id: string, update: T.GameUpdate) => call<T.Game>("update_game", { id, update }),
  removeGame: (id: string) => call<void>("remove_game", { id }),
  listRemovedGames: () => call<T.RemovedGame[]>("list_removed_games"),
  restoreGame: (id: string) => call<void>("restore_game", { id }),
  addManualGame: (input: T.ManualGameInput) => call<T.Game>("add_manual_game", { input }),
  setTags: (id: string, tags: string[]) => call<string[]>("set_tags", { id, tags }),
  listTags: () => call<string[]>("list_tags"),
  setCustomCover: (id: string, path: string) => call<T.Game>("set_custom_cover", { id, path }),
  gameSessions: (id: string) => call<T.Session[]>("game_sessions", { id }),
  getProfile: (id: string) => call<T.GameProfile>("get_profile", { id }),
  saveProfile: (profile: T.GameProfile) => call<T.GameProfile>("save_profile", { profile }),
  resetProfile: (id: string) => call<T.GameProfile>("reset_profile", { id }),
  previewLaunch: (id: string) => call<string[]>("preview_launch", { id }),
  launchGame: (id: string) => call<void>("launch_game", { id }),
  stopGame: (id: string) => call<void>("stop_game", { id }),
  activeSessions: () => call<T.ActiveSession[]>("active_sessions"),
  platformStatuses: () => call<T.PlatformStatus[]>("platform_statuses"),
  setPlatformEnabled: (id: string, enabled: boolean) => call<void>("set_platform_enabled", { id, enabled }),
  scanLibraries: (platform?: string | null) => call<T.ScanReport[]>("scan_libraries", { platform: platform ?? null }),
  isScanning: () => call<boolean>("is_scanning"),
  desktopEntries: () => call<T.DesktopGame[]>("desktop_entries"),
  detectRunners: () => call<T.Runner[]>("detect_runners"),
  steamDownloads: () => call<T.SteamDownload[]>("steam_downloads"),
  systemMetrics: () => call<T.SystemMetrics>("system_metrics"),
  getStatistics: () => call<T.Statistics>("get_statistics"),
  getSettings: () => call<T.Settings>("get_settings"),
  saveSettings: (settings: T.Settings) => call<T.Settings>("save_settings", { settings }),
  dataInfo: () => call<T.DataInfo>("data_info"),
  backupDatabase: (dest: string) => call<string>("backup_database", { dest }),
  restoreDatabase: (src: string) => call<string>("restore_database", { src }),
  exportConfig: (dest: string) => call<string>("export_config", { dest }),
  clearArtworkCache: () => call<void>("clear_artwork_cache"),
  refreshArtwork: () => call<number>("refresh_artwork"),
  openFolder: (path: string) => call<void>("open_folder", { path }),
};
