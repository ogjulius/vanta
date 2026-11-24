<script lang="ts">
  import { Play, Info, Plus, RefreshCw, ArrowRight, Square, LoaderCircle, Gamepad2 } from "@lucide/svelte";
  import Artwork from "../components/Artwork.svelte";
  import GameCard from "../components/GameCard.svelte";
  import PlatformTag from "../components/PlatformTag.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import { api, assetUrl } from "../api";
  import { app, launch, navigate, scanAll, stop, toastError } from "../state.svelte";
  import { effectivePlaytime, formatDuration, formatTotal, greeting, relativeTime } from "../format";
  import type { Game, Statistics } from "../types";

  let recent = $state<Game[]>([]);
  let favorites = $state<Game[]>([]);
  let added = $state<Game[]>([]);
  let all = $state<Game[]>([]);
  let stats = $state<Statistics | null>(null);
  let loaded = $state(false);

  $effect(() => {
    void app.libraryVersion;
    Promise.all([
      api.listGames({ playedOnly: true, sort: "recent", limit: 12 }),
      api.listGames({ favoritesOnly: true, sort: "recent", limit: 12 }),
      api.listGames({ sort: "added", limit: 12 }),
      api.listGames({ sort: "playtime" }),
      api.getStatistics(),
    ])
      .then(([r, f, a, p, s]) => {
        recent = r;
        favorites = f;
        added = a;
        all = p;
        stats = s;
      })
      .catch((e) => toastError("Couldn't load home", e))
      .finally(() => (loaded = true));
  });

  // Featured: the most recently played installed game, otherwise the most played, otherwise any installed.
  const featured = $derived(
    recent.find((g) => g.installed) ?? all.find((g) => g.installed && effectivePlaytime(g) > 0) ?? all.find((g) => g.installed) ?? all[0] ?? null,
  );
  const running = $derived(featured ? app.running[featured.id] : undefined);
  const weekSeconds = $derived(stats?.last7Days.reduce((n, d) => n + d.seconds, 0) ?? 0);
  const platformCount = $derived(new Set(all.map((g) => g.platform)).size);
  const continueList = $derived(recent.filter((g) => g.id !== featured?.id).slice(0, 10));
</script>

<div class="pb-16">
  {#if !loaded}
    <div class="skeleton h-[480px] w-full"></div>
  {:else if all.length === 0}
    <div class="mx-auto max-w-[1680px] px-[var(--pad)] pt-12">
      <h1 class="rise font-display text-[34px] font-semibold">{greeting()}, {app.userName}.</h1>
      <p class="rise mt-2 text-[14px] text-dim" style="animation-delay:40ms">Welcome to VANTA — one library for every game you own.</p>
      <div class="rise mt-10 overflow-hidden rounded-2xl border border-line bg-surface-1" style="animation-delay:80ms">
        <EmptyState icon={Gamepad2} title="Let's find your games" message="VANTA discovers games from Steam, Epic (via Legendary or Heroic), GOG (via Heroic), and the itch app — all from local data, no passwords required.">
          <button class="btn" onclick={() => scanAll()} disabled={app.scanning}>
            <RefreshCw size={15} class={app.scanning ? "spin" : ""} />{app.scanning ? "Scanning…" : "Scan platforms"}
          </button>
          <button class="btn btn-primary" onclick={() => (app.addGameOpen = true)}><Plus size={15} /> Add a game</button>
        </EmptyState>
        <div class="grid grid-cols-4 border-t border-line">
          {#each app.platforms.filter((p) => p.id !== "local") as p (p.id)}
            <div class="border-r border-line px-5 py-4 last:border-r-0">
              <div class="text-[13px] font-medium">{p.name}</div>
              <div class="mt-1 text-xs {p.detected ? (p.connected ? 'text-emerald-400' : 'text-amber-300') : 'text-faint'}">
                {p.detected ? (p.connected ? "Connected" : "Detected") : "Not found"}
              </div>
            </div>
          {/each}
        </div>
      </div>
    </div>
  {:else}
    <!-- Featured hero -->
    {#if featured}
      <section class="relative h-[min(58vh,560px)] min-h-[400px] w-full overflow-hidden">
        <Artwork name={featured.name} path={featured.hero ?? featured.cover} kind="hero" class="absolute inset-0 h-full w-full scale-[1.02]" eager />
        <div class="pointer-events-none absolute inset-0 bg-gradient-to-t from-bg via-bg/50 to-transparent"></div>
        <div class="pointer-events-none absolute inset-0 bg-gradient-to-r from-bg/95 via-bg/40 to-transparent"></div>
        <div class="absolute inset-x-0 top-0 mx-auto max-w-[1680px] px-[var(--pad)] pt-9">
          <p class="rise eyebrow !text-white/60">{greeting()}, {app.userName}</p>
        </div>
        <div class="absolute inset-x-0 bottom-0 mx-auto max-w-[1680px] px-[var(--pad)] pb-12">
          <div class="rise max-w-xl" style="animation-delay:60ms">
            <div class="eyebrow mb-4 !text-accent-ink">{recent.some((g) => g.id === featured.id) ? "Jump back in" : "Featured from your library"}</div>
            {#if featured.logo}
              <img src={assetUrl(featured.logo)} alt={featured.name} class="mb-5 max-h-[120px] max-w-[420px] object-contain object-left drop-shadow-2xl" draggable="false" />
            {:else}
              <h1 class="font-display mb-4 text-[46px] font-bold leading-[1.02] drop-shadow-xl">{featured.name}</h1>
            {/if}
            <div class="mb-6 flex items-center gap-4 text-[13px] text-white/70">
              <PlatformTag platform={featured.platform} size="md" />
              {#if effectivePlaytime(featured) > 0}<span>{formatDuration(effectivePlaytime(featured))} played</span>{/if}
              {#if featured.lastPlayed}<span>Last played {relativeTime(featured.lastPlayed).toLowerCase()}</span>{/if}
            </div>
            <div class="flex items-center gap-2.5">
              {#if running}
                {#if running.state === "running" && running.pid}
                  <button class="btn btn-lg bg-white/10 backdrop-blur" onclick={() => stop(featured.id)}><Square size={14} fill="currentColor" /> Stop</button>
                {:else}
                  <button class="btn btn-lg btn-primary" disabled><LoaderCircle size={16} class="spin" /> {running.state === "running" ? "Playing" : "Launching…"}</button>
                {/if}
              {:else if featured.installed}
                <button class="btn btn-lg btn-primary" onclick={() => launch(featured.id, featured.name)}><Play size={16} fill="currentColor" /> Play</button>
              {/if}
              <button class="btn btn-lg bg-white/[0.08] backdrop-blur hover:!bg-white/15" onclick={() => navigate("game", featured.id)}><Info size={16} /> Details</button>
            </div>
          </div>
        </div>
      </section>
    {/if}

    <div class="mx-auto max-w-[1680px] px-[var(--pad)]">
      <!-- Pulse strip -->
      <div class="rise -mt-2 mb-10 grid grid-cols-2 gap-px overflow-hidden rounded-xl bg-line hairline lg:grid-cols-4" style="animation-delay:100ms">
        {#each [
          { label: "Games", value: all.length.toLocaleString(), sub: `${all.filter((g) => g.installed).length} installed` },
          { label: "Platforms", value: String(platformCount), sub: "in one library" },
          { label: "This week", value: formatTotal(weekSeconds), sub: "tracked by VANTA" },
          { label: "Sessions", value: (stats?.totalSessions ?? 0).toLocaleString(), sub: "launched from VANTA" },
        ] as s (s.label)}
          <div class="bg-surface-1 px-5 py-4">
            <div class="eyebrow">{s.label}</div>
            <div class="font-display mt-1.5 text-[22px] font-semibold tabular-nums">{s.value}</div>
            <div class="text-[11.5px] text-faint">{s.sub}</div>
          </div>
        {/each}
      </div>

      {#snippet shelf(title: string, list: Game[], target: "recent" | "favorites" | "library")}
        {#if list.length}
          <section class="mb-11">
            <div class="mb-4 flex items-baseline justify-between">
              <h2 class="font-display text-[17px] font-semibold">{title}</h2>
              <button class="group flex items-center gap-1 text-xs text-faint hover:text-text" onclick={() => navigate(target)}>
                See all <ArrowRight size={13} class="t transition-transform group-hover:translate-x-0.5" />
              </button>
            </div>
            <div class="shelf -mx-2 -mt-2 grid gap-[var(--gap)] overflow-x-auto px-2 pb-3 pt-2" style="grid-auto-flow: column; grid-auto-columns: calc(var(--card-min) + 12px)">
              {#each list.slice(0, 12) as game, i (game.id)}
                <GameCard {game} index={i} />
              {/each}
            </div>
          </section>
        {/if}
      {/snippet}

      {@render shelf("Continue playing", continueList, "recent")}
      {@render shelf("Favorites", favorites, "favorites")}
      {@render shelf("Recently added", added, "library")}
    </div>
  {/if}
</div>
