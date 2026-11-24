import type { Game, PlatformId } from "./types";

export const PLATFORM_LABEL: Record<PlatformId, string> = {
  steam: "Steam",
  epic: "Epic Games",
  gog: "GOG",
  itch: "itch.io",
  local: "Local",
};

/** Subtle, desaturated platform hues — identity without rainbow noise. */
export const PLATFORM_HUE: Record<PlatformId, string> = {
  steam: "#66c0f4",
  epic: "#e5e5e5",
  gog: "#b583ff",
  itch: "#fa5c5c",
  local: "#7ee0b8",
};

export function effectivePlaytime(g: Pick<Game, "platformPlaytimeMin" | "trackedSeconds">): number {
  return Math.max(g.platformPlaytimeMin * 60, g.trackedSeconds);
}

export function formatDuration(seconds: number, opts: { short?: boolean } = {}): string {
  if (!seconds || seconds < 60) return seconds > 0 ? "< 1 min" : opts.short ? "0h" : "Never played";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h === 0) return `${m} min`;
  if (h < 10 && m > 0 && !opts.short) return `${h}h ${m}m`;
  return `${h.toLocaleString()}h`;
}

/** Compact total for KPI tiles: "0h", "42m", "3.5h", "128h". */
export function formatTotal(seconds: number): string {
  if (seconds < 60) return "0h";
  if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
  const h = seconds / 3600;
  return `${h < 10 ? h.toFixed(1) : Math.round(h).toLocaleString()}h`;
}

export function relativeTime(ts: number | null | undefined): string {
  if (!ts) return "Never";
  const diff = Date.now() / 1000 - ts;
  if (diff < 60) return "Just now";
  if (diff < 3600) return `${Math.floor(diff / 60)} min ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  const days = Math.floor(diff / 86400);
  if (days === 1) return "Yesterday";
  if (days < 7) return `${days} days ago`;
  return new Date(ts * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric", year: days > 300 ? "numeric" : undefined });
}

export function formatDateTime(ts: number | null | undefined): string {
  if (!ts) return "—";
  return new Date(ts * 1000).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

export function formatBytes(bytes: number | null | undefined, digits = 1): string {
  if (bytes == null || !isFinite(bytes)) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 ? 0 : digits)} ${units[i]}`;
}

export function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return d > 0 ? `${d}d ${h}h ${m}m` : h > 0 ? `${h}h ${m}m` : `${m}m`;
}

/** Deterministic hue from a string, for generated fallback artwork. */
export function hashHue(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return Math.abs(h) % 360;
}

export function greeting(): string {
  const h = new Date().getHours();
  if (h < 5) return "Late night";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

export function debounce<A extends unknown[]>(fn: (...a: A) => void, ms: number): (...a: A) => void {
  let t: ReturnType<typeof setTimeout> | undefined;
  return (...a: A) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...a), ms);
  };
}
