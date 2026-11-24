export type PlatformId = "steam" | "epic" | "gog" | "itch" | "local";

export interface Game {
  id: string;
  platform: PlatformId;
  externalId: string;
  name: string;
  installed: boolean;
  installDir: string | null;
  executable: string | null;
  cover: string | null;
  hero: string | null;
  logo: string | null;
  platformPlaytimeMin: number;
  trackedSeconds: number;
  sessionCount: number;
  lastPlayed: number | null;
  favorite: boolean;
  description: string | null;
  tags: string[];
  source: "auto" | "manual";
  sizeBytes: number | null;
  addedAt: number;
  platformData: Record<string, unknown> | null;
}

export type SortKey = "name" | "recent" | "playtime" | "added";
export type InstallFilter = "all" | "installed" | "not_installed";

export interface GameQuery {
  search?: string | null;
  platforms?: string[];
  installStatus?: InstallFilter | null;
  favoritesOnly?: boolean;
  playedOnly?: boolean;
  tag?: string | null;
  sort?: SortKey | null;
  limit?: number | null;
}

export interface GameUpdate {
  name?: string | null;
  description?: string | null;
  favorite?: boolean | null;
  executable?: string | null;
  installDir?: string | null;
}

export interface ManualGameInput {
  name: string;
  executable: string;
  installDir?: string | null;
  launchArgs?: string | null;
  runner?: "native" | "wine" | "proton" | null;
  platform?: PlatformId | null;
  cover?: string | null;
}

export interface EnvVar {
  key: string;
  value: string;
}

export interface GameProfile {
  gameId: string;
  launchArgs: string;
  env: EnvVar[];
  workingDir: string | null;
  executableOverride: string | null;
  runner: "default" | "native" | "wine" | "proton";
  winePrefix: string | null;
  wineBinary: string | null;
  protonPath: string | null;
  wrapper: string;
}

export interface Session {
  id: number;
  gameId: string;
  gameName: string | null;
  startedAt: number;
  endedAt: number | null;
  durationS: number;
  exitCode: number | null;
  status: "running" | "completed" | "failed" | "interrupted";
  error: string | null;
}

export interface ActiveSession {
  gameId: string;
  gameName: string;
  sessionId: number | null;
  pid: number | null;
  startedAt: number;
  state: string;
}

export interface SessionEvent {
  gameId: string;
  state: "starting" | "running" | "ended" | "failed" | "untracked";
  message: string | null;
  session: Session | null;
}

export interface PlatformStatus {
  id: PlatformId;
  name: string;
  enabled: boolean;
  detected: boolean;
  connected: boolean;
  account: string | null;
  gameCount: number;
  installedCount: number;
  lastSync: number | null;
  lastError: string | null;
  details: string[];
  help: string | null;
}

export interface ScanReport {
  platform: string;
  found: number;
  added: number;
  updated: number;
  markedUninstalled: number;
  error: string | null;
}

export interface Settings {
  startMinimized: boolean;
  launchOnStartup: boolean;
  confirmBeforeClose: boolean;
  defaultLayout: "grid" | "list";
  theme: "midnight" | "graphite";
  accent: string;
  density: "comfortable" | "compact";
  gridSize: "small" | "medium" | "large";
  animations: "full" | "reduced" | "off";
  sidebar: "expanded" | "collapsed";
  autoScan: boolean;
  downloadArtwork: boolean;
  defaultSort: SortKey;
  monitorIntervalMs: number;
  sessionPollSecs: number;
}

export interface Bootstrap {
  version: string;
  settings: Settings;
  userName: string;
  notice: string | null;
}

export interface DayPlaytime {
  date: string;
  seconds: number;
}

export interface GamePlaytime {
  gameId: string;
  name: string;
  platform: PlatformId;
  cover: string | null;
  trackedSeconds: number;
  platformPlaytimeMin: number;
  sessions: number;
  lastPlayed: number | null;
}

export interface Statistics {
  totalTrackedSeconds: number;
  totalPlatformMinutes: number;
  totalSessions: number;
  failedLaunches: number;
  gamesPlayed: number;
  librarySize: number;
  installedCount: number;
  perGame: GamePlaytime[];
  longestSessions: Session[];
  history: Session[];
  last7Days: DayPlaytime[];
}

export interface GpuMetrics {
  name: string;
  source: string;
  utilization: number | null;
  temperature: number | null;
  memoryUsed: number | null;
  memoryTotal: number | null;
  powerWatts: number | null;
}

export interface ProcessMetrics {
  gameId: string;
  gameName: string;
  pid: number;
  processName: string;
  processCount: number;
  cpuPercent: number;
  memoryBytes: number;
}

export interface SystemMetrics {
  cpuName: string;
  cpuCount: number;
  cpuPercent: number;
  perCore: number[];
  memoryTotal: number;
  memoryUsed: number;
  memoryAvailable: number;
  swapTotal: number;
  swapUsed: number;
  uptimeSecs: number;
  loadAverage: [number, number, number];
  gpus: GpuMetrics[];
  gpuNote: string | null;
  games: ProcessMetrics[];
}

export interface DesktopGame {
  name: string;
  executable: string;
  args: string[];
  desktopFile: string;
  icon: string | null;
}

export interface Runner {
  kind: "wine" | "proton";
  name: string;
  path: string;
}

export interface SteamDownload {
  appId: string;
  name: string;
  state: string;
  bytesTotal: number;
  bytesDone: number;
}

export interface RemovedGame {
  id: string;
  name: string;
  platform: PlatformId;
}

export interface DataInfo {
  dataDir: string;
  database: string;
  databaseBytes: number;
  artworkDir: string;
  artworkBytes: number;
  logsDir: string;
  backupsDir: string;
  schemaVersion: number;
}
