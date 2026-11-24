<script lang="ts">
  import { Heart, Play, LoaderCircle } from "@lucide/svelte";
  import Artwork from "./Artwork.svelte";
  import PlatformTag from "./PlatformTag.svelte";
  import { app, launch, navigate } from "../state.svelte";
  import { effectivePlaytime, formatDuration, relativeTime } from "../format";
  import type { Game } from "../types";

  let { game }: { game: Game } = $props();
  const running = $derived(app.running[game.id]);
</script>

<div
  class="row group grid cursor-default items-center gap-4 rounded-lg px-3 py-2 t transition-colors hover:bg-white/[0.035]"
  role="button"
  tabindex="0"
  onclick={() => navigate("game", game.id)}
  onkeydown={(e) => e.key === "Enter" && navigate("game", game.id)}
>
  <Artwork name={game.name} path={game.cover} platform={game.platform} class="h-[54px] w-9 rounded-md hairline {game.installed ? '' : 'uninstalled'}" />
  <div class="min-w-0">
    <div class="flex items-center gap-2">
      <span class="truncate text-[13.5px] font-medium">{game.name}</span>
      {#if game.favorite}<Heart size={12} class="shrink-0 text-accent-ink" fill="currentColor" />{/if}
    </div>
    {#if game.tags.length}
      <div class="mt-0.5 truncate text-[11px] text-faint">{game.tags.join(" · ")}</div>
    {/if}
  </div>
  <PlatformTag platform={game.platform} />
  <span class="text-xs {game.installed ? 'text-dim' : 'text-faint'}">
    {#if running}<span class="text-emerald-400">● Playing</span>{:else}{game.installed ? "Installed" : "Not installed"}{/if}
  </span>
  <span class="text-xs tabular-nums text-dim">{formatDuration(effectivePlaytime(game), { short: true })}</span>
  <span class="text-xs text-faint">{relativeTime(game.lastPlayed)}</span>
  <div class="flex justify-end">
    {#if running?.state === "starting"}
      <LoaderCircle size={16} class="spin text-dim" />
    {:else if game.installed && !running}
      <button
        class="btn btn-icon h-8 w-8 opacity-0 group-hover:opacity-100 focus:opacity-100"
        title="Play"
        onclick={(e) => {
          e.stopPropagation();
          launch(game.id, game.name);
        }}
      >
        <Play size={14} fill="currentColor" />
      </button>
    {/if}
  </div>
</div>

<style>
  .row {
    grid-template-columns: 36px minmax(0, 2.4fr) 110px 100px 70px 100px 40px;
  }
</style>
