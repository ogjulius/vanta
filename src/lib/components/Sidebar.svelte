<script lang="ts">
  import {
    House, Library, HardDrive, Heart, History, Download, ChartColumn, Activity, Settings as Cog,
    PanelLeftClose, PanelLeftOpen, Square,
  } from "@lucide/svelte";
  import { app, nav, navigate, saveSettings, stop, type View } from "../state.svelte";
  import { PLATFORM_HUE } from "../format";

  const items: { view: View; label: string; icon: typeof House }[] = [
    { view: "home", label: "Home", icon: House },
    { view: "library", label: "Library", icon: Library },
    { view: "installed", label: "Installed", icon: HardDrive },
    { view: "favorites", label: "Favorites", icon: Heart },
    { view: "recent", label: "Recently Played", icon: History },
    { view: "downloads", label: "Downloads", icon: Download },
    { view: "stats", label: "Statistics", icon: ChartColumn },
    { view: "performance", label: "Performance", icon: Activity },
  ];

  const collapsed = $derived(app.sidebarCollapsed);
  const runningList = $derived(Object.values(app.running));
  const platforms = $derived(app.platforms.filter((p) => p.id !== "local"));

  function toggle() {
    app.sidebarCollapsed = !app.sidebarCollapsed;
    saveSettings({ sidebar: app.sidebarCollapsed ? "collapsed" : "expanded" });
  }

  function statusText(p: (typeof app.platforms)[number]) {
    if (!p.enabled) return "Disabled";
    if (!p.detected) return "Not installed";
    if (p.connected) return p.account ? `Connected · ${p.account}` : "Connected";
    return "Detected · not signed in";
  }
  function statusColor(p: (typeof app.platforms)[number]) {
    if (!p.enabled || !p.detected) return "#3a3a46";
    return p.connected ? "#34d399" : "#fbbf24";
  }
</script>

<aside
  class="relative z-20 flex h-full shrink-0 flex-col border-r border-line bg-elevated t transition-[width]"
  style="width:{collapsed ? 72 : 244}px"
>
  <!-- Brand -->
  <div class="flex h-[68px] items-center gap-3 px-[18px]">
    <img src="/vanta.svg" alt="" class="h-9 w-9 shrink-0" draggable="false" />
    {#if !collapsed}
      <div class="fade-in min-w-0">
        <div class="font-display text-[15px] font-bold tracking-[0.32em] text-text">VANTA</div>
        <div class="-mt-0.5 text-[10px] tracking-wide text-faint">One library. Every game.</div>
      </div>
    {/if}
  </div>

  <!-- Navigation -->
  <nav class="mt-2 flex flex-col gap-0.5 px-3">
    {#each items as item (item.view)}
      {@const active = nav.view === item.view}
      <button
        class="group relative flex h-10 items-center gap-3 rounded-lg px-3 text-[13px] font-medium t transition-colors
          {active ? 'bg-white/[0.06] text-text' : 'text-dim hover:bg-white/[0.03] hover:text-text'}"
        onclick={() => navigate(item.view)}
        title={collapsed ? item.label : undefined}
      >
        {#if active}<span class="absolute left-0 top-2.5 h-5 w-[3px] rounded-r bg-accent shadow-[0_0_12px_var(--accent)]"></span>{/if}
        <item.icon size={17} strokeWidth={active ? 2.1 : 1.8} class="shrink-0 {active ? 'text-accent-ink' : ''}" />
        {#if !collapsed}<span class="truncate">{item.label}</span>{/if}
        {#if item.view === "downloads" && app.scanning && !collapsed}
          <span class="ml-auto h-1.5 w-1.5 rounded-full bg-accent live-dot" title="Scanning"></span>
        {/if}
      </button>
    {/each}
  </nav>

  <div class="flex-1"></div>

  <!-- Now playing -->
  {#if runningList.length && !collapsed}
    <div class="mx-3 mb-3 space-y-2">
      {#each runningList as r (r.gameId)}
        <div class="rise rounded-xl border border-accent/25 bg-accent/[0.08] p-3">
          <div class="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-accent-ink">
            <span class="live-dot h-1.5 w-1.5 rounded-full bg-emerald-400"></span>
            {r.state === "running" ? "Now playing" : "Launching"}
          </div>
          <div class="mt-1 flex items-center gap-2">
            <button class="min-w-0 flex-1 truncate text-left text-[13px] font-medium" onclick={() => navigate("game", r.gameId)}>{r.gameName}</button>
            {#if r.pid}
              <button class="rounded p-1 text-faint hover:bg-white/10 hover:text-text" title="Stop game" onclick={() => stop(r.gameId)}>
                <Square size={12} fill="currentColor" />
              </button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Platforms -->
  <div class="border-t border-line px-3 pb-2 pt-3">
    {#if !collapsed}
      <div class="eyebrow mb-2 px-3">Platforms</div>
      <div class="flex flex-col">
        {#each platforms as p (p.id)}
          <button
            class="flex h-7 items-center gap-2.5 rounded-md px-3 text-left text-[12px] text-dim t transition-colors hover:bg-white/[0.03] hover:text-text"
            title={statusText(p)}
            onclick={() => navigate("settings")}
          >
            <span class="h-1.5 w-1.5 shrink-0 rounded-full" style="background:{statusColor(p)}; box-shadow: 0 0 8px {p.connected && p.enabled ? statusColor(p) : 'transparent'}"></span>
            <span class="flex-1 truncate">{p.name}</span>
            {#if p.enabled && p.detected}<span class="text-[11px] tabular-nums text-faint">{p.gameCount}</span>{/if}
          </button>
        {/each}
      </div>
    {:else}
      <div class="flex flex-col items-center gap-2 py-1">
        {#each platforms as p (p.id)}
          <span class="h-1.5 w-1.5 rounded-full" style="background:{statusColor(p)}" title="{p.name}: {statusText(p)}"></span>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Profile + settings -->
  <div class="flex items-center gap-2 border-t border-line p-3 {collapsed ? 'flex-col' : ''}">
    <div class="flex min-w-0 flex-1 items-center gap-2.5 {collapsed ? 'justify-center' : ''}">
      <div
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-[12px] font-semibold text-white"
        style="background: linear-gradient(135deg, var(--accent), {PLATFORM_HUE.gog}55)"
        title={app.userName}
      >
        {app.userName.slice(0, 1)}
      </div>
      {#if !collapsed}
        <div class="min-w-0">
          <div class="truncate text-[12.5px] font-medium">{app.userName}</div>
          <div class="text-[10.5px] text-faint">VANTA {app.version}</div>
        </div>
      {/if}
    </div>
    <button
      class="btn btn-ghost btn-icon h-8 w-8 {nav.view === 'settings' ? 'text-accent-ink' : ''}"
      title="Settings"
      onclick={() => navigate("settings")}
    >
      <Cog size={16} />
    </button>
    <button class="btn btn-ghost btn-icon h-8 w-8" title={collapsed ? "Expand sidebar" : "Collapse sidebar"} onclick={toggle}>
      {#if collapsed}<PanelLeftOpen size={16} />{:else}<PanelLeftClose size={16} />{/if}
    </button>
  </div>
</aside>
