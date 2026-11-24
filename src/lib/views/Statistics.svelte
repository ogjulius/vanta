<script lang="ts">
  import { ChartColumn } from "@lucide/svelte";
  import Artwork from "../components/Artwork.svelte";
  import PlatformTag from "../components/PlatformTag.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import { api } from "../api";
  import { app, navigate, toastError } from "../state.svelte";
  import { formatDateTime, formatDuration, formatTotal, relativeTime, PLATFORM_LABEL } from "../format";
  import type { Statistics } from "../types";

  let stats = $state<Statistics | null>(null);
  let hoverDay = $state<number | null>(null);
  let showTable = $state(false);

  $effect(() => {
    void app.libraryVersion;
    api.getStatistics().then((s) => (stats = s)).catch((e) => toastError("Couldn't load statistics", e));
  });

  const week = $derived(stats?.last7Days ?? []);
  const weekMax = $derived(Math.max(3600, ...week.map((d) => d.seconds)));
  // Round the axis max up to a whole number of hours for clean gridlines.
  const axisHours = $derived(Math.ceil(weekMax / 3600));
  const weekTotal = $derived(week.reduce((n, d) => n + d.seconds, 0));
  const hasWeek = $derived(weekTotal > 0);
  const topMax = $derived(Math.max(1, ...(stats?.perGame ?? []).map((g) => Math.max(g.trackedSeconds, g.platformPlaytimeMin * 60))));

  function dayLabel(date: string, short = true) {
    const d = new Date(date + "T12:00:00");
    return d.toLocaleDateString(undefined, short ? { weekday: "short" } : { weekday: "long", month: "short", day: "numeric" });
  }

  // Chart geometry
  const W = 640;
  const H = 220;
  const PAD_L = 34;
  const PAD_B = 26;
  const PAD_T = 12;
  const plotW = W - PAD_L - 8;
  const plotH = H - PAD_B - PAD_T;
  const slot = $derived(plotW / Math.max(1, week.length));
  const barW = $derived(Math.min(44, slot * 0.5));
</script>

<div class="mx-auto max-w-[1400px] px-[var(--pad)] pb-16 pt-9">
  <header class="rise mb-8">
    <h1 class="font-display text-[30px] font-semibold leading-none">Statistics</h1>
    <p class="mt-2.5 text-[13px] text-dim">Built only from real data: sessions tracked by VANTA and playtime your platforms report.</p>
  </header>

  {#if !stats}
    <div class="grid grid-cols-4 gap-4">{#each Array(4) as _, i (i)}<div class="skeleton h-24 rounded-xl"></div>{/each}</div>
  {:else if stats.totalSessions === 0 && stats.perGame.length === 0 && stats.failedLaunches === 0}
    <div class="rounded-2xl border border-line bg-surface-1">
      <EmptyState icon={ChartColumn} title="No play data yet" message="Launch a game from VANTA and your sessions, playtime and history will appear here. Nothing is estimated or invented." />
    </div>
  {:else}
    <!-- KPI tiles -->
    <div class="rise mb-8 grid grid-cols-2 gap-px overflow-hidden rounded-xl bg-line hairline lg:grid-cols-5">
      {#each [
        { label: "Tracked playtime", value: formatTotal(stats.totalTrackedSeconds), sub: "sessions launched from VANTA" },
        { label: "Platform playtime", value: formatTotal(stats.totalPlatformMinutes * 60), sub: "reported by Steam & itch" },
        { label: "Sessions", value: stats.totalSessions.toLocaleString(), sub: `${stats.gamesPlayed} games with playtime` },
        { label: "Failed launches", value: stats.failedLaunches.toLocaleString(), sub: "see history below" },
        { label: "Library", value: stats.librarySize.toLocaleString(), sub: `${stats.installedCount} installed` },
      ] as k (k.label)}
        <div class="bg-surface-1 px-5 py-5">
          <div class="eyebrow">{k.label}</div>
          <div class="font-display mt-2 text-[26px] font-semibold tabular-nums leading-none">{k.value}</div>
          <div class="mt-1.5 text-[11.5px] text-faint">{k.sub}</div>
        </div>
      {/each}
    </div>

    <div class="grid gap-6 xl:grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)]">
      <!-- Weekly chart -->
      <section class="rise rounded-xl border border-line bg-surface-1 p-5" style="animation-delay:40ms">
        <div class="mb-4 flex items-baseline justify-between">
          <div>
            <h2 class="text-[14px] font-semibold">Last 7 days</h2>
            <p class="text-xs text-faint">Hours played per day · {weekTotal ? formatDuration(weekTotal) : "0h"} total</p>
          </div>
          <button class="text-xs text-faint hover:text-text" onclick={() => (showTable = !showTable)}>{showTable ? "Show chart" : "Show table"}</button>
        </div>
        {#if !hasWeek}
          <div class="flex h-[220px] items-center justify-center text-[13px] text-faint">No sessions tracked in the last 7 days.</div>
        {:else if showTable}
          <table class="w-full text-[12.5px]">
            <thead><tr class="text-left text-faint"><th class="py-1.5 font-medium">Day</th><th class="py-1.5 text-right font-medium">Played</th></tr></thead>
            <tbody>
              {#each week as d (d.date)}
                <tr class="border-t border-line"><td class="py-2 text-dim">{dayLabel(d.date, false)}</td><td class="py-2 text-right tabular-nums">{d.seconds ? formatDuration(d.seconds) : "—"}</td></tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <div class="relative">
            <svg viewBox="0 0 {W} {H}" class="w-full" role="img" aria-label="Hours played per day over the last 7 days">
              {#each Array(axisHours + 1) as _, i (i)}
                {@const y = PAD_T + plotH - (i / axisHours) * plotH}
                {#if axisHours <= 6 || i % Math.ceil(axisHours / 5) === 0}
                  <line x1={PAD_L} x2={W - 8} y1={y} y2={y} stroke="var(--line)" stroke-width="1" />
                  <text x={PAD_L - 8} y={y + 3.5} text-anchor="end" fill="var(--text-faint)" font-size="10.5">{i}h</text>
                {/if}
              {/each}
              {#each week as d, i (d.date)}
                {@const h = (d.seconds / (axisHours * 3600)) * plotH}
                {@const x = PAD_L + slot * i + (slot - barW) / 2}
                {@const today = i === week.length - 1}
                <!-- Hit target is the whole column, larger than the bar -->
                <rect x={PAD_L + slot * i} y={PAD_T} width={slot} height={plotH} fill={hoverDay === i ? "rgb(255 255 255 / 0.025)" : "transparent"} role="presentation" onmouseenter={() => (hoverDay = i)} onmouseleave={() => (hoverDay = null)} />
                {#if d.seconds > 0}
                  <path
                    d="M{x},{PAD_T + plotH} v{-Math.max(0, h - 4)} q0,-4 4,-4 h{barW - 8} q4,0 4,4 v{Math.max(0, h - 4)} z"
                    fill="var(--accent)"
                    opacity={hoverDay === null || hoverDay === i ? 1 : 0.45}
                    class="pointer-events-none t transition-opacity"
                  />
                {/if}
                <text x={PAD_L + slot * i + slot / 2} y={H - 8} text-anchor="middle" font-size="11" fill={today ? "var(--text)" : "var(--text-faint)"} font-weight={today ? 600 : 400}>
                  {today ? "Today" : dayLabel(d.date)}
                </text>
              {/each}
            </svg>
            {#if hoverDay !== null}
              {@const d = week[hoverDay]}
              <div
                class="pointer-events-none absolute -translate-x-1/2 rounded-lg border border-line-strong bg-elevated px-3 py-2 text-xs shadow-xl"
                style="left:{((PAD_L + slot * hoverDay + slot / 2) / W) * 100}%; top: 0"
              >
                <div class="text-faint">{dayLabel(d.date, false)}</div>
                <div class="font-medium tabular-nums">{d.seconds ? formatDuration(d.seconds) : "Not played"}</div>
              </div>
            {/if}
          </div>
        {/if}
      </section>

      <!-- Most played -->
      <section class="rise rounded-xl border border-line bg-surface-1 p-5" style="animation-delay:80ms">
        <h2 class="text-[14px] font-semibold">Most played</h2>
        <p class="mb-4 text-xs text-faint">Larger of platform-reported and VANTA-tracked time</p>
        <div class="space-y-3">
          {#each stats.perGame.slice(0, 7) as g (g.gameId)}
            {@const total = Math.max(g.trackedSeconds, g.platformPlaytimeMin * 60)}
            <button class="group flex w-full items-center gap-3 text-left" onclick={() => navigate("game", g.gameId)}>
              <Artwork name={g.name} path={g.cover} platform={g.platform} class="h-11 w-[30px] shrink-0 rounded hairline" />
              <div class="min-w-0 flex-1">
                <div class="flex items-baseline justify-between gap-3">
                  <span class="truncate text-[12.5px] font-medium group-hover:text-accent-ink">{g.name}</span>
                  <span class="shrink-0 text-xs tabular-nums text-dim">{formatDuration(total, { short: true })}</span>
                </div>
                <div class="mt-1.5 h-1 overflow-hidden rounded-full bg-surface-3">
                  <div class="h-full rounded-full bg-accent" style="width:{(total / topMax) * 100}%"></div>
                </div>
              </div>
            </button>
          {/each}
          {#if stats.perGame.length === 0}<p class="py-6 text-center text-[13px] text-faint">No playtime recorded yet.</p>{/if}
        </div>
      </section>
    </div>

    <div class="mt-6 grid gap-6 xl:grid-cols-[minmax(0,1fr)_minmax(0,1.6fr)]">
      <section class="rise rounded-xl border border-line bg-surface-1 p-5" style="animation-delay:120ms">
        <h2 class="mb-4 text-[14px] font-semibold">Longest sessions</h2>
        {#if stats.longestSessions.length === 0}
          <p class="py-6 text-center text-[13px] text-faint">No completed sessions yet.</p>
        {:else}
          <ol class="space-y-2.5">
            {#each stats.longestSessions as s, i (s.id)}
              <li class="flex items-center gap-3 text-[12.5px]">
                <span class="font-display w-5 text-faint">{i + 1}</span>
                <button class="min-w-0 flex-1 truncate text-left hover:text-accent-ink" onclick={() => navigate("game", s.gameId)}>{s.gameName ?? s.gameId}</button>
                <span class="text-faint">{relativeTime(s.startedAt)}</span>
                <span class="w-16 text-right font-medium tabular-nums">{formatDuration(s.durationS, { short: true })}</span>
              </li>
            {/each}
          </ol>
        {/if}
      </section>

      <section class="rise rounded-xl border border-line bg-surface-1 p-5" style="animation-delay:160ms">
        <h2 class="mb-4 text-[14px] font-semibold">Launch history</h2>
        <div class="max-h-[340px] overflow-y-auto">
          <table class="w-full text-[12.5px]">
            <thead class="sticky top-0 bg-surface-1">
              <tr class="text-left text-[10.5px] uppercase tracking-[0.1em] text-faint">
                <th class="pb-2 font-semibold">Game</th><th class="pb-2 font-semibold">Started</th><th class="pb-2 font-semibold">Duration</th><th class="pb-2 font-semibold">Result</th>
              </tr>
            </thead>
            <tbody>
              {#each stats.history as s (s.id)}
                <tr class="border-t border-line">
                  <td class="max-w-[220px] truncate py-2 pr-3">{s.gameName ?? s.gameId}</td>
                  <td class="py-2 pr-3 text-dim">{formatDateTime(s.startedAt)}</td>
                  <td class="py-2 pr-3 tabular-nums text-dim">{s.status === "completed" ? formatDuration(s.durationS, { short: true }) : "—"}</td>
                  <td class="py-2" title={s.error ?? ""}>
                    <span class="inline-flex items-center gap-1.5 {s.status === 'failed' ? 'text-red-300' : s.status === 'completed' ? 'text-dim' : 'text-amber-200'}">
                      <span class="h-1.5 w-1.5 rounded-full {s.status === 'failed' ? 'bg-red-400' : s.status === 'completed' ? 'bg-emerald-400' : 'bg-amber-300'}"></span>
                      {s.status === "completed" ? "Completed" : s.status === "failed" ? "Failed" : s.status === "running" ? "Running" : "Interrupted"}
                    </span>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          {#if stats.history.length === 0}<p class="py-6 text-center text-[13px] text-faint">No launches yet.</p>{/if}
        </div>
      </section>
    </div>
    <p class="mt-6 text-[11.5px] text-faint">Platform playtime comes from {PLATFORM_LABEL.steam}'s and itch's local data; Epic and GOG don't expose playtime locally.</p>
  {/if}
</div>
