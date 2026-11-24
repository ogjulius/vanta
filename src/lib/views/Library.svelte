<script lang="ts">
  import { Search, LayoutGrid, List, RefreshCw, Plus, X, Gamepad2, SearchX, Heart, History, HardDrive } from "@lucide/svelte";
  import GameCard from "../components/GameCard.svelte";
  import GameRow from "../components/GameRow.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import { api } from "../api";
  import { app, libUi, scanAll, toastError } from "../state.svelte";
  import { PLATFORM_HUE, PLATFORM_LABEL } from "../format";
  import type { Game, GameQuery, PlatformId, SortKey } from "../types";

  let { mode }: { mode: "library" | "installed" | "favorites" | "recent" } = $props();

  const titles = {
    library: ["Library", "Every game, from every platform."],
    installed: ["Installed", "Ready to play right now."],
    favorites: ["Favorites", "The ones you keep coming back to."],
    recent: ["Recently Played", "Pick up where you left off."],
  } as const;

  let games = $state<Game[]>([]);
  let loading = $state(true);
  let tags = $state<string[]>([]);
  let searchEl = $state<HTMLInputElement>();
  let debounced = $state(libUi.search);
  let timer: ReturnType<typeof setTimeout>;

  const layout = $derived(libUi.layout ?? app.settings.defaultLayout);
  const sort = $derived<SortKey>(mode === "recent" ? "recent" : (libUi.sort ?? app.settings.defaultSort));
  const availablePlatforms = $derived(
    app.platforms.filter((p) => p.enabled && p.gameCount > 0).map((p) => p.id as PlatformId),
  );
  const filtersActive = $derived(!!debounced || libUi.platforms.length > 0 || libUi.status !== "all" || !!libUi.tag);

  function onSearch(v: string) {
    libUi.search = v;
    clearTimeout(timer);
    timer = setTimeout(() => (debounced = v), 110);
  }

  $effect(() => {
    const q: GameQuery = {
      search: debounced || null,
      platforms: [...libUi.platforms],
      installStatus: mode === "installed" ? "installed" : libUi.status,
      favoritesOnly: mode === "favorites",
      playedOnly: mode === "recent",
      tag: libUi.tag,
      sort,
    };
    void app.libraryVersion;
    let cancelled = false;
    api
      .listGames(q)
      .then((r) => {
        if (!cancelled) games = r;
      })
      .catch((e) => toastError("Couldn't load library", e))
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    void app.libraryVersion;
    api.listTags().then((t) => {
      tags = t;
      if (libUi.tag && !t.includes(libUi.tag)) libUi.tag = null;
    });
  });

  function togglePlatform(p: string) {
    libUi.platforms = libUi.platforms.includes(p) ? libUi.platforms.filter((x) => x !== p) : [...libUi.platforms, p];
  }

  function clearFilters() {
    onSearch("");
    debounced = "";
    libUi.platforms = [];
    libUi.status = "all";
    libUi.tag = null;
  }

  function onKey(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    const typing = target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT";
    if ((e.key === "/" && !typing) || (e.key.toLowerCase() === "k" && (e.ctrlKey || e.metaKey))) {
      e.preventDefault();
      searchEl?.focus();
      searchEl?.select();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="mx-auto max-w-[1680px] px-[var(--pad)] pb-16 pt-9">
  <!-- Header -->
  <header class="rise mb-7 flex flex-wrap items-end justify-between gap-4">
    <div>
      <h1 class="font-display text-[30px] font-semibold leading-none">{titles[mode][0]}</h1>
      <p class="mt-2.5 text-[13px] text-dim">
        {titles[mode][1]}
        {#if !loading}<span class="text-faint"> · {games.length} {games.length === 1 ? "game" : "games"}</span>{/if}
      </p>
    </div>
    <div class="flex items-center gap-2">
      <button class="btn" onclick={() => scanAll()} disabled={app.scanning} title="Rescan all platforms">
        <RefreshCw size={15} class={app.scanning ? "spin" : ""} />
        {app.scanning ? "Syncing…" : "Sync"}
      </button>
      <button class="btn btn-primary" onclick={() => (app.addGameOpen = true)}><Plus size={15} /> Add game</button>
    </div>
  </header>

  <!-- Toolbar -->
  <div class="rise sticky top-0 z-10 -mx-[var(--pad)] mb-6 flex flex-wrap items-center gap-2.5 bg-bg/85 px-[var(--pad)] py-3 backdrop-blur-xl" style="animation-delay:40ms">
    <div class="relative w-[300px] max-w-full">
      <Search size={15} class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-faint" />
      <input
        bind:this={searchEl}
        class="field h-9 pl-9 pr-14"
        placeholder="Search games"
        value={libUi.search}
        oninput={(e) => onSearch(e.currentTarget.value)}
        onkeydown={(e) => e.key === "Escape" && (onSearch(""), (debounced = ""), e.currentTarget.blur())}
      />
      {#if libUi.search}
        <button class="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-faint hover:text-text" onclick={() => (onSearch(""), (debounced = ""))} aria-label="Clear search"><X size={13} /></button>
      {:else}
        <kbd class="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 rounded border border-line-strong px-1.5 text-[10px] text-faint">/</kbd>
      {/if}
    </div>

    {#if availablePlatforms.length > 1}
      <div class="flex items-center gap-1">
        {#each availablePlatforms as p (p)}
          {@const on = libUi.platforms.includes(p)}
          <button
            class="flex h-9 items-center gap-2 rounded-lg px-3 text-xs font-medium t transition-colors {on ? 'bg-white/[0.08] text-text hairline' : 'text-dim hover:bg-white/[0.04] hover:text-text'}"
            onclick={() => togglePlatform(p)}
          >
            <span class="h-1.5 w-1.5 rounded-full" style="background:{PLATFORM_HUE[p]}"></span>{PLATFORM_LABEL[p]}
          </button>
        {/each}
      </div>
    {/if}

    {#if mode !== "installed"}
      <select class="field h-9 w-auto py-0 text-xs" bind:value={libUi.status} aria-label="Installation status">
        <option value="all">All games</option>
        <option value="installed">Installed</option>
        <option value="not_installed">Not installed</option>
      </select>
    {/if}

    {#if tags.length}
      <select class="field h-9 w-auto py-0 text-xs" value={libUi.tag ?? ""} onchange={(e) => (libUi.tag = e.currentTarget.value || null)} aria-label="Tag">
        <option value="">All tags</option>
        {#each tags as t (t)}<option value={t}>#{t}</option>{/each}
      </select>
    {/if}

    {#if filtersActive}
      <button class="btn btn-ghost h-9 text-xs" onclick={clearFilters}><X size={13} /> Clear</button>
    {/if}

    <div class="flex-1"></div>

    {#if mode !== "recent"}
      <select class="field h-9 w-auto py-0 text-xs" value={sort} onchange={(e) => (libUi.sort = e.currentTarget.value as SortKey)} aria-label="Sort">
        <option value="name">Name A–Z</option>
        <option value="recent">Recently played</option>
        <option value="playtime">Most played</option>
        <option value="added">Recently added</option>
      </select>
    {/if}
    <div class="flex rounded-lg bg-surface-2 p-[3px] hairline">
      <button class="rounded-md p-1.5 {layout === 'grid' ? 'bg-surface-3 text-text' : 'text-faint hover:text-dim'}" onclick={() => (libUi.layout = "grid")} title="Grid" aria-label="Grid layout"><LayoutGrid size={15} /></button>
      <button class="rounded-md p-1.5 {layout === 'list' ? 'bg-surface-3 text-text' : 'text-faint hover:text-dim'}" onclick={() => (libUi.layout = "list")} title="List" aria-label="List layout"><List size={15} /></button>
    </div>
  </div>

  <!-- Content -->
  {#if loading}
    <div class="grid gap-[var(--gap)]" style="grid-template-columns: repeat(auto-fill, minmax(var(--card-min), 1fr))">
      {#each Array(12) as _, i (i)}
        <div><div class="skeleton aspect-[2/3] rounded-[var(--radius)]"></div><div class="skeleton mt-3 h-3 w-3/4 rounded"></div></div>
      {/each}
    </div>
  {:else if games.length === 0}
    {#if filtersActive}
      <EmptyState icon={SearchX} title="No matches" message="Nothing in your library matches these filters.">
        <button class="btn" onclick={clearFilters}>Clear filters</button>
      </EmptyState>
    {:else if mode === "favorites"}
      <EmptyState icon={Heart} title="No favorites yet" message="Open any game and tap the heart to pin it here." />
    {:else if mode === "recent"}
      <EmptyState icon={History} title="Nothing played yet" message="Games you launch — from VANTA or from their own launchers — will show up here." />
    {:else if mode === "installed"}
      <EmptyState icon={HardDrive} title="No installed games found" message="Install games through Steam, Legendary, Heroic or itch, then sync — or add one manually.">
        <button class="btn" onclick={() => scanAll()}><RefreshCw size={15} /> Sync now</button>
        <button class="btn btn-primary" onclick={() => (app.addGameOpen = true)}><Plus size={15} /> Add game</button>
      </EmptyState>
    {:else}
      <EmptyState icon={Gamepad2} title="Your library is empty" message="VANTA reads your installed Steam, Epic (Legendary/Heroic), GOG and itch.io games. Sync to discover them, or add a game manually.">
        <button class="btn" onclick={() => scanAll()} disabled={app.scanning}><RefreshCw size={15} class={app.scanning ? "spin" : ""} /> Sync platforms</button>
        <button class="btn btn-primary" onclick={() => (app.addGameOpen = true)}><Plus size={15} /> Add game</button>
      </EmptyState>
    {/if}
  {:else if layout === "grid"}
    <div class="grid gap-x-[var(--gap)] gap-y-[calc(var(--gap)+4px)]" style="grid-template-columns: repeat(auto-fill, minmax(var(--card-min), 1fr))">
      {#each games as game, i (game.id)}
        <GameCard {game} index={i} />
      {/each}
    </div>
  {:else}
    <div class="fade-in">
      <div class="list-head grid gap-4 px-3 pb-2 text-[10.5px] font-semibold uppercase tracking-[0.12em] text-faint">
        <span></span><span>Title</span><span>Platform</span><span>Status</span><span>Played</span><span>Last played</span><span></span>
      </div>
      <div class="border-t border-line pt-1">
        {#each games as game (game.id)}<GameRow {game} />{/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .list-head {
    grid-template-columns: 36px minmax(0, 2.4fr) 110px 100px 70px 100px 40px;
  }
</style>
