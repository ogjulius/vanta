<script lang="ts">
  import { Cpu, MemoryStick, Gauge, Timer, Gamepad2, CircleAlert } from "@lucide/svelte";
  import { api, errMsg } from "../api";
  import { app } from "../state.svelte";
  import { formatBytes, formatUptime } from "../format";
  import type { SystemMetrics } from "../types";

  const HISTORY = 60;
  let m = $state<SystemMetrics | null>(null);
  let error = $state<string | null>(null);
  let cpuHist = $state<number[]>([]);
  let memHist = $state<number[]>([]);
  let gpuHist = $state<number[]>([]);

  function push(arr: number[], v: number) {
    const next = [...arr, v];
    return next.length > HISTORY ? next.slice(next.length - HISTORY) : next;
  }

  async function sample() {
    if (document.visibilityState !== "visible") return;
    try {
      const s = await api.systemMetrics();
      m = s;
      error = null;
      cpuHist = push(cpuHist, s.cpuPercent);
      memHist = push(memHist, (s.memoryUsed / Math.max(1, s.memoryTotal)) * 100);
      if (s.gpus[0]?.utilization != null) gpuHist = push(gpuHist, s.gpus[0].utilization);
    } catch (e) {
      error = errMsg(e);
    }
  }

  // Sampling runs only while this page is open, at the user-configured interval.
  $effect(() => {
    const interval = app.settings.monitorIntervalMs;
    sample();
    const t = setInterval(sample, interval);
    return () => clearInterval(t);
  });

  function spark(values: number[], w = 240, h = 44) {
    if (values.length < 2) return "";
    const step = w / (HISTORY - 1);
    const offset = 0;
    return values.map((v, i) => `${i === 0 ? "M" : "L"}${(offset + i * step).toFixed(1)},${(h - (Math.min(100, v) / 100) * (h - 4) - 2).toFixed(1)}`).join(" ");
  }
  function area(values: number[], w = 240, h = 44) {
    const line = spark(values, w, h);
    if (!line) return "";
    const end = ((values.length - 1) * w) / (HISTORY - 1);
    return `${line} L${end.toFixed(1)},${h} L0,${h} Z`;
  }
  const tempTone = (t: number | null) => (t == null ? "" : t >= 85 ? "text-red-300" : t >= 75 ? "text-amber-200" : "");
</script>

{#snippet sparkline(values: number[], label: string)}
  <svg viewBox="0 0 240 44" class="mt-4 h-11 w-full" preserveAspectRatio="none" role="img" aria-label="{label} over the last {HISTORY} samples">
    <line x1="0" x2="240" y1="22" y2="22" stroke="var(--line)" stroke-dasharray="2 4" />
    <path d={area(values)} fill="var(--accent)" opacity="0.12" />
    <path d={spark(values)} fill="none" stroke="var(--accent)" stroke-width="2" vector-effect="non-scaling-stroke" stroke-linejoin="round" stroke-linecap="round" />
  </svg>
{/snippet}

<div class="mx-auto max-w-[1400px] px-[var(--pad)] pb-16 pt-9">
  <header class="rise mb-8 flex items-end justify-between">
    <div>
      <h1 class="font-display text-[30px] font-semibold leading-none">Performance</h1>
      <p class="mt-2.5 text-[13px] text-dim">Live system metrics, sampled every {(app.settings.monitorIntervalMs / 1000).toFixed(1)}s while this page is open.</p>
    </div>
    {#if m}<div class="flex items-center gap-2 text-xs text-faint"><span class="live-dot h-1.5 w-1.5 rounded-full bg-emerald-400"></span> Live</div>{/if}
  </header>

  {#if error}
    <div class="mb-6 flex items-center gap-2 rounded-xl border border-red-500/20 bg-red-500/[0.06] px-4 py-3 text-sm text-red-300"><CircleAlert size={16} /> {error}</div>
  {/if}

  {#if !m}
    <div class="grid grid-cols-3 gap-4">{#each Array(3) as _, i (i)}<div class="skeleton h-44 rounded-xl"></div>{/each}</div>
  {:else}
    {@const gpu = m.gpus[0]}
    <div class="grid gap-4 lg:grid-cols-3">
      <!-- CPU -->
      <section class="rise rounded-xl border border-line bg-surface-1 p-5">
        <div class="flex items-center justify-between">
          <div class="eyebrow flex items-center gap-2"><Cpu size={13} /> CPU</div>
          <span class="max-w-[60%] truncate text-[11px] text-faint" title={m.cpuName}>{m.cpuName}</span>
        </div>
        <div class="font-display mt-3 text-[38px] font-semibold tabular-nums leading-none">{m.cpuPercent.toFixed(0)}<span class="text-xl text-faint">%</span></div>
        <div class="mt-1 text-xs text-faint">{m.cpuCount} threads · load {m.loadAverage.map((l) => l.toFixed(2)).join(" / ")}</div>
        {@render sparkline(cpuHist, "CPU utilization")}
        <div class="mt-4 grid gap-[3px]" style="grid-template-columns: repeat({Math.min(m.perCore.length, 16)}, 1fr)" aria-label="Per-core utilization">
          {#each m.perCore as c, i (i)}
            <div class="relative h-6 overflow-hidden rounded-[3px] bg-surface-3" title="Thread {i}: {c.toFixed(0)}%">
              <div class="absolute inset-x-0 bottom-0 rounded-[3px] bg-accent t transition-[height]" style="height:{Math.max(4, c)}%; opacity:{0.35 + (c / 100) * 0.65}"></div>
            </div>
          {/each}
        </div>
      </section>

      <!-- Memory -->
      <section class="rise rounded-xl border border-line bg-surface-1 p-5" style="animation-delay:40ms">
        <div class="eyebrow flex items-center gap-2"><MemoryStick size={13} /> Memory</div>
        <div class="font-display mt-3 text-[38px] font-semibold tabular-nums leading-none">
          {((m.memoryUsed / m.memoryTotal) * 100).toFixed(0)}<span class="text-xl text-faint">%</span>
        </div>
        <div class="mt-1 text-xs text-faint">{formatBytes(m.memoryUsed)} used of {formatBytes(m.memoryTotal)}</div>
        {@render sparkline(memHist, "Memory usage")}
        <dl class="mt-4 grid grid-cols-2 gap-3 text-xs">
          <div class="rounded-lg bg-surface-2 px-3 py-2.5"><dt class="text-faint">Available</dt><dd class="mt-0.5 font-medium tabular-nums">{formatBytes(m.memoryAvailable)}</dd></div>
          <div class="rounded-lg bg-surface-2 px-3 py-2.5"><dt class="text-faint">Swap</dt><dd class="mt-0.5 font-medium tabular-nums">{m.swapTotal ? `${formatBytes(m.swapUsed)} / ${formatBytes(m.swapTotal)}` : "None"}</dd></div>
        </dl>
      </section>

      <!-- GPU -->
      <section class="rise rounded-xl border border-line bg-surface-1 p-5" style="animation-delay:80ms">
        <div class="flex items-center justify-between">
          <div class="eyebrow flex items-center gap-2"><Gauge size={13} /> GPU</div>
          {#if gpu}<span class="max-w-[60%] truncate text-[11px] text-faint" title="{gpu.name} · {gpu.source}">{gpu.name}</span>{/if}
        </div>
        {#if gpu}
          <div class="mt-3 flex items-end gap-6">
            <div>
              <div class="font-display text-[38px] font-semibold tabular-nums leading-none">
                {gpu.utilization != null ? gpu.utilization.toFixed(0) : "—"}<span class="text-xl text-faint">{gpu.utilization != null ? "%" : ""}</span>
              </div>
              <div class="mt-1 text-xs text-faint">utilization</div>
            </div>
            <div>
              <div class="font-display text-[22px] font-semibold tabular-nums leading-none {tempTone(gpu.temperature)}">{gpu.temperature != null ? `${gpu.temperature.toFixed(0)}°C` : "—"}</div>
              <div class="mt-1 text-xs text-faint">temperature</div>
            </div>
          </div>
          {#if gpu.utilization != null}{@render sparkline(gpuHist, "GPU utilization")}{/if}
          <dl class="mt-4 grid grid-cols-2 gap-3 text-xs">
            <div class="rounded-lg bg-surface-2 px-3 py-2.5"><dt class="text-faint">VRAM</dt><dd class="mt-0.5 font-medium tabular-nums">{gpu.memoryUsed != null && gpu.memoryTotal ? `${formatBytes(gpu.memoryUsed)} / ${formatBytes(gpu.memoryTotal)}` : "Unavailable"}</dd></div>
            <div class="rounded-lg bg-surface-2 px-3 py-2.5"><dt class="text-faint">Power</dt><dd class="mt-0.5 font-medium tabular-nums">{gpu.powerWatts != null ? `${gpu.powerWatts.toFixed(0)} W` : "Unavailable"}</dd></div>
          </dl>
        {:else}
          <div class="mt-6 rounded-lg border border-dashed border-line-strong px-4 py-6 text-center text-xs leading-relaxed text-faint">
            {m.gpuNote ?? "GPU metrics unavailable."}
          </div>
        {/if}
      </section>
    </div>

    <!-- Running games -->
    <section class="rise mt-4 rounded-xl border border-line bg-surface-1 p-5" style="animation-delay:120ms">
      <div class="mb-4 flex items-center justify-between">
        <div class="eyebrow flex items-center gap-2"><Gamepad2 size={13} /> Running game</div>
        <div class="flex items-center gap-1.5 text-xs text-faint"><Timer size={12} /> System uptime {formatUptime(m.uptimeSecs)}</div>
      </div>
      {#if m.games.length === 0}
        <p class="py-6 text-center text-[13px] text-faint">
          {Object.keys(app.running).length ? "Waiting for the game process to appear…" : "No game is running. Launch one from VANTA to see its CPU and memory usage here."}
        </p>
      {:else}
        <table class="w-full text-[12.5px]">
          <thead><tr class="text-left text-[10.5px] uppercase tracking-[0.1em] text-faint"><th class="pb-2 font-semibold">Game</th><th class="pb-2 font-semibold">Main process</th><th class="pb-2 font-semibold">PID</th><th class="pb-2 font-semibold">Processes</th><th class="pb-2 text-right font-semibold">CPU</th><th class="pb-2 text-right font-semibold">Memory</th></tr></thead>
          <tbody>
            {#each m.games as g (g.gameId)}
              <tr class="border-t border-line">
                <td class="py-2.5 font-medium">{g.gameName}</td>
                <td class="mono py-2.5 text-dim">{g.processName}</td>
                <td class="py-2.5 tabular-nums text-dim">{g.pid}</td>
                <td class="py-2.5 tabular-nums text-dim">{g.processCount}</td>
                <td class="py-2.5 text-right tabular-nums">{g.cpuPercent.toFixed(1)}%</td>
                <td class="py-2.5 text-right tabular-nums">{formatBytes(g.memoryBytes)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>
  {/if}
</div>
