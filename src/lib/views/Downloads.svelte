<script lang="ts">
  import { Download, RefreshCw, CircleCheck, CircleAlert, LoaderCircle } from "@lucide/svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import { api, errMsg } from "../api";
  import { app, scanAll } from "../state.svelte";
  import { formatBytes, relativeTime } from "../format";
  import type { SteamDownload } from "../types";

  let downloads = $state<SteamDownload[]>([]);
  let error = $state<string | null>(null);
  let loaded = $state(false);

  async function poll() {
    try {
      downloads = await api.steamDownloads();
      error = null;
    } catch (e) {
      error = errMsg(e);
    } finally {
      loaded = true;
    }
  }

  $effect(() => {
    poll();
    // Steam rewrites manifests every few seconds while downloading; poll gently, only while visible.
    const t = setInterval(() => document.visibilityState === "visible" && poll(), 4000);
    return () => clearInterval(t);
  });

  const syncable = $derived(app.platforms.filter((p) => p.id !== "local"));
</script>

<div class="mx-auto max-w-[1100px] px-[var(--pad)] pb-16 pt-9">
  <header class="rise mb-8">
    <h1 class="font-display text-[30px] font-semibold leading-none">Downloads</h1>
    <p class="mt-2.5 text-[13px] text-dim">Live progress of installs and updates running in your platform clients, plus library sync status.</p>
  </header>

  <section class="rise mb-10" style="animation-delay:40ms">
    <h2 class="eyebrow mb-3">Active in Steam</h2>
    {#if !loaded}
      <div class="skeleton h-20 rounded-xl"></div>
    {:else if error}
      <div class="rounded-xl border border-red-500/20 bg-red-500/[0.06] px-4 py-3 text-sm text-red-300">{error}</div>
    {:else if downloads.length === 0}
      <div class="rounded-xl border border-line bg-surface-1">
        <EmptyState icon={Download} title="No active downloads" message="Install or update games in Steam and their progress appears here. VANTA doesn't download games itself — it reads progress from your clients." />
      </div>
    {:else}
      <div class="space-y-3">
        {#each downloads as d (d.appId)}
          {@const pct = d.bytesTotal ? Math.min(100, (d.bytesDone / d.bytesTotal) * 100) : 0}
          <div class="rounded-xl border border-line bg-surface-1 px-5 py-4">
            <div class="flex items-center justify-between gap-4">
              <div class="min-w-0">
                <div class="truncate text-[14px] font-medium">{d.name}</div>
                <div class="mt-0.5 text-xs capitalize text-faint">{d.state}</div>
              </div>
              <div class="text-right text-xs tabular-nums text-dim">
                <div class="font-display text-[15px] font-semibold text-text">{pct.toFixed(0)}%</div>
                {formatBytes(d.bytesDone)} / {formatBytes(d.bytesTotal)}
              </div>
            </div>
            <div class="mt-3 h-1.5 overflow-hidden rounded-full bg-surface-3">
              <div class="h-full rounded-full bg-accent t transition-[width]" style="width:{pct}%"></div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </section>

  <section class="rise" style="animation-delay:80ms">
    <div class="mb-3 flex items-center justify-between">
      <h2 class="eyebrow">Library sync</h2>
      <button class="btn h-8 text-xs" onclick={() => scanAll()} disabled={app.scanning}>
        <RefreshCw size={13} class={app.scanning ? "spin" : ""} /> {app.scanning ? "Syncing…" : "Sync all"}
      </button>
    </div>
    <div class="divide-y divide-line rounded-xl border border-line bg-surface-1">
      {#each syncable as p (p.id)}
        <div class="flex items-center gap-4 px-5 py-3.5">
          {#if app.scanning && p.enabled && p.detected}
            <LoaderCircle size={16} class="spin text-accent-ink" />
          {:else if p.lastError}
            <CircleAlert size={16} class="text-red-400" />
          {:else if p.detected && p.enabled}
            <CircleCheck size={16} class="text-emerald-400" />
          {:else}
            <span class="h-4 w-4 rounded-full border border-line-strong"></span>
          {/if}
          <div class="min-w-0 flex-1">
            <div class="text-[13px] font-medium">{p.name}</div>
            <div class="truncate text-xs text-faint">
              {#if !p.enabled}Disabled in Settings{:else if !p.detected}Not installed on this system{:else if p.lastError}{p.lastError}{:else}{p.gameCount} games · {p.installedCount} installed · synced {relativeTime(p.lastSync).toLowerCase()}{/if}
            </div>
          </div>
          {#if p.enabled && p.detected}
            <button class="btn btn-ghost h-8 text-xs" disabled={app.scanning} onclick={() => scanAll(p.id)}>Sync</button>
          {/if}
        </div>
      {/each}
    </div>
  </section>
</div>
