<script lang="ts">
  import { Heart, Play, LoaderCircle } from "@lucide/svelte";
  import Artwork from "./Artwork.svelte";
  import PlatformTag from "./PlatformTag.svelte";
  import { app, launch, navigate } from "../state.svelte";
  import { effectivePlaytime, formatDuration } from "../format";
  import type { Game } from "../types";

  let { game, index = 0 }: { game: Game; index?: number } = $props();
  const running = $derived(app.running[game.id]);
  const playtime = $derived(effectivePlaytime(game));
</script>

<div class="card group rise" style="animation-delay:{Math.min(index, 24) * 18}ms">
  <button
    class="cover relative block aspect-[2/3] w-full overflow-hidden rounded-[var(--radius)] bg-surface-1 text-left"
    onclick={() => navigate("game", game.id)}
    aria-label="Open {game.name}"
  >
    <Artwork name={game.name} path={game.cover} platform={game.platform} class="h-full w-full {game.installed ? '' : 'uninstalled'}" />
    <div class="shade pointer-events-none absolute inset-0"></div>
    <div class="ring pointer-events-none absolute inset-0 rounded-[var(--radius)]"></div>

    {#if running}
      <div class="absolute left-2.5 top-2.5 flex items-center gap-1.5 rounded-full bg-black/70 px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider text-white backdrop-blur">
        <span class="live-dot h-1.5 w-1.5 rounded-full bg-emerald-400"></span>
        {running.state === "running" ? "Playing" : "Starting"}
      </div>
    {:else if !game.installed}
      <div class="absolute left-2.5 top-2.5 rounded-full bg-black/65 px-2.5 py-1 text-[10px] font-medium text-white/70 backdrop-blur">Not installed</div>
    {/if}
    {#if game.favorite}
      <div class="absolute right-2.5 top-2.5 text-white drop-shadow"><Heart size={14} fill="currentColor" /></div>
    {/if}
  </button>

  {#if game.installed && !running}
    <button
      class="play absolute flex h-11 w-11 items-center justify-center rounded-full bg-accent text-white shadow-lg"
      onclick={(e) => {
        e.stopPropagation();
        launch(game.id, game.name);
      }}
      aria-label="Play {game.name}"
      title="Play"
    >
      <Play size={17} fill="currentColor" class="ml-0.5" />
    </button>
  {:else if running?.state === "starting"}
    <div class="absolute flex h-11 w-11 items-center justify-center rounded-full bg-black/70 text-white" style="right:10px; bottom: calc(10px + var(--meta-h))">
      <LoaderCircle size={18} class="spin" />
    </div>
  {/if}

  <div class="meta pt-3">
    <button class="block w-full truncate text-left text-[13.5px] font-medium text-text" onclick={() => navigate("game", game.id)}>{game.name}</button>
    <div class="mt-1 flex items-center justify-between gap-2">
      <PlatformTag platform={game.platform} />
      {#if playtime > 0}
        <span class="text-[11px] tabular-nums text-faint">{formatDuration(playtime, { short: true })}</span>
      {/if}
    </div>
  </div>
</div>

<style>
  .card {
    position: relative;
    --meta-h: 52px;
  }
  .cover {
    transform: translateZ(0);
    transition: transform calc(260ms * var(--motion)) cubic-bezier(0.2, 0.7, 0.2, 1),
      box-shadow calc(260ms * var(--motion)) ease;
    box-shadow: 0 1px 0 rgb(255 255 255 / 0.03), 0 10px 30px -18px rgb(0 0 0 / 0.9);
  }
  .card:hover .cover {
    transform: translateY(calc(-4px * var(--motion)));
    box-shadow: 0 24px 50px -24px var(--accent-glow), 0 18px 40px -20px rgb(0 0 0 / 0.9);
  }
  .ring {
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.06);
    transition: box-shadow calc(200ms * var(--motion));
  }
  .card:hover .ring {
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--accent) 50%, rgb(255 255 255 / 0.2));
  }
  .shade {
    background: linear-gradient(180deg, transparent 55%, rgb(0 0 0 / 0.55));
    opacity: 0;
    transition: opacity calc(220ms * var(--motion));
  }
  .card:hover .shade {
    opacity: 1;
  }
  .play {
    right: 10px;
    bottom: calc(10px + var(--meta-h));
    opacity: 0;
    transform: translateY(calc(6px * var(--motion))) scale(0.92);
    transition: opacity calc(200ms * var(--motion)), transform calc(220ms * var(--motion)) cubic-bezier(0.2, 0.7, 0.2, 1),
      background calc(150ms * var(--motion));
    box-shadow: 0 10px 30px -6px var(--accent-glow);
  }
  .card:hover .play,
  .play:focus-visible {
    opacity: 1;
    transform: none;
  }
  .play:hover {
    background: color-mix(in oklab, var(--accent) 85%, white);
  }
  :global(.uninstalled img) {
    filter: grayscale(0.7) brightness(0.6);
  }
  .meta {
    height: var(--meta-h);
  }
</style>
