<script lang="ts">
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import {
    SlidersHorizontal, Palette, Library, Plug, Activity, Database, RefreshCw, FolderOpen, Download, Upload, FileJson,
    Trash2, ChevronDown, LoaderCircle, Undo2, ImageDown,
  } from "@lucide/svelte";
  import Toggle from "../components/Toggle.svelte";
  import Segmented from "../components/Segmented.svelte";
  import { api, errMsg } from "../api";
  import { app, confirmAction, refreshPlatforms, saveSettings, scanAll, toast, toastError } from "../state.svelte";
  import { formatBytes, relativeTime, PLATFORM_HUE } from "../format";
  import type { DataInfo, RemovedGame } from "../types";

  type Section = "general" | "appearance" | "library" | "integrations" | "performance" | "data";
  let section = $state<Section>("general");
  let data = $state<DataInfo | null>(null);
  let removed = $state<RemovedGame[]>([]);
  let expanded = $state<string | null>(null);
  let busy = $state<string | null>(null);

  const sections: { id: Section; label: string; icon: typeof Palette }[] = [
    { id: "general", label: "General", icon: SlidersHorizontal },
    { id: "appearance", label: "Appearance", icon: Palette },
    { id: "library", label: "Library", icon: Library },
    { id: "integrations", label: "Integrations", icon: Plug },
    { id: "performance", label: "Performance", icon: Activity },
    { id: "data", label: "Data", icon: Database },
  ];

  const accents = [
    { name: "Violet", value: "#8b5cf6" },
    { name: "Electric", value: "#a855f7" },
    { name: "Indigo", value: "#6366f1" },
    { name: "Azure", value: "#3b82f6" },
    { name: "Cyan", value: "#06b6d4" },
    { name: "Emerald", value: "#10b981" },
    { name: "Rose", value: "#f43f5e" },
    { name: "Amber", value: "#f59e0b" },
  ];

  const s = $derived(app.settings);

  function loadData() {
    api.dataInfo().then((d) => (data = d)).catch((e) => toastError("Couldn't read data info", e));
  }
  $effect(() => {
    void app.libraryVersion;
    if (section === "data" || section === "performance") loadData();
    if (section === "library") api.listRemovedGames().then((r) => (removed = r)).catch(() => {});
    if (section === "integrations") refreshPlatforms();
  });

  async function togglePlatform(id: string, enabled: boolean) {
    try {
      await api.setPlatformEnabled(id, enabled);
      await refreshPlatforms();
      if (enabled) scanAll(id);
    } catch (e) {
      toastError("Couldn't update integration", e);
    }
  }

  function stamp() {
    const d = new Date();
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}`;
  }

  async function backup() {
    const dest = await saveDialog({ defaultPath: `${data?.backupsDir ?? ""}/vanta-backup-${stamp()}.db`, filters: [{ name: "VANTA backup", extensions: ["db"] }] });
    if (!dest) return;
    busy = "backup";
    try {
      const p = await api.backupDatabase(dest);
      toast("success", "Backup created", p);
    } catch (e) {
      toastError("Backup failed", e);
    } finally {
      busy = null;
    }
  }

  async function restore() {
    const src = await openDialog({ multiple: false, defaultPath: data?.backupsDir, filters: [{ name: "VANTA backup", extensions: ["db"] }] });
    if (typeof src !== "string") return;
    const ok = await confirmAction({
      title: "Restore this backup?",
      message: "Your current library, profiles, history and settings will be replaced by the backup. A safety copy of the current database is saved first.",
      confirmLabel: "Restore",
      danger: true,
    });
    if (!ok) return;
    busy = "restore";
    try {
      const safety = await api.restoreDatabase(src);
      app.settings = await api.getSettings();
      toast("success", "Backup restored", `Previous data saved to ${safety}`);
      loadData();
    } catch (e) {
      toastError("Restore failed", e);
    } finally {
      busy = null;
    }
  }

  async function exportCfg() {
    const dest = await saveDialog({ defaultPath: `vanta-config-${stamp()}.json`, filters: [{ name: "JSON", extensions: ["json"] }] });
    if (!dest) return;
    try {
      toast("success", "Configuration exported", await api.exportConfig(dest));
    } catch (e) {
      toastError("Export failed", e);
    }
  }

  async function clearCache() {
    const ok = await confirmAction({ title: "Clear artwork cache?", message: "Cached artwork is deleted and fetched again on the next sync. Custom covers you chose are kept.", confirmLabel: "Clear cache" });
    if (!ok) return;
    try {
      await api.clearArtworkCache();
      toast("success", "Artwork cache cleared");
      loadData();
    } catch (e) {
      toastError("Couldn't clear cache", e);
    }
  }

  async function refreshArt() {
    busy = "art";
    try {
      const n = await api.refreshArtwork();
      toast("success", "Artwork refreshed", n ? `Updated artwork for ${n} game${n === 1 ? "" : "s"}` : "No new artwork found");
      loadData();
    } catch (e) {
      toastError("Artwork refresh failed", e);
    } finally {
      busy = null;
    }
  }

  async function restoreGame(g: RemovedGame) {
    try {
      await api.restoreGame(g.id);
      removed = removed.filter((r) => r.id !== g.id);
      toast("success", "Restored to library", g.name);
    } catch (e) {
      toastError("Couldn't restore", e);
    }
  }

  function open(path: string | undefined) {
    if (path) api.openFolder(path).catch((e) => toast("error", "Couldn't open folder", errMsg(e)));
  }
</script>

{#snippet row(title: string, desc: string, control: import("svelte").Snippet)}
  <div class="flex items-center justify-between gap-8 py-4">
    <div class="min-w-0">
      <div class="text-[13.5px] font-medium">{title}</div>
      <div class="mt-0.5 text-[12px] leading-relaxed text-faint">{desc}</div>
    </div>
    <div class="shrink-0">{@render control()}</div>
  </div>
{/snippet}

<div class="mx-auto flex max-w-[1180px] gap-10 px-[var(--pad)] pb-16 pt-9">
  <nav class="sticky top-9 h-fit w-[200px] shrink-0">
    <h1 class="font-display mb-6 text-[30px] font-semibold leading-none">Settings</h1>
    <div class="flex flex-col gap-0.5">
      {#each sections as sec (sec.id)}
        <button
          class="flex h-9 items-center gap-2.5 rounded-lg px-3 text-[13px] font-medium t transition-colors {section === sec.id ? 'bg-white/[0.06] text-text' : 'text-dim hover:bg-white/[0.03] hover:text-text'}"
          onclick={() => (section = sec.id)}
        >
          <sec.icon size={15} class={section === sec.id ? "text-accent-ink" : ""} />{sec.label}
        </button>
      {/each}
    </div>
  </nav>

  <div class="min-w-0 flex-1 pt-[54px]">
    {#key section}
      <div class="fade-in">
        {#if section === "general"}
          <h2 class="font-display mb-2 text-lg font-semibold">General</h2>
          <div class="divide-y divide-line">
            {#snippet t1()}<Toggle checked={s.startMinimized} onchange={(v) => saveSettings({ startMinimized: v })} label="Start minimized" />{/snippet}
            {@render row("Start minimized", "Open VANTA minimized to the taskbar.", t1)}
            {#snippet t2()}<Toggle checked={s.launchOnStartup} onchange={(v) => saveSettings({ launchOnStartup: v })} label="Launch on startup" />{/snippet}
            {@render row("Launch on startup", "Adds an entry to ~/.config/autostart so VANTA starts when you log in.", t2)}
            {#snippet t3()}<Toggle checked={s.confirmBeforeClose} onchange={(v) => saveSettings({ confirmBeforeClose: v })} label="Confirm before closing" />{/snippet}
            {@render row("Confirm before closing", "Ask before the window closes. Running games keep running either way.", t3)}
            {#snippet t4()}<Segmented value={s.defaultLayout} options={[{ value: "grid", label: "Grid" }, { value: "list", label: "List" }]} onchange={(v) => saveSettings({ defaultLayout: v })} />{/snippet}
            {@render row("Default library layout", "How your library is shown when you open VANTA.", t4)}
          </div>
          <p class="mt-6 text-[12px] text-faint">Interface language: English. Additional languages are not available in this release.</p>
        {:else if section === "appearance"}
          <h2 class="font-display mb-2 text-lg font-semibold">Appearance</h2>
          <div class="divide-y divide-line">
            {#snippet a1()}<Segmented value={s.theme} options={[{ value: "midnight", label: "Midnight" }, { value: "graphite", label: "Graphite" }]} onchange={(v) => saveSettings({ theme: v })} />{/snippet}
            {@render row("Dark theme", "Midnight is pure black for OLED; Graphite is a softer charcoal.", a1)}
            {#snippet a2()}
              <div class="flex items-center gap-2">
                {#each accents as a (a.value)}
                  <button
                    class="h-6 w-6 rounded-full t transition-transform hover:scale-110 {s.accent === a.value ? 'ring-2 ring-white/80 ring-offset-2 ring-offset-bg' : ''}"
                    style="background:{a.value}"
                    title={a.name}
                    aria-label="{a.name} accent"
                    onclick={() => saveSettings({ accent: a.value })}
                  ></button>
                {/each}
                <input type="color" class="h-6 w-6 cursor-pointer rounded-full border-0 bg-transparent p-0" value={s.accent} onchange={(e) => saveSettings({ accent: e.currentTarget.value })} title="Custom color" aria-label="Custom accent color" />
              </div>
            {/snippet}
            {@render row("Accent color", "Used for highlights, buttons and charts.", a2)}
            {#snippet a3()}<Segmented value={s.density} options={[{ value: "comfortable", label: "Comfortable" }, { value: "compact", label: "Compact" }]} onchange={(v) => saveSettings({ density: v })} />{/snippet}
            {@render row("Interface density", "Spacing between elements.", a3)}
            {#snippet a4()}<Segmented value={s.gridSize} options={[{ value: "small", label: "Small" }, { value: "medium", label: "Medium" }, { value: "large", label: "Large" }]} onchange={(v) => saveSettings({ gridSize: v })} />{/snippet}
            {@render row("Grid size", "Size of game covers in grid views.", a4)}
            {#snippet a5()}<Segmented value={s.animations} options={[{ value: "full", label: "Full" }, { value: "reduced", label: "Reduced" }, { value: "off", label: "Off" }]} onchange={(v) => saveSettings({ animations: v })} />{/snippet}
            {@render row("Animations", "Motion intensity for transitions and hover effects.", a5)}
            {#snippet a6()}<Segmented value={s.sidebar} options={[{ value: "expanded", label: "Expanded" }, { value: "collapsed", label: "Collapsed" }]} onchange={(v) => { app.sidebarCollapsed = v === "collapsed"; saveSettings({ sidebar: v }); }} />{/snippet}
            {@render row("Sidebar", "Default sidebar state. You can also toggle it from the sidebar itself.", a6)}
          </div>
        {:else if section === "library"}
          <h2 class="font-display mb-2 text-lg font-semibold">Library</h2>
          <div class="divide-y divide-line">
            {#snippet l1()}<Toggle checked={s.autoScan} onchange={(v) => saveSettings({ autoScan: v })} label="Automatic scanning" />{/snippet}
            {@render row("Automatic scanning", "Sync all enabled platforms each time VANTA starts.", l1)}
            {#snippet l2()}<Toggle checked={s.downloadArtwork} onchange={(v) => saveSettings({ downloadArtwork: v })} label="Download artwork" />{/snippet}
            {@render row("Download artwork", "Fetch missing covers from platform CDNs. When off, only artwork already on disk (e.g. Steam's cache) is used.", l2)}
            {#snippet l3()}
              <select class="field h-9 w-44 py-0 text-xs" value={s.defaultSort} onchange={(e) => saveSettings({ defaultSort: e.currentTarget.value as typeof s.defaultSort })}>
                <option value="name">Name A–Z</option><option value="recent">Recently played</option><option value="playtime">Most played</option><option value="added">Recently added</option>
              </select>
            {/snippet}
            {@render row("Default sorting", "Initial sort order in library views.", l3)}
            {#snippet l4()}<button class="btn h-9 text-xs" onclick={refreshArt} disabled={busy === "art"}>{#if busy === "art"}<LoaderCircle size={13} class="spin" />{:else}<ImageDown size={13} />{/if} Refresh now</button>{/snippet}
            {@render row("Metadata refresh", "Look for missing artwork for every game now. Images are cached and never downloaded twice.", l4)}
            {#snippet l5()}<button class="btn h-9 text-xs" onclick={() => scanAll()} disabled={app.scanning}><RefreshCw size={13} class={app.scanning ? "spin" : ""} /> Scan now</button>{/snippet}
            {@render row("Rescan libraries", "Discover newly installed or removed games on all enabled platforms.", l5)}
          </div>
          <h3 class="eyebrow mb-3 mt-10">Removed games</h3>
          {#if removed.length === 0}
            <p class="text-[12.5px] text-faint">Games you remove from VANTA appear here so you can bring them back.</p>
          {:else}
            <div class="divide-y divide-line rounded-xl border border-line bg-surface-1">
              {#each removed as g (g.id)}
                <div class="flex items-center gap-3 px-4 py-2.5 text-[13px]">
                  <span class="h-1.5 w-1.5 rounded-full" style="background:{PLATFORM_HUE[g.platform]}"></span>
                  <span class="flex-1 truncate">{g.name}</span>
                  <button class="btn btn-ghost h-8 text-xs" onclick={() => restoreGame(g)}><Undo2 size={13} /> Restore</button>
                </div>
              {/each}
            </div>
          {/if}
        {:else if section === "integrations"}
          <h2 class="font-display mb-1 text-lg font-semibold">Integrations</h2>
          <p class="mb-6 text-[12.5px] text-faint">VANTA reads local client data only. It never asks for, stores, or logs platform passwords or tokens — sign-in always happens in each platform's own tooling.</p>
          <div class="space-y-3">
            {#each app.platforms as p (p.id)}
              {@const open = expanded === p.id}
              <div class="overflow-hidden rounded-xl border border-line bg-surface-1">
                <div class="flex items-center gap-4 px-5 py-4">
                  <span class="h-2.5 w-2.5 shrink-0 rounded-full" style="background:{PLATFORM_HUE[p.id]}"></span>
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-2.5">
                      <span class="text-[14px] font-medium">{p.name}</span>
                      {#if !p.enabled}
                        <span class="rounded-full bg-white/5 px-2 py-0.5 text-[10.5px] text-faint">Disabled</span>
                      {:else if !p.detected}
                        <span class="rounded-full bg-white/5 px-2 py-0.5 text-[10.5px] text-faint">Not installed</span>
                      {:else if p.connected}
                        <span class="rounded-full bg-emerald-400/10 px-2 py-0.5 text-[10.5px] text-emerald-300">Connected{p.account ? ` · ${p.account}` : ""}</span>
                      {:else}
                        <span class="rounded-full bg-amber-300/10 px-2 py-0.5 text-[10.5px] text-amber-200">Detected · not signed in</span>
                      {/if}
                    </div>
                    <div class="mt-0.5 text-xs text-faint">
                      {#if p.lastError}<span class="text-red-300">{p.lastError}</span>
                      {:else if p.detected}{p.gameCount} games · {p.installedCount} installed{p.lastSync ? ` · synced ${relativeTime(p.lastSync).toLowerCase()}` : ""}
                      {:else}{p.help?.split(".")[0]}.{/if}
                    </div>
                  </div>
                  {#if p.enabled && p.detected && p.id !== "local"}
                    <button class="btn btn-ghost h-8 text-xs" disabled={app.scanning} onclick={() => scanAll(p.id)}><RefreshCw size={13} class={app.scanning ? "spin" : ""} /> Sync</button>
                  {/if}
                  <button class="btn btn-ghost btn-icon h-8 w-8" onclick={() => (expanded = open ? null : p.id)} aria-label="Diagnostics" title="Connection diagnostics">
                    <ChevronDown size={15} class="t transition-transform {open ? 'rotate-180' : ''}" />
                  </button>
                  <Toggle checked={p.enabled} onchange={(v) => togglePlatform(p.id, v)} label="Enable {p.name}" />
                </div>
                {#if open}
                  <div class="fade-in border-t border-line bg-black/20 px-5 py-4 text-[12px]">
                    <div class="eyebrow mb-2">Diagnostics</div>
                    {#if p.details.length}
                      <pre class="mono mb-3 whitespace-pre-wrap break-all leading-relaxed text-dim">{p.details.join("\n")}</pre>
                    {:else}
                      <p class="mb-3 text-faint">Nothing detected on this system.</p>
                    {/if}
                    {#if p.help}<p class="leading-relaxed text-dim">{p.help}</p>{/if}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {:else if section === "performance"}
          <h2 class="font-display mb-2 text-lg font-semibold">Performance</h2>
          <div class="divide-y divide-line">
            {#snippet p1()}
              <select class="field h-9 w-36 py-0 text-xs" value={String(s.monitorIntervalMs)} onchange={(e) => saveSettings({ monitorIntervalMs: Number(e.currentTarget.value) })}>
                {#each [1000, 2000, 3000, 5000, 10000] as v (v)}<option value={String(v)}>Every {v / 1000}s</option>{/each}
              </select>
            {/snippet}
            {@render row("Monitoring interval", "How often the Performance page samples metrics. Nothing is sampled while the page is closed.", p1)}
            {#snippet p2()}
              <Segmented value={s.sessionPollSecs <= 3 ? "high" : s.sessionPollSecs >= 15 ? "low" : "balanced"} options={[{ value: "low", label: "Minimal" }, { value: "balanced", label: "Balanced" }, { value: "high", label: "Precise" }]} onchange={(v) => saveSettings({ sessionPollSecs: v === "low" ? 15 : v === "high" ? 2 : 5 })} />
            {/snippet}
            {@render row("Background resource usage", "How often VANTA checks whether a game launched through Steam, Legendary or Heroic is still running (15s / 5s / 2s). Applies to the next launch.", p2)}
            {#snippet p3()}<div class="flex items-center gap-3"><span class="text-xs tabular-nums text-dim">{data ? formatBytes(data.artworkBytes) : "…"}</span><button class="btn h-9 text-xs" onclick={clearCache}><Trash2 size={13} /> Clear</button></div>{/snippet}
            {@render row("Artwork cache", "Locally cached covers and backgrounds.", p3)}
          </div>
        {:else if section === "data"}
          <h2 class="font-display mb-2 text-lg font-semibold">Data</h2>
          {#if data}
            <div class="mb-8 divide-y divide-line rounded-xl border border-line bg-surface-1 text-[12.5px]">
              {#each [["Database", data.database, `${formatBytes(data.databaseBytes)} · schema v${data.schemaVersion}`], ["Data folder", data.dataDir, ""], ["Logs", data.logsDir, ""], ["Backups", data.backupsDir, ""]] as [k, v, meta] (k)}
                <div class="flex items-center gap-4 px-4 py-3">
                  <span class="w-24 shrink-0 text-faint">{k}</span>
                  <span class="mono min-w-0 flex-1 truncate text-dim" title={v}>{v}</span>
                  {#if meta}<span class="shrink-0 text-faint">{meta}</span>{/if}
                  {#if k !== "Database"}<button class="btn btn-ghost btn-icon h-7 w-7" title="Open folder" onclick={() => open(v)}><FolderOpen size={13} /></button>{/if}
                </div>
              {/each}
            </div>
          {/if}
          <div class="divide-y divide-line">
            {#snippet d1()}<button class="btn h-9 text-xs" onclick={backup} disabled={busy === "backup"}>{#if busy === "backup"}<LoaderCircle size={13} class="spin" />{:else}<Download size={13} />{/if} Create backup</button>{/snippet}
            {@render row("Backup", "Save a consistent snapshot of your entire library, profiles, history and settings.", d1)}
            {#snippet d2()}<button class="btn h-9 text-xs" onclick={restore} disabled={busy === "restore"}>{#if busy === "restore"}<LoaderCircle size={13} class="spin" />{:else}<Upload size={13} />{/if} Restore…</button>{/snippet}
            {@render row("Restore", "Replace current data with a backup. The backup is validated first and your current data is saved as a safety copy.", d2)}
            {#snippet d3()}<button class="btn h-9 text-xs" onclick={exportCfg}><FileJson size={13} /> Export JSON</button>{/snippet}
            {@render row("Export configuration", "Export games, tags, launch profiles and settings as readable JSON.", d3)}
          </div>
        {/if}
      </div>
    {/key}
  </div>
</div>
