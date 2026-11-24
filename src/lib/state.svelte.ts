import { listen } from "@tauri-apps/api/event";
import { api, errMsg } from "./api";
import type { ActiveSession, PlatformStatus, SessionEvent, Settings } from "./types";

export type View =
  | "home"
  | "library"
  | "installed"
  | "favorites"
  | "recent"
  | "downloads"
  | "stats"
  | "performance"
  | "settings"
  | "game";

export const DEFAULT_SETTINGS: Settings = {
  startMinimized: false,
  launchOnStartup: false,
  confirmBeforeClose: false,
  defaultLayout: "grid",
  theme: "midnight",
  accent: "#8b5cf6",
  density: "comfortable",
  gridSize: "medium",
  animations: "full",
  sidebar: "expanded",
  autoScan: true,
  downloadArtwork: true,
  defaultSort: "recent",
  monitorIntervalMs: 2000,
  sessionPollSecs: 5,
};

export const app = $state({
  ready: false,
  bootError: null as string | null,
  version: "",
  userName: "",
  notice: null as string | null,
  settings: { ...DEFAULT_SETTINGS } as Settings,
  platforms: [] as PlatformStatus[],
  scanning: false,
  /** Bumped whenever the backend reports library changes; views re-query on change. */
  libraryVersion: 0,
  running: {} as Record<string, ActiveSession>,
  sidebarCollapsed: false,
  addGameOpen: false,
});

export const nav = $state({
  view: "home" as View,
  gameId: null as string | null,
  history: [] as { view: View; gameId: string | null }[],
});

export function navigate(view: View, gameId: string | null = null) {
  if (nav.view === view && nav.gameId === gameId) return;
  nav.history.push({ view: nav.view, gameId: nav.gameId });
  if (nav.history.length > 30) nav.history.shift();
  nav.view = view;
  nav.gameId = gameId;
}

export function back() {
  const prev = nav.history.pop();
  if (prev) {
    nav.view = prev.view;
    nav.gameId = prev.gameId;
  } else {
    nav.view = "library";
    nav.gameId = null;
  }
}

// ---------------------------------------------------------------- toasts

export interface Toast {
  id: number;
  kind: "info" | "success" | "error" | "warning";
  title: string;
  message?: string;
}
export const toasts = $state<Toast[]>([]);
let toastId = 0;
export function toast(kind: Toast["kind"], title: string, message?: string, ms = kind === "error" ? 9000 : 4500) {
  const id = ++toastId;
  toasts.push({ id, kind, title, message });
  setTimeout(() => dismissToast(id), ms);
}
export function dismissToast(id: number) {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}
export function toastError(title: string, e: unknown) {
  toast("error", title, errMsg(e));
}

// ----------------------------------------------------------- confirmations

export const confirmState = $state({
  open: false,
  title: "",
  message: "",
  confirmLabel: "Confirm",
  danger: false,
  resolve: null as null | ((v: boolean) => void),
});

export function confirmAction(opts: { title: string; message: string; confirmLabel?: string; danger?: boolean }): Promise<boolean> {
  return new Promise((resolve) => {
    confirmState.title = opts.title;
    confirmState.message = opts.message;
    confirmState.confirmLabel = opts.confirmLabel ?? "Confirm";
    confirmState.danger = opts.danger ?? false;
    confirmState.resolve = resolve;
    confirmState.open = true;
  });
}

// ------------------------------------------------------------- settings

export function applySettings(s: Settings) {
  const root = document.documentElement;
  root.style.setProperty("--accent", s.accent);
  root.dataset.theme = s.theme;
  root.dataset.density = s.density;
  root.dataset.grid = s.gridSize;
  root.dataset.motion = s.animations;
}

export async function saveSettings(patch: Partial<Settings>) {
  const previous = { ...app.settings };
  const next = { ...app.settings, ...patch };
  app.settings = next;
  applySettings(next);
  try {
    app.settings = await api.saveSettings(next);
    applySettings(app.settings);
  } catch (e) {
    app.settings = previous;
    applySettings(previous);
    toastError("Setting not saved", e);
  }
}

// ------------------------------------------------------------ platforms

export async function refreshPlatforms() {
  try {
    app.platforms = await api.platformStatuses();
  } catch (e) {
    toastError("Could not read platform status", e);
  }
}

export async function scanAll(platform?: string) {
  try {
    const reports = await api.scanLibraries(platform);
    const failed = reports.filter((r) => r.error);
    const added = reports.reduce((n, r) => n + r.added, 0);
    if (failed.length) {
      toast("warning", "Some platforms could not be scanned", failed.map((f) => `${f.platform}: ${f.error}`).join("\n"));
    } else {
      toast("success", "Library synced", added ? `${added} new game${added === 1 ? "" : "s"} added` : "Everything is up to date");
    }
  } catch (e) {
    toastError("Scan failed", e);
  } finally {
    refreshPlatforms();
  }
}

// --------------------------------------------------------------- launching

export async function launch(gameId: string, name: string) {
  if (app.running[gameId]) return;
  app.running[gameId] = { gameId, gameName: name, sessionId: null, pid: null, startedAt: Date.now() / 1000, state: "starting" };
  try {
    await api.launchGame(gameId);
  } catch (e) {
    delete app.running[gameId];
    toastError(`Couldn't launch ${name}`, e);
  }
}

export async function stop(gameId: string) {
  try {
    await api.stopGame(gameId);
  } catch (e) {
    toastError("Couldn't stop the game", e);
  }
}

// --------------------------------------------------------------- bootstrap

export async function boot() {
  try {
    const b = await api.bootstrap();
    app.version = b.version;
    app.userName = b.userName;
    app.notice = b.notice;
    app.settings = b.settings;
    app.sidebarCollapsed = b.settings.sidebar === "collapsed";
    applySettings(b.settings);
    app.scanning = await api.isScanning();
    for (const s of await api.activeSessions()) app.running[s.gameId] = s;
    app.ready = true;
    refreshPlatforms();
  } catch (e) {
    app.bootError = errMsg(e);
  }

  await listen("library-updated", () => {
    app.libraryVersion++;
  });
  await listen<boolean>("scan-state", (e) => {
    app.scanning = e.payload;
    if (!e.payload) refreshPlatforms();
  });
  await listen<SessionEvent>("session", (e) => {
    const ev = e.payload;
    const name = app.running[ev.gameId]?.gameName ?? ev.session?.gameName ?? "Game";
    switch (ev.state) {
      case "starting":
        break;
      case "running":
        app.running[ev.gameId] = {
          gameId: ev.gameId,
          gameName: name,
          sessionId: ev.session?.id ?? null,
          pid: null,
          startedAt: ev.session?.startedAt ?? Date.now() / 1000,
          state: "running",
        };
        api.activeSessions().then((list) => {
          for (const s of list) app.running[s.gameId] = s;
        });
        break;
      case "ended":
        delete app.running[ev.gameId];
        if (ev.session && ev.session.durationS >= 60) {
          toast("info", `Session ended · ${name}`, `Played for ${Math.round(ev.session.durationS / 60)} min`);
        }
        break;
      case "failed":
        delete app.running[ev.gameId];
        toast("error", `${name} failed to launch`, ev.message ?? undefined);
        break;
      case "untracked":
        delete app.running[ev.gameId];
        toast("warning", "Session not tracked", ev.message ?? undefined, 10000);
        break;
    }
    app.libraryVersion++;
  });
}

// ------------------------------------------------------- library filters
/** Library filter state survives navigation within a session. */
export const libUi = $state({
  search: "",
  platforms: [] as string[],
  status: "all" as "all" | "installed" | "not_installed",
  tag: null as string | null,
  sort: null as null | "name" | "recent" | "playtime" | "added",
  layout: null as null | "grid" | "list",
});
